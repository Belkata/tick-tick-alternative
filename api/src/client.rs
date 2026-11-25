//! A blocking client for the daemon's socket (the CLI and the Flutter bridge are synchronous).

use std::io::{BufRead, BufReader};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::Value;

use crate::paths;
use crate::proto::{Event, Hello, Reply, Request};

/// The daemon went away. `sent` says whether the request may have reached it: if it
/// didn't, retrying elsewhere is safe even for a write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Disconnected {
    pub sent: bool,
}

impl std::fmt::Display for Disconnected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.sent {
            f.write_str("lost the connection to utre-daemon before it answered; check and try again")
        } else {
            f.write_str("utre-daemon isn't running")
        }
    }
}

impl std::error::Error for Disconnected {}

/// One request at a time over one connection; calls from several threads take turns.
pub struct Client {
    conn: Mutex<BufReader<UnixStream>>,
    hello: Hello,
}

impl Client {
    /// Connect to the daemon at `socket` and ask which database it serves.
    pub fn connect(socket: &Path) -> anyhow::Result<Client> {
        let stream = UnixStream::connect(socket)?;
        no_sigpipe(&stream);
        stream.set_read_timeout(Some(Duration::from_secs(30)))?;
        let mut conn = BufReader::new(stream);
        let hello = serde_json::from_value(exchange(&mut conn, &Request::Hello)?)?;
        Ok(Client {
            conn: Mutex::new(conn),
            hello,
        })
    }

    /// The daemon at the default socket, but only if it serves `db`. `None` when no daemon
    /// runs (or it serves another file), so the caller opens the database itself.
    pub fn for_db(db: &Path) -> Option<Client> {
        let socket = paths::socket();
        // No daemon is the normal case: check with a plain connect so no error value is built.
        drop(UnixStream::connect(&socket).ok()?);
        let client = Client::connect(&socket).ok()?;
        paths::same_file(Path::new(&client.hello.db), db).then_some(client)
    }

    pub fn hello(&self) -> &Hello {
        &self.hello
    }

    /// Send a request and wait for its reply. A dead daemon gives a [`Disconnected`] error.
    pub fn call(&self, req: &Request) -> anyhow::Result<Value> {
        let mut conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        exchange(&mut conn, req)
    }
}

fn exchange(conn: &mut BufReader<UnixStream>, req: &Request) -> anyhow::Result<Value> {
    let mut line = serde_json::to_string(req)?;
    line.push('\n');
    if send_all(conn.get_ref(), line.as_bytes()).is_err() {
        return Err(Disconnected { sent: false }.into());
    }
    let mut reply = String::new();
    match conn.read_line(&mut reply) {
        Ok(0) | Err(_) => return Err(Disconnected { sent: true }.into()),
        Ok(_) => {}
    }
    match serde_json::from_str(&reply)? {
        Reply::Ok(v) => Ok(v),
        Reply::Err(e) => Err(anyhow::anyhow!(e)),
    }
}

/// Writing to a socket whose daemon died raises SIGPIPE, which kills a process that hasn't
/// ignored it. Rust binaries ignore it, but the Flutter app (Rust as a library) doesn't, so
/// send without the signal: `MSG_NOSIGNAL` on Linux, `SO_NOSIGPIPE` on macOS.
fn send_all(stream: &UnixStream, mut bytes: &[u8]) -> std::io::Result<()> {
    #[cfg(target_os = "linux")]
    const FLAGS: libc::c_int = libc::MSG_NOSIGNAL;
    #[cfg(not(target_os = "linux"))]
    const FLAGS: libc::c_int = 0;
    while !bytes.is_empty() {
        // SAFETY: the fd is open for the stream's lifetime and the buffer is valid for `len`.
        let n = unsafe {
            libc::send(
                stream.as_raw_fd(),
                bytes.as_ptr().cast(),
                bytes.len(),
                FLAGS,
            )
        };
        if n < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(err);
        }
        bytes = &bytes[n as usize..];
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn no_sigpipe(stream: &UnixStream) {
    let on: libc::c_int = 1;
    // SAFETY: a valid fd and a correctly sized option value.
    unsafe {
        libc::setsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_NOSIGPIPE,
            (&on as *const libc::c_int).cast(),
            std::mem::size_of::<libc::c_int>() as libc::socklen_t,
        );
    }
}

#[cfg(not(target_os = "macos"))]
fn no_sigpipe(_stream: &UnixStream) {}

/// Subscribe on a connection of its own and call `on_event` for each change until the
/// daemon goes away (or `on_event` returns false). Blocks; run it on its own thread.
pub fn subscribe(socket: &Path, mut on_event: impl FnMut(Event) -> bool) -> anyhow::Result<()> {
    let stream = UnixStream::connect(socket)?;
    no_sigpipe(&stream);
    let mut line = serde_json::to_string(&Request::Subscribe)?;
    line.push('\n');
    send_all(&stream, line.as_bytes())?;
    for line in BufReader::new(stream).lines() {
        let event: Event = serde_json::from_str(&line?)?;
        if !on_event(event) {
            break;
        }
    }
    Ok(())
}

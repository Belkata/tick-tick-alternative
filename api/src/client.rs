//! A blocking client for the daemon's socket (the CLI and the Flutter bridge are synchronous).

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::Value;

use crate::paths;
use crate::proto::{Event, Hello, Reply, Request};

/// One request at a time over one connection; calls from several threads take turns.
pub struct Client {
    conn: Mutex<BufReader<UnixStream>>,
    hello: Hello,
}

impl Client {
    /// Connect to the daemon at `socket` and ask which database it serves.
    pub fn connect(socket: &Path) -> anyhow::Result<Client> {
        let stream = UnixStream::connect(socket)?;
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
        let client = Client::connect(&paths::socket()).ok()?;
        paths::same_file(Path::new(&client.hello.db), db).then_some(client)
    }

    pub fn hello(&self) -> &Hello {
        &self.hello
    }

    pub fn call(&self, req: &Request) -> anyhow::Result<Value> {
        let mut conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        exchange(&mut conn, req)
    }
}

fn exchange(conn: &mut BufReader<UnixStream>, req: &Request) -> anyhow::Result<Value> {
    let mut line = serde_json::to_string(req)?;
    line.push('\n');
    conn.get_mut().write_all(line.as_bytes())?;
    let mut reply = String::new();
    if conn.read_line(&mut reply)? == 0 {
        anyhow::bail!("the daemon closed the connection");
    }
    match serde_json::from_str(&reply)? {
        Reply::Ok(v) => Ok(v),
        Reply::Err(e) => Err(anyhow::anyhow!(e)),
    }
}

/// Subscribe on a connection of its own and call `on_event` for each change until the
/// daemon goes away (or `on_event` returns false). Blocks; run it on its own thread.
pub fn subscribe(socket: &Path, mut on_event: impl FnMut(Event) -> bool) -> anyhow::Result<()> {
    let mut stream = UnixStream::connect(socket)?;
    let mut line = serde_json::to_string(&Request::Subscribe)?;
    line.push('\n');
    stream.write_all(line.as_bytes())?;
    for line in BufReader::new(stream).lines() {
        let event: Event = serde_json::from_str(&line?)?;
        if !on_event(event) {
            break;
        }
    }
    Ok(())
}

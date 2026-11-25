//! The daemon protocol end to end: a real DB thread and socket, a blocking client.

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use tt_api::server::{self, Db};
use tt_api::{Api, Client, Event, TaskView, ViewKind};
use tt_core::{Store, local_tz};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tt-api-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Start a daemon on its own runtime thread; returns the socket path.
fn start_daemon(dir: &std::path::Path) -> (PathBuf, PathBuf) {
    let db = dir.join("tt.db");
    let sock = dir.join("tt.sock");
    let (ready_tx, ready_rx) = mpsc::channel();
    let (db2, sock2) = (db.clone(), sock.clone());
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let handle = Db::open(&db2).unwrap();
            let listener = server::bind(&sock2).unwrap();
            ready_tx.send(()).unwrap();
            server::serve(listener, handle, None).await.unwrap();
        });
    });
    ready_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    (db, sock)
}

#[test]
fn remote_calls_match_local_ones() {
    let dir = temp_dir("calls");
    let (db, sock) = start_daemon(&dir);
    let client = Client::connect(&sock).unwrap();
    assert!(tt_api::paths::same_file(
        std::path::Path::new(&client.hello().db),
        &db
    ));
    let api = Api::remote(client);
    assert!(api.is_remote());

    let t = api.add_task("Buy milk").unwrap();
    assert_eq!(api.inbox().unwrap().len(), 1);
    api.set_priority(&t.id, 2).unwrap();
    api.tag_by_path(&t.id, "home/kitchen").unwrap();
    assert!(
        api.inbox().unwrap().is_empty(),
        "a tag triages the task out of Inbox"
    );

    let tz = local_tz();
    let now = 1_795_000_000_000;
    let sb = api.sidebar(now, tz).unwrap();
    assert_eq!(
        sb.tags.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
        ["home", "kitchen"]
    );
    let tag = api.find_tag("home/kitchen").unwrap().unwrap();
    let rows = api
        .tasks(
            &TaskView {
                kind: ViewKind::Tag,
                id: Some(tag.id),
            },
            now,
            tz,
        )
        .unwrap();
    assert_eq!(rows[0].title, "Buy milk");
    assert_eq!(rows[0].priority, 2);

    // Errors come back as errors, with the core's message.
    let err = api.set_title("no-such-id", "x").unwrap_err().to_string();
    assert!(err.contains("not found"), "{err}");

    // The same calls in-process see the same data.
    drop(api);
    let local = Api::local(Store::open(&db).unwrap());
    assert_eq!(local.get_task(&t.id).unwrap().unwrap().priority, 2);
}

#[test]
fn subscribers_hear_writes_and_outside_commits() {
    let dir = temp_dir("events");
    let (db, sock) = start_daemon(&dir);
    let (tx, rx) = mpsc::channel();
    let sub_sock = sock.clone();
    std::thread::spawn(move || {
        tt_api::client::subscribe(&sub_sock, |Event::Changed { version }| {
            tx.send(version).is_ok()
        })
        .unwrap();
    });
    std::thread::sleep(Duration::from_millis(200)); // let the subscription register

    let api = Api::remote(Client::connect(&sock).unwrap());
    api.add_task("through the daemon").unwrap();
    let v1 = rx
        .recv_timeout(Duration::from_secs(3))
        .expect("event for a daemon write");

    // A reader doesn't count as a change.
    api.inbox().unwrap();

    // Another process writes the file directly: the daemon notices within its poll interval.
    Store::open(&db)
        .unwrap()
        .add_task("behind its back")
        .unwrap();
    let v2 = rx
        .recv_timeout(Duration::from_secs(3))
        .expect("event for an outside commit");
    assert!(v2 > v1);
    assert_eq!(api.inbox().unwrap().len(), 2);
}

#[test]
fn a_second_daemon_cannot_take_a_live_socket() {
    let dir = temp_dir("bind");
    let (_db, sock) = start_daemon(&dir);
    let rt = tokio::runtime::Runtime::new().unwrap();
    let err = rt
        .block_on(async { server::bind(&sock).map(|_| ()) })
        .unwrap_err();
    assert!(err.to_string().contains("already listening"), "{err}");
}

/// A daemon on its own runtime that the test can stop (dropping the runtime closes every
/// connection, like the process exiting).
struct Daemon {
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Daemon {
    fn start(db: &std::path::Path, sock: &std::path::Path) -> Daemon {
        let (stop, stop_rx) = tokio::sync::oneshot::channel::<()>();
        let (ready_tx, ready_rx) = mpsc::channel();
        let (db, sock) = (db.to_path_buf(), sock.to_path_buf());
        let thread = std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                let handle = Db::open(&db).unwrap();
                let listener = server::bind(&sock).unwrap();
                ready_tx.send(()).unwrap();
                tokio::select! {
                    _ = server::serve(listener, handle, None) => {}
                    _ = stop_rx => {}
                }
            });
            drop(rt);
        });
        ready_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        Daemon {
            stop: Some(stop),
            thread: Some(thread),
        }
    }

    fn stop(mut self) {
        let _ = self.stop.take().unwrap().send(());
        self.thread.take().unwrap().join().unwrap();
    }
}

#[test]
fn clients_survive_the_daemon_restarting() {
    let dir = temp_dir("restart");
    let (db, sock) = (dir.join("tt.db"), dir.join("tt.sock"));
    // SAFETY: only this test reads TT_SOCKET (the others connect to explicit paths).
    unsafe { std::env::set_var("TT_SOCKET", &sock) };

    let daemon = Daemon::start(&db, &sock);
    let api = Api::connect_or_open(&db).unwrap();
    assert!(api.is_remote());
    api.add_task("before the restart").unwrap();

    // The daemon dies: a read recovers by opening the file and still answers.
    daemon.stop();
    assert_eq!(api.inbox().unwrap().len(), 1);
    assert!(!api.is_remote(), "fell back to the file");

    // Opened directly, it notices another process's commit.
    assert!(!api.outside_change());
    Store::open(&db)
        .unwrap()
        .add_task("from elsewhere")
        .unwrap();
    assert!(api.outside_change());

    // The daemon is back (systemd restarted it): switch to it.
    let daemon = Daemon::start(&db, &sock);
    assert!(!api.outside_change());
    assert!(api.upgrade());
    assert!(api.is_remote());
    assert_eq!(api.inbox().unwrap().len(), 2);

    // It dies again; a write sent to the dead socket never reached it, so it's retried on the file.
    daemon.stop();
    api.add_task("after the second restart").unwrap();
    assert!(!api.is_remote());
    assert_eq!(api.inbox().unwrap().len(), 3);
}

#[test]
fn sync_requests_need_a_sync_node_and_peers_go_through_the_table() {
    let dir = temp_dir("syncreq");
    let (db, sock) = start_daemon(&dir);
    let api = Api::remote(Client::connect(&sock).unwrap());
    let err = api.sync_now().unwrap_err().to_string();
    assert!(err.contains("doesn't run sync"), "{err}");
    assert!(api.peers().unwrap().is_empty());
    Store::open(&db)
        .unwrap()
        .add_peer("n1", "phone", None, 1)
        .unwrap();
    assert_eq!(api.peers().unwrap()[0].name, "phone");
    assert!(api.remove_peer("n1").unwrap());
    // In-process there's no daemon at all.
    let local = Api::local(Store::open(&db).unwrap());
    assert!(
        local
            .sync_status()
            .unwrap_err()
            .to_string()
            .contains("tt-daemon")
    );
}

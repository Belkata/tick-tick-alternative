//! tt-daemon: owns the desktop database and serves it to the app and `tt` (M2, plan/15).
//!
//! - **Socket:** `tt_api::server` runs one DB thread and answers JSON-line requests on
//!   `$XDG_RUNTIME_DIR/tick-alt.sock` (macOS: the data dir), with change events for subscribers.
//!   Commits other processes make to the file (e.g. `tt` while no daemon ran) are noticed and broadcast.
//! - **Tray**: the Inbox count and sync status, refreshed on every change. Linux: a
//!   StatusNotifierItem (`tray_sni.rs`). macOS: a menu-bar icon (`tray_mac.rs`), which owns the
//!   main thread for AppKit, so the tokio runtime runs on a second thread there.
//! - **Sync** (M3, `sync.rs`): the iroh sync node serves paired devices and syncs with them
//!   every 90 s and a few seconds after local edits; `tt sync` and the app reach it via the socket.
//! - **Reminders:** every 5 s the core is asked for reminders due since the last check;
//!   each becomes a notification (notify-rust) with Done and Snooze actions. On macOS this is
//!   `NSUserNotificationCenter` for now (no app bundle yet), which shows the actions in one menu.
//!
//! Flags: `--db <path>`, `--socket <path>`, `--no-tray`, `--no-sync`, `--no-relay` (sync on
//! direct addresses only); `tt-daemon notify <id>` sends one
//! reminder right away. `$TT_SNOOZE_SECS` shortens Snooze for testing; `$TT_APP` is what Open starts.

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use tokio::sync::mpsc;
mod sync;
#[cfg_attr(target_os = "linux", path = "tray_sni.rs")]
#[cfg_attr(target_os = "macos", path = "tray_mac.rs")]
#[cfg_attr(
    not(any(target_os = "linux", target_os = "macos")),
    path = "tray_none.rs"
)]
mod tray;

use std::sync::Arc;

use tt_api::server::{self, Db, SyncControl};
use tt_api::{Request, TzName};
use tt_core::{ReminderFire, Task, local_tz};
use tt_sync::Relays;
use tt_sync::service::SyncService;

/// How often the tray re-reads the Inbox count and the reminder loop checks the schedule.
const TICK: Duration = Duration::from_secs(5);

/// Work the tray menu and notification callbacks hand to the main loop.
#[derive(Debug)]
enum Cmd {
    Refresh,
    /// Sync with every paired device now (tray menu).
    SyncNow,
    /// Re-read the sync status line for the tray.
    SyncStatus,
    /// Show a notification for this task now.
    Notify {
        task_id: String,
        title: String,
    },
    Complete {
        task_id: String,
    },
    Snooze {
        task_id: String,
        title: String,
    },
    /// Start or raise the app, with the tray host's Wayland activation token if it gave one.
    OpenApp {
        token: Option<String>,
    },
    Quit,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// `--db <path>` if given, else `$TT_DB`, else the platform data directory (as `tt`).
fn db_path() -> Result<PathBuf> {
    match flag("--db") {
        Some(p) => Ok(PathBuf::from(p)),
        None => Ok(tt_api::paths::default_db()?),
    }
}

/// This machine's name, shown to paired devices.
fn hostname() -> String {
    machine_name()
        .or_else(|| std::env::var("HOSTNAME").ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "desktop".into())
}

/// The value after `name` on the command line.
fn flag(name: &str) -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == name {
            return args.next();
        }
    }
    None
}

#[cfg(not(target_os = "macos"))]
fn machine_name() -> Option<String> {
    std::fs::read_to_string("/etc/hostname").ok()
}

/// The name set in System Settings > General > About (macOS has no /etc/hostname).
#[cfg(target_os = "macos")]
fn machine_name() -> Option<String> {
    let out = std::process::Command::new("/usr/sbin/scutil")
        .args(["--get", "ComputerName"])
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn has_flag(name: &str) -> bool {
    std::env::args().skip(1).any(|a| a == name)
}

/// Shows a reminder notification and reports the chosen action back to the main loop.
fn notify(tx: mpsc::UnboundedSender<Cmd>, task_id: String, title: String) {
    tokio::task::spawn_blocking(move || {
        let mut n = notify_rust::Notification::new();
        n.appname("Tick Alt")
            .summary(&title)
            .body("Reminder")
            .icon("appointment-soon")
            // Dunst runs the "default" action on middle click when it's the only one and
            // otherwise opens its dmenu picker, so no default here: Done and Snooze only.
            .action("done", "Done")
            .action("snooze", "Snooze 10 min");
        #[cfg(all(unix, not(target_os = "macos")))]
        n.hint(notify_rust::Hint::Category("im.received".into()))
            .timeout(notify_rust::Timeout::Never);
        let shown = n.show();
        let handle = match shown {
            Ok(h) => h,
            Err(e) => return eprintln!("notification failed: {e}"),
        };
        handle.wait_for_action(|action| {
            eprintln!("notification {task_id}: {action}");
            // macOS's NSUserNotificationCenter reports the button's label, not its id.
            let cmd = match action {
                "done" | "Done" => Cmd::Complete { task_id },
                "snooze" | "Snooze 10 min" => Cmd::Snooze { task_id, title },
                _ => return, // "__closed" (dismissed or expired)
            };
            let _ = tx.send(cmd);
        });
    });
}

fn snooze_delay() -> Duration {
    std::env::var("TT_SNOOZE_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(600))
}

/// Start the Flutter app (`$TT_APP`, else `tick_alt` on PATH). A release build is single
/// instance, so if it's already open this raises its window. The token lets Wayland focus it.
#[cfg(not(target_os = "macos"))]
fn open_app(token: Option<String>) {
    let app = std::env::var("TT_APP").unwrap_or_else(|_| "tick_alt".into());
    let mut cmd = std::process::Command::new(&app);
    if let Some(t) = &token {
        cmd.env("XDG_ACTIVATION_TOKEN", t)
            .env("DESKTOP_STARTUP_ID", t);
    }
    match cmd.spawn() {
        // Reap it when it exits (a second instance exits right after raising the first).
        Ok(mut child) => drop(std::thread::spawn(move || child.wait())),
        Err(e) => eprintln!("could not start {app}: {e}"),
    }
}

/// Start or raise the app with `open`: `$TT_APP` (a path to the .app), else by bundle id.
#[cfg(target_os = "macos")]
fn open_app(_token: Option<String>) {
    let mut cmd = std::process::Command::new("/usr/bin/open");
    match std::env::var("TT_APP") {
        Ok(app) => cmd.arg(app),
        Err(_) => cmd.args(["-b", "dev.tickalt.tickAlt"]),
    };
    match cmd.status() {
        Ok(s) if s.success() => {}
        Ok(s) => eprintln!("could not open the app: open exited with {s}"),
        Err(e) => eprintln!("could not open the app: {e}"),
    }
}

#[cfg(not(target_os = "macos"))]
#[tokio::main]
async fn main() -> Result<()> {
    run().await
}

#[cfg(target_os = "macos")]
fn main() {
    tray::run_app(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?
            .block_on(run())
    })
}

async fn run() -> Result<()> {
    let db_file = db_path()?;
    let socket = flag("--socket")
        .map(PathBuf::from)
        .unwrap_or_else(tt_api::paths::socket);

    // The daemon owns the database (plan/03 rule 3): one DB thread, served on the socket.
    let db = Db::open(&db_file).with_context(|| format!("opening {}", db_file.display()))?;
    let listener = server::bind(&socket)?;
    eprintln!("tt-daemon: {} on {}", db_file.display(), socket.display());

    // M3 sync: the node serves paired devices; without it the daemon still works locally.
    let sync = if has_flag("--no-sync") {
        None
    } else {
        let relays = if has_flag("--no-relay") {
            Relays::Off
        } else {
            Relays::N0
        };
        match SyncService::start(&db_file, hostname(), relays).await {
            Ok(s) => {
                eprintln!("tt-daemon: sync node {}", s.status().node_id);
                Some(s)
            }
            Err(e) => {
                eprintln!("tt-daemon: sync is off: {e:#}");
                None
            }
        }
    };
    tokio::spawn({
        let db = db.clone();
        let control = sync
            .clone()
            .map(|s| Arc::new(sync::Control(s)) as Arc<dyn SyncControl>);
        async move {
            if let Err(e) = server::serve(listener, db, control).await {
                eprintln!("tt-daemon: socket: {e:#}");
            }
        }
    });

    let (tx, mut rx) = mpsc::unbounded_channel();
    let inbox: Vec<Task> = db.get(Request::Inbox {}).await?;
    let handle = if has_flag("--no-tray") {
        None
    } else {
        let sync_line = sync.as_ref().map(|s| s.status_line());
        Some(tray::Tray::start(inbox, sync_line, tx.clone()).await?)
    };

    // `tt-daemon notify <id>`: one notification for a task right away (testing actions).
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(i) = args.iter().position(|a| a == "notify") {
        let suffix = args.get(i + 1).context("notify needs a task id")?.clone();
        let id: Option<String> = db.get(Request::ResolveTaskId { suffix }).await?;
        let task: Option<Task> = match id {
            Some(id) => db.get(Request::GetTask { id }).await?,
            None => None,
        };
        let task = task.context("no such task")?;
        let _ = tx.send(Cmd::Notify {
            task_id: task.id,
            title: task.title,
        });
    }

    // Auto-sync: shortly after startup, every 90 s, and a few seconds after local edits
    // (the sync's own writes arrive as outside commits, which don't count as edits).
    if let Some(svc) = &sync {
        let tx = tx.clone();
        svc.spawn_auto(db.subscribe_writes(), move || {
            let _ = tx.send(Cmd::SyncStatus);
        });
    }

    // Op-log compaction (plan/16): a minute after startup, then daily.
    tokio::spawn({
        let db = db.clone();
        async move {
            tokio::time::sleep(Duration::from_secs(60)).await;
            loop {
                match db.get::<usize>(Request::Compact {}).await {
                    Ok(0) => {}
                    Ok(n) => {
                        eprintln!("tt-daemon: compacted the op log: dropped {n} superseded ops")
                    }
                    Err(e) => eprintln!("tt-daemon: compaction: {e:#}"),
                }
                tokio::time::sleep(Duration::from_secs(24 * 3600)).await;
            }
        }
    });

    // Refresh the tray on every change, wherever it came from (app, `tt`, an outside write).
    let mut changes = db.subscribe();
    let tz = TzName(local_tz().name().to_string());
    let mut last_check = now_ms();
    let mut tick = tokio::time::interval(TICK);
    loop {
        tokio::select! {
            _ = tick.tick() => {
                // Fire reminders due since the last tick.
                let (from, until) = (last_check, now_ms());
                last_check = until;
                let _ = tx.send(Cmd::SyncStatus);
                let req = Request::UpcomingReminders { from, until, tz: tz.clone() };
                match db.get::<Vec<ReminderFire>>(req).await {
                    Ok(fires) => for f in fires {
                        notify(tx.clone(), f.task_id, f.title);
                    },
                    Err(e) => eprintln!("reminders: {e:#}"),
                }
            }
            Ok(_) = changes.recv() => { let _ = tx.send(Cmd::Refresh); }
            Some(cmd) = rx.recv() => match cmd {
                Cmd::Refresh => match db.get::<Vec<Task>>(Request::Inbox {}).await {
                    Ok(inbox) => if let Some(h) = &handle {
                        h.set_inbox(inbox).await;
                    },
                    Err(e) => eprintln!("inbox: {e:#}"),
                },
                Cmd::SyncNow => if let Some(svc) = sync.clone() {
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        for o in svc.round().await {
                            match o.error {
                                None => eprintln!("tt-daemon: synced with {}", o.peer_name),
                                Some(e) => eprintln!("tt-daemon: {}: {e}", o.peer_name),
                            }
                        }
                        let _ = tx.send(Cmd::SyncStatus);
                    });
                },
                Cmd::SyncStatus => if let (Some(h), Some(svc)) = (&handle, &sync) {
                    h.set_sync_line(svc.status_line()).await;
                },
                Cmd::Notify { task_id, title } => notify(tx.clone(), task_id, title),
                Cmd::Complete { task_id } => {
                    // The DB thread broadcasts the change, which refreshes the tray.
                    if let Err(e) = db.call(Request::CompleteTask { id: task_id }).await {
                        eprintln!("complete: {e}");
                    }
                }
                Cmd::Snooze { task_id, title } => {
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(snooze_delay()).await;
                        let _ = tx.send(Cmd::Notify { task_id, title });
                    });
                }
                Cmd::OpenApp { token } => open_app(token),
                Cmd::Quit => break,
            },
            _ = tokio::signal::ctrl_c() => break,
        }
    }
    if let Some(h) = handle {
        h.shutdown().await;
    }
    if let Some(s) = sync {
        s.shutdown().await;
    }
    Ok(())
}

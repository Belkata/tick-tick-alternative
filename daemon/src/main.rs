//! tt-daemon, Spike 4: a tray icon and reminder notifications on Linux.
//!
//! - The tray is a StatusNotifierItem (ksni), so waybar's `tray` module shows it.
//!   It shows the Inbox count, with a menu: Open, Notify Inbox, Sync now (stub), Quit.
//! - The reminder loop asks the core for reminders due since the last check and
//!   sends freedesktop notifications (notify-rust) with Done and Snooze actions.
//!   Done completes the task through the core; Snooze shows it again later.
//!
//! Spike quality: M2 turns this into the real daemon that owns the DB and serves
//! the socket API (plan/08). For now every DB access opens the store briefly, the
//! same way the CLI does, and SQLite's WAL mode handles the other writers.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use ksni::TrayMethods;
use tokio::sync::mpsc;
use tt_core::{Store, Task, local_tz};

/// How often the tray re-reads the Inbox count and the reminder loop checks the schedule.
const TICK: Duration = Duration::from_secs(5);

/// Work the tray menu and notification callbacks hand to the main loop.
#[derive(Debug)]
enum Cmd {
    Refresh,
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
    OpenApp,
    Quit,
}

struct TickTray {
    inbox: Vec<Task>,
    tx: mpsc::UnboundedSender<Cmd>,
}

impl TickTray {
    fn send(&self, cmd: Cmd) {
        let _ = self.tx.send(cmd);
    }
}

impl ksni::Tray for TickTray {
    fn id(&self) -> String {
        "tick-alt".into()
    }

    fn title(&self) -> String {
        format!("Tick Alt ({})", self.inbox.len())
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        [22, 32, 48]
            .into_iter()
            .map(|s| check_icon(s, !self.inbox.is_empty()))
            .collect()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        let n = self.inbox.len();
        ksni::ToolTip {
            title: "Tick Alt".into(),
            description: match n {
                0 => "Inbox is empty".into(),
                1 => "1 task in Inbox".into(),
                n => format!("{n} tasks in Inbox"),
            },
            ..Default::default()
        }
    }

    /// Left click opens the app.
    fn activate(&mut self, _x: i32, _y: i32) {
        self.send(Cmd::OpenApp);
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::*;
        let mut items: Vec<ksni::MenuItem<Self>> = vec![
            StandardItem {
                label: format!("Inbox: {}", self.inbox.len()),
                enabled: false,
                ..Default::default()
            }
            .into(),
        ];
        // The first few Inbox tasks, each one sends a notification for itself (spike: tests actions).
        for task in self.inbox.iter().take(5) {
            let (task_id, title) = (task.id.clone(), task.title.clone());
            items.push(
                StandardItem {
                    label: format!("  {}", task.title),
                    activate: Box::new(move |t: &mut Self| {
                        t.send(Cmd::Notify {
                            task_id: task_id.clone(),
                            title: title.clone(),
                        })
                    }),
                    ..Default::default()
                }
                .into(),
            );
        }
        items.extend([
            MenuItem::Separator,
            StandardItem {
                label: "Open".into(),
                icon_name: "window-new".into(),
                activate: Box::new(|t: &mut Self| t.send(Cmd::OpenApp)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Refresh".into(),
                icon_name: "view-refresh".into(),
                activate: Box::new(|t: &mut Self| t.send(Cmd::Refresh)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Sync now (M3)".into(),
                enabled: false,
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit".into(),
                icon_name: "application-exit".into(),
                activate: Box::new(|t: &mut Self| t.send(Cmd::Quit)),
                ..Default::default()
            }
            .into(),
        ]);
        items
    }
}

/// A rounded teal square with a white check mark, ARGB32 big-endian.
/// Dimmed when the Inbox is empty.
fn check_icon(size: i32, active: bool) -> ksni::Icon {
    let s = size as f32;
    let (r, g, b) = if active {
        (0x0f, 0x9d, 0x8f)
    } else {
        (0x6b, 0x7f, 0x7d)
    };
    let radius = s * 0.22;
    // The check mark: two segments in unit coordinates.
    let seg = [((0.27, 0.53), (0.43, 0.69)), ((0.43, 0.69), (0.74, 0.34))];
    let stroke = (s * 0.09).max(1.6);
    let mut data = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            // Coverage of the rounded square, antialiased over one pixel.
            let dx = (radius - px).max(px - (s - radius)).max(0.0);
            let dy = (radius - py).max(py - (s - radius)).max(0.0);
            let inside = (radius - (dx * dx + dy * dy).sqrt() + 0.5).clamp(0.0, 1.0);
            // Distance to the check mark.
            let d = seg
                .iter()
                .map(|&((ax, ay), (bx, by))| seg_dist(px, py, ax * s, ay * s, bx * s, by * s))
                .fold(f32::MAX, f32::min);
            let on_check = (stroke / 2.0 - d + 0.5).clamp(0.0, 1.0);
            let mix = |c: u8| (c as f32 * (1.0 - on_check) + 255.0 * on_check) as u8;
            data.extend([(inside * 255.0) as u8, mix(r), mix(g), mix(b)]);
        }
    }
    ksni::Icon {
        width: size,
        height: size,
        data,
    }
}

fn seg_dist(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let (vx, vy) = (bx - ax, by - ay);
    let t = (((px - ax) * vx + (py - ay) * vy) / (vx * vx + vy * vy)).clamp(0.0, 1.0);
    let (cx, cy) = (ax + t * vx - px, ay + t * vy - py);
    (cx * cx + cy * cy).sqrt()
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Same lookup as `tt`: --db, then $TT_DB, then the platform data directory.
fn db_path() -> Result<PathBuf> {
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--db" {
            return args.next().map(PathBuf::from).context("--db needs a path");
        }
    }
    if let Some(p) = std::env::var_os("TT_DB") {
        return Ok(PathBuf::from(p));
    }
    let dir = dirs::data_dir()
        .context("no platform data directory; pass --db")?
        .join("tick-alt");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("tt.db"))
}

/// Runs a closure against a freshly opened store on the blocking pool.
async fn with_store<R: Send + 'static>(
    db: &Path,
    f: impl FnOnce(&Store) -> tt_core::Result<R> + Send + 'static,
) -> Result<R> {
    let db = db.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let store = Store::open(&db).with_context(|| format!("opening {}", db.display()))?;
        Ok(f(&store)?)
    })
    .await?
}

/// Shows a reminder notification and reports the chosen action back to the main loop.
fn notify(tx: mpsc::UnboundedSender<Cmd>, task_id: String, title: String) {
    tokio::task::spawn_blocking(move || {
        let shown = notify_rust::Notification::new()
            .appname("Tick Alt")
            .summary(&title)
            .body("Reminder")
            .icon("appointment-soon")
            // Dunst runs the "default" action on middle click when it's the only one and
            // otherwise opens its dmenu picker, so no default here: Done and Snooze only.
            .action("done", "Done")
            .action("snooze", "Snooze 10 min")
            .hint(notify_rust::Hint::Category("im.received".into()))
            .timeout(notify_rust::Timeout::Never)
            .show();
        let handle = match shown {
            Ok(h) => h,
            Err(e) => return eprintln!("notification failed: {e}"),
        };
        handle.wait_for_action(|action| {
            eprintln!("notification {task_id}: {action}");
            let cmd = match action {
                "done" => Cmd::Complete { task_id },
                "snooze" => Cmd::Snooze { task_id, title },
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

/// Starts the Flutter app: $TT_APP, else `tick_alt` on PATH.
fn open_app() {
    let app = std::env::var("TT_APP").unwrap_or_else(|_| "tick_alt".into());
    if let Err(e) = std::process::Command::new(&app).spawn() {
        eprintln!("could not start {app}: {e}");
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let db = db_path()?;
    eprintln!("tt-daemon: {}", db.display());

    // `tt-daemon notify <id>`: one notification for a task, then wait for its action.
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (tx, mut rx) = mpsc::unbounded_channel();

    let inbox = with_store(&db, |s| s.inbox()).await?;
    let tray = TickTray {
        inbox,
        tx: tx.clone(),
    };
    let handle = tray
        .spawn()
        .await
        .context("starting the tray (is a SNI host like waybar running?)")?;

    if let Some(i) = args.iter().position(|a| a == "notify") {
        let suffix = args.get(i + 1).context("notify needs a task id")?.clone();
        let task = with_store(&db, move |s| {
            Ok(match s.resolve_task_id(&suffix)? {
                Some(id) => s.get_task(&id)?,
                None => None,
            })
        })
        .await?
        .context("no such task")?;
        let _ = tx.send(Cmd::Notify {
            task_id: task.id,
            title: task.title,
        });
    }

    let tz = local_tz();
    let mut last_check = now_ms();
    let mut tick = tokio::time::interval(TICK);
    loop {
        tokio::select! {
            _ = tick.tick() => {
                // Fire reminders due since the last tick, then refresh the count.
                let (from, until) = (last_check, now_ms());
                last_check = until;
                match with_store(&db, move |s| s.upcoming_reminders(from, until, tz)).await {
                    Ok(fires) => for f in fires {
                        notify(tx.clone(), f.task_id, f.title);
                    },
                    Err(e) => eprintln!("reminders: {e:#}"),
                }
                let _ = tx.send(Cmd::Refresh);
            }
            Some(cmd) = rx.recv() => match cmd {
                Cmd::Refresh => match with_store(&db, |s| s.inbox()).await {
                    Ok(inbox) => { handle.update(|t| t.inbox = inbox).await; }
                    Err(e) => eprintln!("inbox: {e:#}"),
                },
                Cmd::Notify { task_id, title } => notify(tx.clone(), task_id, title),
                Cmd::Complete { task_id } => {
                    match with_store(&db, move |s| s.complete_task(&task_id)).await {
                        Ok(_) => { let _ = tx.send(Cmd::Refresh); }
                        Err(e) => eprintln!("complete: {e:#}"),
                    }
                }
                Cmd::Snooze { task_id, title } => {
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(snooze_delay()).await;
                        let _ = tx.send(Cmd::Notify { task_id, title });
                    });
                }
                Cmd::OpenApp => open_app(),
                Cmd::Quit => break,
            },
            _ = tokio::signal::ctrl_c() => break,
        }
    }
    handle.shutdown().await;
    Ok(())
}

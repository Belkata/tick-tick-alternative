//! The macOS menu-bar icon (tray-icon), with the same interface as `tray_sni.rs`.
//!
//! AppKit needs its run loop on the main thread, so on macOS `main` hands the main thread to
//! [`run_app`] and the daemon runs its tokio runtime on a second thread. Everything that
//! touches the icon hops to the main thread through GCD's main queue and keeps its state in a
//! main-thread `thread_local`. The process is an accessory app: no Dock icon or menu bar.
//!
//! Shows a template icon (filled while the Inbox has tasks, an outline when it's empty) with
//! the Inbox count beside it. Menu: the first Inbox tasks, Open, Refresh, the sync status and
//! Sync now, Quit.

use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use dispatch2::DispatchQueue;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
use tokio::sync::mpsc;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use utre_core::Task;

use crate::Cmd;

/// How many Inbox tasks the menu lists.
const MENU_TASKS: usize = 5;

/// Runs the daemon on a second thread and AppKit on this (the main) thread. Exits the
/// process when the daemon returns.
pub fn run_app(daemon: impl FnOnce() -> Result<()> + Send + 'static) -> ! {
    let mtm = MainThreadMarker::new().expect("run_app must be called on the main thread");
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    std::thread::spawn(move || {
        let code = match daemon() {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("Error: {e:?}");
                1
            }
        };
        std::process::exit(code);
    });
    app.run();
    std::process::exit(0)
}

/// What the icon shows. Lives on the main thread.
struct State {
    icon: Option<TrayIcon>,
    inbox: Vec<Task>,
    sync_line: Option<String>,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

/// Run `f` on the main thread with the icon's state (once it exists), then redraw.
fn on_main(f: impl FnOnce(&mut State) + Send + 'static) {
    DispatchQueue::main().exec_async(move || {
        STATE.with(|s| {
            if let Some(state) = s.borrow_mut().as_mut() {
                f(state);
                state.redraw();
            }
        })
    });
}

/// The running menu-bar icon (a handle the daemon's thread can hold).
pub struct Tray {
    /// The tasks the menu lists, for turning a click back into a task.
    listed: Arc<Mutex<Vec<Task>>>,
}

impl Tray {
    pub async fn start(
        inbox: Vec<Task>,
        sync_line: Option<String>,
        tx: mpsc::UnboundedSender<Cmd>,
    ) -> Result<Tray> {
        let listed = Arc::new(Mutex::new(first(&inbox)));
        MenuEvent::set_event_handler(Some({
            let listed = listed.clone();
            move |e: MenuEvent| {
                let cmd = match e.id.as_ref() {
                    "open" => Cmd::OpenApp { token: None },
                    "refresh" => Cmd::Refresh,
                    "sync" => Cmd::SyncNow,
                    "quit" => Cmd::Quit,
                    id => match id.strip_prefix("task:").and_then(|id| {
                        let listed = listed.lock().unwrap();
                        listed.iter().find(|t| t.id == id).cloned()
                    }) {
                        // As on Linux: a task sends a notification for itself (tests actions).
                        Some(t) => Cmd::Notify {
                            task_id: t.id,
                            title: t.title,
                        },
                        None => return,
                    },
                };
                let _ = tx.send(cmd);
            }
        }));
        DispatchQueue::main().exec_async(move || {
            let icon = TrayIconBuilder::new()
                .with_id("utre")
                .with_icon_templated(check_icon(36, !inbox.is_empty()))
                .with_tooltip("Utre")
                .build();
            let icon = match icon {
                Ok(i) => Some(i),
                Err(e) => {
                    eprintln!("utre-daemon: menu-bar icon: {e}");
                    None
                }
            };
            let state = State {
                icon,
                inbox,
                sync_line,
            };
            state.redraw();
            STATE.with(|s| *s.borrow_mut() = Some(state));
        });
        Ok(Tray { listed })
    }

    pub async fn set_inbox(&self, inbox: Vec<Task>) {
        *self.listed.lock().unwrap() = first(&inbox);
        on_main(move |s| s.inbox = inbox);
    }

    pub async fn set_sync_line(&self, line: String) {
        on_main(move |s| s.sync_line = Some(line));
    }

    pub async fn shutdown(self) {
        DispatchQueue::main().exec_async(|| STATE.with(|s| drop(s.borrow_mut().take())));
    }
}

fn first(inbox: &[Task]) -> Vec<Task> {
    inbox.iter().take(MENU_TASKS).cloned().collect()
}

impl State {
    fn redraw(&self) {
        let Some(icon) = &self.icon else { return };
        let n = self.inbox.len();
        let _ = icon.set_icon_templated(Some(check_icon(36, n > 0)));
        icon.set_title((n > 0).then(|| n.to_string()));
        let inbox_line = match n {
            0 => "Inbox is empty".to_string(),
            1 => "1 task in Inbox".to_string(),
            n => format!("{n} tasks in Inbox"),
        };
        let tooltip = match &self.sync_line {
            Some(line) => format!("{inbox_line}\n{line}"),
            None => inbox_line,
        };
        let _ = icon.set_tooltip(Some(tooltip));
        icon.set_menu(Some(Box::new(self.menu())));
    }

    fn menu(&self) -> Menu {
        let menu = Menu::new();
        let add = |item: &dyn tray_icon::menu::IsMenuItem| {
            let _ = menu.append(item);
        };
        add(&MenuItem::new(
            format!("Inbox: {}", self.inbox.len()),
            false,
            None,
        ));
        for task in self.inbox.iter().take(MENU_TASKS) {
            add(&MenuItem::with_id(
                format!("task:{}", task.id),
                format!("    {}", task.title),
                true,
                None,
            ));
        }
        add(&PredefinedMenuItem::separator());
        add(&MenuItem::with_id("open", "Open Utre", true, None));
        add(&MenuItem::with_id("refresh", "Refresh", true, None));
        add(&PredefinedMenuItem::separator());
        let sync_line = self.sync_line.clone();
        add(&MenuItem::new(
            sync_line.clone().unwrap_or_else(|| "Sync is off".into()),
            false,
            None,
        ));
        add(&MenuItem::with_id(
            "sync",
            "Sync now",
            sync_line.is_some(),
            None,
        ));
        add(&PredefinedMenuItem::separator());
        add(&MenuItem::with_id("quit", "Quit Utre daemon", true, None));
        menu
    }
}

/// A template image (only alpha counts; macOS tints it for the menu bar), RGBA.
/// Active: a filled rounded square with the check mark cut out. Empty: the square's outline
/// with the check mark drawn in it.
fn check_icon(size: u32, active: bool) -> Icon {
    let s = size as f32;
    let inset = s * 0.12;
    let radius = s * 0.2;
    let border = s * 0.075;
    let seg = [((0.29, 0.52), (0.44, 0.67)), ((0.44, 0.67), (0.72, 0.36))];
    let stroke = s * 0.1;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            // Signed distance to the rounded square (negative inside).
            let (lo, hi) = (inset + radius, s - inset - radius);
            let dx = (lo - px).max(px - hi).max(0.0);
            let dy = (lo - py).max(py - hi).max(0.0);
            let sd = (dx * dx + dy * dy).sqrt() - radius;
            let inside = (0.5 - sd).clamp(0.0, 1.0);
            let ring = inside * (0.5 + sd + border).clamp(0.0, 1.0);
            let d = seg
                .iter()
                .map(|&((ax, ay), (bx, by))| seg_dist(px, py, ax * s, ay * s, bx * s, by * s))
                .fold(f32::MAX, f32::min);
            let check = (stroke / 2.0 - d + 0.5).clamp(0.0, 1.0);
            let alpha = if active {
                inside * (1.0 - check)
            } else {
                ring.max(check)
            };
            rgba.extend([0, 0, 0, (alpha * 255.0) as u8]);
        }
    }
    Icon::from_rgba(rgba, size, size).expect("icon size matches its pixels")
}

fn seg_dist(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let (vx, vy) = (bx - ax, by - ay);
    let t = (((px - ax) * vx + (py - ay) * vy) / (vx * vx + vy * vy)).clamp(0.0, 1.0);
    let (cx, cy) = (ax + t * vx - px, ay + t * vy - py);
    (cx * cx + cy * cy).sqrt()
}

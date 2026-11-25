//! The Linux tray (Spike 4): a StatusNotifierItem (ksni) in waybar's `tray` module showing the
//! Inbox count and sync status, refreshed on every change. Menu: the first Inbox tasks,
//! Open, Refresh, the sync status and Sync now, Quit.

use std::time::Duration;

use anyhow::{Context, Result};
use ksni::TrayMethods;
use tokio::sync::mpsc;
use tt_core::Task;

use crate::Cmd;

/// The running tray. Every platform's tray module has this interface.
pub struct Tray(ksni::Handle<TickTray>);

impl Tray {
    /// Without a tray host yet (waybar starts after a systemd unit), keep running: ksni
    /// registers the item as soon as a StatusNotifierWatcher appears.
    pub async fn start(
        inbox: Vec<Task>,
        sync_line: Option<String>,
        tx: mpsc::UnboundedSender<Cmd>,
    ) -> Result<Tray> {
        let handle = TickTray {
            inbox,
            sync_line,
            tx,
        }
        .assume_sni_available(true)
        .spawn()
        .await
        .context("starting the tray")?;
        Ok(Tray(handle))
    }

    pub async fn set_inbox(&self, inbox: Vec<Task>) {
        self.0.update(|t| t.inbox = inbox).await;
    }

    pub async fn set_sync_line(&self, line: String) {
        self.0
            .update(|t| {
                if t.sync_line.as_deref() != Some(&line) {
                    t.sync_line = Some(line)
                }
            })
            .await;
    }

    pub async fn shutdown(self) {
        self.0.shutdown().await;
    }
}

pub struct TickTray {
    inbox: Vec<Task>,
    /// "Synced 3 min ago"…; `None` when the daemon runs without sync.
    sync_line: Option<String>,
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
            description: {
                let inbox = match n {
                    0 => "Inbox is empty".to_string(),
                    1 => "1 task in Inbox".to_string(),
                    n => format!("{n} tasks in Inbox"),
                };
                match &self.sync_line {
                    Some(line) => format!("{inbox}\n{line}"),
                    None => inbox,
                }
            },
            ..Default::default()
        }
    }

    /// Left click opens the app.
    fn activate(&mut self, _x: i32, _y: i32) {
        self.send(Cmd::OpenApp {
            token: activation_token(),
        });
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
                activate: Box::new(|t: &mut Self| {
                    t.send(Cmd::OpenApp {
                        token: activation_token(),
                    })
                }),
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
                label: self
                    .sync_line
                    .clone()
                    .unwrap_or_else(|| "Sync is off".into()),
                enabled: false,
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Sync now".into(),
                icon_name: "emblem-synchronizing".into(),
                enabled: self.sync_line.is_some(),
                activate: Box::new(|t: &mut Self| t.send(Cmd::SyncNow)),
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

/// The tray host's XDG activation token for the click being handled. It may arrive just
/// after the event, so look twice (ksni's advice).
fn activation_token() -> Option<String> {
    ksni::current_xdg_activation_token().or_else(|| {
        std::thread::sleep(Duration::from_millis(1));
        ksni::current_xdg_activation_token()
    })
}

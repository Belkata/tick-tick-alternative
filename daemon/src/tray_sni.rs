//! The Linux tray (Spike 4): a StatusNotifierItem (ksni) in waybar's `tray` module showing the
//! Inbox count and sync status, refreshed on every change. Menu: the first Inbox tasks,
//! Open, Refresh, the sync status and Sync now, Quit.

use std::time::Duration;

use anyhow::{Context, Result};
use ksni::TrayMethods;
use tokio::sync::mpsc;
use utre_core::Task;

use crate::Cmd;
use crate::tray_mark::Mark;

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
        "utre".into()
    }

    fn title(&self) -> String {
        format!("Utre ({})", self.inbox.len())
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
            title: "Utre".into(),
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

/// The app icon at tray size: a rounded teal square with the white u-check, and the amber
/// dot while the Inbox has tasks. A grey square, without the dot, when it is empty. ARGB32
/// big-endian; the square reads on both dark and light bars.
fn check_icon(size: i32, active: bool) -> ksni::Icon {
    let s = size as f32;
    let mark = Mark::new(s, 0.64);
    let radius = s * 0.22;
    let mut data = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            // Coverage of the rounded square, antialiased over one pixel.
            let dx = (radius - px).max(px - (s - radius)).max(0.0);
            let dy = (radius - py).max(py - (s - radius)).max(0.0);
            let inside = (radius - (dx * dx + dy * dy).sqrt() + 0.5).clamp(0.0, 1.0);
            // Teal, lighter at the top left like the app icon; grey when empty.
            let k = (px + py) / (2.0 * s);
            let bg = if active {
                [
                    lerp(0x19, 0x07, k),
                    lerp(0xb8, 0x5e, k),
                    lerp(0xa5, 0x55, k),
                ]
            } else {
                [0x6b, 0x7f, 0x7d]
            };
            let mut rgb = bg.map(f32::from);
            let ink = mark.stroke(px, py);
            rgb = rgb.map(|c| c * (1.0 - ink) + 255.0 * ink);
            if active {
                let dot = mark.dot(px, py);
                let amber = [0xff as f32, 0xb4 as f32, 0x40 as f32];
                for (c, a) in rgb.iter_mut().zip(amber) {
                    *c = *c * (1.0 - dot) + a * dot;
                }
            }
            data.extend([
                (inside * 255.0) as u8,
                rgb[0] as u8,
                rgb[1] as u8,
                rgb[2] as u8,
            ]);
        }
    }
    ksni::Icon {
        width: size,
        height: size,
        data,
    }
}

fn lerp(a: u8, b: u8, k: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * k) as u8
}

/// The tray host's XDG activation token for the click being handled. It may arrive just
/// after the event, so look twice (ksni's advice).
fn activation_token() -> Option<String> {
    ksni::current_xdg_activation_token().or_else(|| {
        std::thread::sleep(Duration::from_millis(1));
        ksni::current_xdg_activation_token()
    })
}

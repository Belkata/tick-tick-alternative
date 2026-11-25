//! Platforms without a tray yet: the same interface as `tray_sni.rs`, doing nothing.

use anyhow::Result;
use tokio::sync::mpsc;
use utre_core::Task;

use crate::Cmd;

pub struct Tray;

impl Tray {
    pub async fn start(
        _inbox: Vec<Task>,
        _sync_line: Option<String>,
        _tx: mpsc::UnboundedSender<Cmd>,
    ) -> Result<Tray> {
        eprintln!("utre-daemon: no tray on this platform yet");
        Ok(Tray)
    }

    pub async fn set_inbox(&self, _inbox: Vec<Task>) {}

    pub async fn set_sync_line(&self, _line: String) {}

    pub async fn shutdown(self) {}
}

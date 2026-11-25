//! The daemon's answers to control requests on the socket (`utre_api::server::SyncControl`):
//! sync (`utre_sync::service::SyncService`, for `tt sync` and the app's Devices page) and
//! Google (`google.rs`). Either can be missing (`--no-sync`, `--no-google`).

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde_json::Value;
use utre_api::Request;
use utre_api::server::SyncControl;
use utre_sync::service::{SyncService, outcome};

use crate::google::GoogleService;

/// What the socket's control requests are answered by. (A struct of our own: the trait
/// and the services are foreign types.)
pub struct Control {
    pub sync: Option<Arc<SyncService>>,
    pub google: Option<Arc<GoogleService>>,
}

impl Control {
    fn sync(&self) -> Result<&Arc<SyncService>, String> {
        self.sync
            .as_ref()
            .ok_or_else(|| "this utre-daemon doesn't run sync".to_string())
    }

    fn google(&self) -> Result<&Arc<GoogleService>, String> {
        self.google
            .as_ref()
            .ok_or_else(|| "this utre-daemon doesn't run Google (--no-google)".to_string())
    }
}

impl SyncControl for Control {
    fn handle(
        &self,
        req: Request,
    ) -> Pin<Box<dyn Future<Output = Result<Value, String>> + Send + '_>> {
        Box::pin(async move {
            let v = match req {
                Request::SyncPair => serde_json::to_value(
                    self.sync()?
                        .node
                        .pairing_ticket()
                        .map_err(|e| format!("{e:#}"))?,
                ),
                Request::SyncJoin { ticket } => {
                    let sync = self.sync()?;
                    let r = sync
                        .node
                        .join(&ticket)
                        .await
                        .map_err(|e| format!("{e:#}"))?;
                    sync.refresh_peers().await;
                    serde_json::to_value(outcome(r))
                }
                Request::SyncNow => serde_json::to_value(self.sync()?.round().await),
                Request::SyncStatus => serde_json::to_value(self.sync()?.status()),
                Request::GoogleStatus => serde_json::to_value(self.google()?.status()),
                Request::GoogleSignIn => serde_json::to_value(self.google()?.sign_in().await?),
                Request::GoogleSignOut => {
                    self.google()?.sign_out().await?;
                    serde_json::to_value(())
                }
                Request::GoogleSync => {
                    self.google()?.sync_now().await?;
                    serde_json::to_value(())
                }
                other => return Err(format!("not a control request: {other:?}")),
            };
            v.map_err(|e| e.to_string())
        })
    }
}

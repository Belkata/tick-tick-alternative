//! The daemon's side of sync: `tt_sync::service::SyncService` answers `tt sync` and the
//! app's Devices page through the socket (`tt_api::server::SyncControl`).

use std::future::Future;
use std::pin::Pin;

use serde_json::Value;
use tt_api::Request;
use tt_api::server::SyncControl;
use tt_sync::service::{SyncService, outcome};

/// The service behind the socket (a newtype: both the trait and the type are foreign).
pub struct Control(pub std::sync::Arc<SyncService>);

impl SyncControl for Control {
    fn handle(
        &self,
        req: Request,
    ) -> Pin<Box<dyn Future<Output = Result<Value, String>> + Send + '_>> {
        Box::pin(async move {
            let v = match req {
                Request::SyncPair => serde_json::to_value(
                    self.0.node.pairing_ticket().map_err(|e| format!("{e:#}"))?,
                ),
                Request::SyncJoin { ticket } => {
                    let r = self
                        .0
                        .node
                        .join(&ticket)
                        .await
                        .map_err(|e| format!("{e:#}"))?;
                    self.0.refresh_peers().await;
                    serde_json::to_value(outcome(r))
                }
                Request::SyncNow => serde_json::to_value(self.0.round().await),
                Request::SyncStatus => serde_json::to_value(self.0.status()),
                other => return Err(format!("not a sync request: {other:?}")),
            };
            v.map_err(|e| e.to_string())
        })
    }
}

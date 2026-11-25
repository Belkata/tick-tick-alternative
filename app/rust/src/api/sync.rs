//! Spike 2: iroh sync for Dart. Thin wrapper over utre-sync.

use std::path::Path;
use std::time::Duration;

use flutter_rust_bridge::frb;
use utre_sync::{Relays, SyncNode};

/// The outcome of pulling a peer's open tasks.
pub struct PullResult {
    pub peer_device: String,
    pub peer_id: String,
    pub titles: Vec<String>,
    pub elapsed_ms: u64,
    /// "direct", "relay" or "unknown" when the reply arrived.
    pub first_path: String,
    /// The same after waiting a few seconds for hole punching.
    pub final_path: String,
    pub rtt_ms: Option<u64>,
    pub paths: Vec<String>,
}

pub struct ServedItem {
    pub peer_device: String,
    pub peer_id: String,
    pub task_count: u32,
}

/// An iroh node. It owns its tokio runtime because iroh needs one and frb's
/// worker threads don't provide it.
#[frb(opaque)]
pub struct SyncPeer {
    rt: tokio::runtime::Runtime,
    node: Option<SyncNode>,
}

impl SyncPeer {
    /// Bind a node with the key at `key_path` (created on first run), serving `db_path`.
    pub fn start(db_path: String, key_path: String, device: String) -> anyhow::Result<SyncPeer> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?;
        let node = rt.block_on(SyncNode::start(
            db_path.into(),
            Path::new(&key_path),
            device,
            Relays::N0,
        ))?;
        Ok(Self {
            rt,
            node: Some(node),
        })
    }

    /// Wait for a home relay so the ticket works across networks. False on timeout.
    pub fn wait_online(&self, timeout_ms: u64) -> bool {
        self.rt
            .block_on(self.node().wait_online(Duration::from_millis(timeout_ms)))
    }

    pub fn node_id(&self) -> String {
        self.node().id()
    }

    pub fn ticket(&self) -> anyhow::Result<String> {
        self.node().ticket()
    }

    pub fn addrs(&self) -> Vec<String> {
        self.node().addrs()
    }

    pub fn pull(&self, ticket: String) -> anyhow::Result<PullResult> {
        let r = self.rt.block_on(self.node().pull(&ticket))?;
        Ok(PullResult {
            peer_device: r.peer_device,
            peer_id: r.peer_id,
            titles: r.tasks.into_iter().map(|t| t.title).collect(),
            elapsed_ms: r.elapsed.as_millis() as u64,
            first_path: r.first_path.as_str().into(),
            final_path: r.final_path.as_str().into(),
            rtt_ms: r.rtt.map(|d| d.as_millis() as u64),
            paths: r.paths,
        })
    }

    /// Connections this node has served, oldest first.
    pub fn served(&self) -> Vec<ServedItem> {
        self.node()
            .served()
            .into_iter()
            .map(|s| ServedItem {
                peer_device: s.peer_device,
                peer_id: s.peer_id,
                task_count: s.task_count as u32,
            })
            .collect()
    }

    fn node(&self) -> &SyncNode {
        self.node.as_ref().expect("node is only taken on drop")
    }
}

impl Drop for SyncPeer {
    fn drop(&mut self) {
        if let Some(node) = self.node.take() {
            self.rt.block_on(node.shutdown());
        }
    }
}

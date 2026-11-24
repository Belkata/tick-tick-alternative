//! The change stream behind `TaskDb.changes()`. Outside `api/` so flutter_rust_bridge
//! doesn't try to expose it to Dart.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard};

use crate::frb_generated::StreamSink;

/// Change-stream subscribers and the version they've seen. Shared with the thread that
/// relays the daemon's events.
#[derive(Default)]
pub(crate) struct Changes {
    pub(crate) sinks: Mutex<Vec<StreamSink<u32>>>,
    pub(crate) version: AtomicU32,
}

impl Changes {
    pub(crate) fn tick(&self) {
        let v = self.version.fetch_add(1, Ordering::SeqCst) + 1;
        // A sink whose Dart stream was cancelled fails to add; drop it.
        self.sinks().retain(|sink| sink.add(v).is_ok());
    }

    pub(crate) fn sinks(&self) -> MutexGuard<'_, Vec<StreamSink<u32>>> {
        self.sinks.lock().unwrap_or_else(|e| e.into_inner())
    }
}


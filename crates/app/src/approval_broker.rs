//! In-process waiters for pending Approvals (agent loop ↔ control-plane decide).

use keryx_domain::{ApprovalId, ApprovalStatus};
use std::collections::HashMap;
use std::sync::Mutex;
use tokio::sync::oneshot;

/// Notifies agent-loop waiters when an Approval is decided.
#[derive(Debug, Default)]
pub struct ApprovalBroker {
    waiters: Mutex<HashMap<ApprovalId, oneshot::Sender<ApprovalStatus>>>,
}

impl ApprovalBroker {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a waiter; returns the receiver the agent loop should await.
    pub fn register(&self, id: ApprovalId) -> oneshot::Receiver<ApprovalStatus> {
        let (tx, rx) = oneshot::channel();
        if let Ok(mut map) = self.waiters.lock() {
            map.insert(id, tx);
        }
        rx
    }

    /// Deliver a decision to a waiting agent loop (if any).
    pub fn resolve(&self, id: ApprovalId, status: ApprovalStatus) {
        if let Ok(mut map) = self.waiters.lock() {
            if let Some(tx) = map.remove(&id) {
                let _ = tx.send(status);
            }
        }
    }
}

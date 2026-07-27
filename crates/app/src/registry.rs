use keryx_domain::{RunId, SessionId};
use std::collections::{HashMap, HashSet};
use tokio_util::sync::CancellationToken;

/// Tracks Active root Runs (Session exclusivity + global cap) and Child Run cancel trees.
#[derive(Debug, Default)]
pub struct ActiveRunRegistry {
    /// Session → Active **root** Run only (Child Runs do not occupy this slot).
    by_session: HashMap<SessionId, RunId>,
    cancel_tokens: HashMap<RunId, CancellationToken>,
    /// Parent root/child → set of active child Run ids.
    children: HashMap<RunId, HashSet<RunId>>,
    /// Child → parent (for cleanup).
    parent_of: HashMap<RunId, RunId>,
}

impl ActiveRunRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Count of Active **root** Runs (global concurrency cap).
    #[must_use]
    pub fn active_count(&self) -> usize {
        self.by_session.len()
    }

    #[must_use]
    pub fn active_for_session(&self, session_id: SessionId) -> Option<RunId> {
        self.by_session.get(&session_id).copied()
    }

    /// Register a new Active **root** Run. Caller must have already enforced exclusivity and cap.
    pub fn register(&mut self, session_id: SessionId, run_id: RunId) -> CancellationToken {
        let token = CancellationToken::new();
        self.by_session.insert(session_id, run_id);
        self.cancel_tokens.insert(run_id, token.clone());
        token
    }

    /// Register a Child Run under `parent_id` (does not take a Session root slot).
    pub fn register_child(&mut self, parent_id: RunId, child_id: RunId) -> CancellationToken {
        let token = CancellationToken::new();
        self.cancel_tokens.insert(child_id, token.clone());
        self.children.entry(parent_id).or_default().insert(child_id);
        self.parent_of.insert(child_id, parent_id);
        token
    }

    pub fn clear(&mut self, session_id: SessionId, run_id: RunId) {
        if self.by_session.get(&session_id) == Some(&run_id) {
            self.by_session.remove(&session_id);
        }
        self.clear_run_tree(run_id);
    }

    /// Clear a Child Run from the tree without touching the Session root slot.
    pub fn clear_child(&mut self, child_id: RunId) {
        if let Some(parent) = self.parent_of.remove(&child_id) {
            if let Some(set) = self.children.get_mut(&parent) {
                set.remove(&child_id);
                if set.is_empty() {
                    self.children.remove(&parent);
                }
            }
        }
        self.cancel_tokens.remove(&child_id);
        // Also drop any nested children of this child (if any).
        if let Some(nested) = self.children.remove(&child_id) {
            for n in nested {
                self.parent_of.remove(&n);
                self.cancel_tokens.remove(&n);
            }
        }
    }

    fn clear_run_tree(&mut self, run_id: RunId) {
        self.cancel_tokens.remove(&run_id);
        if let Some(kids) = self.children.remove(&run_id) {
            for child in kids {
                self.parent_of.remove(&child);
                self.clear_run_tree(child);
            }
        }
        self.parent_of.remove(&run_id);
    }

    /// Cancel a Run and all descendants (cascade).
    pub fn cancel_tree(&self, run_id: RunId) {
        if let Some(token) = self.cancel_tokens.get(&run_id) {
            token.cancel();
        }
        if let Some(kids) = self.children.get(&run_id) {
            for child in kids {
                self.cancel_tree(*child);
            }
        }
    }
}

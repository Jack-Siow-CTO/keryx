use keryx_domain::{RunId, SessionId};
use std::collections::HashMap;
use tokio_util::sync::CancellationToken;

/// Tracks Active Runs for Session exclusivity and the global concurrency cap.
#[derive(Debug, Default)]
pub struct ActiveRunRegistry {
    by_session: HashMap<SessionId, RunId>,
    cancel_tokens: HashMap<RunId, CancellationToken>,
}

impl ActiveRunRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn active_count(&self) -> usize {
        self.by_session.len()
    }

    #[must_use]
    pub fn active_for_session(&self, session_id: SessionId) -> Option<RunId> {
        self.by_session.get(&session_id).copied()
    }

    /// Register a new Active Run. Caller must have already enforced exclusivity and cap.
    pub fn register(&mut self, session_id: SessionId, run_id: RunId) -> CancellationToken {
        let token = CancellationToken::new();
        self.by_session.insert(session_id, run_id);
        self.cancel_tokens.insert(run_id, token.clone());
        token
    }

    pub fn clear(&mut self, session_id: SessionId, run_id: RunId) {
        if self.by_session.get(&session_id) == Some(&run_id) {
            self.by_session.remove(&session_id);
        }
        self.cancel_tokens.remove(&run_id);
    }

    #[must_use]
    pub fn cancel_token(&self, run_id: RunId) -> Option<CancellationToken> {
        self.cancel_tokens.get(&run_id).cloned()
    }
}

use crate::{PrincipalId, SessionId};
use serde::{Deserialize, Serialize};

/// Durable conversational and policy context that may span multiple Runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub principal_id: PrincipalId,
}

impl Session {
    #[must_use]
    pub fn new(principal_id: PrincipalId) -> Self {
        Self {
            id: SessionId::new(),
            principal_id,
        }
    }
}

use serde::{Deserialize, Serialize};
use std::fmt;

/// Authenticated identity that initiates control-plane actions.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PrincipalId(String);

impl PrincipalId {
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PrincipalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl From<&str> for PrincipalId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for PrincipalId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

/// Principal recorded on Session/Run creation from the presenting operator token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Principal {
    pub id: PrincipalId,
}

impl Principal {
    #[must_use]
    pub fn new(id: impl Into<PrincipalId>) -> Self {
        Self { id: id.into() }
    }
}

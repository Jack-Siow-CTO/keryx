use crate::{PrincipalId, RunId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Durable identifier for a Memory entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MemoryId(Uuid);

impl MemoryId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for MemoryId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for MemoryId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::str::FromStr for MemoryId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

/// Curated knowledge retained across Sessions (distinct from Transcript).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: MemoryId,
    pub content: String,
    /// Optional short label / category.
    pub label: Option<String>,
    /// Provenance: creating Run when known.
    pub source_run_id: Option<RunId>,
    /// Provenance: Principal that wrote the entry.
    pub source_principal_id: Option<PrincipalId>,
}

impl MemoryEntry {
    #[must_use]
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            id: MemoryId::new(),
            content: content.into(),
            label: None,
            source_run_id: None,
            source_principal_id: None,
        }
    }
}

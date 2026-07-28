use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Durable Artifact id (Worker data-dir blob + metadata).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ArtifactId(Uuid);

impl ArtifactId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ArtifactId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ArtifactId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::str::FromStr for ArtifactId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

/// Artifact content kind for Console viewers (ADR 0026).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Text,
    Diff,
    Image,
    Json,
    Terminal,
}

impl ArtifactKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Diff => "diff",
            Self::Image => "image",
            Self::Json => "json",
            Self::Terminal => "terminal",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "text" => Some(Self::Text),
            "diff" => Some(Self::Diff),
            "image" => Some(Self::Image),
            "json" => Some(Self::Json),
            "terminal" => Some(Self::Terminal),
            _ => None,
        }
    }
}

/// Artifact metadata (blob bytes live under Worker data dir).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactMeta {
    pub id: ArtifactId,
    pub kind: ArtifactKind,
    pub media_type: String,
    pub byte_len: u64,
    pub created_at: i64,
    pub run_id: Option<String>,
    pub session_id: Option<String>,
    pub summary: String,
    /// Inline text blob for text/diff/terminal/json (Worker data-dir file optional).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_text: Option<String>,
}

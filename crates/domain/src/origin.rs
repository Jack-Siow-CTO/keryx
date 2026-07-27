use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

/// Channel that initiated a Run (control plane, Gateway platform, or Schedule).
///
/// Wire form: `control_plane`, `schedule`, or `gateway:{platform}` (e.g. `gateway:telegram`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum RunOrigin {
    #[default]
    ControlPlane,
    Schedule,
    Gateway {
        platform: String,
    },
}

impl RunOrigin {
    /// Control-plane origin (trusted operator API).
    #[must_use]
    pub fn control_plane() -> Self {
        Self::ControlPlane
    }

    /// Schedule origin (unattended triggers; reduced Policy by default).
    #[must_use]
    pub fn schedule() -> Self {
        Self::Schedule
    }

    /// Gateway origin for a messaging platform (reduced Policy by default).
    ///
    /// `platform` is a stable lowercase token (e.g. `telegram`, `discord`).
    #[must_use]
    pub fn gateway(platform: impl Into<String>) -> Self {
        Self::Gateway {
            platform: platform.into(),
        }
    }

    /// Canonical wire / persistence string.
    #[must_use]
    pub fn as_str(&self) -> String {
        match self {
            Self::ControlPlane => "control_plane".to_string(),
            Self::Schedule => "schedule".to_string(),
            Self::Gateway { platform } => format!("gateway:{platform}"),
        }
    }

    /// Whether this origin is treated as reduced-trust (non–control-plane).
    #[must_use]
    pub fn is_reduced_trust(&self) -> bool {
        !matches!(self, Self::ControlPlane)
    }
}

impl fmt::Display for RunOrigin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_str())
    }
}

/// Parse failure for Run origin wire form.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("invalid Run origin: {0}")]
pub struct ParseRunOriginError(String);

impl FromStr for RunOrigin {
    type Err = ParseRunOriginError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "control_plane" => Ok(Self::ControlPlane),
            "schedule" => Ok(Self::Schedule),
            other if let Some(platform) = other.strip_prefix("gateway:") => {
                if platform.is_empty()
                    || platform.contains(':')
                    || platform
                        .chars()
                        .any(|c| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
                {
                    return Err(ParseRunOriginError(s.to_string()));
                }
                Ok(Self::Gateway {
                    platform: platform.to_string(),
                })
            }
            other => Err(ParseRunOriginError(other.to_string())),
        }
    }
}

impl Serialize for RunOrigin {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.as_str())
    }
}

impl<'de> Deserialize<'de> for RunOrigin {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        RunOrigin::from_str(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_round_trip() {
        for origin in [
            RunOrigin::control_plane(),
            RunOrigin::schedule(),
            RunOrigin::gateway("telegram"),
            RunOrigin::gateway("discord"),
        ] {
            let wire = origin.as_str();
            assert_eq!(RunOrigin::from_str(&wire).unwrap(), origin);
        }
    }

    #[test]
    fn rejects_invalid_gateway() {
        assert!(RunOrigin::from_str("gateway:").is_err());
        assert!(RunOrigin::from_str("gateway:bad:extra").is_err());
        assert!(RunOrigin::from_str("unknown").is_err());
    }
}

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use keryx_domain::Principal;

use crate::error::ApiError;
use crate::state::AppState;

/// Allowlist mapping bearer operator tokens to Principal identities (ADR 0004).
#[derive(Debug, Clone, Default)]
pub struct OperatorTokenTable {
    tokens: std::collections::HashMap<String, keryx_domain::PrincipalId>,
}

impl OperatorTokenTable {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_token(
        mut self,
        token: impl Into<String>,
        principal_id: impl Into<keryx_domain::PrincipalId>,
    ) -> Self {
        self.tokens.insert(token.into(), principal_id.into());
        self
    }

    #[must_use]
    pub fn resolve(&self, token: &str) -> Option<Principal> {
        self.tokens.get(token).cloned().map(|id| Principal { id })
    }
}

/// Authenticated Principal extracted from `Authorization: Bearer <token>`.
#[derive(Debug, Clone)]
pub struct AuthPrincipal(pub Principal);

impl FromRequestParts<AppState> for AuthPrincipal {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| ApiError::unauthorized("missing authorization"))?;

        let token = header
            .strip_prefix("Bearer ")
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .ok_or_else(|| ApiError::unauthorized("invalid authorization scheme"))?;

        let principal = state
            .tokens
            .resolve(token)
            .ok_or_else(|| ApiError::unauthorized("invalid operator token"))?;

        Ok(AuthPrincipal(principal))
    }
}

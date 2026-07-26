use keryx_app::ModelError;
use reqwest::StatusCode;

/// Strip known secret substrings from error messages (fail closed on leakage).
#[must_use]
pub fn redact_secrets(message: &str, secrets: &[String]) -> String {
    let mut out = message.to_string();
    for secret in secrets {
        if secret.is_empty() {
            continue;
        }
        if out.contains(secret) {
            out = out.replace(secret, "[REDACTED]");
        }
    }
    out
}

/// Map HTTP status to a ModelError without embedding response bodies that might contain secrets.
#[must_use]
pub fn map_http_status(provider: &str, status: StatusCode, secrets: &[String]) -> ModelError {
    let raw = if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        format!("{provider}: session expired or rejected (HTTP {status})")
    } else {
        format!("{provider}: upstream HTTP {status}")
    };
    ModelError::new(redact_secrets(&raw, secrets))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_cookie_material() {
        let secrets = vec!["session=abc123xyz".into()];
        let msg = "failed Cookie: session=abc123xyz end";
        assert_eq!(
            redact_secrets(msg, &secrets),
            "failed Cookie: [REDACTED] end"
        );
    }

    #[test]
    fn unauthorized_is_session_message() {
        let err = map_http_status("openai_web", StatusCode::UNAUTHORIZED, &[]);
        assert!(err.to_string().contains("session expired or rejected"));
        assert!(!err.to_string().contains("Bearer"));
    }
}

use serde_json::Value;
use std::collections::HashMap;
use std::env;
use std::path::Path;

/// Operator-supplied consumer web session material (never log).
#[derive(Debug, Clone, Default)]
pub struct ConsumerWebAuth {
    pub cookie_header: Option<String>,
    pub bearer_token: Option<String>,
    pub extra_headers: HashMap<String, String>,
}

impl ConsumerWebAuth {
    #[must_use]
    pub fn is_usable(&self) -> bool {
        self.cookie_header.as_ref().is_some_and(|s| !s.is_empty())
            || self.bearer_token.as_ref().is_some_and(|s| !s.is_empty())
    }

    /// Values that must never appear in errors or logs.
    #[must_use]
    pub fn secret_values(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(c) = &self.cookie_header {
            if !c.is_empty() {
                out.push(c.clone());
            }
        }
        if let Some(t) = &self.bearer_token {
            if !t.is_empty() {
                out.push(t.clone());
            }
        }
        for v in self.extra_headers.values() {
            if !v.is_empty() {
                out.push(v.clone());
            }
        }
        out
    }
}

/// Config for a consumer web Model provider.
#[derive(Debug, Clone)]
pub struct ConsumerWebConfig {
    pub provider_name: String,
    pub base_url: String,
    pub path: String,
    pub model: String,
    pub auth: ConsumerWebAuth,
    pub user_agent: String,
    /// When non-empty, only these model ids are accepted (per-run override included).
    pub allowed_models: Vec<String>,
}

impl ConsumerWebConfig {
    /// Resolve the model id for a request: override → config default, with optional allowlist.
    pub fn resolve_model(&self, override_model: Option<&str>) -> Result<String, String> {
        let model = override_model
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(self.model.as_str())
            .to_string();
        if !self.allowed_models.is_empty() && !self.allowed_models.iter().any(|m| m == &model) {
            return Err(format!(
                "{}: model '{model}' not in allowlist {:?}",
                self.provider_name, self.allowed_models
            ));
        }
        Ok(model)
    }
}

impl ConsumerWebConfig {
    #[must_use]
    pub fn chat_url(&self) -> String {
        let base = self.base_url.trim_end_matches('/');
        let path = if self.path.starts_with('/') {
            self.path.clone()
        } else {
            format!("/{}", self.path)
        };
        format!("{base}{path}")
    }
}

/// Read a secret from `ENV` or `ENV_FILE` (trim; empty → None).
pub fn load_secret(env_key: &str, file_key: &str) -> Result<Option<String>, String> {
    if let Ok(v) = env::var(env_key) {
        let t = v.trim().to_string();
        if !t.is_empty() {
            return Ok(Some(t));
        }
    }
    if let Ok(path) = env::var(file_key) {
        let contents = std::fs::read_to_string(Path::new(&path))
            .map_err(|e| format!("failed to read {file_key}={path}: {e}"))?;
        let trimmed = contents.trim().to_string();
        if trimmed.is_empty() {
            return Ok(None);
        }
        return Ok(Some(trimmed));
    }
    Ok(None)
}

/// Convenience: try env then file for a logical secret pair.
pub fn load_secret_pair(env_key: &str) -> Result<Option<String>, String> {
    let file_key = format!("{env_key}_FILE");
    load_secret(env_key, &file_key)
}

/// Load optional JSON object of extra headers from a path env var.
pub fn read_headers_file(env_key: &str) -> Result<HashMap<String, String>, String> {
    let Ok(path) = env::var(env_key) else {
        return Ok(HashMap::new());
    };
    let contents = std::fs::read_to_string(Path::new(&path))
        .map_err(|e| format!("failed to read {env_key}={path}: {e}"))?;
    let value: Value = serde_json::from_str(&contents)
        .map_err(|e| format!("invalid JSON in {env_key}={path}: {e}"))?;
    let obj = value
        .as_object()
        .ok_or_else(|| format!("{env_key} must be a JSON object of string headers"))?;
    let mut map = HashMap::new();
    for (k, v) in obj {
        let s = v
            .as_str()
            .ok_or_else(|| format!("header {k} in {env_key} must be a string"))?;
        if !s.is_empty() {
            map.insert(k.clone(), s.to_string());
        }
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn usable_requires_cookie_or_token() {
        assert!(!ConsumerWebAuth::default().is_usable());
        assert!(ConsumerWebAuth {
            cookie_header: Some("a=b".into()),
            ..Default::default()
        }
        .is_usable());
        assert!(ConsumerWebAuth {
            bearer_token: Some("tok".into()),
            ..Default::default()
        }
        .is_usable());
    }

    #[test]
    fn load_secret_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secret");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "  super-secret  ").unwrap();
        // SAFETY: test-only env mutation in single-threaded unit test.
        std::env::set_var("KERYX_TEST_SECRET_FILE", path.to_str().unwrap());
        std::env::remove_var("KERYX_TEST_SECRET");
        let v = load_secret("KERYX_TEST_SECRET", "KERYX_TEST_SECRET_FILE").unwrap();
        assert_eq!(v.as_deref(), Some("super-secret"));
        std::env::remove_var("KERYX_TEST_SECRET_FILE");
    }
}

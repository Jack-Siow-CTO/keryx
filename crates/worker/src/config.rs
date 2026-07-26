use keryx_app::{RunBudgets, RunLimits};
use keryx_model::{ConsumerWebAuth, ConsumerWebConfig};
use std::collections::HashMap;
use std::env;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

/// Worker configuration loaded from environment (secrets never committed).
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub bind: SocketAddr,
    pub data_dir: PathBuf,
    /// token -> principal_id
    pub operator_tokens: Vec<(String, String)>,
    pub global_active_cap: usize,
    pub budgets: RunBudgets,
    pub workspace_roots: Vec<PathBuf>,
    pub allowed_tools: Vec<String>,
    pub openai: Option<ProviderConfig>,
    pub grok: Option<ProviderConfig>,
    pub openai_web: Option<ConsumerWebConfig>,
    pub grok_web: Option<ConsumerWebConfig>,
    pub default_provider: String,
}

#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub api_key: String,
    pub model: String,
    pub base_url: Option<String>,
}

impl WorkerConfig {
    /// Load from environment variables and optional secret files.
    ///
    /// | Variable | Purpose |
    /// |----------|---------|
    /// | `KERYX_BIND` | Loopback socket (`127.0.0.1:8787` default) |
    /// | `KERYX_DATA_DIR` | SQLite data directory |
    /// | `KERYX_OPERATOR_TOKEN` | Bearer token (or `KERYX_OPERATOR_TOKEN_FILE`) |
    /// | `KERYX_OPERATOR_PRINCIPAL` | Principal id (default `operator`) |
    /// | `KERYX_GLOBAL_ACTIVE_CAP` | Concurrent Active Runs across Sessions |
    /// | `KERYX_WORKSPACE_ROOTS` | Colon-separated allowlisted roots |
    /// | `OPENAI_API_KEY` / `OPENAI_API_KEY_FILE` | OpenAI credentials |
    /// | `OPENAI_MODEL` | OpenAI model id |
    /// | `OPENAI_BASE_URL` | Optional OpenAI base URL |
    /// | `XAI_API_KEY` / `XAI_API_KEY_FILE` | Grok credentials |
    /// | `XAI_MODEL` | Grok model id |
    /// | `XAI_BASE_URL` | Optional xAI base URL |
    /// | `CHATGPT_WEB_*` | Consumer ChatGPT web session (`openai_web`) — see `docs/deploy/consumer-web-sessions.md` |
    /// | `GROK_WEB_*` | Consumer Grok web session (`grok_web`) |
    /// | `KERYX_DEFAULT_PROVIDER` | `fake` \| `openai` \| `grok` \| `openai_web` \| `grok_web` |
    pub fn from_env() -> Result<Self, String> {
        let bind = parse_bind(env::var("KERYX_BIND").ok())?;
        let data_dir = env::var("KERYX_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./data"));

        let token = read_secret("KERYX_OPERATOR_TOKEN", "KERYX_OPERATOR_TOKEN_FILE")?;
        let principal = env::var("KERYX_OPERATOR_PRINCIPAL").unwrap_or_else(|_| "operator".into());
        let operator_tokens = match token {
            Some(t) if !t.is_empty() => vec![(t, principal)],
            _ => {
                return Err("KERYX_OPERATOR_TOKEN or KERYX_OPERATOR_TOKEN_FILE is required".into())
            }
        };

        let global_active_cap = env::var("KERYX_GLOBAL_ACTIVE_CAP")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(2);

        let max_duration_secs = env::var("KERYX_BUDGET_MAX_DURATION_SECS")
            .ok()
            .and_then(|s| s.parse().ok());
        let max_tokens = env::var("KERYX_BUDGET_MAX_TOKENS")
            .ok()
            .and_then(|s| s.parse().ok());
        let max_tool_calls = env::var("KERYX_BUDGET_MAX_TOOL_CALLS")
            .ok()
            .and_then(|s| s.parse().ok());

        let workspace_roots = env::var("KERYX_WORKSPACE_ROOTS")
            .ok()
            .map(|s| {
                s.split(':')
                    .filter(|p| !p.is_empty())
                    .map(PathBuf::from)
                    .collect()
            })
            .unwrap_or_default();

        let allowed_tools = env::var("KERYX_ALLOWED_TOOLS")
            .ok()
            .map(|s| {
                s.split(',')
                    .map(str::trim)
                    .filter(|t| !t.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_else(|| vec!["read_file".into(), "write_file".into()]);

        let openai =
            read_secret("OPENAI_API_KEY", "OPENAI_API_KEY_FILE")?.map(|api_key| ProviderConfig {
                api_key,
                model: env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".into()),
                base_url: env::var("OPENAI_BASE_URL").ok(),
            });

        let grok = read_secret("XAI_API_KEY", "XAI_API_KEY_FILE")?.map(|api_key| ProviderConfig {
            api_key,
            model: env::var("XAI_MODEL").unwrap_or_else(|_| "grok-3".into()),
            base_url: env::var("XAI_BASE_URL").ok(),
        });

        let openai_web = load_openai_web_config()?;
        let grok_web = load_grok_web_config()?;

        let default_provider = env::var("KERYX_DEFAULT_PROVIDER").unwrap_or_else(|_| "fake".into());

        Ok(Self {
            bind,
            data_dir,
            operator_tokens,
            global_active_cap,
            budgets: RunBudgets {
                max_duration: max_duration_secs.map(Duration::from_secs),
                max_tokens,
                max_tool_calls,
            },
            workspace_roots,
            allowed_tools,
            openai,
            grok,
            openai_web,
            grok_web,
            default_provider,
        })
    }

    #[must_use]
    pub fn run_limits(&self) -> RunLimits {
        RunLimits::default()
            .with_global_cap(self.global_active_cap)
            .with_budgets(self.budgets.clone())
    }
}

fn parse_bind(raw: Option<String>) -> Result<SocketAddr, String> {
    let raw = raw.unwrap_or_else(|| "127.0.0.1:8787".into());
    let addr: SocketAddr = raw
        .parse()
        .map_err(|e| format!("invalid KERYX_BIND '{raw}': {e}"))?;
    // Fail closed: only loopback by default product path.
    if !addr.ip().is_loopback() {
        return Err(format!(
            "KERYX_BIND must be a loopback address (got {}); Tailnet edge terminates TLS and proxies to loopback",
            addr.ip()
        ));
    }
    let _default = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8787);
    Ok(addr)
}

fn read_secret(env_key: &str, file_key: &str) -> Result<Option<String>, String> {
    if let Ok(v) = env::var(env_key) {
        if !v.is_empty() {
            return Ok(Some(v));
        }
    }
    if let Ok(path) = env::var(file_key) {
        let contents = std::fs::read_to_string(&path)
            .map_err(|e| format!("failed to read {file_key}={path}: {e}"))?;
        let trimmed = contents.trim().to_string();
        if trimmed.is_empty() {
            return Ok(None);
        }
        return Ok(Some(trimmed));
    }
    Ok(None)
}

fn read_headers_file(env_key: &str) -> Result<HashMap<String, String>, String> {
    keryx_model::read_headers_file(env_key)
}

fn load_openai_web_config() -> Result<Option<ConsumerWebConfig>, String> {
    let token = read_secret("CHATGPT_WEB_ACCESS_TOKEN", "CHATGPT_WEB_ACCESS_TOKEN_FILE")?;
    let cookie = read_secret("CHATGPT_WEB_COOKIE", "CHATGPT_WEB_COOKIE_FILE")?;
    let auth = ConsumerWebAuth {
        cookie_header: cookie,
        bearer_token: token,
        extra_headers: read_headers_file("CHATGPT_WEB_HEADERS_FILE")?,
    };
    if !auth.is_usable() {
        return Ok(None);
    }
    Ok(Some(ConsumerWebConfig {
        provider_name: "openai_web".into(),
        base_url: env::var("CHATGPT_WEB_BASE_URL").unwrap_or_else(|_| "https://chatgpt.com".into()),
        path: env::var("CHATGPT_WEB_PATH").unwrap_or_else(|_| "/backend-api/conversation".into()),
        model: env::var("CHATGPT_WEB_MODEL").unwrap_or_else(|_| "auto".into()),
        auth,
        user_agent: env::var("CHATGPT_WEB_USER_AGENT").unwrap_or_else(|_| {
            "Mozilla/5.0 (compatible; KeryxWorker/0.1; +https://github.com/Jack-Siow-CTO/keryx)"
                .into()
        }),
    }))
}

fn load_grok_web_config() -> Result<Option<ConsumerWebConfig>, String> {
    let cookie = read_secret("GROK_WEB_COOKIE", "GROK_WEB_COOKIE_FILE")?;
    let auth = ConsumerWebAuth {
        cookie_header: cookie,
        bearer_token: None,
        extra_headers: read_headers_file("GROK_WEB_HEADERS_FILE")?,
    };
    let cookie_missing = match auth.cookie_header.as_ref() {
        None => true,
        Some(s) => s.is_empty(),
    };
    if cookie_missing {
        return Ok(None);
    }
    Ok(Some(ConsumerWebConfig {
        provider_name: "grok_web".into(),
        base_url: env::var("GROK_WEB_BASE_URL").unwrap_or_else(|_| "https://grok.com".into()),
        path: env::var("GROK_WEB_PATH")
            .unwrap_or_else(|_| "/rest/app-chat/conversations/new".into()),
        model: env::var("GROK_WEB_MODEL").unwrap_or_else(|_| "grok".into()),
        auth,
        user_agent: env::var("GROK_WEB_USER_AGENT").unwrap_or_else(|_| {
            "Mozilla/5.0 (compatible; KeryxWorker/0.1; +https://github.com/Jack-Siow-CTO/keryx)"
                .into()
        }),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_loopback_bind() {
        let err = parse_bind(Some("0.0.0.0:8787".into())).unwrap_err();
        assert!(err.contains("loopback"), "{err}");
    }

    #[test]
    fn accepts_loopback_bind() {
        let addr = parse_bind(Some("127.0.0.1:9999".into())).unwrap();
        assert!(addr.ip().is_loopback());
        assert_eq!(addr.port(), 9999);
    }
}

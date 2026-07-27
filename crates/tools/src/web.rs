//! Pluggable web_search / web_extract tools with SSRF fail-closed defaults.

use async_trait::async_trait;
use keryx_app::{ToolCall, ToolError, ToolResult, ToolRuntime, ToolSpec};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

/// Max characters returned from web_extract content (body truncated beyond this).
pub const WEB_EXTRACT_MAX_CHARS: usize = 32_768;
/// Max HTTP body bytes read for extract.
pub const WEB_EXTRACT_MAX_BYTES: usize = 512_000;
/// Default max search hits.
pub const WEB_SEARCH_DEFAULT_MAX: usize = 5;

/// One web search hit for tool results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// Port for pluggable web search backends.
#[async_trait]
pub trait WebSearchBackend: Send + Sync {
    async fn search(&self, query: &str, max_results: usize) -> Result<Vec<SearchHit>, ToolError>;
}

/// Port for pluggable web extract backends.
#[async_trait]
pub trait WebExtractBackend: Send + Sync {
    async fn extract(&self, url: &str) -> Result<String, ToolError>;
}

/// Fixed search results for Seam 1 (no live network).
#[derive(Debug, Default)]
pub struct FixedWebSearch {
    pub hits: Vec<SearchHit>,
}

#[async_trait]
impl WebSearchBackend for FixedWebSearch {
    async fn search(&self, query: &str, max_results: usize) -> Result<Vec<SearchHit>, ToolError> {
        if query.trim().is_empty() {
            return Err(ToolError::Failed(
                "web_search: query must not be empty".into(),
            ));
        }
        let mut out = self.hits.clone();
        out.truncate(max_results.max(1));
        Ok(out)
    }
}

/// Map URL → body for Seam 1 extract doubles (no live network).
#[derive(Debug, Default)]
pub struct FixedWebExtract {
    pub pages: HashMap<String, String>,
    /// When true, still run SSRF URL checks before returning fixtures.
    pub enforce_ssrf: bool,
}

#[async_trait]
impl WebExtractBackend for FixedWebExtract {
    async fn extract(&self, url: &str) -> Result<String, ToolError> {
        if self.enforce_ssrf {
            validate_public_http_url(url)?;
        }
        self.pages
            .get(url)
            .cloned()
            .ok_or_else(|| ToolError::Failed(format!("web_extract: no fixture for {url}")))
    }
}

/// Backend that fails closed when no provider credentials/config are present.
#[derive(Debug, Default)]
pub struct UnconfiguredWebSearch;

#[async_trait]
impl WebSearchBackend for UnconfiguredWebSearch {
    async fn search(&self, _query: &str, _max_results: usize) -> Result<Vec<SearchHit>, ToolError> {
        Err(ToolError::Failed(
            "web_search: no provider configured (set KERYX_WEB_SEARCH_PROVIDER or inject a backend)"
                .into(),
        ))
    }
}

/// Policy-gated web tools.
pub struct WebTools {
    allowed: HashSet<String>,
    search: Arc<dyn WebSearchBackend>,
    extract: Arc<dyn WebExtractBackend>,
}

impl WebTools {
    #[must_use]
    pub fn new(
        allowed: HashSet<String>,
        search: Arc<dyn WebSearchBackend>,
        extract: Arc<dyn WebExtractBackend>,
    ) -> Self {
        Self {
            allowed,
            search,
            extract,
        }
    }
}

#[async_trait]
impl ToolRuntime for WebTools {
    async fn invoke(&self, call: ToolCall) -> Result<ToolResult, ToolError> {
        if !self.allowed.contains(&call.name) {
            return Err(ToolError::Denied(format!(
                "unknown or disallowed tool '{}'",
                call.name
            )));
        }
        match call.name.as_str() {
            "web_search" => self.web_search(&call.arguments).await,
            "web_extract" => self.web_extract(&call.arguments).await,
            other => Err(ToolError::Denied(format!(
                "unknown or disallowed tool '{other}'"
            ))),
        }
    }

    fn catalog(&self) -> Vec<ToolSpec> {
        let mut out = Vec::new();
        if self.allowed.contains("web_search") {
            out.push(ToolSpec::empty_params(
                "web_search",
                "Search the public web",
            ));
        }
        if self.allowed.contains("web_extract") {
            out.push(ToolSpec::empty_params(
                "web_extract",
                "Fetch and extract text from a public HTTP(S) URL",
            ));
        }
        out
    }
}

impl WebTools {
    async fn web_search(&self, args: &Value) -> Result<ToolResult, ToolError> {
        let query = arg_string(args, "query")?;
        let max_results = args
            .get("max_results")
            .and_then(Value::as_u64)
            .unwrap_or(WEB_SEARCH_DEFAULT_MAX as u64)
            .clamp(1, 20) as usize;

        let hits = self.search.search(&query, max_results).await?;
        let mut lines = Vec::new();
        for (i, hit) in hits.iter().enumerate() {
            lines.push(format!(
                "{}. {} — {}\n   {}",
                i + 1,
                truncate(&hit.title, 120),
                truncate(&hit.url, 200),
                truncate(&hit.snippet, 240)
            ));
        }
        let content = if lines.is_empty() {
            format!("no results for {query:?}")
        } else {
            lines.join("\n")
        };
        let summary = format!(
            "web_search query={} hits={}",
            truncate(&query, 40),
            hits.len()
        );
        Ok(ToolResult { content, summary })
    }

    async fn web_extract(&self, args: &Value) -> Result<ToolResult, ToolError> {
        let url = arg_string(args, "url")?;
        // Fail closed before provider: scheme + host + private IP policy.
        validate_public_http_url(&url)?;

        let body = self.extract.extract(&url).await?;
        let truncated = if body.chars().count() > WEB_EXTRACT_MAX_CHARS {
            let t: String = body.chars().take(WEB_EXTRACT_MAX_CHARS).collect();
            format!("{t}\n…[truncated]")
        } else {
            body
        };
        let summary = format!(
            "web_extract url={} chars={}",
            truncate(&redact_url_for_event(&url), 80),
            truncated.chars().count()
        );
        Ok(ToolResult {
            content: truncated,
            summary,
        })
    }
}

/// Validate URL is http(s) to a non-private destination (SSRF fail-closed).
///
/// Blocks: non-http schemes, missing host, localhost names, private/link-local/
/// metadata IP literals. Hostname resolution for extract backends should also
/// re-check resolved addresses (see [`assert_resolved_public`]).
pub fn validate_public_http_url(url: &str) -> Result<(), ToolError> {
    let url = url.trim();
    if url.is_empty() {
        return Err(ToolError::Failed("web_extract: empty url".into()));
    }
    if url.len() > 2048 {
        return Err(ToolError::Failed("web_extract: url too long".into()));
    }

    let parsed = url::Url::parse(url)
        .map_err(|e| ToolError::Failed(format!("web_extract: invalid url: {e}")))?;

    match parsed.scheme() {
        "http" | "https" => {}
        other => {
            return Err(ToolError::Denied(format!(
                "web_extract: scheme '{other}' denied (http/https only)"
            )));
        }
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| ToolError::Denied("web_extract: url missing host".into()))?;

    let host_l = host.to_ascii_lowercase();
    if host_l == "localhost"
        || host_l.ends_with(".localhost")
        || host_l.ends_with(".local")
        || host_l == "metadata.google.internal"
    {
        return Err(ToolError::Denied(format!(
            "web_extract: host '{host}' denied (private/local)"
        )));
    }

    // IP literal host
    if let Ok(ip) = host.parse::<IpAddr>() {
        if !is_public_ip(ip) {
            return Err(ToolError::Denied(format!(
                "web_extract: address {ip} denied (private/link-local/metadata)"
            )));
        }
    }

    // Bracketed IPv6 in URL host_str is without brackets usually; Url::host handles it.
    if let Some(url::Host::Ipv4(v4)) = parsed.host() {
        if !is_public_ip(IpAddr::V4(v4)) {
            return Err(ToolError::Denied(format!(
                "web_extract: address {v4} denied (private/link-local/metadata)"
            )));
        }
    }
    if let Some(url::Host::Ipv6(v6)) = parsed.host() {
        if !is_public_ip(IpAddr::V6(v6)) {
            return Err(ToolError::Denied(format!(
                "web_extract: address {v6} denied (private/link-local/metadata)"
            )));
        }
    }

    Ok(())
}

/// True when IP is allowed for outbound web_extract (public unicast only).
#[must_use]
pub fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                || o[0] == 0
                // CGNAT 100.64/10
                || (o[0] == 100 && (o[1] & 0xc0) == 64)
                // Benchmarking 198.18.0.0/15
                || (o[0] == 198 && (o[1] == 18 || o[1] == 19))
                // Reserved 240.0.0.0/4
                || (o[0] & 0xf0) == 0xf0
                || v4.is_multicast())
        }
        IpAddr::V6(v6) => {
            // IPv4-mapped :ffff:a.b.c.d
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public_ip(IpAddr::V4(v4));
            }
            // IPv4-compatible ::a.b.c.d (deprecated but still appear)
            let seg = v6.segments();
            if seg[0] == 0
                && seg[1] == 0
                && seg[2] == 0
                && seg[3] == 0
                && seg[4] == 0
                && seg[5] == 0
                && !(seg[6] == 0 && seg[7] == 1)
            {
                // ::ffff:x is mapped (handled above); remaining ::x.x.x.x as compatible
                if let Some(v4) = v6.to_ipv4() {
                    return is_public_ip(IpAddr::V4(v4));
                }
            }
            // Documentation 2001:db8::/32
            if seg[0] == 0x2001 && seg[1] == 0x0db8 {
                return false;
            }
            // Manual ULA / link-local checks (avoid Ipv6Addr methods that need MSRV 1.84+).
            let s0 = v6.segments()[0];
            let unique_local = (s0 & 0xfe00) == 0xfc00; // fc00::/7
            let unicast_link_local = (s0 & 0xffc0) == 0xfe80; // fe80::/10
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || unique_local
                || unicast_link_local)
        }
    }
}

/// Fail closed if any resolved socket address is non-public (DNS rebinding defense).
pub fn assert_resolved_public(addrs: &[SocketAddr]) -> Result<(), ToolError> {
    if addrs.is_empty() {
        return Err(ToolError::Failed(
            "web_extract: host resolved to no addresses".into(),
        ));
    }
    for addr in addrs {
        if !is_public_ip(addr.ip()) {
            return Err(ToolError::Denied(format!(
                "web_extract: resolved address {} denied (private/link-local/metadata)",
                addr.ip()
            )));
        }
    }
    Ok(())
}

/// Max redirect hops for web_extract (each hop re-validated for SSRF).
const WEB_EXTRACT_MAX_REDIRECTS: usize = 3;

/// HTTP extract backend with SSRF checks on URL and resolved addresses.
///
/// Redirects are followed **manually**: each hop runs form validation + DNS +
/// public-IP checks, then connects with a DNS pin to the resolved address
/// (reduces rebinding TOCTOU vs stock redirect following).
#[derive(Debug, Default)]
pub struct HttpWebExtract;

impl HttpWebExtract {
    pub fn new() -> Result<Self, ToolError> {
        Ok(Self)
    }
}

#[async_trait]
impl WebExtractBackend for HttpWebExtract {
    async fn extract(&self, url: &str) -> Result<String, ToolError> {
        let mut current = url.to_string();
        for hop in 0..=WEB_EXTRACT_MAX_REDIRECTS {
            let (status, location, body) = fetch_one_hop(&current).await?;
            if status.is_redirection() {
                if hop == WEB_EXTRACT_MAX_REDIRECTS {
                    return Err(ToolError::Denied("web_extract: too many redirects".into()));
                }
                let next = location.ok_or_else(|| {
                    ToolError::Failed("web_extract: redirect without Location".into())
                })?;
                // Resolve relative Location against current URL.
                let base = url::Url::parse(&current)
                    .map_err(|e| ToolError::Failed(format!("web_extract: bad base url: {e}")))?;
                let joined = base
                    .join(&next)
                    .map_err(|e| ToolError::Failed(format!("web_extract: bad Location: {e}")))?;
                // Fail closed before connecting to the next hop.
                validate_public_http_url(joined.as_str())?;
                current = joined.to_string();
                continue;
            }
            if !status.is_success() {
                return Err(ToolError::Failed(format!("web_extract: HTTP {status}")));
            }
            return Ok(body);
        }
        Err(ToolError::Denied(
            "web_extract: redirect loop exhausted".into(),
        ))
    }
}

/// One non-following GET with form SSRF checks + DNS pin to public addresses only.
async fn fetch_one_hop(
    url: &str,
) -> Result<(reqwest::StatusCode, Option<String>, String), ToolError> {
    validate_public_http_url(url)?;
    let parsed = url::Url::parse(url)
        .map_err(|e| ToolError::Failed(format!("web_extract: invalid url: {e}")))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| ToolError::Denied("web_extract: url missing host".into()))?
        .to_string();
    let port = parsed.port_or_known_default().unwrap_or(80);

    // Resolve (or use literal) and require every answer is public, then pin connect.
    let pin_addr: SocketAddr = if let Ok(ip) = host.parse::<IpAddr>() {
        if !is_public_ip(ip) {
            return Err(ToolError::Denied(format!(
                "web_extract: address {ip} denied (private/link-local/metadata)"
            )));
        }
        SocketAddr::new(ip, port)
    } else {
        let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), port))
            .await
            .map_err(|e| ToolError::Failed(format!("web_extract: DNS lookup failed: {e}")))?
            .collect();
        assert_resolved_public(&addrs)?;
        addrs[0]
    };

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("keryx-web-extract/0.1")
        // Pin hostname to the public address we just validated (rebinding defense).
        .resolve(&host, pin_addr)
        .build()
        .map_err(|e| ToolError::Failed(format!("web_extract client: {e}")))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| ToolError::Failed(format!("web_extract fetch: {e}")))?;

    let status = response.status();
    let location = response
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    let bytes = response
        .bytes()
        .await
        .map_err(|e| ToolError::Failed(format!("web_extract body: {e}")))?;
    let slice = if bytes.len() > WEB_EXTRACT_MAX_BYTES {
        &bytes[..WEB_EXTRACT_MAX_BYTES]
    } else {
        &bytes
    };
    let text = String::from_utf8_lossy(slice).into_owned();
    Ok((status, location, text))
}

/// Route tool names to the first registered runtime that claims them.
pub struct CompositeTools {
    routes: Vec<(HashSet<String>, Arc<dyn ToolRuntime>)>,
}

impl CompositeTools {
    #[must_use]
    pub fn new() -> Self {
        Self { routes: Vec::new() }
    }

    #[must_use]
    pub fn with(mut self, names: HashSet<String>, runtime: Arc<dyn ToolRuntime>) -> Self {
        if !names.is_empty() {
            self.routes.push((names, runtime));
        }
        self
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.routes.is_empty()
    }
}

impl Default for CompositeTools {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolRuntime for CompositeTools {
    async fn invoke(&self, call: ToolCall) -> Result<ToolResult, ToolError> {
        for (names, runtime) in &self.routes {
            if names.contains(&call.name) {
                return runtime.invoke(call).await;
            }
        }
        Err(ToolError::Denied(format!(
            "unknown or disallowed tool '{}'",
            call.name
        )))
    }

    fn catalog(&self) -> Vec<ToolSpec> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        for (names, runtime) in &self.routes {
            for spec in runtime.catalog() {
                if names.contains(&spec.name) && seen.insert(spec.name.clone()) {
                    out.push(spec);
                }
            }
        }
        out
    }
}

fn arg_string(args: &Value, key: &str) -> Result<String, ToolError> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| ToolError::Failed(format!("missing string argument '{key}'")))
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        let t: String = s.chars().take(max).collect();
        format!("{t}…")
    } else {
        s.to_string()
    }
}

fn redact_url_for_event(url: &str) -> String {
    // Strip userinfo (credentials in URL) for events/logs.
    if let Ok(mut parsed) = url::Url::parse(url) {
        let _ = parsed.set_username("");
        let _ = parsed.set_password(None);
        return parsed.to_string();
    }
    truncate(url, 80)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssrf_denies_private_and_metadata() {
        for bad in [
            "http://127.0.0.1/",
            "http://localhost/admin",
            "http://192.168.1.1/",
            "http://10.0.0.5/",
            "http://169.254.169.254/latest/meta-data/",
            "http://[::1]/",
            "file:///etc/passwd",
            "ftp://example.com/",
        ] {
            let err = validate_public_http_url(bad).unwrap_err();
            let s = err.to_string();
            assert!(
                s.contains("denied") || s.contains("invalid") || s.contains("scheme"),
                "expected deny for {bad}: {s}"
            );
        }
    }

    #[test]
    fn ssrf_allows_public_https() {
        validate_public_http_url("https://example.com/path").unwrap();
        validate_public_http_url("http://93.184.216.34/").unwrap(); // example.com-ish public
    }

    #[tokio::test]
    async fn fixed_search_and_extract_round_trip() {
        let search = Arc::new(FixedWebSearch {
            hits: vec![SearchHit {
                title: "Example".into(),
                url: "https://example.com".into(),
                snippet: "Example Domain".into(),
            }],
        });
        let extract = Arc::new(FixedWebExtract {
            pages: HashMap::from([("https://example.com".into(), "Example Domain body".into())]),
            enforce_ssrf: true,
        });
        let tools = WebTools::new(
            HashSet::from(["web_search".into(), "web_extract".into()]),
            search,
            extract,
        );

        let s = tools
            .invoke(ToolCall {
                name: "web_search".into(),
                arguments: serde_json::json!({ "query": "example" }),
            })
            .await
            .unwrap();
        assert!(s.content.contains("example.com"));
        assert!(s.summary.contains("hits=1"));
        assert!(!s.summary.contains("Example Domain body"));

        let e = tools
            .invoke(ToolCall {
                name: "web_extract".into(),
                arguments: serde_json::json!({ "url": "https://example.com" }),
            })
            .await
            .unwrap();
        assert!(e.content.contains("Example Domain body"));
        assert!(e.summary.contains("chars="));
    }

    #[tokio::test]
    async fn extract_denies_private_before_backend() {
        let tools = WebTools::new(
            HashSet::from(["web_extract".into()]),
            Arc::new(UnconfiguredWebSearch),
            Arc::new(FixedWebExtract {
                pages: HashMap::from([("http://127.0.0.1/".into(), "nope".into())]),
                enforce_ssrf: false,
            }),
        );
        let err = tools
            .invoke(ToolCall {
                name: "web_extract".into(),
                arguments: serde_json::json!({ "url": "http://127.0.0.1/" }),
            })
            .await
            .unwrap_err();
        assert!(err.to_string().contains("denied"), "{err}");
    }

    #[test]
    fn redirect_location_to_private_fails_form_check() {
        // Manual redirect hops call validate_public_http_url before connect.
        for loc in [
            "http://127.0.0.1/secret",
            "http://169.254.169.254/latest/meta-data/",
            "http://10.1.2.3/admin",
            "http://192.168.0.1/",
        ] {
            assert!(
                validate_public_http_url(loc).is_err(),
                "redirect target must be denied: {loc}"
            );
        }
    }

    #[test]
    fn ipv4_mapped_and_compatible_loopback_denied() {
        assert!(!is_public_ip("::ffff:127.0.0.1".parse().unwrap()));
        assert!(!is_public_ip("::ffff:10.0.0.1".parse().unwrap()));
        assert!(!is_public_ip("::ffff:169.254.169.254".parse().unwrap()));
    }
}

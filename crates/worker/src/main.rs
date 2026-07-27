//! Keryx worker binary — composition root for domain, app, and adapters (ADR 0008).

mod config;

use clap::{Parser, Subcommand};
use config::WorkerConfig;
use keryx_api::{router, AppState, OperatorTokenTable};
use keryx_app::{ControlPlane, DenyAllTools, SessionStore};
use keryx_model::{
    ChatGptCodexProvider, ChatGptWebProvider, FakeModelProvider, GrokWebProvider,
    MultiModelProvider, OpenAiCompatibleConfig, OpenAiCompatibleProvider,
};
use keryx_storage::SqliteSessionStore;
use keryx_tools::WorkspaceFsTools;
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::{info, warn};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Parser)]
#[command(
    name = "keryx",
    about = "Keryx agent Worker — loopback control plane",
    version = VERSION
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Start the Worker control plane (default when no subcommand is given)
    Serve,
    /// Print version and exit
    Version,
    /// Check config readiness (token, bind, data dir, providers) without serving
    Doctor,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Serve) {
        Command::Version => {
            println!("keryx {VERSION}");
        }
        Command::Doctor => {
            if let Err(err) = doctor() {
                eprintln!("keryx doctor: {err}");
                std::process::exit(1);
            }
        }
        Command::Serve => {
            init_tracing();
            if let Err(err) = run().await {
                eprintln!("keryx worker failed: {err}");
                std::process::exit(1);
            }
        }
    }
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

async fn run() -> Result<(), String> {
    let config = WorkerConfig::from_env()?;
    info!(
        bind = %config.bind,
        data_dir = %config.data_dir.display(),
        default_provider = %config.default_provider,
        "starting keryx worker"
    );

    let store = Arc::new(SqliteSessionStore::open(&config.data_dir)?);
    let model = build_model_provider(&config)?;
    let tools = build_tools(&config);
    let control = Arc::new(ControlPlane::with_tools(
        Arc::clone(&store),
        model,
        config.run_limits(),
        tools,
    ));

    let mut tokens = OperatorTokenTable::new();
    for (token, principal) in &config.operator_tokens {
        tokens = tokens.with_token(token, principal.as_str());
    }
    let state = AppState::new(control, tokens);
    let app = router(state);

    let listener = TcpListener::bind(config.bind)
        .await
        .map_err(|e| format!("bind {}: {e}", config.bind))?;
    let local = listener
        .local_addr()
        .map_err(|e| format!("local_addr: {e}"))?;
    info!(%local, "control plane listening (loopback)");

    let store_for_shutdown = Arc::clone(&store);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|e| format!("server error: {e}"))?;

    // Graceful shutdown: Active Runs → interrupted in durable store (ADR 0006).
    match store_for_shutdown.interrupt_active_runs().await {
        Ok(n) => info!(interrupted = n, "shutdown: marked active runs interrupted"),
        Err(e) => warn!(error = %e, "shutdown: failed to interrupt active runs"),
    }

    info!("keryx worker stopped");
    Ok(())
}

fn doctor() -> Result<(), String> {
    println!("keryx doctor {VERSION}");
    let config = WorkerConfig::from_env()?;

    let mut issues = 0u32;
    let mut warn_count = 0u32;

    // Bind
    if config.bind.ip().is_loopback() {
        println!("ok   bind is loopback ({})", config.bind);
    } else {
        println!("fail bind is not loopback ({})", config.bind);
        issues += 1;
    }

    // Operator tokens (never print secret values)
    let n_tokens = config.operator_tokens.len();
    if n_tokens == 0 {
        println!("fail no operator tokens configured");
        issues += 1;
    } else {
        let principals: Vec<&str> = config
            .operator_tokens
            .iter()
            .map(|(_, p)| p.as_str())
            .collect();
        println!("ok   operator tokens configured: {n_tokens} (principals: {principals:?})");
    }

    // Data directory
    match ensure_data_dir_writable(&config.data_dir) {
        Ok(()) => println!("ok   data dir writable ({})", config.data_dir.display()),
        Err(e) => {
            println!(
                "fail data dir not writable ({}): {e}",
                config.data_dir.display()
            );
            issues += 1;
        }
    }

    // Providers that would register
    let mut available = vec!["fake".to_string()];
    if config.openai.is_some() {
        available.push("openai".into());
    }
    if config.grok.is_some() {
        available.push("grok".into());
    }
    if config.openai_web.is_some() {
        available.push("openai_web".into());
    }
    if std::env::var("CHATGPT_WEB_ACCESS_TOKEN").is_ok()
        || std::env::var("CHATGPT_WEB_ACCESS_TOKEN_FILE").is_ok()
    {
        // openai_codex registers from env when the Codex/ChatGPT access token is present
        available.push("openai_codex".into());
    }
    if config.grok_web.is_some() {
        available.push("grok_web".into());
    }
    println!("ok   providers available: {available:?}");

    if available.iter().any(|p| p == &config.default_provider) {
        println!(
            "ok   default provider '{}' is registered",
            config.default_provider
        );
    } else {
        println!(
            "fail default provider '{}' is not available (have {available:?})",
            config.default_provider
        );
        issues += 1;
    }

    if config.default_provider == "fake" {
        println!(
            "warn default provider is 'fake' — set OPENAI_API_KEY / XAI_API_KEY for real models"
        );
        warn_count += 1;
    }

    // Tools
    if config.workspace_roots.is_empty() {
        println!("warn no KERYX_WORKSPACE_ROOTS — file tools disabled (DenyAll)");
        warn_count += 1;
    } else {
        println!(
            "ok   workspace roots: {:?} allowed_tools={:?}",
            config.workspace_roots, config.allowed_tools
        );
    }

    // Optional live health probe if something is already listening
    let health_url = format!("http://{}/health", config.bind);
    match probe_health(&health_url) {
        Ok(body) => println!("ok   live health at {health_url}: {body}"),
        Err(e) => {
            println!("info no live Worker at {health_url} ({e}) — start with: keryx");
        }
    }

    println!();
    if issues > 0 {
        return Err(format!("{issues} check(s) failed, {warn_count} warning(s)"));
    }
    println!("doctor: all required checks passed ({warn_count} warning(s))");
    println!("next: start with `keryx`, then run scripts/smoke.sh");
    Ok(())
}

fn ensure_data_dir_writable(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let probe = dir.join(".keryx-doctor-write-test");
    std::fs::write(&probe, b"ok").map_err(|e| e.to_string())?;
    std::fs::remove_file(&probe).map_err(|e| e.to_string())?;
    Ok(())
}

fn probe_health(url: &str) -> Result<String, String> {
    // Tiny blocking probe so doctor stays sync and dependency-light.
    // Use std::process curl when available; otherwise skip with a clear message.
    let output = std::process::Command::new("curl")
        .args(["-fsS", "--max-time", "2", url])
        .output()
        .map_err(|e| format!("curl not available: {e}"))?;
    if !output.status.success() {
        return Err("connection failed".into());
    }
    String::from_utf8(output.stdout).map_err(|e| e.to_string())
}

fn build_model_provider(config: &WorkerConfig) -> Result<Arc<MultiModelProvider>, String> {
    let mut providers: HashMap<String, Arc<dyn keryx_app::ModelProvider>> = HashMap::new();
    providers.insert("fake".into(), Arc::new(FakeModelProvider::greeting()));

    if let Some(openai) = &config.openai {
        let mut cfg = OpenAiCompatibleConfig::openai(&openai.api_key, &openai.model);
        if let Some(base) = &openai.base_url {
            cfg = cfg.with_base_url(base);
        }
        let provider = OpenAiCompatibleProvider::new(cfg).map_err(|e| e.to_string())?;
        providers.insert("openai".into(), Arc::new(provider));
    }

    if let Some(grok) = &config.grok {
        let mut cfg = OpenAiCompatibleConfig::grok(&grok.api_key, &grok.model);
        if let Some(base) = &grok.base_url {
            cfg = cfg.with_base_url(base);
        }
        let provider = OpenAiCompatibleProvider::new(cfg).map_err(|e| e.to_string())?;
        providers.insert("grok".into(), Arc::new(provider));
    }

    if let Some(web) = &config.openai_web {
        let provider = ChatGptWebProvider::new(web.clone()).map_err(|e| e.to_string())?;
        providers.insert("openai_web".into(), Arc::new(provider));
        info!("registered model provider openai_web (consumer session)");
    }

    // ChatGPT Plus/Pro subscription via Codex OAuth (not Platform API key).
    match ChatGptCodexProvider::from_env() {
        Ok(Some(provider)) => {
            providers.insert("openai_codex".into(), Arc::new(provider));
            info!("registered model provider openai_codex (ChatGPT subscription)");
        }
        Ok(None) => {}
        Err(e) => return Err(format!("openai_codex config: {e}")),
    }

    if let Some(web) = &config.grok_web {
        let provider = GrokWebProvider::new(web.clone()).map_err(|e| e.to_string())?;
        providers.insert("grok_web".into(), Arc::new(provider));
        info!("registered model provider grok_web (consumer session)");
    }

    if !providers.contains_key(&config.default_provider) {
        return Err(format!(
            "KERYX_DEFAULT_PROVIDER='{}' is not available (configured: {:?})",
            config.default_provider,
            providers.keys().collect::<Vec<_>>()
        ));
    }

    Ok(Arc::new(MultiModelProvider::new(
        config.default_provider.clone(),
        providers,
    )))
}

fn build_tools(config: &WorkerConfig) -> Arc<dyn keryx_app::ToolRuntime> {
    if config.workspace_roots.is_empty() {
        return Arc::new(DenyAllTools);
    }
    let allowed: HashSet<String> = config.allowed_tools.iter().cloned().collect();
    Arc::new(WorkspaceFsTools::new(
        config.workspace_roots.clone(),
        allowed,
    ))
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    info!("shutdown signal received");
}

/// Used by integration smoke tests to bind an ephemeral loopback port.
#[allow(dead_code)]
pub(crate) fn loopback_ephemeral() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 0))
}

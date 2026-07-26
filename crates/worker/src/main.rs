//! Keryx worker binary — composition root for domain, app, and adapters (ADR 0008).

mod config;

use config::WorkerConfig;
use keryx_api::{router, AppState, OperatorTokenTable};
use keryx_app::{ControlPlane, DenyAllTools, SessionStore};
use keryx_model::{
    FakeModelProvider, MultiModelProvider, OpenAiCompatibleConfig, OpenAiCompatibleProvider,
};
use keryx_storage::SqliteSessionStore;
use keryx_tools::WorkspaceFsTools;
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::{info, warn};

#[tokio::main]
async fn main() {
    init_tracing();
    if let Err(err) = run().await {
        eprintln!("keryx worker failed: {err}");
        std::process::exit(1);
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

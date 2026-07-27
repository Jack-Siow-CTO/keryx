//! Keryx worker binary — composition root for domain, app, and adapters (ADR 0008).

mod config;

use clap::{Parser, Subcommand};
use config::WorkerConfig;
use keryx_api::{router, AppState, OperatorTokenTable, ProviderCatalog, ProviderInfo};
use keryx_app::{ControlPlane, DenyAllTools, SessionStore};
use keryx_model::{register_from_env, RegisteredProviders};
use keryx_storage::SqliteSessionStore;
use keryx_tools::WorkspaceFsTools;
use std::collections::HashSet;
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
    let registered = register_from_env()?;
    info!(
        bind = %config.bind,
        data_dir = %config.data_dir.display(),
        default_provider = %registered.default_provider,
        providers = ?registered.provider_names(),
        "starting keryx worker"
    );

    let store = Arc::new(SqliteSessionStore::open(&config.data_dir)?);
    let tools = build_tools(&config);
    let control = Arc::new(ControlPlane::with_tools(
        Arc::clone(&store),
        Arc::clone(&registered.multi),
        config.run_limits(),
        tools,
    ));

    let mut tokens = OperatorTokenTable::new();
    for (token, principal) in &config.operator_tokens {
        tokens = tokens.with_token(token, principal.as_str());
    }
    let catalog = catalog_from_registered(&registered);
    let state = AppState::with_providers(control, tokens, catalog);
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

fn catalog_from_registered(registered: &RegisteredProviders) -> ProviderCatalog {
    ProviderCatalog {
        default: Some(registered.default_provider.clone()),
        providers: registered
            .descriptors
            .iter()
            .map(|d| ProviderInfo {
                name: d.name.clone(),
                auth_kind: d.auth_kind.as_str().to_string(),
                display_name: d.display_name.clone(),
                default_model: d.default_model.clone(),
                models: d.models.clone(),
                registered: d.registered,
                supports_model_override: d.supports_model_override,
            })
            .collect(),
    }
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

    // Real model providers only (registry)
    match register_from_env() {
        Ok(registered) => {
            println!(
                "ok   providers registered: {:?} (default={})",
                registered.provider_names(),
                registered.default_provider
            );
            for d in &registered.descriptors {
                println!(
                    "     - {} [{}] model={} auth={}",
                    d.name,
                    d.display_name,
                    d.default_model,
                    d.auth_kind.as_str()
                );
            }
        }
        Err(e) => {
            println!("fail model providers: {e}");
            issues += 1;
        }
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
    let output = std::process::Command::new("curl")
        .args(["-fsS", "--max-time", "2", url])
        .output()
        .map_err(|e| format!("curl not available: {e}"))?;
    if !output.status.success() {
        return Err("connection failed".into());
    }
    String::from_utf8(output.stdout).map_err(|e| e.to_string())
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

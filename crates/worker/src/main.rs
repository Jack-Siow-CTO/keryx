//! Keryx worker binary — composition root for domain, app, and adapters (ADR 0008).

mod config;

use clap::{Parser, Subcommand};
use config::WorkerConfig;
use keryx_api::{router, AppState, OperatorTokenTable, ProviderCatalog, ProviderInfo};
use keryx_app::{ControlPlane, DenyAllTools, RunContextConfig, SessionStore};
use keryx_domain::{Principal, PrincipalId, RunOrigin};
use keryx_gateway::{run_telegram_long_poll, ChatAllowlist};
use keryx_model::{register_from_env, RegisteredProviders};
use keryx_storage::SqliteSessionStore;
use keryx_tools::{
    build_mcp_runtimes, CompositeTools, HttpWebExtract, McpDoctorReport, McpRuntimeBundle,
    MemoryTools, SystemExecRunner, TerminalTools, UnconfiguredWebSearch, WebTools,
    WorkspaceFsTools,
};
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
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
            if let Err(err) = doctor().await {
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
    let mcp_bundle = load_mcp_bundle(&config).await;
    let tools = build_tools(&config, Arc::clone(&store), mcp_bundle.runtime.clone());
    let run_context = RunContextConfig {
        soul_path: config.soul_path.clone(),
        context_files: config.context_files.clone(),
        workspace_roots: config.workspace_roots.clone(),
        missing: keryx_app::MissingContextPolicy::Soft,
    };
    let mut control = ControlPlane::with_tools_and_context(
        Arc::clone(&store),
        Arc::clone(&registered.multi),
        config.run_limits(),
        tools,
        run_context,
    );
    // Connect ≠ allow: only config policy_allowlist + KERYX_POLICY_EXTRA_TOOLS.
    let mut extras = mcp_bundle.control_plane_extra.clone();
    extras.extend(config.policy_extra_tools.iter().cloned());
    if !extras.is_empty() {
        control = control.with_control_plane_extra_tools(extras);
    }
    if !mcp_bundle.high_blast.is_empty() {
        control = control.with_high_blast_tools(mcp_bundle.high_blast.clone());
    }
    let control = Arc::new(control);
    // Keep MCP registry alive for stdio child lifetime / shutdown Drop.
    let _mcp_keep_alive = mcp_bundle.runtime.clone();

    let mut tokens = OperatorTokenTable::new();
    for (token, principal) in &config.operator_tokens {
        tokens = tokens.with_token(token, principal.as_str());
    }
    let catalog = catalog_from_registered(&registered);
    // Keep a direct Arc for Gateways (HTTP holds its own clone via AppState).
    let control_for_gateway = Arc::clone(&control);
    let skills_root = std::env::var("KERYX_SKILLS_ROOT")
        .ok()
        .map(std::path::PathBuf::from)
        .or_else(|| Some(std::path::PathBuf::from("./skills")));
    let artifacts_dir = Some(config.data_dir.join("artifacts"));
    let state = AppState::with_providers(control, tokens, catalog)
        .with_console_paths(skills_root, artifacts_dir);
    let app = router(state);

    // Telegram Gateway long-poll (optional; fail closed if token invalid at getMe).
    if let Ok(tg_token) = std::env::var("KERYX_TELEGRAM_BOT_TOKEN") {
        if !tg_token.trim().is_empty() {
            let allow =
                ChatAllowlist::from_env_csv(std::env::var("KERYX_TELEGRAM_ALLOWED_CHAT_IDS").ok());
            let principal_name = config
                .operator_tokens
                .first()
                .map(|(_, p)| p.clone())
                .unwrap_or_else(|| "operator".into());
            let principal = Principal {
                id: PrincipalId::new(principal_name),
            };
            let max_wait_secs = std::env::var("KERYX_TELEGRAM_RUN_TIMEOUT_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(180u64);
            tokio::spawn(async move {
                info!("telegram gateway task starting");
                if let Err(e) = run_telegram_long_poll(
                    control_for_gateway,
                    tg_token,
                    principal,
                    allow,
                    Duration::from_secs(max_wait_secs),
                )
                .await
                {
                    warn!(error = %e, "telegram gateway stopped");
                }
            });
        }
    }

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

async fn doctor() -> Result<(), String> {
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

    // Soul + Context files (soft-missing; distinct from Memory/Skill)
    match &config.soul_path {
        Some(p) if p.is_file() => println!("ok   Soul path {}", p.display()),
        Some(p) => {
            println!(
                "warn Soul path configured but missing ({}) — Runs continue without Soul",
                p.display()
            );
            warn_count += 1;
        }
        None => println!("info no KERYX_SOUL_PATH — Runs start without Soul"),
    }
    if config.context_files.is_empty() {
        println!("info no KERYX_CONTEXT_FILES — no workspace Context auto-attach");
    } else {
        println!("ok   Context files configured: {:?}", config.context_files);
    }

    // v2 readiness surfaces
    let skills = std::env::var("KERYX_SKILLS_ROOT").unwrap_or_else(|_| "./skills".into());
    if std::path::Path::new(&skills).is_dir() {
        println!("ok   skills root {skills}");
    } else {
        println!("warn skills root missing ({skills}) — create or set KERYX_SKILLS_ROOT");
        warn_count += 1;
    }
    match std::env::var("KERYX_TELEGRAM_BOT_TOKEN") {
        Ok(t) if !t.is_empty() => println!("ok   Telegram Gateway token configured"),
        _ => println!("info Telegram Gateway disabled (no KERYX_TELEGRAM_BOT_TOKEN)"),
    }
    // Discord live gateway is not wired yet; ignore KERYX_DISCORD_BOT_TOKEN if set.
    match std::process::Command::new("docker")
        .args(["info"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
    {
        Ok(s) if s.success() => println!("ok   Docker available for exec backend"),
        _ => {
            println!("warn Docker not available — reduced-origin local exec stays denied");
            warn_count += 1;
        }
    }
    if std::env::var("KERYX_BIND")
        .ok()
        .and_then(|b| b.parse::<std::net::SocketAddr>().ok())
        .is_some_and(|a| !a.ip().is_loopback())
    {
        println!("fail public/non-loopback bind is dangerous misconfig");
        issues += 1;
    }
    // MCP client servers (static config; never print secret values)
    match &config.mcp_config {
        Some(cfg) => {
            if let Some(path) = &config.mcp_config_path {
                println!(
                    "ok   MCP config path {} ({} server(s))",
                    path.display(),
                    cfg.servers.len()
                );
            }
            // Doctor connect attempt (fail closed per server; Worker still healthy).
            let bundle = build_mcp_runtimes(cfg).await;
            if !config.policy_extra_tools.is_empty() {
                println!(
                    "ok   KERYX_POLICY_EXTRA_TOOLS: {:?}",
                    config.policy_extra_tools
                );
            }
            bundle.doctor.print_lines();
            // Drop registry so stdio children exit after doctor.
            drop(bundle);
        }
        None => {
            McpDoctorReport::default().print_lines();
        }
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

async fn load_mcp_bundle(config: &WorkerConfig) -> McpRuntimeBundle {
    match &config.mcp_config {
        Some(cfg) => {
            let bundle = build_mcp_runtimes(cfg).await;
            for s in &bundle.doctor.servers {
                if s.connected {
                    info!(
                        server_id = %s.server_id,
                        tools = s.discovered_tools.len(),
                        "MCP server connected"
                    );
                } else {
                    warn!(
                        server_id = %s.server_id,
                        error = s.error.as_deref().unwrap_or("unknown"),
                        "MCP server failed (contributes zero tools)"
                    );
                }
            }
            bundle
        }
        None => McpRuntimeBundle {
            runtime: None,
            control_plane_extra: Vec::new(),
            high_blast: Vec::new(),
            doctor: McpDoctorReport::default(),
        },
    }
}

fn build_tools(
    config: &WorkerConfig,
    store: Arc<SqliteSessionStore>,
    mcp: Option<Arc<keryx_tools::McpClientRegistry>>,
) -> Arc<dyn keryx_app::ToolRuntime> {
    let allowed: HashSet<String> = config.allowed_tools.iter().cloned().collect();
    let mut composite = CompositeTools::new();

    let fs_names: HashSet<String> = ["read_file", "write_file", "apply_patch", "search_files"]
        .into_iter()
        .map(str::to_string)
        .filter(|n| allowed.contains(n))
        .collect();
    if !fs_names.is_empty() && !config.workspace_roots.is_empty() {
        composite = composite.with(
            fs_names,
            Arc::new(WorkspaceFsTools::new(
                config.workspace_roots.clone(),
                allowed.clone(),
            )),
        );
    }

    let web_names: HashSet<String> = ["web_search", "web_extract"]
        .into_iter()
        .map(str::to_string)
        .filter(|n| allowed.contains(n))
        .collect();
    if !web_names.is_empty() {
        let search: Arc<dyn keryx_tools::WebSearchBackend> = Arc::new(UnconfiguredWebSearch);
        let extract: Arc<dyn keryx_tools::WebExtractBackend> = match HttpWebExtract::new() {
            Ok(http) => Arc::new(http),
            Err(_) => Arc::new(keryx_tools::FixedWebExtract::default()),
        };
        composite = composite.with(
            web_names,
            Arc::new(WebTools::new(allowed.clone(), search, extract)),
        );
    }

    let mem_names: HashSet<String> = [
        "memory_read",
        "memory_write",
        "memory_update",
        "memory_delete",
        "memory_search",
        "session_search",
    ]
    .into_iter()
    .map(str::to_string)
    .filter(|n| allowed.contains(n))
    .collect();
    if !mem_names.is_empty() {
        composite = composite.with(
            mem_names.clone(),
            Arc::new(MemoryTools::new(Arc::clone(&store), allowed.clone())),
        );
    }

    let term_names: HashSet<String> = ["run_terminal", "shell_exec"]
        .into_iter()
        .map(str::to_string)
        .filter(|n| allowed.contains(n))
        .collect();
    if !term_names.is_empty() {
        // Worker default origin for static tool wiring is control_plane; per-run
        // backend selection still honors Run origin inside TerminalTools when
        // constructed with origin (here control_plane; reduced origins deny local).
        let mut term = TerminalTools::new(
            allowed,
            Arc::new(SystemExecRunner::new()),
            RunOrigin::ControlPlane,
        );
        if !config.workspace_roots.is_empty() {
            term = term.with_cwd_roots(config.workspace_roots.clone());
        }
        composite = composite.with(term_names, Arc::new(term));
    }

    // MCP client tools: registered independently of KERYX_ALLOWED_TOOLS.
    // Policy still gates invoke (connect ≠ allow).
    if let Some(registry) = mcp {
        let names = registry.registered_names();
        if !names.is_empty() {
            info!(count = names.len(), "registering MCP client tools");
            composite = composite.with(names, registry);
        }
    }

    if composite.is_empty() {
        return Arc::new(DenyAllTools);
    }
    Arc::new(composite)
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

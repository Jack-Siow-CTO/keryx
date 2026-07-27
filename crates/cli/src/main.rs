//! Keryx CLI — control-plane client only (no agent loop).

use clap::{Parser, Subcommand};
use serde_json::Value;
use std::process::ExitCode;

#[derive(Parser, Debug)]
#[command(name = "keryx-cli", about = "Keryx control-plane CLI client")]
struct Cli {
    /// Control plane base URL (loopback Worker).
    #[arg(long, env = "KERYX_URL", default_value = "http://127.0.0.1:8787")]
    url: String,
    /// Operator bearer token.
    #[arg(long, env = "KERYX_OPERATOR_TOKEN")]
    token: Option<String>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Create a Session.
    SessionCreate,
    /// List is not a full REST list in v1 — create is primary.
    SessionShow { id: String },
    /// Start a Run.
    RunStart {
        session_id: String,
        goal: String,
        #[arg(long)]
        provider: Option<String>,
        #[arg(long)]
        model: Option<String>,
    },
    /// Get Run status.
    RunShow { run_id: String },
    /// Cancel a Run.
    RunCancel { run_id: String },
    /// Follow Run SSE events until terminal (or max lines).
    RunEvents {
        run_id: String,
        #[arg(long, default_value_t = 200)]
        max_events: usize,
    },
    /// List pending Approvals.
    ApprovalsList,
    /// Approve an Approval.
    Approve { approval_id: String },
    /// Deny an Approval.
    Deny { approval_id: String },
    /// List model providers catalog.
    Providers,
    /// Health check (no auth).
    Health,
    /// List schedules.
    SchedulesList,
    /// Basic doctor against a live control plane.
    Doctor,
    /// Line-oriented TUI: slash commands for session/run/approve (control-plane only).
    Tui,
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("keryx-cli error: {e}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<(), String> {
    let client = reqwest::Client::new();
    let base = cli.url.trim_end_matches('/');
    let token = cli.token.as_deref();

    match cli.command {
        Commands::Health => {
            let v: Value = client
                .get(format!("{base}/health"))
                .send()
                .await
                .map_err(|e| e.to_string())?
                .error_for_status()
                .map_err(|e| e.to_string())?
                .json()
                .await
                .map_err(|e| e.to_string())?;
            println!("{}", serde_json::to_string_pretty(&v).unwrap());
        }
        Commands::SessionCreate => {
            let v = authed_json(&client, base, token, "POST", "/v1/sessions", None).await?;
            println!("{}", serde_json::to_string_pretty(&v).unwrap());
        }
        Commands::SessionShow { id } => {
            // No dedicated GET session in thin API — report id for scripting.
            println!("{{\"id\":\"{id}\"}}");
        }
        Commands::RunStart {
            session_id,
            goal,
            provider,
            model,
        } => {
            let mut body = serde_json::json!({ "goal": goal });
            if let Some(p) = provider {
                body["provider"] = Value::String(p);
            }
            if let Some(m) = model {
                body["model"] = Value::String(m);
            }
            let v = authed_json(
                &client,
                base,
                token,
                "POST",
                &format!("/v1/sessions/{session_id}/runs"),
                Some(body),
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&v).unwrap());
        }
        Commands::RunShow { run_id } => {
            let v = authed_json(
                &client,
                base,
                token,
                "GET",
                &format!("/v1/runs/{run_id}"),
                None,
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&v).unwrap());
        }
        Commands::RunCancel { run_id } => {
            let v = authed_json(
                &client,
                base,
                token,
                "POST",
                &format!("/v1/runs/{run_id}/cancel"),
                None,
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&v).unwrap());
        }
        Commands::RunEvents { run_id, max_events } => {
            let tok = token.ok_or("KERYX_OPERATOR_TOKEN required")?;
            let resp = client
                .get(format!("{base}/v1/runs/{run_id}/events"))
                .header("authorization", format!("Bearer {tok}"))
                .header("accept", "text/event-stream")
                .send()
                .await
                .map_err(|e| e.to_string())?
                .error_for_status()
                .map_err(|e| e.to_string())?;
            let text = resp.text().await.map_err(|e| e.to_string())?;
            let mut n = 0usize;
            for line in text.lines() {
                println!("{line}");
                if line.starts_with("event:") {
                    n += 1;
                    if n >= max_events {
                        break;
                    }
                }
            }
        }
        Commands::ApprovalsList => {
            let v = authed_json(
                &client,
                base,
                token,
                "GET",
                "/v1/approvals?pending=true",
                None,
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&v).unwrap());
        }
        Commands::Approve { approval_id } => {
            let v = authed_json(
                &client,
                base,
                token,
                "POST",
                &format!("/v1/approvals/{approval_id}/approve"),
                None,
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&v).unwrap());
        }
        Commands::Deny { approval_id } => {
            let v = authed_json(
                &client,
                base,
                token,
                "POST",
                &format!("/v1/approvals/{approval_id}/deny"),
                None,
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&v).unwrap());
        }
        Commands::Providers => {
            let v = authed_json(&client, base, token, "GET", "/v1/providers", None).await?;
            println!("{}", serde_json::to_string_pretty(&v).unwrap());
        }
        Commands::SchedulesList => {
            let v = authed_json(&client, base, token, "GET", "/v1/schedules", None).await?;
            println!("{}", serde_json::to_string_pretty(&v).unwrap());
        }
        Commands::Doctor => {
            println!("keryx-cli doctor");
            match client.get(format!("{base}/health")).send().await {
                Ok(r) if r.status().is_success() => println!("ok   control plane health"),
                Ok(r) => println!("fail health HTTP {}", r.status()),
                Err(e) => println!("fail health: {e}"),
            }
            if token.is_none() {
                println!("warn no KERYX_OPERATOR_TOKEN — authenticated checks skipped");
            } else if let Ok(v) =
                authed_json(&client, base, token, "GET", "/v1/providers", None).await
            {
                println!("ok   providers reachable (default={:?})", v.get("default"));
            } else {
                println!("fail providers (auth or network)");
            }
        }
        Commands::Tui => {
            println!("keryx TUI (control-plane client; no agent loop)");
            println!("slash commands: /help /session /run <session> <goal> /cancel <run> /approve <id> /deny <id> /approvals /events <run> /quit");
            println!("interrupt-and-redirect: /cancel <run> then /run <session> <new goal>");
            use std::io::{self, BufRead, Write};
            let stdin = io::stdin();
            let mut session_id: Option<String> = None;
            loop {
                print!("keryx> ");
                let _ = io::stdout().flush();
                let mut line = String::new();
                if stdin.lock().read_line(&mut line).is_err() {
                    break;
                }
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if line == "/quit" || line == "/exit" {
                    break;
                }
                if line == "/help" {
                    println!("/session — create session\n/run <sid> <goal> — start run\n/cancel <rid> — cancel\n/approvals — list\n/approve <id> /deny <id>\n/events <rid> — stream events\n/quit");
                    continue;
                }
                if line == "/session" {
                    match authed_json(&client, base, token, "POST", "/v1/sessions", None).await {
                        Ok(v) => {
                            session_id = v.get("id").and_then(|x| x.as_str()).map(str::to_string);
                            println!("{v}");
                        }
                        Err(e) => eprintln!("{e}"),
                    }
                    continue;
                }
                if let Some(rest) = line.strip_prefix("/run ") {
                    let mut parts = rest.splitn(2, ' ');
                    let sid = parts
                        .next()
                        .map(str::to_string)
                        .or_else(|| session_id.clone())
                        .unwrap_or_default();
                    let goal = parts.next().unwrap_or("").to_string();
                    if sid.is_empty() || goal.is_empty() {
                        eprintln!("usage: /run <session_id> <goal>");
                        continue;
                    }
                    match authed_json(
                        &client,
                        base,
                        token,
                        "POST",
                        &format!("/v1/sessions/{sid}/runs"),
                        Some(serde_json::json!({ "goal": goal })),
                    )
                    .await
                    {
                        Ok(v) => println!("{v}"),
                        Err(e) => eprintln!("{e}"),
                    }
                    continue;
                }
                if let Some(rid) = line.strip_prefix("/cancel ") {
                    match authed_json(
                        &client,
                        base,
                        token,
                        "POST",
                        &format!("/v1/runs/{}/cancel", rid.trim()),
                        None,
                    )
                    .await
                    {
                        Ok(v) => println!("{v}"),
                        Err(e) => eprintln!("{e}"),
                    }
                    continue;
                }
                if line == "/approvals" {
                    match authed_json(
                        &client,
                        base,
                        token,
                        "GET",
                        "/v1/approvals?pending=true",
                        None,
                    )
                    .await
                    {
                        Ok(v) => println!("{v}"),
                        Err(e) => eprintln!("{e}"),
                    }
                    continue;
                }
                if let Some(id) = line.strip_prefix("/approve ") {
                    match authed_json(
                        &client,
                        base,
                        token,
                        "POST",
                        &format!("/v1/approvals/{}/approve", id.trim()),
                        None,
                    )
                    .await
                    {
                        Ok(v) => println!("{v}"),
                        Err(e) => eprintln!("{e}"),
                    }
                    continue;
                }
                if let Some(id) = line.strip_prefix("/deny ") {
                    match authed_json(
                        &client,
                        base,
                        token,
                        "POST",
                        &format!("/v1/approvals/{}/deny", id.trim()),
                        None,
                    )
                    .await
                    {
                        Ok(v) => println!("{v}"),
                        Err(e) => eprintln!("{e}"),
                    }
                    continue;
                }
                if let Some(rid) = line.strip_prefix("/events ") {
                    // Reuse events fetch once (non-interactive stream snapshot).
                    match authed_json(
                        &client,
                        base,
                        token,
                        "GET",
                        &format!("/v1/runs/{}", rid.trim()),
                        None,
                    )
                    .await
                    {
                        Ok(v) => println!("run status: {v}"),
                        Err(e) => eprintln!("{e}"),
                    }
                    continue;
                }
                eprintln!("unknown command; /help");
            }
        }
    }
    Ok(())
}

async fn authed_json(
    client: &reqwest::Client,
    base: &str,
    token: Option<&str>,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> Result<Value, String> {
    let tok = token.ok_or("KERYX_OPERATOR_TOKEN required")?;
    let url = format!("{base}{path}");
    let mut req = match method {
        "GET" => client.get(&url),
        "POST" => client.post(&url),
        _ => return Err(format!("unsupported method {method}")),
    };
    req = req.header("authorization", format!("Bearer {tok}"));
    if let Some(b) = body {
        req = req.header("content-type", "application/json").json(&b);
    }
    let resp = req.send().await.map_err(|e| e.to_string())?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("HTTP {status}: {text}"));
    }
    if text.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

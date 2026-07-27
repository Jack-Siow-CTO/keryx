# Keryx

**Keryx** (Greek κῆρυξ — *herald, messenger*) is a minimal, secure Rust **agent Worker**: a long-running process that accepts intent, runs a bounded agent loop with tools, and returns outcomes—without the weight of a full multi-agent framework.

Host it on your laptop or a Linux worker. Reach it from Mac/phone over a private Tailnet if you want. Prefer official OpenAI and Grok (xAI) API keys for models.

## Quick Start (about 5 minutes)

**Prerequisites:** [Rust](https://rustup.rs/) (stable), `curl`, macOS or Linux.

```bash
git clone https://github.com/Jack-Siow-CTO/keryx.git
cd keryx
./scripts/install.sh
```

Load config and start the Worker (loopback only):

```bash
set -a && source ~/.config/keryx/env && set +a
keryx doctor    # readiness checks
keryx           # control plane on 127.0.0.1:8787
```

In another terminal, verify:

```bash
set -a && source ~/.config/keryx/env && set +a
export KERYX_URL=http://127.0.0.1:8787
./scripts/smoke.sh
```

Configure **at least one real model provider** (there is no runtime fake):

1. Official APIs: `OPENAI_API_KEY` and/or `XAI_API_KEY` in `~/.config/keryx/env`
2. Or ChatGPT subscription: `codex login` → `./scripts/sync-chatgpt-codex-auth.sh` → point `CHATGPT_WEB_ACCESS_TOKEN_FILE` at the synced file (`openai_codex`)
3. Or Grok web session: `GROK_WEB_COOKIE_FILE` (`grok_web`)
4. Set `KERYX_DEFAULT_PROVIDER` when more than one is registered
5. Restart `keryx` and re-run `./scripts/smoke.sh` (e.g. `KERYX_SMOKE_PROVIDER=openai_codex`)

Full install options: [docs/deploy/install.md](docs/deploy/install.md).  
Ready-to-use ladder: [docs/deploy/operator-checklist.md](docs/deploy/operator-checklist.md).

## Why Keryx

| Principle | What it means |
|-----------|----------------|
| **Minimal** | Small core surface. Only what an agent needs to plan, act, and report. |
| **Extensible** | Clear ports for tools, models, memory, and transport. |
| **Performant** | Rust: low latency, tight memory, always-on worker hosts. |
| **Secure** | Loopback bind, operator token, path-jailed file tools, fail closed. |
| **Reliable** | SQLite durability, cancel and budgets, clean interrupt on crash. |

## Install

| Method | When to use |
|--------|-------------|
| `./scripts/install.sh` | Default — builds with Cargo, writes config dirs |
| `cargo install --path crates/worker --locked` | Manual from-source |
| `docker compose up --build` | Optional container path |
| systemd (`--system`) | Always-on Linux host |

```bash
# user install (default)
./scripts/install.sh

# Linux system install
sudo ./scripts/install.sh --system
sudo systemctl enable --now keryx
```

Details: [docs/deploy/install.md](docs/deploy/install.md).

## Configure

Copy of the template lives at [`.env.example`](.env.example). Install places it at `~/.config/keryx/env` (mode `600`).

| Variable | Purpose |
|----------|---------|
| `KERYX_OPERATOR_TOKEN` | Required bearer token for `/v1/*` |
| `KERYX_BIND` | Default `127.0.0.1:8787` (loopback only) |
| `KERYX_DATA_DIR` | SQLite directory |
| `KERYX_DEFAULT_PROVIDER` | `openai` \| `grok` \| `openai_codex` \| `openai_web` \| `grok_web` |
| `OPENAI_API_KEY` / `XAI_API_KEY` | Official model credentials |
| `CHATGPT_WEB_ACCESS_TOKEN_FILE` | Codex / ChatGPT subscription OAuth token |
| `GROK_WEB_COOKIE_FILE` | Grok web session cookie |
| `KERYX_WORKSPACE_ROOTS` | Colon-separated file-tool roots |

Prefer `*_FILE` secret paths in production. Never commit tokens or keys.

## Run

```bash
# foreground
set -a && source ~/.config/keryx/env && set +a
keryx

# checks (config + optional live /health)
keryx doctor
keryx version
```

**systemd** (after system install): `sudo systemctl enable --now keryx`  
**Docker:** see [docs/deploy/install.md](docs/deploy/install.md#method-3--docker-optional)

## Use the control plane

```bash
export KERYX_URL=http://127.0.0.1:8787
export TOKEN=...   # same as KERYX_OPERATOR_TOKEN

curl -sS "$KERYX_URL/health"

curl -sS -X POST "$KERYX_URL/v1/sessions" \
  -H "authorization: Bearer $TOKEN"

curl -sS -X POST "$KERYX_URL/v1/sessions/<SESSION_ID>/runs" \
  -H "authorization: Bearer $TOKEN" \
  -H "content-type: application/json" \
  -d '{"goal":"summarize my notes","provider":"openai","model":"gpt-5.6-sol"}'

# list registered providers + default models
curl -sS "$KERYX_URL/v1/providers" \
  -H "authorization: Bearer $TOKEN"

curl -sSN "$KERYX_URL/v1/runs/<RUN_ID>/events" \
  -H "authorization: Bearer $TOKEN" \
  -H "accept: text/event-stream"

curl -sS -X POST "$KERYX_URL/v1/runs/<RUN_ID>/cancel" \
  -H "authorization: Bearer $TOKEN"
```

Or: `./scripts/smoke.sh`.

## Models

| Provider | Auth | Notes |
|----------|------|-------|
| `openai` | Platform API key | Official Chat Completions |
| `grok` | xAI API key | Official OpenAI-compatible |
| `openai_codex` | Codex OAuth (`codex login`) | ChatGPT Plus/Pro subscription wire |
| `openai_web` | Browser cookie (+ optional token) | Unofficial; cookie required to register |
| `grok_web` | Browser cookie | Unofficial Grok web session |

There is **no** runtime `fake` provider. Boot fails closed if no real secrets are configured.

```bash
# ~/.config/keryx/env — pick one or more
OPENAI_API_KEY=sk-...
# or after: codex login && ./scripts/sync-chatgpt-codex-auth.sh
CHATGPT_WEB_ACCESS_TOKEN_FILE=$HOME/.config/keryx/chatgpt-access-token
KERYX_DEFAULT_PROVIDER=openai_codex
```

Per-run model: `{"goal":"…","provider":"openai_codex","model":"gpt-5.6-sol"}`.  
Catalog: `GET /v1/providers`.  
Live tests: [docs/deploy/live-model-verification.md](docs/deploy/live-model-verification.md).  
Consumer/Codex detail: [docs/deploy/consumer-web-sessions.md](docs/deploy/consumer-web-sessions.md).

## Secure remote access (optional)

Default product path is **local loopback**. For Mac/phone:

```text
device --Tailscale HTTPS--> Caddy (Tailnet IPs only) --> Worker 127.0.0.1
```

- Do **not** bind the Worker on `0.0.0.0`
- Do **not** use Tailscale Funnel/Serve as the product path
- Tailscale is reachability only; the **operator token** is still required

Guide: [docs/deploy/tailnet-edge.md](docs/deploy/tailnet-edge.md).

## Verify readiness

| Level | What |
|-------|------|
| 1 | `cargo test --workspace` |
| 2 | install + real provider + `./scripts/smoke.sh` |
| 3 | Codex sub / OpenAI / Grok paths |
| 4 | systemd always-on |
| 5 | Tailnet edge |

Checklist: [docs/deploy/operator-checklist.md](docs/deploy/operator-checklist.md).

## Development

```bash
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

Hexagonal workspace (`domain` ← `app` ← adapters ← `worker`). Glossary: [CONTEXT.md](CONTEXT.md). Spec: [docs/specs/0001-keryx-v1-worker.md](docs/specs/0001-keryx-v1-worker.md). ADRs: [docs/adr/](docs/adr/).

## Status

**v1 Worker** — control plane (Sessions, Runs, SSE, auth, budgets, cancel), SQLite durability, workspace file tools, OpenAI/Grok providers, optional consumer-web adapters, install scripts and deploy docs for public self-host.

## Non-goals (for now)

- Matching every feature of larger multi-agent frameworks
- Opaque “do anything” automation without policy
- Public internet bind or multi-tenant SaaS
- Shell/exec and browser tools in v1

## Name

*Keryx* is the herald—the official messenger. It fits a system whose job is to carry work, speak to tools and models, and bring back a clear answer.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

# Operator checklist — declare Keryx ready

Use this ladder after install ([install.md](./install.md)). Stop when your intended use is covered.

## Level 1 — Automated gates

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
```

- [ ] All green locally (or CI on `main`)

## Level 2 — Local Worker (real model provider)

There is **no** runtime fake model. Configure at least one of: Platform API keys, Codex OAuth token, or Grok web cookie.

```bash
./scripts/install.sh          # or cargo install --path crates/worker --locked
# edit ~/.config/keryx/env — add secrets (see .env.example)
set -a && source ~/.config/keryx/env && set +a
keryx doctor                  # must list registered real providers
keryx                         # terminal 1
./scripts/smoke.sh            # terminal 2 — needs KERYX_OPERATOR_TOKEN
# optional: KERYX_SMOKE_PROVIDER=openai_codex KERYX_SMOKE_MODEL=gpt-5.6-sol
```

- [ ] `GET /health` → `{"status":"ok"}`
- [ ] Missing token → **401**
- [ ] `GET /v1/providers` lists registered providers (no `fake`)
- [ ] Session + Run completes with a real provider
- [ ] SSE stream shows terminal `run.completed` (or equivalent terminal status)

## Level 3 — Auth path matrix (local)

| Path | Provider | Config |
|------|----------|--------|
| ChatGPT Platform API | `openai` | `OPENAI_API_KEY` / `OPENAI_MODEL` |
| Codex / ChatGPT sub | `openai_codex` | `codex login` + sync script + token file |
| Grok official API | `grok` | `XAI_API_KEY` / `XAI_MODEL` |
| Grok web sub | `grok_web` | `GROK_WEB_COOKIE_FILE` |

```bash
export KERYX_URL=http://127.0.0.1:8787
export KERYX_OPERATOR_TOKEN=...
export KERYX_SMOKE_PROVIDER=openai_codex   # or openai / grok / grok_web
# optional: KERYX_SMOKE_MODEL=...
./scripts/smoke.sh
```

Or opt-in crate tests: [live-model-verification.md](./live-model-verification.md).

- [ ] At least one path you pay for returns a real model result
- [ ] Failures do not print API keys / cookies / tokens in logs/SSE

## Level 4 — Always-on host (optional)

```bash
sudo ./scripts/install.sh --system
sudo systemctl enable --now keryx
curl -sS http://127.0.0.1:8787/health
```

- [ ] Survives reboot (`systemctl is-active keryx`)
- [ ] After restart, prior Sessions still load; interrupted Active Runs are `interrupted`

## Level 5 — Tailnet remote (optional)

Follow [tailnet-edge.md](./tailnet-edge.md).

- [ ] Caddy (or equivalent) on Tailscale IPs only → `127.0.0.1:8787`
- [ ] Health over HTTPS from Mac/phone
- [ ] Authenticated Session + Run over Tailnet
- [ ] No Funnel / public expose

## Ready when

| Use case | Minimum level |
|----------|----------------|
| Local experiments with a real provider | Level 2 |
| Daily agent with paid APIs / Codex sub | Level 3 |
| Always-on personal host | Level 4 |
| Mac + phone clients | Level 5 |

**Not ready for:** multi-tenant public SaaS, unauthenticated internet exposure, or treating consumer web sessions as a supported product default over official APIs.

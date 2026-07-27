# Operator checklist — declare Keryx ready

Use this ladder after install ([install.md](./install.md)). Stop when your intended use is covered.

## Level 1 — Automated gates

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
```

- [ ] All green locally (or CI on `main`)

## Level 2 — Local Worker (fake model)

```bash
./scripts/install.sh          # or cargo install --path crates/worker --locked
set -a && source ~/.config/keryx/env && set +a
keryx doctor
keryx                         # terminal 1
./scripts/smoke.sh            # terminal 2 — needs KERYX_OPERATOR_TOKEN in env
```

- [ ] `GET /health` → `{"status":"ok"}`
- [ ] Missing token → **401**
- [ ] Session + Run with `fake` completes
- [ ] SSE stream shows terminal `run.completed` (or equivalent terminal status)

## Level 3 — Real model (API keys)

```bash
# in env file:
# OPENAI_API_KEY=sk-...
# and/or XAI_API_KEY=xai-...
# KERYX_DEFAULT_PROVIDER=openai   # or grok
```

Restart Worker, then:

```bash
export KERYX_URL=http://127.0.0.1:8787
export KERYX_OPERATOR_TOKEN=...
# optional: KERYX_SMOKE_PROVIDER=openai
./scripts/smoke.sh
```

Or opt-in crate tests: [live-model-verification.md](./live-model-verification.md).

- [ ] Run with `openai` or `grok` returns a real model result
- [ ] Failures do not print API keys in logs/SSE

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
| Local experiments | Level 2 |
| Daily agent with paid APIs | Level 3 |
| Always-on personal host | Level 4 |
| Mac + phone clients | Level 5 |

**Not ready for:** multi-tenant public SaaS, unauthenticated internet exposure, or treating consumer web sessions as a supported product default.

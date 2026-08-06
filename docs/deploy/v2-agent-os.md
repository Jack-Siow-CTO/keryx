# Keryx v2 — Operator deploy notes (agent OS)

Aligned with ADR 0012 and spec 0002. Extends v1 install without discarding SQLite data.

## New env surfaces

| Variable | Purpose |
|----------|---------|
| `KERYX_SOUL_PATH` | Operator Soul document (soft-missing) |
| `KERYX_CONTEXT_FILES` | Colon-separated workspace Context files |
| `KERYX_SKILLS_ROOT` | Skills package root (`name/SKILL.md`) |
| `KERYX_ALLOWED_TOOLS` | Comma list: workspace FS, web, memory, `run_terminal` (defaults match Worker compose) |
| `KERYX_TELEGRAM_BOT_TOKEN` | Telegram Gateway (fail closed if invalid) |
| `KERYX_DOCKER_IMAGE` | Default Docker image for reduced-origin exec |
| `KERYX_MCP_CONFIG` | Static MCP client servers JSON (restart to apply) — see [mcp-user-capabilities.md](./mcp-user-capabilities.md) |
| `KERYX_POLICY_EXTRA_TOOLS` | Extra exact tool names for control_plane Policy (comma-separated), including `mcp.<id>.<tool>` |

Long-tail product integrations (Gmail/Slack APIs/HA, …) enter as **MCP client Tools**, not first-party crates. Recipes, Policy allowlist, high-blast Approval, and doctor: **[mcp-user-capabilities.md](./mcp-user-capabilities.md)** (spec 0003).

## Approvals

High-blast actions (for example local terminal and MCP high-blast tools) create pending Approvals:

```bash
keryx-cli approvals-list
keryx-cli approve <id>
keryx-cli deny <id>
```

## Schedules

```bash
# HTTP
curl -sS -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"goal":"briefing","interval_secs":3600,"next_fire_at":…}' \
  http://127.0.0.1:8787/v1/schedules
curl -sS -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"now":…}' http://127.0.0.1:8787/v1/schedules/tick
```

**Missed/double-fire:** `tick` with deterministic `now`. Same-second double tick does not re-fire. Missed intervals advance one step (no catch-up storm). Documented in `Schedule::record_fire`.

## Gateways

Telegram is the live Gateway on the Worker. Discord exists only as origin/fixture mapping in `keryx-gateway` (no live bot task). Gateways call control-plane ports only (`origin=gateway:*`, reduced Policy). Seam 3 fixture tests require **no live bots**.

### Telegram (live long-poll on the Worker)

1. Talk to [@BotFather](https://t.me/BotFather) → `/newbot` → copy the **HTTP API token**.
2. On the worker (`~/.config/keryx/env`):

```bash
KERYX_TELEGRAM_BOT_TOKEN=123456:ABC...   # mode 600
# Strongly recommended: lock to your DM chat id
KERYX_TELEGRAM_ALLOWED_CHAT_IDS=YOUR_NUMERIC_CHAT_ID
KERYX_TELEGRAM_RUN_TIMEOUT_SECS=180
```

3. Restart: `systemctl --user restart keryx`
4. Open a DM with the bot, send any text. You should get `… working` then the Run result.
5. Logs: `journalctl --user -u keryx -f` — look for `telegram gateway long-poll starting` and `telegram gateway started Run`.

Chat id: message the bot once, then either use `@userinfobot` / `@getidsbot`, or read `chat_id` from worker logs when allowlist is empty.

**Security:** empty `KERYX_TELEGRAM_ALLOWED_CHAT_IDS` allows *any* Telegram user who finds the bot. Prefer an allowlist.

## Exec backends

- `control_plane` origin: local terminal allowed under **Approval**
- `gateway:*` / `schedule`: Docker default; local denied

## Optional live/Docker verification (not merge-required)

```bash
# Live models
KERYX_LIVE_MODELS=1 cargo test -p keryx-model --features …  # see live-model-verification.md
# Real Docker exec smoke (optional main/tag)
docker info && keryx doctor
```

Default CI remains free of live model/Telegram/Discord network.

## Doctor

```bash
keryx doctor   # loopback, token, data dir, providers, Soul, skills, Docker, Gateway tokens
```

Fails closed on non-loopback bind.

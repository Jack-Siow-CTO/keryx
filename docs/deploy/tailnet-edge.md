# Keryx Tailnet edge deploy (jack-agent-worker style)

**Prereq:** complete a local install and smoke first — [install.md](./install.md), [operator-checklist.md](./operator-checklist.md) (through Level 3 recommended). Example Caddyfile: [`deploy/Caddyfile.example`](../../deploy/Caddyfile.example).

This document is the operator guide for hosting Keryx on a private host
with **Tailnet-only HTTPS** in front of a **loopback-only Worker**.

Vocabulary matches `CONTEXT.md`: **Worker**, **Edge**, **Principal**,
**control plane**. Architectural decisions: ADR 0003 (loopback + Tailnet edge),
ADR 0004 (operator token auth), ADR 0006 (SQLite durability). Daily-use ship bar:
[ADR 0035](../adr/0035-daily-use-core-ship-bar.md).

## Edge URL (operators and Console)

| Item | Value |
|------|--------|
| **Canonical Edge base URL** | `https://<host-magicdns-or-ts-name>:8443` |
| **jack-agent-worker example** | `https://jack-agent-worker.tail68a74b.ts.net:8443` |
| **Not the Edge** | Host/T3 HTTPS on **`:443`** — different service; do not point Console at bare `:443` for the control plane |
| **Loopback (on host only)** | `http://127.0.0.1:8787` |

**Console settings:** set Worker base URL to the **`:8443` Edge URL** above (plus operator bearer token). Do not use the T3 app port as the control plane.

Port **`:8443`** is the daily-use default when the host already serves another Tailnet HTTPS site on `:443`. Clients must include the port. No Funnel / public Edge.

## Topology (fail closed)

```text
Mac / phone  --Tailscale HTTPS :8443-->  Caddy (Tailscale IPs only)
                                              |
                                              v
                                   Worker control plane (127.0.0.1:8787 only)
```

| Layer | Role | Not |
|-------|------|-----|
| **Tailscale** | Reachability between devices and host | Application authorization |
| **Edge (Caddy)** | Terminate HTTPS on Tailnet addresses (typically **`:8443`**); reverse-proxy to loopback | Public internet listener, Funnel/Serve product path, host `:443` app ports |
| **Worker** | Authenticated control plane (Sessions, Runs, SSE) | LAN / `0.0.0.0` bind |

**Rules:**

- Do **not** bind the Worker on `0.0.0.0` or any non-loopback address.
- Do **not** enable Tailscale Serve/Funnel as the product path.
- Do **not** treat Tailscale identity as sufficient app auth.
- Every control-plane call requires a bearer **operator token**.
- SSH is for administration only—not the steady-state app data path for Mac/phone clients.

## Worker configuration

Secrets load from the environment or secret **files** (never commit keys).

| Variable | Required | Description |
|----------|----------|-------------|
| `KERYX_OPERATOR_TOKEN` or `KERYX_OPERATOR_TOKEN_FILE` | yes | Bearer token for Principals |
| `KERYX_OPERATOR_PRINCIPAL` | no | Principal id recorded on Session/Run (default `operator`) |
| `KERYX_BIND` | no | Loopback socket only (default `127.0.0.1:8787`) |
| `KERYX_DATA_DIR` | no | SQLite data directory (default `./data`) |
| `KERYX_GLOBAL_ACTIVE_CAP` | no | Max concurrent Active Runs across Sessions (default `2`) |
| `KERYX_WORKSPACE_ROOTS` | no | Colon-separated Workspace roots for file tools |
| `KERYX_ALLOWED_TOOLS` | no | Comma-separated tool allowlist (default `read_file,write_file,apply_patch,search_files`) |
| `KERYX_DEFAULT_PROVIDER` | when multiple | `openai` \| `grok` \| `openai_codex` \| … (no fake) |
| `OPENAI_API_KEY` / `OPENAI_API_KEY_FILE` | for OpenAI | API credentials |
| `OPENAI_MODEL` | no | Default model id |
| `OPENAI_BASE_URL` | no | Override base URL (tests/fixtures) |
| `XAI_API_KEY` / `XAI_API_KEY_FILE` | for Grok | API credentials |
| `XAI_MODEL` | no | Default model id |
| `XAI_BASE_URL` | no | Override base URL |
| `RUST_LOG` | no | Tracing filter (e.g. `info`) |

Example (host):

```bash
export KERYX_OPERATOR_TOKEN="$(cat /run/secrets/keryx-operator-token)"
export KERYX_DATA_DIR=/var/lib/keryx
export KERYX_BIND=127.0.0.1:8787
export KERYX_WORKSPACE_ROOTS=/var/lib/keryx/workspace
export KERYX_DEFAULT_PROVIDER=openai   # or grok / openai_codex when secrets present
# optional:
# export OPENAI_API_KEY_FILE=/run/secrets/openai-api-key
# export XAI_API_KEY_FILE=/run/secrets/xai-api-key

cargo run -p keryx-worker --release
# binary name: keryx
```

Non-loopback `KERYX_BIND` is rejected at startup.

## Edge: Caddy on Tailscale IPs only

Illustrative Caddyfile (adjust hostnames/IPs to your tailnet). Prefer **`:8443`** so host `:443` can stay free for other Tailnet services:

```caddy
# Bind only to this host's Tailscale address(es)—not public interfaces.
# Replace 100.x.y.z with the host Tailscale IPv4 (or use a MagicDNS name
# that resolves only on the tailnet).

{
    admin off
    auto_https disable_redirects
}

https://keryx.your-tailnet.ts.net:8443 {
    bind 100.x.y.z
    reverse_proxy 127.0.0.1:8787
}
```

Checklist for the edge:

- [ ] Caddy (or equivalent) listens only on Tailscale addresses (typically **`:8443`**)
- [ ] Upstream is `127.0.0.1` (or loopback UDS later)—never a LAN IP product path
- [ ] TLS is terminated at the edge; Worker stays HTTP on loopback
- [ ] No Funnel/public expose flags enabled for this site
- [ ] Console / operators document the full base URL including **`:8443`**

## Health verification

On the host:

```bash
curl -sS http://127.0.0.1:8787/health
# {"status":"ok"}
```

Over the Tailnet (from a **second** tailnet node with Tailscale up — not only on-host):

```bash
# Include :8443. If MagicDNS fails on the client, pin the Tailscale IPv4:
curl -sS --resolve keryx.your-tailnet.ts.net:8443:100.x.y.z \
  https://keryx.your-tailnet.ts.net:8443/health
# {"status":"ok"}

# Auth challenge (must be 401 without bearer token):
curl -sS -o /dev/null -w '%{http_code}\n' --resolve keryx.your-tailnet.ts.net:8443:100.x.y.z \
  -X POST https://keryx.your-tailnet.ts.net:8443/v1/sessions
# 401
```

Authenticated smoke (replace token and base URL):

```bash
export KERYX_URL=https://keryx.your-tailnet.ts.net:8443
export TOKEN=... # same as KERYX_OPERATOR_TOKEN

# Create Session
curl -sS -X POST "$KERYX_URL/v1/sessions" \
  -H "authorization: Bearer $TOKEN"

# Start Run ("provider":"openai"|"grok"|"openai_codex"|…, optional "model")
curl -sS -X POST "$KERYX_URL/v1/sessions/<SESSION_ID>/runs" \
  -H "authorization: Bearer $TOKEN" \
  -H "content-type: application/json" \
  -d '{"goal":"hello from tailnet"}'

# Stream Run events (SSE)
curl -sSN "$KERYX_URL/v1/runs/<RUN_ID>/events" \
  -H "authorization: Bearer $TOKEN" \
  -H "accept: text/event-stream"

# Cancel
curl -sS -X POST "$KERYX_URL/v1/runs/<RUN_ID>/cancel" \
  -H "authorization: Bearer $TOKEN"
```

Missing/invalid tokens must return **401** with no Session/Run side effects.

## Operator checklist — Mac and phone

1. Install/sign in to Tailscale on the device; confirm you can ping the worker host.
2. Point Console base URL at `https://<host MagicDNS>:8443` (Edge HTTPS only — **not** host `:443` / T3).
3. Configure the client with the operator bearer token (not Tailscale ACLs alone).
4. Create a Session, start a Run, watch SSE events, cancel if needed.
5. After Worker restarts, list/get Sessions; interrupted Active Runs appear as `interrupted`—start a **new** Run (no mid-loop resume).
6. Prefer HTTPS over Tailnet for day-to-day use; use SSH only for host admin, logs, and deploys.

## Host CI job shell (daily-use bar)

Required host suite for [ADR 0035](../adr/0035-daily-use-core-ship-bar.md) lives as:

| Artifact | Role |
|----------|------|
| [`scripts/keryx-host-ci.sh`](../../scripts/keryx-host-ci.sh) | Host scenario runner (may stay red until product tickets land) |
| [`deploy/keryx-host-ci.service`](../../deploy/keryx-host-ci.service) + [`.timer`](../../deploy/keryx-host-ci.timer) | User systemd oneshot + periodic trigger on `jack-agent-worker` |

After a second tailnet node proves Edge health + auth challenge, set `KERYX_EDGE_SECOND_NODE_OK=1` in the Worker env file. Optional `KERYX_EDGE_URL` overrides the default Edge base URL in the script.

## Telegram bot token rotate (Gateway)

1. In BotFather, revoke/reissue the bot token (old token retired).
2. Set `KERYX_TELEGRAM_BOT_TOKEN` in the Worker env file (never commit the token).
3. Keep `KERYX_TELEGRAM_ALLOWED_CHAT_IDS` allowlist; restart the Worker user unit.
4. Confirm allowlisted chat still maps; non-allowlisted stays fail-closed (`getMe` ok; no Session from foreign chats).

## What not to do

- Bind Worker to `0.0.0.0` / public interfaces
- Enable Tailscale Funnel or Serve as the product exposure path
- Put operator tokens or model API keys in git
- Skip bearer auth because “it’s only on the tailnet”
- Expect mid-tool exactly-once resume after crash
- Point Console at host **`:443`** when Edge is **`:8443`**

## Related

- Spec: `docs/specs/0001-keryx-v1-worker.md`
- Daily-use ship bar: `docs/adr/0035-daily-use-core-ship-bar.md`
- Live model opt-in (manual/nightly only): `docs/deploy/live-model-verification.md`
- Glossary: `CONTEXT.md`

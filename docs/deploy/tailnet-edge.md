# Keryx Tailnet edge deploy (jack-agent-worker style)

This document is the operator guide for hosting Keryx v1 on a private host
with **Tailnet-only HTTPS** in front of a **loopback-only Worker**.

Vocabulary matches `CONTEXT.md`: **Worker**, **Edge**, **Principal**,
**control plane**. Architectural decisions: ADR 0003 (loopback + Tailnet edge),
ADR 0004 (operator token auth), ADR 0006 (SQLite durability).

## Topology (fail closed)

```text
Mac / phone  --Tailscale HTTPS-->  Caddy (Tailscale IPs only)
                                         |
                                         v
                              Worker control plane (127.0.0.1 only)
```

| Layer | Role | Not |
|-------|------|-----|
| **Tailscale** | Reachability between devices and host | Application authorization |
| **Edge (Caddy)** | Terminate HTTPS on Tailnet addresses; reverse-proxy to loopback | Public internet listener, Funnel/Serve product path |
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
| `KERYX_ALLOWED_TOOLS` | no | Comma-separated tool allowlist (default `read_file,write_file`) |
| `KERYX_DEFAULT_PROVIDER` | no | `fake` \| `openai` \| `grok` (default `fake`) |
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
export KERYX_DEFAULT_PROVIDER=fake   # or openai / grok when keys present
# optional:
# export OPENAI_API_KEY_FILE=/run/secrets/openai-api-key
# export XAI_API_KEY_FILE=/run/secrets/xai-api-key

cargo run -p keryx-worker --release
# binary name: keryx
```

Non-loopback `KERYX_BIND` is rejected at startup.

## Edge: Caddy on Tailscale IPs only

Illustrative Caddyfile (adjust hostnames/IPs to your tailnet):

```caddy
# Bind only to this host's Tailscale address(es)—not public interfaces.
# Replace 100.x.y.z with the host Tailscale IPv4 (or use a MagicDNS name
# that resolves only on the tailnet).

https://keryx.your-tailnet.ts.net {
    bind 100.x.y.z
    reverse_proxy 127.0.0.1:8787
}
```

Checklist for the edge:

- [ ] Caddy (or equivalent) listens only on Tailscale addresses
- [ ] Upstream is `127.0.0.1` (or loopback UDS later)—never a LAN IP product path
- [ ] TLS is terminated at the edge; Worker stays HTTP on loopback
- [ ] No Funnel/public expose flags enabled for this site

## Health verification

On the host:

```bash
curl -sS http://127.0.0.1:8787/health
# {"status":"ok"}
```

Over the Tailnet (from Mac/phone with Tailscale up):

```bash
curl -sS https://keryx.your-tailnet.ts.net/health
```

Authenticated smoke (replace token and base URL):

```bash
export KERYX_URL=https://keryx.your-tailnet.ts.net
export TOKEN=... # same as KERYX_OPERATOR_TOKEN

# Create Session
curl -sS -X POST "$KERYX_URL/v1/sessions" \
  -H "authorization: Bearer $TOKEN"

# Start Run (fake provider default, or "provider":"openai"|"grok")
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
2. Open `https://<keryx MagicDNS or Tailscale host>` (edge HTTPS only).
3. Configure the client with the operator bearer token (not Tailscale ACLs alone).
4. Create a Session, start a Run, watch SSE events, cancel if needed.
5. After Worker restarts, list/get Sessions; interrupted Active Runs appear as `interrupted`—start a **new** Run (no mid-loop resume).
6. Prefer HTTPS over Tailnet for day-to-day use; use SSH only for host admin, logs, and deploys.

## What not to do

- Bind Worker to `0.0.0.0` / public interfaces
- Enable Tailscale Funnel or Serve as the product exposure path
- Put operator tokens or model API keys in git
- Skip bearer auth because “it’s only on the tailnet”
- Expect mid-tool exactly-once resume after crash

## Related

- Spec: `docs/specs/0001-keryx-v1-worker.md`
- Live model opt-in (manual/nightly only): `docs/deploy/live-model-verification.md`
- Glossary: `CONTEXT.md`

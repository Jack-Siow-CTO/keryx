# MCP user-added capabilities (operator recipe)

User-added long-tail product integrations enter Keryx as **MCP client Tools** on the Worker—not plugins, not first-party SaaS crates, and not Gateways.

**Apply path:** edit static config → restart Worker. Hot reload is not required.

Related: [spec 0003](../specs/0003-mcp-user-capabilities.md), [v2 agent OS](v2-agent-os.md).

## Concepts (glossary)

| Term | Role |
|------|------|
| **MCP server** | External process or HTTP endpoint that exposes tools |
| **Tool** | Namespaced invoke: `mcp.<server_id>.<tool_name>` |
| **Policy** | Exact allowlist; **connect ≠ allow** |
| **Approval** | High-blast tools wait for Principal decide |
| **Gateway** | Chat surfaces only (Telegram/Discord)—not product APIs |

## Env

```bash
# Path to MCP servers JSON (mode 600 recommended)
KERYX_MCP_CONFIG=/var/lib/keryx/mcp.json

# Optional: extra exact tool names for control_plane Policy only
# (comma-separated; does not apply to gateway/schedule origins)
KERYX_POLICY_EXTRA_TOOLS=mcp.gmail.search,mcp.slack.list_channels
```

See `.env.example` for the full Worker env surface.

## Config file shape

```json
{
  "servers": [
    {
      "server_id": "gmail",
      "enabled": true,
      "transport": {
        "type": "stdio",
        "command": "npx",
        "args": ["-y", "@example/mcp-gmail", "--stdio"]
      },
      "env_files": {
        "GMAIL_TOKEN": "/run/secrets/gmail-token"
      },
      "policy_allowlist": ["search", "get_message", "send"],
      "high_blast": ["send"],
      "timeout_ms": 30000,
      "max_result_chars": 50000
    },
    {
      "server_id": "slack_api",
      "transport": {
        "type": "remote",
        "url": "https://mcp.example.internal/slack",
        "auth_token_file": "/run/secrets/slack-mcp-token"
      },
      "policy_allowlist": ["list_channels", "post_message"],
      "high_blast": ["post_message"]
    }
  ]
}
```

Rules:

- `server_id`: lowercase letter start, then `[a-z0-9_]*` only.
- Tools register as `mcp.<server_id>.<peer_local_name>`.
- `policy_allowlist` and `high_blast` use **peer-local** names; Keryx namespaces them.
- Credentials: env / secret files only (mode 600). Never put OAuth tokens in Soul, Skills, Memory, or chat.
- Optional `tool_filter`: only register a subset of discovered tools.
- Transport is **per server**: `stdio` or `remote`.
- **Remote transport** is **HTTP JSON-RPC POST** only (simple request/response). This is not a full MCP SSE / streamable-HTTP client lifecycle (no long-lived SSE session negotiation). After `initialize`, Keryx sends `notifications/initialized` fire-and-forget, then `tools/list`.
- **Stdio env isolation**: the child process starts with `env_clear()` and receives only a small safe base (`PATH`, `HOME`, `LANG`, `TMPDIR`, …) plus the server’s config `env` / `env_files`. Worker secrets (`KERYX_*`, provider API keys) are **not** inherited unless the operator explicitly lists them in that server’s env.
- **Apply path**: edit config → **restart Worker** (no control-plane MCP CRUD; no agent self-install in this slice).

## Mail-class recipe (stdio + secret file)

1. Obtain a Gmail-class MCP binary/server and its OAuth/setup docs (outside Keryx).
2. Write the token to a secret file (`chmod 600`), e.g. `/run/secrets/gmail-token`.
3. Add a server block with `server_id` (e.g. `gmail`), stdio `command`/`args`, and `env_files`.
4. Put read tools on `policy_allowlist` (e.g. `search`); put `send` (or equivalent) on both `policy_allowlist` and `high_blast`.
5. Restart the Worker.
6. Run `keryx doctor` — expect the server `connected`, discovered namespaced tools listed, allowlist vs high_blast visible (no secret values).
7. Start a **control-plane** Run that requests `mcp.gmail.search` (no Approval) or `mcp.gmail.send` (Approval required).

## Messaging-API-class recipe (remote HTTP)

Same pattern as mail-class, but:

- `transport.type = "remote"` with `url` + `auth_token_file` / `auth_token_env`.
- Prefer marking channel-post / message-send tools as `high_blast`.
- Do **not** confuse this with Slack/Telegram **as chat** (that is Gateway work).

## Policy defaults

| Origin | MCP tools |
|--------|-----------|
| `control_plane` | Only exact names from config `policy_allowlist` + `KERYX_POLICY_EXTRA_TOOLS`. Discovery alone does not allowlist. No fixture MCP tools are baked into production Policy defaults. |
| `gateway:*` / `schedule` | **None** by default. Extras are not applied to reduced origins. |

Unknown tools fail closed. Child Runs inherit a **frozen parent Policy snapshot** at spawn (cannot gain tools the parent lacked, and cannot expand if Worker config changes mid-process).

## Approval

- Only **config-declared** high-blast names (no substring heuristics).
- Deny / cancel / timeout → tool failure (fail closed). Approval wait defaults to **300s** (`APPROVAL_TIMEOUT`); on expiry the Approval is CAS-denied and the tool fails closed.
- Approval / tool-event summaries redact secret-like keys after key normalization (`api_key`, `apiKey`, `API-KEY`, `token`, `password`, `authorization`, `bearer`, `access_token`, `refresh_token`, `client_secret`, …), including nested objects, and truncate bodies.

## Doctor

```bash
keryx doctor
```

Reports per server: configured / connected / error, discovered namespaced tools, control_plane allowlist, high_blast. Never prints secret file contents or bearer tokens.

## Failure modes

| Situation | Behavior |
|-----------|----------|
| Missing binary / bad URL / missing secret file | That server contributes **zero** tools; Worker still starts by default |
| Disconnect mid-call | Invoke fails closed |
| Tool not on Policy | Denied with clear Transcript/event reason |
| High-blast deny | Tool fails closed |

## Explicit non-goals (this slice)

- Dynamic plugins / control-plane MCP CRUD / agent self-install  
- Keryx OAuth broker  
- First-party Gmail/Slack Rust tool crates  
- MCP **server export** as ship bar  
- Auto-allow all tools from a server  

## Verify without live SaaS

Seam 1 uses a mock MCP peer in CI. Optional on-host check: run a local stdio MCP echo server, point `KERYX_MCP_CONFIG` at it, restart, `keryx doctor`, then a control-plane Run.

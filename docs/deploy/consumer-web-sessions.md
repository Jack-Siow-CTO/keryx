# Consumer web session Model providers

Opt-in adapters that use **operator-exported** ChatGPT or Grok **browser session** material (cookies / access tokens) instead of official API keys. See ADR 0010.

## Warnings (read first)

- Wire formats are **unofficial** and **break without notice**.
- This may violate vendor Terms of Service. **You own that risk.**
- Prefer official API keys (`openai`, `grok`) when available.
- Sessions expire; Keryx does **not** open a browser or refresh logins for you.
- Never commit cookies/tokens; never put them in git, Transcript, or logs.
- Control-plane auth remains the operator bearer token. Web session ≠ Principal.

## Provider keys

| `provider` value | Product | Auth inputs |
|------------------|---------|-------------|
| `openai_web` | ChatGPT web (Plus/Pro/Codex-in-browser style) | access token and/or cookie |
| `grok_web` | Grok web | cookie (+ optional extra headers) |
| `openai` / `grok` | Official APIs | API keys (existing) |
| `fake` | In-process | none |

Select per Run:

```json
{ "goal": "summarize my notes", "provider": "openai_web" }
```

Or set Worker default:

```bash
export KERYX_DEFAULT_PROVIDER=openai_web
```

Web providers are **registered only when secrets resolve**. Selecting an unregistered provider fails closed.

## Environment / secret files

### ChatGPT web (`openai_web`)

| Variable | Purpose |
|----------|---------|
| `CHATGPT_WEB_ACCESS_TOKEN` or `CHATGPT_WEB_ACCESS_TOKEN_FILE` | Bearer access token from browser session (preferred when available) |
| `CHATGPT_WEB_COOKIE` or `CHATGPT_WEB_COOKIE_FILE` | Full `Cookie` header value (optional if token alone works for your export) |
| `CHATGPT_WEB_HEADERS_FILE` | Optional JSON object of extra headers |
| `CHATGPT_WEB_BASE_URL` | Default `https://chatgpt.com` (override for fixtures/proxies) |
| `CHATGPT_WEB_PATH` | Default `/backend-api/conversation` |
| `CHATGPT_WEB_MODEL` | Model label sent on the wire when required |
| `CHATGPT_WEB_USER_AGENT` | Optional UA override |

At least one of **access token** or **cookie** must be non-empty to register `openai_web`.

### Grok web (`grok_web`)

| Variable | Purpose |
|----------|---------|
| `GROK_WEB_COOKIE` or `GROK_WEB_COOKIE_FILE` | Full `Cookie` header value (required to register) |
| `GROK_WEB_HEADERS_FILE` | Optional JSON object (e.g. challenge/signature style headers from a captured request) |
| `GROK_WEB_BASE_URL` | Default `https://grok.com` |
| `GROK_WEB_PATH` | Default `/rest/app-chat/conversations/new` |
| `GROK_WEB_MODEL` | Model label when required |
| `GROK_WEB_USER_AGENT` | Optional UA override |

### Exporting secrets (operator procedure)

1. Log in to ChatGPT or Grok in a normal browser.
2. Open DevTools → Network; trigger a chat request.
3. Copy the `Authorization: Bearer …` value and/or `Cookie` header (and any required custom headers).
4. Write them to mode-`600` files outside git, e.g. `/run/secrets/chatgpt-access-token`.
5. Point Keryx at those files with `*_FILE` env vars.
6. When the session expires, re-export; Keryx will fail the Run with a non-secret “session expired or rejected” style error.

Do **not** paste secrets into issue trackers or chat logs.

## Example Worker env

```bash
export KERYX_OPERATOR_TOKEN="$(cat /run/secrets/keryx-operator-token)"
export KERYX_DATA_DIR=/var/lib/keryx
export KERYX_BIND=127.0.0.1:8787

# Optional official APIs still work in parallel:
# export OPENAI_API_KEY_FILE=/run/secrets/openai-api-key

export CHATGPT_WEB_ACCESS_TOKEN_FILE=/run/secrets/chatgpt-access-token
export CHATGPT_WEB_COOKIE_FILE=/run/secrets/chatgpt-cookie
# export KERYX_DEFAULT_PROVIDER=openai_web

export GROK_WEB_COOKIE_FILE=/run/secrets/grok-cookie
# export GROK_WEB_HEADERS_FILE=/run/secrets/grok-extra-headers.json

cargo run -p keryx-worker --release
```

## Live verification (opt-in only)

```bash
export KERYX_LIVE_MODELS=1
# plus CHATGPT_WEB_* and/or GROK_WEB_* secrets
cargo test -p keryx-model --test live_consumer_web -- --ignored --nocapture
```

Default CI never requires consumer secrets or live consumer network (ADR 0009 / 0010).

## Related

- ADR 0010, ADR 0005 (updated), ADR 0009
- Official API live path: `docs/deploy/live-model-verification.md`
- Tailnet edge: `docs/deploy/tailnet-edge.md`

# Consumer web session & subscription Model providers

Opt-in adapters that use **operator-exported** ChatGPT or Grok session material instead of (or alongside) official Platform API keys. See ADR 0010.

Local-first layout: secrets under `~/.config/keryx/` (mode `600`). The Worker never opens a browser.

## Warnings (read first)

- Wire formats for browser/Codex paths are **unofficial** and **break without notice**.
- This may violate vendor Terms of Service. **You own that risk.**
- Prefer official API keys (`openai`, `grok`) when available.
- Sessions expire; Keryx does **not** refresh logins for you.
- Never commit cookies/tokens; never put them in git, Transcript, or logs.
- Control-plane auth remains the operator bearer token. Web/Codex session ≠ Principal.
- There is **no** runtime `fake` provider.

## Provider matrix

| `provider` | Product | Auth kind | Inputs |
|------------|---------|-----------|--------|
| `openai` | OpenAI Platform API | `api_key` | `OPENAI_API_KEY` |
| `grok` | xAI official API | `api_key` | `XAI_API_KEY` |
| `openai_codex` | ChatGPT Plus/Pro via Codex OAuth | `oauth_access_token` | access token from `codex login` |
| `openai_web` | ChatGPT browser conversation | `browser_session` | **cookie** (token-only needs `CHATGPT_WEB_FORCE=1`) |
| `grok_web` | Grok web chat | `browser_session` | cookie (+ optional headers) |

Select per Run (optional model override):

```json
{ "goal": "summarize my notes", "provider": "openai_codex", "model": "gpt-5.6-sol" }
```

Or Worker default:

```bash
export KERYX_DEFAULT_PROVIDER=openai_codex
```

Providers register **only when secrets resolve**. Selecting an unregistered provider fails closed.

List what is registered:

```bash
curl -sS -H "authorization: Bearer $TOKEN" "$KERYX_URL/v1/providers"
```

## Codex / ChatGPT subscription (`openai_codex`)

Preferred path for Plus/Pro usage via the same OAuth material as the Codex CLI.

```bash
codex login
./scripts/sync-chatgpt-codex-auth.sh
# writes ~/.config/keryx/chatgpt-access-token (+ chatgpt-account-id)

# ~/.config/keryx/env
CHATGPT_WEB_ACCESS_TOKEN_FILE=$HOME/.config/keryx/chatgpt-access-token
CHATGPT_ACCOUNT_ID_FILE=$HOME/.config/keryx/chatgpt-account-id
# defaults if unset: CHATGPT_CODEX_MODEL=gpt-5.6-sol, CHATGPT_CODEX_REASONING_EFFORT=low
# CHATGPT_CODEX_MODEL=gpt-5.6-sol
# CHATGPT_CODEX_REASONING_EFFORT=low
# optional alias: CHATGPT_CODEX_ACCESS_TOKEN_FILE=...
KERYX_DEFAULT_PROVIDER=openai_codex
```

| Variable | Purpose |
|----------|---------|
| `CHATGPT_WEB_ACCESS_TOKEN` / `*_FILE` | Codex OAuth access token |
| `CHATGPT_CODEX_ACCESS_TOKEN` / `*_FILE` | Optional alias for the same token |
| `CHATGPT_ACCOUNT_ID` / `*_FILE` | Account id (or JWT claim fallback) |
| `CHATGPT_CODEX_MODEL` | Default model id (**default `gpt-5.6-sol`**) |
| `CHATGPT_CODEX_MODELS` | Optional comma allowlist |
| `CHATGPT_CODEX_REASONING_EFFORT` | `low` \| `medium` \| `high` (**default `low`**) |
| `CHATGPT_CODEX_PATH` | Default `/backend-api/codex/responses` |
| `CHATGPT_WEB_BASE_URL` | Default `https://chatgpt.com` |

**Not** a Platform API key (`sk-…`). Token-only material registers `openai_codex`, not `openai_web` (unless `CHATGPT_WEB_FORCE=1`).

## ChatGPT browser session (`openai_web`)

| Variable | Purpose |
|----------|---------|
| `CHATGPT_WEB_COOKIE` / `*_FILE` | Full `Cookie` header (required to auto-register) |
| `CHATGPT_WEB_ACCESS_TOKEN` / `*_FILE` | Optional bearer with cookie |
| `CHATGPT_WEB_HEADERS_FILE` | Optional JSON extra headers |
| `CHATGPT_WEB_PATH` | Default `/backend-api/conversation` |
| `CHATGPT_WEB_MODEL` | Default `gpt-5.6-sol` |
| `CHATGPT_WEB_FORCE` | `1` to register token-only as `openai_web` |

## Grok web session (`grok_web`)

This is the **subscription / browser** path for Grok. Official paid API remains `grok` + `XAI_API_KEY`.

| Variable | Purpose |
|----------|---------|
| `GROK_WEB_COOKIE` / `*_FILE` | Full `Cookie` header (required) |
| `GROK_WEB_HEADERS_FILE` | Optional JSON (challenge/signature headers) |
| `GROK_WEB_BASE_URL` | Default `https://grok.com` |
| `GROK_WEB_PATH` | Default `/rest/app-chat/conversations/new` |
| `GROK_WEB_MODEL` | Default `grok-4.5` |
| `GROK_WEB_REASONING_EFFORT` | Default `medium` |
| `GROK_WEB_MODELS` | Optional allowlist |

### Exporting browser secrets

1. Log in to ChatGPT or Grok in a normal browser.
2. DevTools → Network; trigger a chat request.
3. Copy `Cookie` and any required custom headers (and Bearer if present).
4. Write to mode-`600` files under `~/.config/keryx/`.
5. Re-export when the session expires.

## Live verification (opt-in only)

```bash
export KERYX_LIVE_MODELS=1
# CHATGPT_WEB_* and/or GROK_WEB_* secrets
cargo test -p keryx-model --test live_consumer_web -- --ignored --nocapture
```

Default CI never requires consumer secrets (ADR 0009 / 0010).

## Related

- ADR 0010, ADR 0005
- Official API live path: `docs/deploy/live-model-verification.md`
- Tailnet edge: `docs/deploy/tailnet-edge.md`

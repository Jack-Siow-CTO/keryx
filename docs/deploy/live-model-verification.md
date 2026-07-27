# Opt-in live model verification (local)

Live Model provider calls are **never** part of default CI (ADR 0009).
This document is the local operator matrix for real credentials.

## Gate

| Gate | How |
|------|-----|
| Default CI / `cargo test` | Fixture Seam 1/2 only — no live network |
| Live verification | `KERYX_LIVE_MODELS=1` **and** real secrets present |

## Matrix

| Provider | Secrets | Test |
|----------|---------|------|
| `openai` | `OPENAI_API_KEY` | `live_openai_grok` |
| `grok` | `XAI_API_KEY` | `live_openai_grok` |
| `openai_codex` | Codex access token (sync script) | `live_consumer_web` (`live_openai_codex_completion`) |
| `openai_web` | cookie / force + token | `live_consumer_web` |
| `grok_web` | `GROK_WEB_COOKIE` | `live_consumer_web` |

## Official APIs

```bash
export KERYX_LIVE_MODELS=1
export OPENAI_API_KEY=sk-...          # or OPENAI_API_KEY_FILE
export OPENAI_MODEL=gpt-5.6-sol       # default if unset
export OPENAI_REASONING_EFFORT=low    # default if unset
export XAI_API_KEY=xai-...
export XAI_MODEL=grok-4.5             # default if unset
export XAI_REASONING_EFFORT=medium    # default if unset

cargo test -p keryx-model --test live_openai_grok -- --ignored --nocapture
```

## Codex subscription + consumer web

```bash
export KERYX_LIVE_MODELS=1
# after: codex login && ./scripts/sync-chatgpt-codex-auth.sh
export CHATGPT_WEB_ACCESS_TOKEN_FILE=$HOME/.config/keryx/chatgpt-access-token
export CHATGPT_ACCOUNT_ID_FILE=$HOME/.config/keryx/chatgpt-account-id
# optional GROK_WEB_COOKIE_FILE=...

cargo test -p keryx-model --test live_consumer_web -- --ignored --nocapture
```

## Worker path (local)

```bash
set -a && source ~/.config/keryx/env && set +a
keryx doctor
keryx
# other terminal:
export KERYX_OPERATOR_TOKEN=...
export KERYX_SMOKE_PROVIDER=openai_codex   # or openai / grok_web / …
# export KERYX_SMOKE_MODEL=gpt-5.6-sol
./scripts/smoke.sh

curl -sS -H "authorization: Bearer $KERYX_OPERATOR_TOKEN" \
  http://127.0.0.1:8787/v1/providers
```

## Separation from CI

| Layer | Live models? |
|-------|--------------|
| L1–L3 Seam 1 / unit / Seam 2 fixtures | No |
| L4 smoke (in-process test double) | No network |
| L5 live | Opt-in only |

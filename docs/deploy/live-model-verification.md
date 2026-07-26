# Opt-in live OpenAI and Grok verification

Live Model provider calls are **never** part of default CI (ADR 0009).
This document describes the explicit opt-in path for operators and developers
who want to exercise real OpenAI and Grok credentials.

## Mechanism

| Gate | How |
|------|-----|
| Default CI / `cargo test` | Fixture and Seam 1/2 only—no live network to OpenAI/xAI |
| Live verification | Env flag `KERYX_LIVE_MODELS=1` **and** real API keys present |

Ignored tests (or manual scripts) must skip cleanly when the flag or keys are
absent so merge gates never fail for missing credentials.

## Environment

```bash
export KERYX_LIVE_MODELS=1

# OpenAI
export OPENAI_API_KEY=sk-...          # or OPENAI_API_KEY_FILE=/path
export OPENAI_MODEL=gpt-4o-mini       # optional

# Grok (xAI)
export XAI_API_KEY=xai-...            # or XAI_API_KEY_FILE=/path
export XAI_MODEL=grok-3               # optional
```

## Run live tests

```bash
# From repo root; only runs live tests when opt-in is set.
KERYX_LIVE_MODELS=1 cargo test -p keryx-model --test live_openai_grok -- --ignored --nocapture
```

Without `KERYX_LIVE_MODELS=1`, the live test file is skipped / ignored and
`cargo test` remains green.

## What live path exercises

1. One **OpenAI** completion via the shared OpenAI-compatible client
2. One **Grok (xAI)** completion via the same client shape with xAI base URL

Failures here are **operator/credentials/provider** issues—not Seam 1 control-plane
or Seam 2 fixture regressions. Keep them separate from merge gates.

## Operator Worker path (optional)

With keys configured on the Worker host:

```bash
export KERYX_DEFAULT_PROVIDER=openai   # or grok
export OPENAI_API_KEY_FILE=/run/secrets/openai-api-key
# ...
keryx
```

Start a Run with `"provider":"openai"` or `"provider":"grok"` over the control plane.

## Separation from CI

| Layer | Command / gate | Live models? |
|-------|----------------|--------------|
| L1–L3 Seam 1 / unit | `cargo test` | No |
| Seam 2 fixtures | `cargo test -p keryx-model` | No (wiremock) |
| L4 smoke | worker smoke tests | Fake model only |
| L5 live | `KERYX_LIVE_MODELS=1` + ignored tests | Yes, opt-in only |

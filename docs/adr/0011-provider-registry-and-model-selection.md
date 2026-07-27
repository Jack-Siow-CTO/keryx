# Provider registry, real-only runtime, per-run model selection

## Decision

- **Accepted**: a single env-driven **provider registry** (`keryx_model::register_from_env`) registers only **real** model providers (`openai`, `grok`, `openai_codex`, `openai_web`, `grok_web`) when secrets are present.
- **Accepted**: **no runtime `fake` provider** on the Worker. Boot and doctor fail closed if no real provider registers or `KERYX_DEFAULT_PROVIDER` is invalid/`fake`.
- **Accepted**: per-run optional `model` on `POST .../runs` and `ModelRequest`, resolved as override → provider default → allowlist check.
- **Accepted**: `GET /v1/providers` returns non-secret descriptors (auth kind, default model, allowlist).
- **Rejected**: silent fake default; auto-scanning `~/.codex/auth.json` without an explicit `*_FILE` path; embedding browser login in the Worker.

## Rationale

Local operators pay for Codex, Platform API, or Grok access. A always-on `fake` path hid misconfiguration. Centralizing registration keeps Codex OAuth, API keys, and browser sessions extendable without touching the agent loop. Catalog + model field make multi-model selection maintainable for clients.

## Constraints

- `FakeModelProvider` remains for Seam 1 / in-process tests only.
- Consumer/Codex wires stay unofficial (ADR 0010); secrets never enter SQLite, SSE, or logs.
- When multiple providers register and `KERYX_DEFAULT_PROVIDER` is unset, fail with the available list (or auto-pick only if exactly one).

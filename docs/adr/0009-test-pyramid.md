# CI owns domain, adapters, and control plane; live models are opt-in

Merge gates run unit tests for domain/app, contract tests for SQLite/providers/tools with fixtures (no live network by default), and in-process control-plane tests for auth, concurrency, SSE, and cancel. Worker binary smoke runs on main/tag. Live OpenAI/Grok calls are explicit opt-in (nightly or manual). Chosen to protect reliability and security properties without making flaky paid model calls a merge blocker.

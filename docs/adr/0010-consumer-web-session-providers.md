# Opt-in consumer web session Model providers

Keryx may optionally use **operator-supplied** consumer web session material (cookies and/or access tokens from ChatGPT web or Grok web) as Model provider credentials, registered as `openai_web` and `grok_web` alongside official API-key providers.

## Decision

- **Accepted**: consumer web adapters fed only by env/secret files the operator exports from their browser (or a helper they run).
- **Rejected**: Worker-embedded browser login, interactive CAPTCHA, automatic cookie refresh UIs, and treating Tailscale membership or raw cookies as application Principal auth.
- Official API keys (`openai`, `grok`) remain first-class and preferred for reliability and CI.

## Rationale

Operators sometimes hold consumer subscriptions without (or instead of) API keys. Allowing operator-exported session secrets lets them drive Runs from those subscriptions without forking the agent loop. Keeping secrets out of the browser-automation path preserves a smaller Worker and clearer fail-closed boundaries.

## Constraints

- Endpoints and request shapes are **unofficial and break without notice**.
- Using consumer sites this way may violate vendor Terms of Service; the **operator owns that risk**.
- Sessions expire; missing/invalid session → clear `ModelError` / failed Run with **no secret echo**.
- Never store session secrets in SQLite, Transcript, SSE, or logs.
- Register web providers only when secrets are present; do not auto-select them over `fake`/API keys.
- Default CI and Seam 2 fixtures must not require live consumer network.

## Alternatives considered

- API keys only (previous ADR 0005 stance) — simpler, but blocks consumer-only operators.
- Worker Playwright login — heavy, fragile, larger security surface.
- Multi-user “bring your ChatGPT login” tenancy — out of scope for v1 personal Worker.

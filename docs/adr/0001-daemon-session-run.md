# Daemon product with Session + Run work model

Keryx v1 is a long-running Worker daemon, not a one-shot CLI product. Work is modeled as durable Sessions that host bounded Runs (each Run is one agent-loop execution). Chosen over pure fire-and-forget Jobs so multi-turn context and cancel/budget boundaries stay first-class without renaming later.

## Considered Options

- Task CLI only — too weak for always-on workers
- Job-only daemon — simple, but multi-turn reappears as ad-hoc state
- Session-only — blurs cancel/retry boundaries
- Session + Run (accepted) — Session for context, Run for execution lifecycle

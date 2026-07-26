# Keryx

**Keryx** (Greek κῆρυξ — *herald, messenger*) is a minimal, extensible, performant, secure, and reliable Rust-based agentic system in the spirit of Hermes-style agents: a lean messenger that takes intent, routes work, and returns results—without the weight of a full framework.

Built for personal and team use, designed to run efficiently on machines like [jack-agent-worker](https://github.com/Jack-Siow-CTO), where resource efficiency and operational control matter.

## Why Keryx

Most agent stacks optimize for demos and feature breadth. Keryx optimizes for **running as a serious system**:

| Principle | What it means |
|-----------|----------------|
| **Minimal** | Small core surface. No kitchen-sink runtime. Only what an agent needs to plan, act, and report. |
| **Extensible** | Clear extension points for tools, models, memory, and transports—without forking the core for every experiment. |
| **Performant** | Rust by default: low latency, tight memory, predictable CPU. Suitable for always-on worker hosts. |
| **Secure** | Least privilege, explicit tool boundaries, careful secret handling. Security is a design constraint, not a later audit. |
| **Reliable** | Fail closed, recover cleanly, log enough to debug. Prefer boring correctness over clever magic. |

## What it is

Keryx is a **Hermes-inspired, forked-type agentic system**—not a line-by-line port of any single project, but a purpose-built Rust agent shaped by the same ideas:

- An **agent loop** that can reason, call tools, and iterate toward a goal
- **Tool use** as first-class, sandboxed capabilities rather than unbounded shell access
- A **messenger** model: receive task → execute with constraints → deliver outcome
- A **small, hostable binary** you control end-to-end (build, deploy, observe)

It is intentionally scoped. Features land when they serve the principles above.

## Goals

- Ship a **production-minded personal agent** you can host on your own infrastructure
- Keep the core **understandable in one sitting** and safe to change
- Make extension **cheap** (new tools, providers, policies) without growing a monolith
- Prefer **static linking / simple deploy** patterns friendly to Linux workers
- Stay **honest about limits**: reliability and security beat autonomous freelancing

## Non-goals (for now)

- Matching every feature of larger multi-agent frameworks
- Opaque “do anything” automation without explicit policy
- Heavy plugin ecosystems before the core is solid
- Cloud lock-in or mandatory third-party control planes

## Status

**v1 Worker implementation in progress.** Hexagonal Rust workspace with control-plane Seam 1 tests (auth, Session/Run, SSE, concurrency, SQLite, workspace tools) and Seam 2 model fixtures (OpenAI/Grok, no live network in default CI).

```bash
cargo test --workspace
cargo run -p keryx-worker   # requires KERYX_OPERATOR_TOKEN; binds 127.0.0.1 only
```

**Model providers:** official API keys (`openai`, `grok`) and optional consumer web sessions (`openai_web`, `grok_web`) via operator-exported cookies/tokens — see `docs/deploy/consumer-web-sessions.md` and ADR 0010. Prefer API keys when available.

Deploy notes: `docs/deploy/tailnet-edge.md`. Live model opt-in: `docs/deploy/live-model-verification.md`.

## Name

*Keryx* is the herald—the official messenger. It fits a system whose job is to carry work, speak to tools and models, and bring back a clear answer.

## License

License to be decided when the first implementation lands.

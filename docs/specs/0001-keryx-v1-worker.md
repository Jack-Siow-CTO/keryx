# Keryx v1 Worker — Product & System Spec

Status: **ready-for-agent**  
Aligned with: `CONTEXT.md`, ADRs 0001–0009  
Test seams: (1) Control plane in-process, (2) Model provider contracts  

---

## Problem Statement

I want a personal agent I actually host and trust—not a demo framework.

Existing agent stacks are heavy, opaque, and hard to run as a serious always-on system on my own machine (`jack-agent-worker`). I need something I can reach from my Mac and phone, that uses the model subscriptions I already pay for (Grok / OpenAI), that can safely touch files in an allowlisted workspace, and that stays small enough to understand, extend, and operate without becoming a second full-time job.

Today I do not have a single, minimal, reliable Worker that:

- accepts my intent as durable work I can continue across turns,
- runs a bounded agent loop with clear cancel and budget limits,
- exposes a secure remote API over my private Tailnet (not the public internet),
- survives process restarts without losing conversation context,
- fails closed on auth and tool policy,
- is implemented in Rust for predictable performance and resource use on a worker host.

Without that, agent work stays fragmented across CLI tools, ad-hoc scripts, and systems I do not control end-to-end.

---

## Solution

Build **Keryx**: a Hermes-inspired, long-running **Worker** daemon in Rust.

From my perspective:

1. Keryx runs on `jack-agent-worker` as an always-on service.
2. My Mac and phone reach it over **Tailscale HTTPS** (same topology as existing worker services: edge proxy on Tailnet addresses only → loopback app).
3. I authenticate with an **operator token** (not “Tailscale means full power”).
4. I open a **Session**, start **Runs** toward goals, watch live progress (model tokens and tool steps), cancel if needed, and continue later on the same Session.
5. Runs may use **OpenAI** or **Grok (xAI)** and **workspace file tools** under Policy.
6. After a Worker restart, my Sessions and transcripts are still there; any in-flight Run is marked interrupted—I start a new Run to continue.

The system is intentionally minimal: thin core, compile-in adapters, clear extension points, strong tests at the control-plane seam—not a kitchen-sink multi-agent framework.

---

## User Stories

### Operator / host owner

1. As an operator, I want Keryx to run as a long-lived Worker on my Linux host, so that I do not relaunch a full agent stack for every request.
2. As an operator, I want the Worker to bind only to loopback, so that the app is not accidentally exposed on LAN or the public internet.
3. As an operator, I want a Tailnet-only HTTPS edge in front of the Worker, so that my Mac and phone can reach it the same way other worker services do.
4. As an operator, I want configuration for data directory, bind address, concurrency cap, tokens, model credentials, and workspace roots, so that I can deploy without recompiling.
5. As an operator, I want secrets (operator tokens, model API keys) loaded from the environment or secret files, so that they never live in the git repo.
6. As an operator, I want graceful shutdown, so that Active Runs are marked interrupted cleanly instead of leaving corrupt state.
7. As an operator, I want structured logs for control-plane and Run lifecycle events, so that I can debug production issues on the worker.
8. As an operator, I want a health endpoint on loopback, so that systemd or local checks can verify the Worker is up.
9. As an operator, I want low and predictable memory/CPU use under idle and light load, so that the Worker can share a machine with other services.
10. As an operator, I want a documented deploy shape matching jack-agent-worker conventions, so that ops knowledge transfers from existing services.

### Principal / authenticated client (Mac or phone)

11. As a Principal, I want to present a bearer operator token on every control-plane call, so that unauthenticated callers cannot drive the agent.
12. As a Principal, I want missing or invalid tokens to fail closed with no Session or Run side effects, so that auth bugs never become silent.
13. As a Principal, I want my actions attributed to a Principal identity derived from the token, so that per-device tokens can be added later without rewriting history.
14. As a Principal on a Mac, I want to create a Session over HTTPS on the Tailnet, so that I can start multi-turn work from my laptop.
15. As a Principal on a phone, I want the same control-plane API over HTTPS, so that I am not forced through SSH tunnels.
16. As a Principal, I want to list and get Sessions I am allowed to see, so that I can resume prior work.
17. As a Principal, I want to start a Run with a goal (and optional model/provider selection within policy), so that the agent acts toward a concrete objective.
18. As a Principal, I want to stream Run events while a Run is Active, so that I know the agent is progressing and what it is doing.
19. As a Principal, I want model token deltas in the stream when the provider supports streaming, so that responses feel live on phone and Mac.
20. As a Principal, I want tool started/finished events with summarized (non-secret) detail, so that I can see file operations without dumping entire file bodies by default.
21. As a Principal, I want to cancel an Active Run, so that runaway or wrong work stops promptly.
22. As a Principal, I want a durable Run record after completion, failure, cancel, or interrupt, so that I can inspect outcomes after disconnect.
23. As a Principal, I want to start a new Run on an existing Session, so that multi-turn work reuses the Transcript.
24. As a Principal, I want reconnect-friendly access (GET Run / Session after SSE drop), so that flaky mobile networks do not lose the result.
25. As a Principal, I want clear error messages when a Run is rejected (Active Run present, global cap full, policy deny), so that clients can act without guessing.

### Session and Run lifecycle

26. As a Principal, I want a Session to hold durable conversational and policy context, so that Runs share a coherent Transcript.
27. As a Principal, I want a Run to be one bounded agent-loop execution, so that cancel, budget, and retry boundaries are clear.
28. As a Principal, I want at most one Active Run per Session, so that Transcript and tool side effects stay coherent.
29. As a Principal, I want concurrent Runs across different Sessions up to a global cap, so that I can run separate workstreams without a second Worker process.
30. As a Principal, I want a rejected or queued policy when Session already has an Active Run, so that clients never observe two writers on one Transcript.
31. As a Principal, I want budgets for time, tokens, and tool-call count per Run, so that cost and hang risk stay bounded.
32. As a Principal, I want budget nearing/exceeded signals, so that I understand why a Run stopped.
33. As a Principal, I want an interrupted status when the Worker dies mid-Run, so that I know to continue with a new Run rather than waiting forever.
34. As a Principal, I want Session Transcript preserved across Worker restarts, so that phone and Mac clients survive deploys and reboots.
35. As an operator, I want local SQLite durability on the Worker host, so that I do not operate Postgres for a single-operator agent.

### Agent loop, models, and tools

36. As a Principal, I want the agent loop to alternate model reasoning and tool use until stop conditions, so that goals can complete multi-step work.
37. As a Principal, I want to use OpenAI as a Model provider, so that my OpenAI subscription/API access is usable from Keryx.
38. As a Principal, I want to use Grok (xAI) as a Model provider, so that my Grok subscription/API access is usable from Keryx.
39. As a Principal, I want provider credentials configured on the Worker (not scraped browser sessions), so that automation stays reliable and legitimate.
40. As a Principal, I want a shared OpenAI-compatible client shape across providers where possible, so that behavior is consistent and maintenance cost stays low.
41. As a Principal, I want the model to request workspace file reads, so that the agent can inspect allowlisted files.
42. As a Principal, I want the model to request workspace file writes, so that the agent can produce or edit allowlisted files.
43. As a Principal, I want path confinement under Workspace roots, so that `../` and absolute escapes cannot leave the jail.
44. As a Principal, I want tool allowlists on Session/Run Policy, so that only intended Tools are invocable.
45. As a Principal, I want default-deny Policy, so that unknown tools and paths fail closed.
46. As a Principal, I want secrets redacted in streamed tool arguments and results, so that tokens and keys do not leak to clients or logs carelessly.
47. As a Principal, I want Session Transcript to include messages and tool results needed for subsequent Runs, so that multi-turn continuity works without external memory products.
48. As an operator, I want shell/exec and browser tools deferred, so that v1 blast radius stays limited while the core hardens.

### Extensibility and codebase health

49. As a developer, I want a hexagonal Cargo workspace (domain, app, adapters, worker binary), so that boundaries stay enforceable.
50. As a developer, I want domain types named after the glossary (Session, Run, Principal, Policy, Tool, Transcript, Run event), so that code and docs share one language.
51. As a developer, I want adapters compile-in via registries, so that we avoid dynamic plugin loading security and packaging tax in v1.
52. As a developer, I want the worker binary as the only composition root, so that wiring stays explicit and testable.
53. As a developer, I want stable ports for Model provider and Tool, so that new providers/tools can land without rewriting the agent loop.
54. As a developer, I want transport concerns (HTTP/SSE) outside domain logic, so that the core stays free of framework churn.
55. As a future client author, I want a documented control-plane contract (REST-ish JSON + SSE), so that Mac, phone, and CLI clients can be built independently.

### Reliability, security, and quality

56. As an operator, I want Tailscale treated as reachability only, so that mesh membership is not confused with application authorization.
57. As an operator, I want no public bind and no Serve/Funnel product path, so that exposure matches the worker security contract.
58. As a Principal, I want cancel to clear Active Run state, so that Sessions do not get permanently stuck.
59. As a developer, I want CI to run without live model network calls, so that merges are not blocked by flaky paid APIs.
60. As a developer, I want optional live model tests, so that real OpenAI/Grok integration can still be verified deliberately.
61. As a developer, I want control-plane tests with fake models to prove auth, concurrency, SSE, policy, and cancel, so that system correctness is cheap to protect.
62. As a developer, I want model adapter contract tests against HTTP fixtures, so that request shaping and stream parsing stay correct for OpenAI and Grok.
63. As an operator, I want Worker binary smoke on main/tag, so that boot/bind regressions are caught before deploy.
64. As a Principal, I want deterministic rejection when global concurrency is exhausted, so that overload is visible rather than silent queue infinity (unless explicit queue policy is added later).

### Observability of Run events

65. As a Principal, I want a `run.started` event, so that clients know execution began.
66. As a Principal, I want `model.started` / `model.delta` / `model.finished` events, so that model phases are visible.
67. As a Principal, I want `tool.started` / `tool.finished` events, so that tool phases are visible.
68. As a Principal, I want terminal `run.completed` / `run.failed` / `run.cancelled` events, so that clients can close the stream correctly.
69. As a Principal, I want optional `run.budget` warnings, so that approaching limits are visible before hard stop.
70. As a Principal, I want the event taxonomy stable and small, so that clients do not depend on raw provider payloads.

---

## Implementation Decisions

These decisions encode ADRs 0001–0009 and the aligned grill session. Vocabulary matches `CONTEXT.md`.

### Product shape

- Keryx v1 is a **long-running Worker daemon**, not a one-shot CLI product. A thin CLI may later act as a client; it is not the spine.
- Work is modeled as **Session + Run**: Session is durable context; Run is one bounded agent-loop execution (cancelable, budgeted).
- **Active Run**: at most one per Session. Across Sessions, parallelism is allowed up to a **configured global cap**.
- Clients: Mac and phone over Tailnet; Worker hosted on jack-agent-worker-class hosts.

### Network topology

- **Control plane** binds **loopback only** (HTTP/JSON).
- **Run observation** uses **SSE** (not WebSocket-first).
- Commands (create Session, start Run, cancel Run, get resources) are request/response HTTP.
- **Edge**: Tailnet-bound reverse proxy (Caddy on Tailscale IPs only) terminates HTTPS and forwards to loopback—same topology as existing T3-style worker services.
- No public internet listeners; no Tailscale Serve/Funnel as the product path.
- Optional same-host UDS may be added later without changing the domain model; not required for v1 remote clients.

### Authentication and identity

- Every control-plane call requires a configured **bearer operator token** (allowlist of tokens supported).
- Tailscale provides **reachability only**, not application authorization.
- Domain records a **Principal** derived from the presenting token on Session/Run creation.
- v1 may use one shared operator token across devices; design must not prevent per-device tokens later.
- Anonymous loopback trust is rejected for a tool-capable agent.

### Capability surface (v1)

**Core owns:**

- Agent loop and Session/Run lifecycle
- Tool interface and invocation protocol
- Policy enforcement (allowlists, workspace roots, budgets, cancel)
- Run event log / emission

**v1 Model providers:**

- **OpenAI** and **Grok (xAI)** first
- Credentials = configured API access (subscription entitlements expressed as API keys/base URLs/model IDs), **not** browser-session scraping
- Prefer a shared OpenAI-compatible HTTP client shape with provider-specific configuration

**v1 Tools:**

- Workspace **file read** and **file write** under allowlisted **Workspace** roots
- Path confinement mandatory; default deny

**v1 Memory:**

- **Session Transcript** only (messages + tool results for that Session)
- No vector DB / long-term memory product in v1

**Deferred:**

- Shell/exec, browser tools
- Dynamic `.so` plugins
- Full multi-user accounts / OAuth

### Durability

- Local **SQLite** on the Worker for Sessions, Transcripts, and Run records.
- Active Runs are **not** mid-loop resumed after crash.
- On process death: Active Run → **failed/interrupted** in the Run record; client continues via **new Run** on same Session.
- Data lives under a configured data directory on the host.

### Run events (SSE taxonomy)

Stable, small set (names illustrative but contract should stay this shape):

| Event | Role |
|-------|------|
| `run.started` | Active Run begins |
| `model.started` / `model.delta` / `model.finished` | Provider call; deltas when streaming |
| `tool.started` / `tool.finished` | Tool boundaries; summarized args/results |
| `run.budget` | Optional soft warnings |
| `run.completed` / `run.failed` / `run.cancelled` | Terminal |

Rules:

- Append-only observations for one Run
- Transcript is durable conversational truth after persist
- Redact secrets on the wire
- Clients may GET Run record after the fact (reconnect)

### Module architecture (hexagonal workspace)

Cargo workspace with hard dependency direction:

```text
domain ← app ← {storage, model, tools, api} ← worker
```

| Module | Responsibility |
|--------|----------------|
| **domain** | Session, Run, Principal, Policy, Tool port, Run events, pure rules |
| **app** | Agent loop orchestration, budgets, concurrency (Active Run + global cap), cancel coordination |
| **storage** | SQLite persistence for Session, Transcript, Run record |
| **model** | OpenAI + Grok adapters |
| **tools** | Workspace filesystem tools |
| **api** | HTTP control plane, SSE, auth middleware |
| **worker** | Binary composition root: config load, bind loopback, graceful shutdown |

Rules:

- Domain has no HTTP/SQLite/provider SDKs
- App depends on ports/traits, not concrete `reqwest`/`sqlx` types
- Adapters implement ports; only worker wires them
- Adapters **compile in** via registries
- Map domain errors to transport errors only at the API boundary
- Prefer deep modules (e.g. execute Run hides loop internals)

### Control-plane API (behavioral contract)

Exact routes may be chosen at implementation time; behavior must include:

- Authenticate all mutating and sensitive reads with bearer token
- Create / get / list Sessions
- Start Run on a Session (reject or clearly signal if Active Run exists; enforce global cap)
- Cancel Active Run
- Get Run record (status, budgets consumed, result/failure, enough event history for debug)
- Subscribe to SSE Run event stream for an Active (or recently active) Run
- Health check suitable for local supervision

### Configuration (minimum)

- Loopback bind address/port
- Data directory (SQLite)
- Operator token allowlist
- Global Run concurrency cap
- Default budgets (time, tokens, tool calls)
- Model provider configs (OpenAI and Grok: base URL, API key, default model id)
- Workspace root allowlist
- Tool allowlist defaults

### Edge (ops, outside core domain logic)

- Document and support deploy with Tailnet-only reverse proxy to loopback
- Edge is not part of agent domain logic; misconfiguration of public bind is a fail-closed ops concern
- Align with jack-agent-worker posture: SSH administrative only; app data path is Tailnet HTTPS

### Implementation non-negotiables

- Fail closed on auth, policy deny, path escape
- No mid-tool exactly-once resume in v1
- No public bind in default configuration
- Do not treat Tailscale identity as sufficient app auth in v1

---

## Testing Decisions

### What makes a good test

- Assert **external behavior** at agreed seams, not private helper structure.
- Prefer the **highest seam** that still makes failures local and fast.
- No live OpenAI/Grok network in default CI.
- Tests should catch: auth holes, double Active Run, cap overload, path escape, budget stop, cancel races, SSE contract breaks, SQLite restart loss, provider stream parse bugs.
- Avoid LLM answer-quality evals as merge gates in v1.

### Confirmed seams

#### Seam 1 — Control plane (primary)

In-process Worker control plane (HTTP + SSE + auth + app wiring) with:

- Fake Model provider (scripted completions, tool-calls, stream deltas)
- Temporary SQLite data directory
- Temporary Workspace root
- Configured operator token(s)

Covers: Principal/token fail-closed, Session/Run lifecycle, Active Run exclusivity, global cap, agent loop via fake model, Policy (allowlist, budgets, path jail), workspace fs tools, SSE event order, cancel, durability after reopen of same store.

This is the primary merge-gate confidence surface (~bulk of L1/L3 and much of L2 tool/storage behavior).

#### Seam 2 — Model provider contract (secondary)

OpenAI and Grok adapters tested against **fixture/mock HTTP** (no live keys):

- Request shaping (URL, auth header, model id)
- Stream parse → deltas / finished
- Error mapping

Live provider calls remain **explicit opt-in** (manual/nightly), never required to merge.

### Layers (ADR 0009)

| Layer | Gate |
|-------|------|
| L1 Domain/app behavior | Prefer Seam 1; pure domain tests only when API tests are awkward for pure state-machine edges |
| L2 Adapter contracts | Seam 2 + storage/fs behavior via Seam 1 |
| L3 Control plane | Seam 1 |
| L4 Worker binary smoke | On main/tag: boot, bind loopback, health, one fake-model Run |
| L5 Live models | Opt-in only |

### Modules under test (by behavior)

- Domain/app rules: Session/Run transitions, concurrency, budgets, Policy
- Storage: persist/reopen Transcript and Run records; interrupted Active Run
- Tools: path confinement and read/write under Workspace
- Model adapters: OpenAI + Grok fixtures
- API: auth middleware, SSE taxonomy, cancel, rejection modes

### Prior art

- Greenfield repository: **no existing test suite**. Establish Seam 1 harness first, then Seam 2 fixtures.
- Follow ADR 0009; do not invent additional product seams (Caddy E2E, phone UI) for v1 CI.

### Tooling expectations

- Workspace `cargo test` in CI
- `clippy` (deny warnings) and `rustfmt` in CI
- Feature or env flag for live model tests (e.g. opt-in env)

---

## Out of Scope

For this v1 spec, the following are explicitly out of scope:

- Shell/exec tools, browser automation tools, Android/desktop control integration
- Dynamic plugin loading (`.so` / script plugins)
- Mid-loop checkpoint resume of Active Runs / exactly-once tool replay
- Multi-user accounts, OAuth, team RBAC, multi-tenant SaaS control plane
- Queue-primary control plane (NATS/Redis/SQS as the interactive API)
- gRPC-first public API
- Public internet exposure, Tailscale Funnel/Serve as product defaults
- Matching feature breadth of large multi-agent frameworks
- Long-term vector memory / external knowledge bases
- First-party polished Mac/phone UI apps (API must enable them; shipping UI is separate)
- iOS/Android store clients as deliverables of this spec
- Billing, usage metering dashboards, multi-host orchestration
- Using consumer web session cookies instead of API credentials for OpenAI/Grok
- LLM qualitative eval harness as a merge requirement

---

## Further Notes

### Source of truth for language and decisions

| Doc | Use |
|-----|-----|
| `CONTEXT.md` | Ubiquitous language; prefer these terms in code and tickets |
| `docs/adr/0001`–`0009` | Hard architectural decisions and rejected alternatives |
| This spec | Implementable product behavior and test seams |

If code and glossary disagree, **fix the glossary or amend it deliberately**—do not invent synonyms (Job/Task/Conversation) for Session/Run.

### Suggested implementation order (non-binding guidance)

1. Workspace skeleton + domain types + pure rules  
2. App loop with fake Model provider + Policy  
3. SQLite storage + restart semantics  
4. Workspace fs tools + path jail  
5. HTTP control plane + auth + SSE (Seam 1 green)  
6. OpenAI + Grok adapters (Seam 2 green)  
7. Worker binary + config + graceful shutdown  
8. Document Tailnet edge deploy; optional smoke  
9. Opt-in live model verification  

### Open parameters (defaults allowed at implementer discretion if documented)

- Exact global concurrency default (recommend starting low, e.g. 1–2)
- Exact default budgets
- Exact HTTP route paths and JSON field names (must remain consistent once published)
- Choice of Rust HTTP stack and SQLite crate (keep domain free of them)
- Whether same-Session second start_run is **hard reject** vs **short queue**—document the choice; prefer simple hard reject for v1 unless queue is trivial and tested

### Relationship to Hermes

Keryx is **Hermes-inspired**, not a line-by-line port of any single upstream. Minimal messenger semantics (intent → constrained action → outcome) matter more than feature parity with any named framework.

### Success criteria for “v1 done”

- Worker runs on loopback with token auth  
- Mac/phone can use Tailnet HTTPS edge to create Session, start Run, stream events, cancel, read results  
- OpenAI and Grok providers work with configured API credentials  
- Workspace file tools obey Policy and path jail  
- SQLite survives restart; interrupted Runs are explicit  
- Seam 1 + Seam 2 tests pass in CI without live model calls  
- No public bind in supported deploy docs  

---

## Appendix — ADR index

| ADR | Decision |
|-----|----------|
| 0001 | Daemon product; Session + Run work model |
| 0002 | Session-serial Runs; capped multi-Session parallel |
| 0003 | Loopback HTTP + Tailnet HTTPS edge |
| 0004 | Operator token auth + Principal in domain |
| 0005 | Thin core; OpenAI + Grok; fs tools; no exec |
| 0006 | SQLite durable Sessions; no mid-loop resume |
| 0007 | SSE Run events with milestones + token deltas |
| 0008 | Small hexagonal Cargo workspace |
| 0009 | CI pyramid; live models opt-in |

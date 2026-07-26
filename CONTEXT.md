# Keryx

Keryx is a Hermes-inspired agentic messenger: a long-running worker that accepts intent, runs bounded agent loops with tools, and returns outcomes.

## Language

**Worker**:
The long-running Keryx process that accepts work, enforces policy, and delivers outcomes.
_Avoid_: Server, host, agent runtime (when referring to the process itself)

**Session**:
Durable conversational and policy context that may span multiple Runs (messages, memory pointers, constraints).
_Avoid_: Conversation, chat, thread (unless UI-facing labels)

**Run**:
One bounded execution of the agent loop toward a goal (or until a stop policy fires). Cancelable and budgeted independently of other Runs.
_Avoid_: Job, task, step, turn (a turn may be *inside* a Run)

**Agent loop**:
The iterative cycle within a Run: model reasoning → optional tool calls → observation → continue or stop.
_Avoid_: Pipeline, workflow (reserved for multi-step external orchestration if introduced later)

**Active Run**:
The single Run currently executing for a given Session. A Session admits at most one Active Run at a time; further Runs wait or are rejected per policy.
_Avoid_: Current job, in-flight task

**Control plane**:
The Worker-facing API that creates Sessions, starts and cancels Runs, and streams Run events and results.
_Avoid_: Frontend, gateway (the gateway is the edge, not the control plane)

**Edge**:
The Tailnet-only reverse proxy that terminates HTTPS and forwards to the Worker's loopback control plane. Not part of the agent domain logic.
_Avoid_: Public API, tunnel, Serve/Funnel

**Principal**:
The authenticated identity that initiates control-plane actions (create Session, start or cancel Run). v1 may map many devices to one operator token; the domain still records which Principal acted.
_Avoid_: User, account, tenant (until multi-human product needs them)

**Model provider**:
An adapter that turns a Run's conversation state into model completions (and optional tool-call requests). v1 ships OpenAI and Grok (xAI) providers first.
_Avoid_: LLM, backend, brain

**Tool**:
A named, policy-gated capability a Run may invoke (for example workspace file read/write). Concrete tools are adapters; the core owns the interface and enforcement.
_Avoid_: Function, skill, plugin (plugin reserved for future dynamic loading if ever needed)

**Policy**:
The constraints applied to a Session or Run: tool allowlists, workspace roots, budgets (time, tokens, tool calls), and cancel rules.
_Avoid_: Guardrails, permissions (OS permissions are separate)

**Workspace**:
An allowlisted filesystem root (or roots) within which file tools may operate for a Session/Run.
_Avoid_: Sandbox, project (project may mean a git repo later)

**Transcript**:
The ordered Session history of messages and tool results available to subsequent Runs.
_Avoid_: Chat log, context window (context window is a model limit, not stored state)

**Run record**:
The durable metadata and outcome of a Run (status, budgets consumed, result or failure reason, event history for debug/replay of observations). An interrupted Active Run is recorded as failed/interrupted, not resumed mid-loop.
_Avoid_: Job row, execution log (alone)

**Run event**:
An append-only observation emitted while a Run is Active (model progress, tool boundaries, budgets, terminal status). Clients consume Run events over the control plane stream; the Transcript remains the durable conversational truth.
_Avoid_: Log line, webhook, notification

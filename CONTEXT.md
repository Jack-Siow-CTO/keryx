# Keryx

Keryx is a personal agent OS: a long-running Worker that accepts intent from a control plane and Gateways, runs bounded agent loops with tools, memory, and skills, and returns outcomes under Policy.

## Language

**Worker**:
The long-running Keryx process that accepts work, enforces policy, and delivers outcomes.
_Avoid_: Server, host, agent runtime (when referring to the process itself)

**Session**:
Durable conversational and policy context that may span multiple Runs (messages, memory pointers, constraints). In Console, one Session is one chat thread in the messaging list (title, last-message preview, attention badges, Active Run indicators are UI over this aggregate—not a second domain object). New chat creates an empty Session under operator defaults; the first Send starts the first root Run—no mandatory create wizard.
_Avoid_: Conversation (prefer Session in domain speech), chat/thread (UI-facing labels for Session only), channel (UI label only)

**Run**:
One bounded execution of the agent loop toward a goal (or until a stop policy fires). Cancelable and budgeted independently of other Runs. In Console, sending a message in an idle Session starts a root Run with that message as the goal; an Active root Run never accepts a silent second root Run—composer exposes wait / cancel / cancel-and-re-run (steer only if the control plane supports it).
_Avoid_: Job, task, step, turn (a turn may be *inside* a Run)

**Child Run**:
A Run spawned by a parent Run to perform delegated work with its own transcript slice, budgets, and tool Policy subset.
_Avoid_: Subagent process, thread, fork (unless referring to OS processes)

**Agent loop**:
The iterative cycle within a Run: model reasoning → optional tool calls → observation → continue or stop.
_Avoid_: Pipeline, workflow (reserved for multi-step external orchestration if introduced later)

**Active Run**:
The single root Run currently executing for a given Session. Child Runs may run under that root; the Session still admits only one Active root Run at a time.
_Avoid_: Current job, in-flight task

**Control plane**:
The Worker-facing API that creates Sessions, starts and cancels Runs, manages Approvals, Schedules, and Memory, and streams Run events and results. System of record for work.
_Avoid_: Frontend, gateway (the Gateway is messaging, not the control plane)

**Console**:
The first-party graphical Operator client (mobile and desktop) that drives the control plane as a Principal. Product metaphor is a messaging client: the home surface is a chat list of Sessions (plus thin system rows for cross-Session attention), not a dual-rail operator cockpit and not a Gateway or agent-loop host. Layout is messenger master–detail (list | thread on wide; stacked on narrow), with an optional contextual third pane for Session info or artifacts—not a permanent Inbox rail or always-on activity column. Global operator surfaces (Memory, Skills, Schedules, Settings) live under a profile/overflow hub; per-Session Policy and Workspace live under Session info on the open chat.
_Avoid_: App (alone), frontend, dashboard, Slack client, chat client (as the product name), Gateway

**Inbox**:
The control-plane read projection of cross-Session items that need the Principal now (pending Approvals, failed or interrupted Runs, and similar actionable alerts). In Console it surfaces as a thin system row or service chat in the chat list (and as in-thread attention), not as a peer navigation rail equal to Sessions, and not as a durable notification log or separate write aggregate—items view existing Approvals and Runs.
_Avoid_: Notifications feed (alone), Activity tab (UI chrome), queue, channel, notification (as a domain entity)

**Edge**:
The Tailnet-only reverse proxy that terminates HTTPS and forwards to the Worker's loopback control plane. Not part of the agent domain logic.
_Avoid_: Public API, tunnel, Serve/Funnel, Gateway

**Gateway**:
A messaging adapter that maps external chat platforms to control-plane Sessions and Runs (and delivers outcomes back).
_Avoid_: Edge, webhook service (alone), bot framework

**Run origin**:
The channel that initiated a Run (control plane, a Gateway platform, or a Schedule). Policy and Approval requirements depend on origin.
_Avoid_: Source, client type, transport

**Principal**:
The authenticated identity that initiates control-plane actions (create Session, start or cancel Run, approve high-blast work). v1/v2 may map many devices to one operator token; the domain still records which Principal acted.
_Avoid_: User, account, tenant (until multi-human product needs them)

**Approval**:
An operator decision required before a high-blast action proceeds (for example exec, skill auto-apply outside trusted context, computer-use outside allowlists). In Console: always discoverable via the Inbox (“Needs you”) system row, and when the related Session is open also as a sticky in-thread action card—not list-only, not modal-only as the default.
_Avoid_: Permission prompt, OAuth consent

**Model provider**:
An adapter that turns a Run's conversation state into model completions (and optional tool-call requests).
_Avoid_: LLM, backend, brain

**Tool**:
A named, policy-gated executable capability a Run may invoke (for example workspace file read, terminal, browser, or a namespaced MCP tool). Concrete tools are adapters; the core owns the interface and enforcement.
_Avoid_: Function, plugin (plugin reserved for future dynamic native loading if ever needed), capability pack, integration (when meaning an installable product unit)

**MCP server**:
An operator-configured external Model Context Protocol peer that contributes Tools to the Worker under fixed namespaces and Policy. Long-tail product integrations (mail, chat APIs, home automation, …) enter as MCP servers, not first-party core Tools.
_Avoid_: Plugin, capability pack, native extension

**Skill**:
A versioned, load-on-demand procedure or knowledge package the agent may inject into a Run's context. Distinct from a Tool (skills are data; tools are executable). Skills may document how to use MCP Tools; they do not grant executable power.
_Avoid_: Plugin, prompt pack, SOP (unless UI-facing)

**Memory**:
Durable, curated knowledge retained across Sessions (facts, preferences, project state). Distinct from Transcript.
_Avoid_: Vector store, embeddings, RAG (implementation); chat history

**Transcript**:
The ordered Session history of messages available to subsequent Runs and to Console. User and assistant entries are prose; tool entries are compact structured observations (name, status, summary, artifact references), not unbounded dumps. In Console the thread is layered: prose is first-class chat messages; tool/Child-Run/status participation is collapsible activity in the same timeline—not a flat bubble log of every event, and not a separate Chat vs Activity tab as the default. Distinct from live Run events and from Memory.
_Avoid_: Chat log (as domain term), context window (context window is a model limit, not stored state), Memory, event stream

**Soul**:
The operator-level personality and standing instructions document loaded into Runs.
_Avoid_: System prompt (alone), persona pack, jailbreak file

**Context file**:
A project- or workspace-scoped document automatically or on demand attached to Runs for that Workspace.
_Avoid_: README dump, .env, Soul

**Policy**:
The constraints applied to a Session or Run: tool allowlists (including explicit MCP tool names), workspace roots, budgets (time, tokens, tool calls), Run origin rules, Approval rules, and cancel rules. Discovering or enabling an MCP server does not by itself authorize its Tools.
_Avoid_: Guardrails, permissions (OS permissions are separate)

**Workspace**:
An allowlisted filesystem root (or roots) within which file tools may operate for a Session/Run.
_Avoid_: Sandbox (sandbox is an exec isolation backend), project (project may mean a git repo later)

**Schedule**:
A durable trigger that starts Runs on a cadence or at a time, with a frozen Policy snapshot and Run origin `schedule`.
_Avoid_: Cron job (OS cron), timer, alarm

**Run record**:
The durable metadata and outcome of a Run (status, budgets consumed, result or failure reason, event history for debug/replay of observations). An interrupted Active Run is recorded as failed/interrupted, not resumed mid-loop.
_Avoid_: Job row, execution log (alone)

**Run event**:
An append-only observation emitted while a Run is Active (model progress, tool boundaries, budgets, terminal status). Clients consume Run events over the control plane stream; the Transcript remains the durable conversational truth.
_Avoid_: Log line, webhook, notification

**Artifact**:
Durable bytes produced during a Run (for example terminal capture, patch/diff, or screenshot) stored by the Worker and referenced from Transcript or Run events. Fetched through the control plane under Principal auth; not a Workspace file path and not an inline Transcript dump.
_Avoid_: Attachment (chat product), blob (implementation), S3 object, media file (alone)

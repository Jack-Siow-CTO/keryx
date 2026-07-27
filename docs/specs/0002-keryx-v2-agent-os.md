# Keryx v2 — Personal Agent OS

Status: **ready-for-agent**  
Aligned with: `CONTEXT.md`, ADRs 0001–0012  
Test seams: (1) Control plane in-process (extended), (2) Model provider contracts, (3) Gateway adapter contracts  
Supersedes product *direction* of thin v1-only surface (ADR 0005 deferred items); v1 runtime remains valid until this spec is implemented.

---

## Problem Statement

I already have a minimal Keryx **Worker**: durable **Sessions** and **Runs**, operator-token auth, SSE progress, SQLite survival, OpenAI/Grok (and optional consumer) models, and path-jailed file read/write. That spine works—but it is not yet a personal agent I can live in.

Compared with a Hermes-class personal agent OS, Keryx cannot yet:

- run shell work on my worker host under real isolation and Approval,
- browse or drive a GUI when files and search are not enough,
- remember facts across Sessions or grow reusable Skills,
- take work from Telegram/Discord while I am away from the control plane,
- schedule unattended Runs,
- delegate parallel Child Runs,
- extend via MCP without forking the core,
- or give me a first-party CLI/TUI that feels like an agent OS rather than a curl cookbook.

I do not want a second opaque Python mega-framework, multi-tenant SaaS, or a rewrite that abandons Session/Run, Policy, and Tailnet-only reachability. I want **my** Worker on `jack-agent-worker` to become a **personal agent OS**: control-plane system of record, Gateways as adapters, fail-closed trust with Approvals, and Hermes-inspired breadth where we deliberately chose it—not infinite plugin zoo parity.

Without v2, I either stay stuck at “file-editing daemon with an API,” or I abandon Keryx for a heavier stack I do not control end-to-end.

---

## Solution

Ship **Keryx v2**: evolve the existing Rust Worker into a personal agent OS on the same hexagonal spine.

From my perspective:

1. The **control plane** remains how all durable work is created, inspected, cancelled, approved, scheduled, and streamed (HTTP + SSE, loopback; Tailnet **Edge** unchanged).
2. **Telegram** and **Discord** **Gateways** turn chat into Sessions/Runs with **Run origin** `gateway:*` and **reduced Policy** unless I escalate.
3. Runs can use a broad **Tool** set: richer files, local/Docker terminal, web search/extract, isolated browser, isolated computer-use, Memory, Skills, todo/clarify/session_search, fenced in-process `execute_code`, MCP tools.
4. I get **Memory** (curated facts + FTS), **Skills** (document packages + always-on learning loop with trusted auto-apply only), **Soul** + workspace **Context files**, and **Schedules** that fire Runs unattended.
5. A parent Run may spawn **Child Runs** for delegation without breaking one Active **root** Run per Session.
6. High-blast actions hit an **Approval** queue; I approve from CLI/TUI/control plane.
7. **CLI + full TUI** are first-party clients of the control plane (desktop app is future).
8. **MCP client + server** extend and expose capabilities under the same Policy and Principal rules.
9. After restart, Sessions, Transcripts, Memory, Skills metadata, Schedules, Approvals, and Run records remain; interrupted Active Runs are not mid-loop resumed.

Hermes is inspiration and a capability checklist—not a line-by-line port. Long-tail integrations (Home Assistant, Spotify, …) arrive via MCP/Skills, not first-party core.

---

## User Stories

### Operator / host owner

1. As an operator, I want the Worker to remain a long-lived daemon on my Linux host, so that the agent OS is always available without relaunching a full stack.
2. As an operator, I want the control plane to bind only to loopback, so that LAN/public exposure is not accidental.
3. As an operator, I want the Tailnet Edge topology unchanged (HTTPS on Tailscale IPs → loopback), so that Mac/phone reachability matches existing worker services.
4. As an operator, I want secrets (operator tokens, model keys, bot tokens, MCP credentials) loaded from env or secret files, so that nothing secret lives in git.
5. As an operator, I want configuration for data dir, concurrency, default budgets, Workspace roots, exec backends, Gateway enablement, skills root, Soul path, and MCP servers, so that I can deploy without recompiling for routine changes.
6. As an operator, I want graceful shutdown that marks Active root and Child Runs interrupted cleanly, so that durability stays coherent.
7. As an operator, I want structured logs for control-plane, Gateway, Schedule fire, and Approval lifecycle events, so that production debugging is possible.
8. As an operator, I want a health endpoint on loopback, so that systemd or local checks can supervise the Worker.
9. As an operator, I want Docker available as an exec backend default for reduced-Policy Runs, so that gateway-origin work cannot casually own the host user.
10. As an operator, I want local exec available for trusted control-plane origin under Policy and Approval, so that I can administer the worker itself.
11. As an operator, I want an isolated agent desktop session for computer-use, so that the agent does not drive my personal interactive session by default.
12. As an operator, I want isolated browser profiles for browser tools, so that agent cookies stay separate from my daily browser.
13. As an operator, I want MCP server listen configuration fail closed without operator auth bypass, so that exporting tools does not open an unauthenticated backdoor.
14. As an operator, I want documented install/upgrade paths that extend v1 without discarding SQLite data, so that Sessions and Memory survive the v2 rollout.
15. As an operator, I want `keryx doctor` to report v2 readiness (Gateways, Docker, skills root, Soul, MCP), so that misconfiguration is visible before first use.

### Principal / authenticated client

16. As a Principal, I want every control-plane call to require a bearer operator token, so that unauthenticated callers cannot drive the agent OS.
17. As a Principal, I want invalid tokens to fail closed with no Session/Run/Memory/Schedule side effects, so that auth bugs never become silent.
18. As a Principal, I want actions attributed to a Principal derived from the token, so that per-device tokens remain possible later.
19. As a Principal, I want to create, list, and get Sessions, so that I can resume multi-turn work.
20. As a Principal, I want to start a Run with a goal and optional provider/model selection, so that work is explicit and model choice stays under policy.
21. As a Principal, I want Run origin recorded on every Run, so that Policy and audit can depend on how work started.
22. As a Principal, I want to stream Run events (including Child Run linkage, Approval waits, tool and model phases), so that progress is visible live.
23. As a Principal, I want to cancel an Active root Run and have Child Runs cancel with it, so that runaway trees stop promptly.
24. As a Principal, I want durable Run records after complete/fail/cancel/interrupt, so that reconnects still show outcomes.
25. As a Principal, I want clear rejection when a Session already has an Active root Run or the global cap is full, so that clients can act without guessing.
26. As a Principal, I want budgets for time, tokens, and tool calls on root and Child Runs, so that cost and hang risk stay bounded.
27. As a Principal, I want budget nearing/exceeded signals, so that I understand why work stopped.
28. As a Principal, I want secrets redacted in streamed tool arguments and results, so that tokens do not leak to clients or logs carelessly.

### Session, Run, and Child Runs

29. As a Principal, I want a Session to hold durable conversational and Policy context, so that Runs share a coherent Transcript.
30. As a Principal, I want at most one Active **root** Run per Session, so that human-facing Transcript writers stay coherent.
31. As a Principal, I want a parent Run to spawn Child Runs for delegated work, so that parallel workstreams are possible without multi-writer Sessions.
32. As a Principal, I want each Child Run to have an isolated transcript slice, carved budgets, and a Policy subset, so that delegates cannot exceed parent authority.
33. As a Principal, I want Child Run status visible from the parent Run record and events, so that I can follow the tree without ad-hoc logging.
34. As a Principal, I want interrupted root Runs marked interrupted with children interrupted too, so that post-crash recovery is a new Run, not mid-loop resume.
35. As a Principal, I want concurrent root Runs across different Sessions up to a global cap, so that separate workstreams share one Worker.

### Trust, Policy, Approval, and Run origin

36. As a Principal, I want default-deny Policy for unknown tools and paths, so that the system fails closed.
37. As a Principal, I want tool allowlists and Workspace roots enforced on every tool call, so that Policy is not advisory.
38. As a Principal, I want Runs with origin `gateway:*` or `schedule` to start under reduced Policy, so that unattended or chat-origin work cannot inherit full host power by default.
39. As a Principal, I want to escalate a Run’s Policy (or approve specific high-blast actions) from a trusted control-plane client, so that I can deliberately grant power when needed.
40. As a Principal, I want high-blast actions (local exec, skill auto-apply outside trusted rules, computer-use outside allowlists, unrestricted browser navigation, etc.) to create Approval requests, so that I stay in control.
41. As a Principal, I want to list pending Approvals and approve or deny them, so that blocked Runs can proceed or fail explicitly.
42. As a Principal, I want deny to fail the tool call (and optionally the Run per policy), so that silence is not success.
43. As a Principal, I want Approval decisions attributed to a Principal, so that audit trails exist.
44. As an operator, I want configurable which tool classes require Approval per origin, so that risk posture is explicit.

### Tools — files, search, web

45. As a Principal, I want workspace file read and write under path jail, so that basic file work continues from v1.
46. As a Principal, I want apply_patch under Workspace roots, so that precise edits do not require whole-file rewrites.
47. As a Principal, I want search_files under Workspace roots, so that the agent can find code and notes without unrestricted shell.
48. As a Principal, I want web_search via pluggable providers, so that the agent can retrieve current public information.
49. As a Principal, I want web_extract for URLs under SSRF guards (no private IPs by default), so that documentation can be pulled without a full browser.
50. As a Principal, I want web tools size-limited and summarized for events, so that huge pages do not blow streams or logs.

### Tools — terminal and execute_code

51. As a Principal, I want a terminal/process tool with local and Docker backends, so that real host work is possible under Policy.
52. As a Principal, I want Policy to select exec backend (and defaults by Run origin), so that gateway work prefers Docker.
53. As a Principal, I want command/cwd constraints and Approval for high-blast local exec, so that shell is not unbounded.
54. As a Principal, I want process list/kill scoped to agent-spawned processes where applicable, so that cleanup is possible without host-wide process control as a default.
55. As a Principal, I want in-process execute_code for Hermes-like programmatic tool orchestration, so that multi-step tool pipelines can collapse into one model turn.
56. As a Principal, I want execute_code unable to open raw network, spawn processes, or touch arbitrary filesystem/secrets directly, so that the interpreter only reaches the world through Policy-gated tool RPC.
57. As a Principal, I want execute_code disabled by default for gateway/reduced Policy, so that chat-origin code cannot own the Worker process.
58. As a Principal, I want CPU/time/memory quotas on execute_code, so that runaway scripts stop.

### Tools — browser and computer-use

59. As a Principal, I want browser tools (navigate, snapshot/screenshot, click, type, wait, tabs) against an isolated browser profile, so that interactive web work is possible.
60. As a Principal, I want domain allowlists and Approval for high-blast browser actions, so that navigation stays bounded.
61. As a Principal, I want browser tools separate from consumer-web model provider cookies, so that inference auth is not confused with browser automation.
62. As a Principal, I want computer-use tools against an isolated agent desktop, so that non-browser GUIs on the worker can be driven.
63. As a Principal, I want the agent to prefer browser tools for web tasks and computer-use for desktop apps, so that each path stays appropriate.
64. As a Principal, I want attach-to-my-personal-Mac-desktop excluded by default, so that a bad Run cannot hijack my daily GUI session.

### Memory, session search, and recall

65. As a Principal, I want a curated Memory store distinct from Transcript, so that durable facts are not buried in chat logs.
66. As a Principal, I want Memory read/write/search tools, so that the agent can deliberately retain and retrieve knowledge.
67. As a Principal, I want Memory writes from reduced-origin Runs constrained (propose-only or deny) unless escalated, so that Telegram cannot silently rewrite long-term knowledge.
68. As a Principal, I want SQLite FTS across Memory (and session_search across Transcripts), so that “what did we decide weeks ago?” works without a vector DB.
69. As a Principal, I want Memory to survive Worker restarts, so that learning persists.
70. As a Principal, I want Memory entries attributable to source Run/Principal metadata, so that provenance is inspectable.

### Skills and learning loop

71. As a Principal, I want Skills as versioned document packages under a skills root, so that procedures are portable and inspectable.
72. As a Principal, I want skills_list and skill view/load tools, so that the agent uses progressive disclosure instead of stuffing every skill into every prompt.
73. As a Principal, I want an always-on skill learning loop that drafts create/improve proposals from experience, so that the agent OS grows procedural memory.
74. As a Principal, I want skill auto-apply only in trusted context (control_plane origin or escalated, Policy allows manage, optional operator auto-commit setting), so that learning does not become unreviewed prompt injection.
75. As a Principal, I want gateway-origin skill mutation to remain draft/propose only, so that chat cannot silently rewrite Skills.
76. As a Principal, I want skill manage to be Approval-capable high-blast, so that I can gate installs and edits.
77. As a Principal, I want agentskills.io-compatible layout where practical, so that skills are portable without a mandatory Skills Hub client.

### Soul and context files

78. As a Principal, I want a Soul document loaded into Runs, so that the agent has stable personality and standing instructions.
79. As a Principal, I want workspace Context files attached for relevant Sessions, so that project norms shape work without pasting them every time.
80. As a Principal, I want agent edits to Soul/context files treated as high-blast when Policy says so, so that identity files are not casually overwritten.

### Schedules

81. As a Principal, I want to create, list, pause, resume, and delete Schedules via the control plane, so that unattended automation is first-class.
82. As a Principal, I want a Schedule to fire a Run with a frozen Policy snapshot and origin `schedule`, so that unattended work is bounded at authoring time.
83. As a Principal, I want Schedule-fired Runs to use reduced Policy by default, so that cron is not full host power.
84. As a Principal, I want optional delivery of Schedule results via Gateway notify, so that morning briefings can reach Telegram/Discord.
85. As a Principal, I want natural language to help *author* a Schedule (via a Run), not a second hidden scheduler runtime, so that one Schedule system remains authoritative.
86. As an operator, I want Schedule fire durable across restarts (no lost fires or double-fire storms beyond documented semantics), so that automation is trustworthy.

### Gateway (Telegram + Discord)

87. As a Principal on my phone, I want to message the Telegram Gateway and have it start or continue a Session/Run on the control plane, so that I am not tied to curl.
88. As a Principal, I want Discord Gateway support for the same control-plane mapping, so that I have a second always-on surface.
89. As a Principal, I want gateway-origin Runs stamped and reduced-Policy by default, so that chat is not root on my worker.
90. As a Principal, I want final answers and clarify prompts delivered back to the originating chat, so that the loop closes on the surface I used.
91. As a Principal, I want media intake (images/voice notes where the platform allows) wired into vision/TTS paths when configured, so that phone use feels native.
92. As an operator, I want Gateway bot tokens and webhook secrets fail closed when missing/invalid, so that half-configured Gateways do not run open.
93. As an operator, I want Gateways to never implement a parallel agent loop, so that cancel, budgets, and Transcript stay single-sourced on the control plane.
94. As a Principal, I want other messaging platforms possible later via the same Gateway adapter trait without blocking v2 done, so that Telegram+Discord remain the ship bar.

### MCP

95. As a Principal, I want MCP client connections to configured servers, so that external tools appear under namespaced tool names without core forks.
96. As a Principal, I want MCP tools subject to the same Policy allowlists and Approval rules, so that extension is not a Policy bypass.
97. As a Principal, I want MCP server mode to export selected Keryx tools to other agents under operator auth, so that Keryx can be a capability layer.
98. As a Principal, I want MCP server export to refuse unauthenticated or Policy-free access, so that serving is not a backdoor.
99. As a Principal, I want MCP server failure or disconnect to fail closed for in-flight tool calls, so that hangs are visible.

### Media and voice

100. As a Principal, I want vision support for images from Gateway, browser screenshots, and computer-use frames, so that the agent can see.
101. As a Principal, I want TTS for spoken replies on capable surfaces, so that I can listen when typing is inconvenient.
102. As a Principal, I want pluggable image (and optional video) generation tools when API keys exist, so that media generation is not hard-wired to one portal.
103. As a Principal, I want Telegram voice notes as the first voice mode path, so that phone voice works without Discord VC as a gate.

### Operator clients (CLI + TUI)

104. As a Principal, I want a CLI client for session/run/cancel/events/approve/schedule/doctor, so that scripting and SSH ops are first-class.
105. As a Principal, I want a full TUI with streaming output, history, slash commands, and interrupt-and-redirect, so that interactive use feels like an agent OS.
106. As a Principal, I want CLI/TUI to be control-plane clients only, so that they do not fork lifecycle logic.
107. As a Principal, I want Approval flows usable from CLI and TUI, so that high-blast work is operable without raw HTTP.
108. As a Principal, I accept desktop app as future work, so that v2 is not blocked on Electron/native packaging.

### Models (carried from v1, still required)

109. As a Principal, I want OpenAI and Grok providers via official credentials, so that paid API access works.
110. As a Principal, I want optional Codex/consumer web providers when secrets are present, so that subscription paths remain available (ADR 0010/0011).
111. As a Principal, I want no runtime fake provider on the Worker, so that misconfiguration fails closed.
112. As a Principal, I want GET providers catalog and per-run model selection, so that clients can choose models deliberately.

### Small operator tools

113. As a Principal, I want a todo tool for run checklists, so that multi-step goals stay organized inside a Run.
114. As a Principal, I want a clarify tool that surfaces a question to me via control plane and/or Gateway, so that the agent can ask instead of guessing.
115. As a Principal, I want session_search over Transcripts, so that prior conversation text is recallable without opening every Session manually.

### Extensibility and codebase health

116. As a developer, I want domain types named after the glossary (including Gateway, Memory, Skill, Soul, Schedule, Child Run, Run origin, Approval), so that code and docs share one language.
117. As a developer, I want hexagonal dependency direction preserved (domain ← app ← adapters ← worker), so that boundaries stay enforceable.
118. As a developer, I want new capabilities as compile-in adapters and registries, so that dynamic native plugins stay out of scope.
119. As a developer, I want Tool vs Skill distinction enforced in the model, so that executable power and document knowledge do not collapse into one mushy concept.
120. As a developer, I want transport (HTTP/SSE) and Gateway platform SDKs outside domain logic, so that the core stays free of framework churn.
121. As a future client author, I want a documented control-plane contract for Approvals, Schedules, Memory, Skills metadata, and Child Runs, so that independent clients can be built.

### Reliability, security, and quality

122. As an operator, I want Tailscale treated as reachability only, so that mesh membership is not application authorization.
123. As an operator, I want no public bind and no Funnel/Serve product path, so that exposure matches the worker security contract.
124. As a developer, I want CI without live model, live Telegram/Discord, or paid media network calls, so that merges are not flaky.
125. As a developer, I want Seam 1 to cover origin Policy, Approval, Child Runs, Schedule fire, Memory, Skills rules, execute_code fence, and MCP mock behavior, so that agent-OS correctness is cheap to protect.
126. As a developer, I want Seam 2 fixture tests for model adapters, so that request/stream parsing stays correct.
127. As a developer, I want Seam 3 fixture tests for Telegram and Discord adapters, so that platform wire mapping stays correct without live bots.
128. As an operator, I want optional Docker and live-model verification paths documented, so that real environments can still be checked deliberately.

---

## Implementation Decisions

These decisions encode ADRs 0001–0012 and the v2 grill. Vocabulary matches `CONTEXT.md`.

### Product shape

- Keryx v2 is a **personal agent OS** built on the long-running **Worker**, not a one-shot CLI product and not a Hermes line-by-line port (ADR 0012).
- **Control plane is system of record** for Sessions, Runs, Child Runs, Approvals, Schedules, Memory, and event streams.
- **Gateway**, CLI, and TUI are clients/adapters—not alternate agent runtimes.
- **Edge** remains Tailnet HTTPS → loopback only (ADR 0003). Edge ≠ Gateway.
- Single-operator Principal model remains; multi-tenant accounts are out (ADR 0004 extended, not replaced).

### Session / Run / Child Run

- Session + Run work model retained (ADR 0001).
- **One Active root Run per Session** (ADR 0002 refined): Child Runs execute under the parent root; they do not become a second Session-level Active root.
- Child Runs: isolated transcript slice, budget carve-out from parent, Policy subset, cancel cascades parent → children.
- Global multi-Session cap retained (ADR 0002).
- No mid-loop resume after crash (ADR 0006); interrupted root implies interrupted children.

### Run origin and Policy

- Every Run records **Run origin**: at least `control_plane`, `gateway:telegram`, `gateway:discord`, `schedule`.
- Origins `gateway:*` and `schedule` attach **reduced Policy** templates unless escalated via trusted control-plane action.
- Policy includes: tool allowlists, Workspace roots, budgets, exec backend defaults, Approval class requirements, origin rules, skill auto-apply flags.

### Approval

- High-blast actions create durable **Approval** records; Run waits or tool fails per configured mode until approve/deny.
- Control plane exposes list/approve/deny; CLI/TUI consume the same API.
- Decisions attributed to Principal; secrets never stored in Approval payloads beyond redacted summaries.

### Durability

- SQLite remains the local store (ADR 0006), extended for Memory entries, Skills index/metadata (content may live on disk under skills root), Schedules, Approvals, Child Run linkage, Run origin.
- Transcript remains conversational truth; Memory is separate curated store with FTS.
- Data directory configuration unchanged in spirit; migrations must be explicit and forward-safe from v1 schemas.

### Tools (v2 ship set)

| Area | Decision |
|------|----------|
| Files | read, write, apply_patch, search_files; path jail |
| Terminal | local + Docker backends; other backends port-ready only |
| Web | pluggable web_search + web_extract; private IP deny default |
| Browser | isolated Chromium/Playwright-class toolset |
| Computer-use | isolated agent desktop on worker; not personal Mac default |
| Memory | memory read/write/search tools |
| Skills | list/view/load/manage; learning loop as specified |
| execute_code | in-process interpreter, RPC-only fence, quotas, origin gates |
| Operator UX | todo, clarify, session_search |
| MCP | client tools namespaced; server export Policy-bound |

- Tool interface remains policy-gated in app/domain; adapters compile in (ADR 0005/0008 spirit, expanded surface).
- Consumer-web **model** sessions (ADR 0010) stay inference-only—not the browser tool subsystem.

### Skills learning loop

- Always-on: agent may draft skill create/improve from experience.
- Auto-apply only when trusted (control_plane or escalated + Policy + optional auto-commit setting).
- Gateway/reduced origin: draft/propose only.
- No mandatory Skills Hub marketplace client.

### Soul and context files

- Soul: single operator-level document path, loaded into Runs.
- Context files: allowlisted workspace paths auto or on-demand attached.
- Distinct from Memory (facts) and Skills (procedures).

### Schedules

- First-class Schedule resources on the control plane.
- Fire → start Run with frozen Policy snapshot, origin `schedule`.
- Optional Gateway notification of outcomes.
- No separate “proactive always-thinking” daemon beyond Schedules (rejected in grill).

### Gateway

- Ship bar: **Telegram + Discord**.
- Adapter trait for future platforms; platform count is not a completeness gate.
- Inbound: authenticate platform, map to Principal/operator binding, create/continue Session, start Run with origin, reduced Policy.
- Outbound: deliver results/clarify to originating thread/channel.
- Must call control-plane/app ports—no shadow loop.

### MCP

- Client: configured servers; tools in registry under Policy.
- Server: export selected tools; require same auth/Policy path as human Principal control.
- Disconnects fail closed for tool invocation.

### Media

- Vision intake for images from Gateway/browser/computer-use.
- TTS out where surface supports it; Telegram voice notes first.
- Image/video generation pluggable when keyed—not a single-portal hard dependency.
- Full media studio / Discord VC not release gates.

### Clients

- First-party **CLI** and **full TUI** as control-plane clients.
- Desktop app explicitly future (not v2 gate).

### Module architecture

Hexagonal workspace retained and extended by adapter areas (not mandatory new crate-per-feature):

```text
domain ← app ← {storage, model, tools, gateway, mcp, api, …} ← worker
```

| Module area | Responsibility |
|-------------|----------------|
| domain | Session, Run, Child Run, Principal, Policy, Run origin, Approval, Memory, Skill refs, Schedule, Tool port, events, pure rules |
| app | Agent loop, budgets, concurrency, Child Run orchestration, Approval gating, Schedule fire, skill loop hooks, cancel trees |
| storage | SQLite durability for all durable aggregates |
| model | Providers (existing registry; ADR 0011) |
| tools | FS, terminal, web, browser, computer-use, execute_code, memory/skills tools, todo/clarify/search |
| gateway | Telegram + Discord adapters |
| mcp | Client + server adapters |
| api | HTTP control plane, SSE, auth |
| clients | CLI + TUI (may live in worker binary or sibling package; still control-plane clients) |
| worker | Composition root, config, bind, graceful shutdown |

Rules:

- Domain has no HTTP/SQLite/Telegram/Discord/Playwright SDKs.
- App depends on ports/traits, not concrete SDK types.
- Adapters compile in via registries; no dynamic `.so` plugins.
- Map domain errors to transport only at API/Gateway boundaries.

### Control-plane API (behavioral contract)

Exact routes/fields chosen at implementation time; behavior must include v1 plus:

- Approvals: list pending, approve, deny
- Schedules: CRUD, pause/resume, list fire history as needed
- Memory: list/get/search/create/update/delete (or tool-only with read APIs for clients—prefer explicit control-plane read/search for operator clients)
- Skills: list/view metadata; manage may be tool + Approval; drafts visible to operator
- Runs: Child Run linkage on get/list; origin field; Approval-wait status
- Providers catalog unchanged in spirit (ADR 0011)
- SSE taxonomy extended with stable events for: approval.waiting / approval.resolved, child_run.started / child_run.finished (names illustrative), schedule-related if streaming a fire—not raw platform dumps

### Run events (SSE)

- Keep small fixed taxonomy spirit (ADR 0007).
- Add milestones for Approval and Child Runs without leaking secrets.
- Transcript remains durable conversational truth; Memory is separate.

### Configuration (minimum additions over v1)

- Exec: backend defaults, Docker settings, Approval classes
- Skills root, Soul path, context file patterns
- Gateway enable flags + bot tokens/webhook secrets
- Schedule enable + notify targets
- MCP client server list + MCP serve bind/auth
- Browser/computer-use isolation paths
- Web search provider credentials
- Media/TTS provider credentials (optional)
- Skill auto-commit setting for trusted origin

### Relationship to v1

- v1 success criteria remain the baseline; v2 builds on them.
- ADR 0005 describes v1 runtime thin surface; ADR 0012 is v2 target surface.
- Do not break operator token, loopback, SQLite restart, or Session/Run vocabulary.

### Implementation non-negotiables

- Fail closed on auth, Policy deny, path escape, private-IP web fetch, unauthenticated MCP serve
- No mid-tool exactly-once resume
- No public bind defaults
- Tailscale ≠ app auth
- Gateways do not own agent loop
- execute_code has hard RPC fence even though in-process
- Skill auto-apply never silent for gateway origin

### Suggested implementation order (non-binding)

1. Domain expansions: Run origin, Child Run, Approval, Memory, Schedule, Skill refs  
2. Storage migrations + Seam 1 harness extensions  
3. Approval control-plane + CLI approve  
4. File patch/search + terminal local/Docker  
5. execute_code fence  
6. Memory + FTS + session_search  
7. Skills load + learning loop rules  
8. Soul + context files  
9. Web search/extract  
10. Browser isolated + computer-use isolated  
11. Schedules  
12. MCP client + server  
13. Telegram Gateway + Discord Gateway (Seam 3)  
14. Media/TTS wiring  
15. Full TUI  
16. Docs, doctor, optional Docker/live smokes  

Order may be parallelized by adapter owners if Seam 1 contracts stay green.

### Open parameters (implementer discretion if documented)

- Exact HTTP paths and JSON field names (stabilize once published)
- Exact reduced-Policy template contents
- Exact Approval class matrix per tool
- Docker image defaults and resource limits
- Schedule double-fire / missed-fire recovery algorithm details
- TUI stack choice
- Whether Memory mutations are control-plane REST, tools-only, or both (both preferred for operator UX)

---

## Testing Decisions

### What makes a good test

- Assert **external behavior** at agreed seams, not private helper structure.
- Prefer the **highest seam** that still keeps failures local and fast.
- No live OpenAI/Grok, live Telegram/Discord, live browser CDN, or paid media APIs in default CI.
- Catch: auth holes, origin Policy mistakes, Approval bypass, Child Run cancel trees, Schedule fire Policy, Memory/skill write rules, execute_code fence breaks, path/SSRF escapes, MCP Policy bypass, Gateway shadow-loop regressions, SSE contract breaks, SQLite restart loss.
- Avoid LLM answer-quality evals as merge gates.

### Confirmed seams

#### Seam 1 — Control plane (primary)

In-process Worker control plane (HTTP + SSE + auth + app wiring) with:

- Fake Model provider
- Temporary SQLite data directory
- Temporary Workspace and skills roots
- Configured operator token(s)
- Fake/double backends for terminal, web, browser, computer-use, MCP server peer as needed
- Deterministic clock for Schedule tests

Covers: Principal/token fail-closed; Session/root Run lifecycle; Child Runs; Run origin + reduced Policy; Approval wait/approve/deny; Schedules; Memory + FTS; Skills load/manage/auto-apply rules; Soul/context attachment behavior; expanded tools via doubles; execute_code fence; MCP client/server Policy; SSE extensions; cancel trees; durability after store reopen.

This remains the primary merge-gate confidence surface.

#### Seam 2 — Model provider contract (secondary)

Existing role (ADR 0009/0010/0011): OpenAI/Grok (and optional consumer) adapters against fixture/mock HTTP—no live keys. Live providers remain explicit opt-in.

#### Seam 3 — Gateway adapter contracts (secondary)

Telegram and Discord adapters against **platform protocol fixtures** (mock Bot API / webhook payloads):

- Inbound → control-plane Session/Run with correct origin
- Outbound reply/clarify mapping
- Fail closed on bad secrets
- No parallel agent loop (assert interactions go through app/control-plane ports)

No live bots in default CI.

### Explicit non-seams for default CI

- Live Telegram/Discord E2E
- Headed computer-use visual E2E
- Full Docker matrix on every PR (optional main/tag smoke OK)
- Caddy/Tailnet Edge E2E
- Full TUI pixel suites (prefer control-plane + light CLI smoke)
- LLM qualitative evals

### Layers (ADR 0009 extended)

| Layer | Gate |
|-------|------|
| L1 Domain/app behavior | Prefer Seam 1; pure domain tests only for awkward pure state machines |
| L2 Adapter contracts | Seam 2 + Seam 3 + tool doubles via Seam 1 |
| L3 Control plane | Seam 1 |
| L4 Worker binary smoke | Boot, health, one fake-model Run; optional Docker exec smoke on main/tag |
| L5 Live models / live Gateways | Opt-in only |

### Modules under test (by behavior)

- Domain/app: origin Policy, Approval, Child Run trees, Schedule fire, skill auto-apply rules, budgets/cancel
- Storage: migrations; Memory/Schedule/Approval/Child Run durability
- Tools: path jail, SSRF guards, execute_code fence, backend selection
- Gateway: Telegram/Discord fixtures (Seam 3)
- MCP: mock client/server Policy
- API: auth, SSE taxonomy extensions, Approval/Schedule routes
- Model: Seam 2 fixtures

### Prior art

- Existing Seam 1 tests under the API integration suite (hello run, concurrency/budgets/cancel, SSE, SQLite durability, workspace tools).
- Seam 2 fixture tests for OpenAI-compatible and consumer web adapters.
- ADR 0009 pyramid; do not invent Edge/phone UI CI seams for merge gates.

### Tooling expectations

- Workspace `cargo test` in CI
- `clippy` (deny warnings) and `rustfmt` in CI
- Feature or env flags for live model / live Gateway / Docker smoke tests

---

## Out of Scope

For this v2 spec, the following are explicitly out of scope:

- Desktop/native GUI app (explicitly **future**)
- Messaging platform parity beyond Telegram + Discord as ship bar (adapter trait only)
- First-party Home Assistant, Spotify, regional chat bots, kanban, and similar long-tail plugins (use MCP/Skills)
- Vector database / Honcho-class dialectic user modeling as required v2 features
- Skills Hub / remote marketplace client as a release gate
- Research/RL/Atropos training loops (optional trajectory export may come later; not this spec’s gate)
- Multi-Principal tenancy, team RBAC, multi-tenant SaaS
- Mixture-of-agents as a special built-in tool (multi-provider + Child Runs suffice)
- Attach-to-personal browser profiles or personal Mac desktop control as defaults
- SSH / Modal / Daytona / Singularity exec backends as ship blockers (ports may exist)
- Mid-loop checkpoint resume / exactly-once tool replay
- Dynamic native plugin loading (`.so`)
- Public internet bind; Tailscale Funnel/Serve as product defaults
- Queue-primary control plane (NATS/Redis/SQS as interactive API)
- gRPC-first public API
- Billing, usage metering dashboards, multi-host orchestration fabric
- Worker-embedded browser login/CAPTCHA for consumer model sites (ADR 0010 risk remains operator-owned)
- LLM qualitative eval harness as a merge requirement
- Replacing v1 operator-token auth with Tailscale-identity-only auth

---

## Further Notes

### Source of truth for language and decisions

| Doc | Use |
|-----|-----|
| `CONTEXT.md` | Ubiquitous language; prefer these terms in code and tickets |
| `docs/adr/0001`–`0011` | v1 architectural decisions still in force unless refined here |
| `docs/adr/0012` | v2 personal agent OS capability surface decision |
| `docs/specs/0001-keryx-v1-worker.md` | v1 baseline behavior still assumed |
| This spec | Implementable v2 product behavior and test seams |

If code and glossary disagree, **fix the glossary or amend it deliberately**—do not invent synonyms (Job/Task/Conversation/Bot loop) for Session/Run/Gateway.

### Hermes relationship

Keryx is **Hermes-inspired**. Capability gaps were grilles into include/exclude decisions (ADR 0012). Feature parity with Hermes platform count, plugin zoo, or research tooling is explicitly rejected as a v2 definition of done.

### Success criteria for “v2 done”

- Control plane supports Approvals, Schedules, Memory, Skills metadata, Child Runs, Run origin, extended SSE
- Local + Docker terminal, patch/search files, web search/extract, isolated browser, isolated computer-use work under Policy
- execute_code enforces RPC fence and origin rules
- Memory + FTS and session_search work across restarts
- Skills load + learning loop with trusted auto-apply only; gateway drafts only
- Soul + context files attach to Runs
- MCP client + server enforce Policy/auth
- Telegram and Discord Gateways map to control plane with reduced Policy
- CLI + full TUI can operate Sessions, Runs, Approvals, Schedules
- Seam 1 + Seam 2 + Seam 3 pass in CI without live external networks
- No public bind; operator token still required; Tailscale remains reachability only

### Explicit tensions accepted (do not “fix” without an ADR)

1. Full agent-OS ambition with a two-platform messaging ship bar (not 20).  
2. In-process execute_code for Hermes-like feel **with** hard fence (not unfenced power).  
3. Always-on skill learning loop **with** origin-gated auto-apply.  
4. Computer-use included **only** on isolated agent desktop by default.

---

## ADR index (v2-relevant)

| ADR | Decision |
|-----|----------|
| 0001 | Daemon product; Session + Run |
| 0002 | Session-serial root Runs; capped multi-Session parallel (Child Runs refine) |
| 0003 | Loopback HTTP + Tailnet Edge |
| 0004 | Operator token + Principal |
| 0005 | v1 thin surface (historical runtime until v2 ships) |
| 0006 | SQLite durability; no mid-loop resume |
| 0007 | SSE Run events |
| 0008 | Hexagonal workspace |
| 0009 | CI pyramid; live models opt-in |
| 0010 | Consumer web session model providers |
| 0011 | Provider registry; real-only runtime; per-run model |
| 0012 | v2 personal agent OS capability surface |

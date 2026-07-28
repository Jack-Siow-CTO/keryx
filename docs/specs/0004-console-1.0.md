# Keryx Console 1.0 — Full agent OS operator GUI

Status: **ready-for-agent**  
Aligned with: `CONTEXT.md`, ADRs 0001–0012 (Worker spine), **ADRs 0013–0034 (Console)** (0014/0020 superseded by 0031/0032 messaging IA), specs 0001–0003  
Test seams: **(1) Control plane in-process (extended)** · **(4) OpenAPI ↔ Dart API client contract** · secondary: Console widget/golden  
Release strategy: **big-bang 1.0** (ADR 0022)  
Client: Flutter thin Principal client under `console/` · Worker remains system of record  

This document is the product & system spec (PRD) for Console. It synthesizes the grill freeze and ADRs 0013–0034. The Definition of Done table is normative for release gating.

---

## Problem Statement

I already run a Keryx **Worker**: durable **Sessions** and **Runs**, operator-token auth, SSE progress, SQLite survival, models, tools, Approvals, Schedules, Memory, and Skills on a personal host I reach over Tailnet. What I do *not* have is a day-to-day graphical home for that agent OS.

CLI, TUI, and curl are power tools—not a place I live. Telegram/Discord **Gateways** (when enabled) are ambient reduced-Policy chat, not a full-trust operator surface for Approvals, Memory curate, Schedule management, and tool-heavy Run inspection.

I want a **cross-platform Console** (mobile and desktop) that feels like a **messenger for agents**—chat list of Sessions, open thread, Send-first composer, Needs you attention, readable layered conversation—mapped honestly onto Keryx’s domain (Session, Run, Transcript, Approval, Inbox, Memory, Schedule, Skill, Artifact), without:

- becoming a second messaging Gateway,
- hosting a client-side agent loop,
- inventing offline dual sources of truth,
- or shipping a pixel clone of WhatsApp/Telegram/Slack.

Without Console, the personal agent OS stays operable only by people willing to drive HTTP and terminals. The Worker’s control plane is ready to be a system of record; the missing piece is a first-party operator surface worthy of daily use.

---

## Solution

Ship **Keryx Console 1.0**: a **Flutter** multi-platform **Console** that is the **primary Principal client** of the Worker **control plane**.

From my perspective:

1. I install Console on **macOS and iOS** (Linux desktop Should; Android/Windows only if free).
2. I join the **Tailnet** out-of-band, paste **Worker base URL + operator token** once; secrets live in OS secure storage with optional biometric lock.
3. I land in a **chat list** home: **Sessions** as threads with a single agent identity, plus a thin **Needs you** system row (Inbox projection).
4. I open a **Session** thread, read durable **Transcript** prose as chat messages, watch live **Run events** as collapsible activity, and use a messenger **composer** (Send starts a root Run when idle; cancel / cancel-and-rerun when Active—no silent queue).
5. **Approvals** appear as sticky in-thread cards and via Needs you; I approve or deny with full Principal authority (`control_plane` Run origin, not `gateway:*`).
6. I open a **profile hub** for **Memory**, **Schedules**, **Skills**, and **Settings**; per-Session Policy/Workspace live under **Session info**. I pick **provider/model** for Runs and expand **Artifacts** (diff, terminal, screenshot) for rich tool viewing.
7. After kill/reconnect, truth comes from the Worker (REST reload + SSE resubscribe)—not a local write replica.

**Release strategy is big-bang:** the first release labeled real Console includes the full Must gate below—not a Sessions-only pager followed by years of “almost.” Worker API gaps for that gate are **blockers**, not follow-ups.

Hermes-class breadth is inspiration; control-plane canon, Policy, and Tailnet-only reachability remain non-negotiable.

---

## User Stories

### Operator / host owner

1. As an operator, I want Console to talk only to my Worker control plane over HTTPS on the Tailnet, so that the existing loopback + Edge topology stays unchanged.
2. As an operator, I want Console never to bind a public agent API of its own, so that the Worker remains the only system of record.
3. As an operator, I want model API keys and bot secrets to stay on the Worker (env / secret files), so that Console is not a second vault.
4. As an operator, I want Artifacts stored under the Worker data directory with quotas, so that disk use stays bounded on the host.
5. As an operator, I want OpenAPI (or equivalent) checked into the monorepo, so that Rust and Flutter cannot silently drift.
6. As an operator, I want CI path filters for Rust vs Flutter, so that Console changes do not force unrelated Worker rebuilds without cause.
7. As an operator, I want `keryx doctor` / health still meaningful for Console connectivity checks, so that misconfigured Edge or down Worker is obvious.
8. As an operator, I want graceful Worker behavior when Console reconnects after sleep, so that flaky mobile networks do not corrupt Sessions.

### Principal — onboarding and auth

9. As a Principal, I want to enter Worker base URL and bearer operator token once, so that I can authenticate like CLI/TUI.
10. As a Principal, I want the token stored in OS secure storage (Keychain/Keystore), so that it is not in plaintext app prefs.
11. As a Principal, I want optional biometric (or device credential) lock to open Console, so that Approvals on a phone are not one-tap from a lock screen alone.
12. As a Principal, I want invalid tokens to fail closed with no Session/Run/Memory side effects, so that auth bugs never become silent.
13. As a Principal, I want logout to delete local secret and caches, so that a handed-off device does not retain access.
14. As a Principal, I want a connectivity/health check from Settings, so that I can distinguish “bad token” from “can’t reach Edge.”
15. As a Principal, I want push notification payloads never to include the operator token, so that OS notification centers are not a secret store.
16. As a Principal, I want API client types to allow future per-device tokens without rewriting the shell, so that lost-phone revoke can evolve later.

### Messaging navigation and layout

17. As a Principal, I want a chat-list home of Sessions on wide layouts (list | thread), so that ongoing work scans like a messenger.
18. As a Principal on phone, I want stacked navigation (list → full-screen thread; hub via menu), so that messenger principles survive small screens.
19. As a Principal, I want Sessions to behave like durable chat threads, so that multi-turn Transcript work has a stable home.
20. As a Principal, I want Workspace roots not to be the primary sidebar tree, so that Policy path jails are not confused with product folders.
21. As a Principal, I want messenger interaction patterns without WhatsApp/Telegram/Slack visual cloning, so that Keryx keeps its own operator aesthetic.
22. As a Principal, I want system light/dark theme and a single “needs you” accent, so that attention is obvious without noisy chrome.
23. As a Principal, I want comfortable-compact density, so that desktop is efficient and mobile is still readable.
23a. As a Principal, I want a Needs you system row (not a peer permanent rail) for cross-Session Approvals and failed Runs, so that messaging IA does not bury blast-radius decisions.
23b. As a Principal, I want Memory, Skills, Schedules, and Settings under a profile hub, and Policy/Workspace under Session info, so that config does not compete with the chat list.

### Session list projection

24. As a Principal, I want to list Sessions with title, timestamps, last message preview, and Active root Run summary, so that I can scan work like a channel list.
25. As a Principal, I want to rename a Session title, so that UUID channels become human-named.
26. As a Principal, I want default titles derived from the first user goal when I have not renamed, so that new Sessions are not anonymous.
27. As a Principal, I want pending Approval count (or equivalent badge) on a Session row, so that channel-level attention is visible without opening Inbox.
28. As a Principal, I want Session list attention badges to mean Approvals/Active work—not multi-human unread cursors, so that single-operator semantics stay honest.
29. As a Principal, I want New chat to create an empty Session under defaults (no mandatory wizard), so that first Send starts work without a goal cockpit.
30. As a Principal, I want to open a Session and see its detail projection, so that header chips (Active Run, origin hints) are available before scrolling Transcript.

### Inbox

31. As a Principal, I want a unified Inbox feed from the control plane, so that I do not merge Approvals and failed Runs client-side.
32. As a Principal, I want pending Approvals in Inbox with approve/deny actions, so that high-blast work unblocks quickly.
33. As a Principal, I want recent failed or interrupted root Runs in Inbox, so that silent Schedule or overnight failures surface.
34. As a Principal, I want Inbox items to be a read projection over existing records—not a durable notification log, so that resolving Approval clears attention without mark-as-read theater.
35. As a Principal, I want deep links from push (when enabled) to open the relevant Inbox item or Approval, so that background wakeups land correctly.
36. As a Principal, I want Inbox not to spam Gateway chat mirrors as first-class noise, so that ambient messaging stays on Gateways.

### Session main pane — conversation and activity

37. As a Principal, I want durable Transcript prose (user and assistant) as first-class messages, so that the Session feels like a conversation.
38. As a Principal, I want tool and system Run activity as collapsible cards, so that tool-heavy Runs stay readable.
39. As a Principal, I want sticky Approval cards in the Session when a Run is waiting, so that the blocking action is unmistakable.
40. As a Principal, I want Child Run linkage visible in activity (read-only tree), so that delegated work is followable.
41. As a Principal, I want budget and failure reasons visible, so that I understand why work stopped.
42. As a Principal, I want reopen-after-kill to restore conversation from Transcript, not from a client event buffer, so that phone sleep is safe.
43. As a Principal, I want reverse-chronological paged Transcript load (latest first, scroll up for history), so that large Sessions stay usable.
44. As a Principal, I want compact tool rows (name, status, summary, artifact refs) in Transcript, so that expand-in-place viewers have anchors without megabyte messages.

### Composer and Run lifecycle

45. As a Principal, I want Send on an idle Session (primary CTA, not a separate Start Run button) to start a root Run with my text as the goal, so that chat muscle memory matches agent work.
46. As a Principal, I want optional provider/model selection when starting a Run, so that I can choose among registered providers.
47. As a Principal, I want the composer to refuse silent second root Runs when one is Active, so that Session serialism is preserved.
48. As a Principal, I want explicit cancel of the Active root Run, so that runaway work stops.
49. As a Principal, I want cancel-and-rerun with a note as an explicit action, so that redirects are intentional.
50. As a Principal, I want no client-side follow-up queue that pretends the Worker accepted work offline, so that truth stays on the control plane.
51. As a Principal, I want steer-only-when-API-exists, so that Console never fakes mid-Run guidance.
52. As a Principal, I want Child Runs cancelled with the root, so that tree cleanup matches Worker law.
53. As a Principal, I want clear errors when a Run is rejected (Active present, global cap, Policy), so that I can act without guessing.

### Live progress (SSE)

54. As a Principal, I want to stream Run events while a Run is Active, so that progress is live.
55. As a Principal, I want model deltas to paint into the conversation layer, so that answers feel live.
56. As a Principal, I want tool started/finished events to update collapsed activity cards, so that I see work without JSON walls.
57. As a Principal, I want reconnect to reload durable state then resubscribe SSE, so that missed events do not require a client event log of record.
58. As a Principal, I want secrets redacted in streamed summaries, so that tokens do not leak into the UI carelessly.

### Approvals

59. As a Principal, I want to list pending Approvals, so that I can process the queue.
60. As a Principal, I want to approve or deny with Principal attribution, so that audit trails exist.
61. As a Principal, I want deny to fail the tool call (and optionally the Run per Policy), so that silence is not success.
62. As a Principal, I want Approval deep links from Inbox and push, so that time-critical exec does not get buried in a Session.

### Artifacts and rich tool viewers

63. As a Principal, I want terminal output available as an Artifact viewer, so that shell work is inspectable.
64. As a Principal, I want file diff/patch viewers, so that code changes are reviewable in Console.
65. As a Principal, I want browser/computer screenshots as image Artifacts, so that GUI tool outcomes are visible.
66. As a Principal, I want Artifact fetch authenticated as Principal, so that blobs are not world-readable URLs.
67. As a Principal, I want inline multi‑MB blobs kept out of Transcript and SSE, so that streams and DB rows stay healthy.
68. As a Principal, I want basic viewers only—not a full remote-desktop session in Console 1.0, so that computer-use isolation remains on the Worker.

### Memory

69. As a Principal, I want to search Memory from Console, so that durable facts are findable without a Run.
70. As a Principal, I want to read, create, update, and delete Memory entries, so that I can curate the agent’s long-term knowledge.
71. As a Principal, I want Console Memory writes to use the same store as agent `memory_*` tools, so that there is one brain.
72. As a Principal, I want Console writes attributed to my Principal (and not require a synthetic Run), so that provenance is honest.
73. As a Principal, I want reduced-origin Runs still constrained on Memory writes by Policy, so that Gateways cannot silently rewrite knowledge.

### Schedules

74. As a Principal, I want to list Schedules, so that unattended work is visible.
75. As a Principal, I want to create, pause, resume, and delete Schedules, so that I can manage cadence without CLI.
76. As a Principal, I want Schedule-fired Runs to appear under Sessions with origin `schedule`, so that Policy and audit stay correct.

### Skills

77. As a Principal, I want to list Skill packages, so that procedures on the Worker are discoverable.
78. As a Principal, I want to view a Skill’s document content, so that I can read what the agent may load.
79. As a Principal, I want chips or indicators when a Run loaded a Skill, so that progressive disclosure is transparent.
80. As a Principal, I want in-Console Skill package editing to be optional (not a 1.0 gate), so that authoring can stay agent/CLI/filesystem if needed.
81. As a Principal, I want no Skills Hub or marketplace in 1.0, so that scope stays a personal agent OS, not an app store.

### Providers, Soul, MCP (Should / boundaries)

82. As a Principal, I want to pick among registered providers/models when starting a Run, so that model choice stays under policy.
83. As a Principal, I want not to paste OpenAI/xAI keys into Console, so that Worker env remains the secret SoR.
84. As a Principal, I want optional read-only Soul/context path visibility (Should), so that standing instructions are inspectable without a full editor.
85. As a Principal, I want optional MCP status read-only (Should), so that I can see configured peers without Console becoming MCP admin.

### Platforms, quality, and offline

86. As a Principal on macOS, I want a first-class desktop Console, so that master–detail (list | thread) is usable daily.
87. As a Principal on iOS, I want a first-class mobile Console, so that Approvals and Sessions work away from the desk.
88. As a Principal, I want last-fetched caches and composer drafts only—not offline Start Run, so that I never believe work ran when the Worker did not accept it.
89. As a Principal, I want widget/golden coverage for composer modes and Inbox, so that lifecycle UX regressions are caught.
90. As a Principal, I want API client contract tests, so that OpenAPI drift fails before UI lies.

### Explicit non-stories (1.0)

91. As a product owner, I do not want Console to default Runs to `gateway:*` origin, so that trusted operator power stays distinct from ambient chat.
92. As a product owner, I do not want multi-window desktop, custom theme marketplaces, or i18n as 1.0 gates.
93. As a product owner, I do not want web public hosting of Console, so that Tailnet-only posture holds.
94. As a product owner, I do not want OAuth/SSO multi-tenant login as 1.0.

---

## Implementation Decisions

### Product role and release (ADRs 0013, 0022)

1. **Console** is the first-party graphical Operator client—primary day-to-day surface for Sessions, Runs, Approvals, Memory, Schedules, and Skills.
2. Console is **not** a Gateway and **not** an agent-loop host. CLI/TUI remain power tools; Telegram/Discord remain ambient Gateways.
3. Runs started from Console use **control-plane** Run origin (full Principal trust subject to normal Policy)—not `gateway:*` by default.
4. **Big-bang 1.0:** first “real Console” release includes the full Must DoD table; phased Sessions-only ship was rejected as the *release strategy*.
5. Must/Should/Out changes require deliberate edits to this spec (and ADR 0022 consequences), not silent expansion.

### Definition of Done (normative gate)

| # | Capability | Gate | ADR / notes |
|---|------------|------|-------------|
| 1 | Settings: base URL + token (secure storage), biometric lock, connectivity/health | **Must** | 0021 |
| 2 | Messaging shell: chat list of Sessions + Needs you system row; new chat; open thread | **Must** | 0031, 0027, 0028, 0034 |
| 3 | Transcript conversation + live SSE activity (collapsed tools) | **Must** | 0015, 0025 |
| 4 | Composer: idle→Send starts Run; Active→cancel / cancel-and-rerun | **Must** | 0016, 0034; steer post-1.0 until API exists |
| 5 | Approvals: Needs you + sticky in-thread; approve/deny; deep link | **Must** | 0033; push **Should** |
| 6 | Run status: Active chip, budgets/errors, Child Run tree (read-only) | **Must** | |
| 7 | Memory: search + read + write/curate UI | **Must** | 0029 |
| 8 | Schedules: list/create/pause/resume/delete | **Must** | existing control plane |
| 9 | Skills: list/view; indicate what a Run loaded | **Must** | 0030; in-app edit **Should** |
| 10 | Provider/model picker per Run | **Must** | not secret entry |
| 11 | Provider secret entry in Console | **Out** | Worker env / `*_FILE` |
| 12 | Rich tool viewers: diff, terminal, screenshot/snapshot (basic) | **Must** | 0026; full remote desktop **Out** |
| 13 | Soul + context file editor | **Should** | |
| 14 | MCP status / enable UI | **Should** | read-only status OK |
| 15 | Multi-window, custom themes, i18n | **Out** | |
| 16 | Offline write queue / local DB replica | **Out** | 0019 |
| 17 | Device-paired tokens, OAuth | **Out** | 0021 |
| 18 | Web Console / public hosting | **Out** | |
| 19 | Platforms | **Must** macOS + iOS · **Should** Linux · **Out** Android/Windows unless free | 0018 |
| 20 | Tests | **Must** API client contracts + widget/golden composer/Inbox · **Should** integration vs local Worker | |

### Information architecture and UX (ADRs 0031–0034; 0015–0016 reaffirmed; 0014/0020 superseded)

6. **Messaging chat list:** Session = chat thread + thin Needs you system row; not dual-rail peer rails, not Session-only without attention, not Workspace-first sidebar product tree (ADR 0031).
7. **Session thread:** conversation layer = Transcript prose as first-class messages; activity layer = collapsible tools/Child Runs/status; Approvals = sticky in-thread cards + Needs you (ADRs 0015, 0033).
8. **Composer modes:** idle → Send starts root Run (primary CTA); Active → explicit wait/cancel/cancel-and-rerun (steer only if control plane supports it later). Never silent queue or fake second root Run (ADRs 0016, 0034).
9. **Visual system:** messenger *principles*, Keryx *skin*—neutral operator chrome, system light/dark, single needs-you accent, comfortable-compact density (ADR 0032).
10. **Responsive:** wide = list | thread (+ optional contextual third pane); medium/narrow = stack list → thread; hub via profile/menu—not permanent Inbox column.
10a. **New chat:** empty Session under defaults; no mandatory create wizard (ADR 0034).
10b. **Hub vs Session info:** Memory/Skills/Schedules/Settings in profile hub; Policy/Workspace in Session info.

### Transport and client architecture (ADRs 0017, 0018, 0019, 0021, 0024)

11. **Transport:** HTTP REST + SSE Run streams. On open/reconnect: reload durable state, then resubscribe. Client buffers are not SoR.
12. **Push:** OS push only as Inbox wakeup/deep-link pointer—not a parallel event log. Optional for 1.0 (Should), not a substitute for REST.
13. **Stack:** one **Flutter** codebase for mobile and desktop.
14. **Thin client:** presentation state + last-fetched snapshot cache + composer drafts only. No offline mutation queue, no second Transcript model, no client agent assists.
15. **Package shape (conceptual):** pure Dart API client package; app-level session/inbox controllers; Flutter UI. UI state library: **Riverpod** in the app only; API package has no Flutter/Riverpod dependency.
16. **Auth:** base URL + bearer operator token in secure storage; optional biometric app lock; Tailnet reachability out-of-band.
17. **Monorepo:** Console lives under `console/` beside Worker crates; coupling is checked-in **OpenAPI** (or equivalent) under docs API path—not Rust FFI, not a separate product repo.

### Control-plane API expansion (ADRs 0023, 0025–0030)

18. **Growth model:** expand existing `/v1/*` REST + SSE. No Console-only GraphQL/BFF, no gRPC rewrite, no `/v2` API namespace solely for Console.
19. **Illustrative Must endpoints** (exact paths/fields implementer-stable once published in OpenAPI):

| Area | Capability |
|------|------------|
| Sessions | List projection (title, timestamps, active root Run, last message preview, pending_approval_count); get; create; patch title |
| Transcript | Paged get (reverse-chronological); structured messages: id, optional run_id, created_at, role, content; tool compact fields (name, status, summary, artifact_refs) |
| Runs | Existing start/get/cancel/events; Child Run linkage in get + events; viewer-friendly tool event summaries + artifact refs |
| Approvals | Existing list/approve/deny |
| Inbox | `GET` unified projection: approval_pending, run_failed (and similar); actions stay on underlying resources |
| Memory | List/search/get/create/update/delete; same store as tools; Console writes set principal provenance |
| Skills | List/get by name; Run-loaded skill indicators; write/manage UI not a Must |
| Schedules | Existing list/create/pause/resume/delete |
| Providers | Existing list; provider/model on start Run |
| Artifacts | Worker files under data dir + SQLite metadata; authenticated get by id; kinds text/diff/image/json; size quotas |
| Health | Existing |

20. **Transcript vs events:** Transcript is durable conversation truth for the main pane after reconnect. Historical Run events may be exposed for debug/rebuild but are not the default UI timeline.
21. **Artifacts:** large tool outcomes are Worker-side Artifacts referenced from Transcript/events—not inline base64 in messages/SSE; not external object store for 1.0; not Console-only durability.
22. **Session list:** durable title/updated_at (and related) on Worker—not client-only nicknames (multi-device SoR).
23. **Inbox:** control-plane read projection; no Notification aggregate; no multi-human read/unread cursors.
24. **Memory:** dual writers OK (Console REST + tools) against one Memory store; origin Policy still gates tool writes for reduced origins.
25. **Skills:** read-mostly Console for 1.0 Must; package CMS/Hub out.

### Domain glossary terms (Console-related)

Use `CONTEXT.md` vocabulary throughout implementation and UI copy where domain terms appear:

- **Console**, **Inbox**, **Session**, **Run**, **Child Run**, **Transcript**, **Run event**, **Approval**, **Memory**, **Skill**, **Artifact**, **Principal**, **Control plane**, **Worker**, **Policy**, **Schedule**, **Gateway** (not Console), **Workspace** (Policy jail, not sidebar product tree)

### Modules / surfaces to build or extend

26. **Worker domain/app/storage/api:** Session projection fields; structured TranscriptMessage; Artifact metadata + blob storage; Inbox projection; Memory HTTP; Skills HTTP; richer tool event payloads and artifact production from tools; OpenAPI export/check-in.
27. **Console Flutter workspace:** app shell (responsive messenger master–detail), settings/auth, chat list + Needs you, Session thread, composer, SSE subscription lifecycle, profile hub (Memory/Schedules/Skills/Settings), Session info, artifact viewers, Riverpod controllers.
28. **Contract:** checked-in OpenAPI; pure Dart client aligned to it; CI drift failure.

### Non-negotiables

- Control plane remains system of record  
- Fail closed on auth  
- One Active root Run per Session  
- No client-side agent loop  
- No Console as model-key vault  
- No offline Start Run  
- Tailscale ≠ app auth  
- Gateways do not own Console’s Run origin by default  

### Suggested implementation order (non-binding; big-bang ship waits for Must)

1. Domain + storage: Session projection fields, structured Transcript, Artifact store  
2. Control-plane routes: sessions list/get/patch, transcript page, inbox, memory CRUD, skills GET, artifacts GET; richer events  
3. OpenAPI freeze + Seam 1 tests for new routes  
4. Dart `keryx_api` client + contract tests  
5. Flutter shell: auth, chat list + thread, composer Send, SSE  
6. Approvals + Needs you polish  
7. Memory, Schedules, Skills browse  
8. Artifact viewers  
9. Platform packaging macOS/iOS; optional push  
10. Widget/golden + dogfood against big-bang checklist  

### Open parameters (implementer discretion if documented in OpenAPI)

- Exact JSON field names and error shapes  
- Exact Inbox item taxonomy beyond approval_pending / run_failed  
- Artifact GC policy details (age/quota)  
- Whether historical Run events get a dedicated list endpoint vs debug-only  
- APNs/FCM provider choice and whether push ships in 1.0 or immediately after  
- Flutter package layout within `console/`  
- Visual token values (type scale, radii, accent hex)  

---

## Testing Decisions

### What makes a good test

- Assert **external behavior** at agreed seams—not private Flutter widget tree structure or Rust private helpers.
- Prefer the **highest seam** that still keeps failures local and fast.
- No live model providers, live Tailnet Edge E2E, or paid push providers in default CI.
- Catch: auth holes; missing Session/Transcript/Inbox/Memory/Skills/Artifact routes; structured Transcript regressions; composer lifecycle violations (double root Run); Approval path breaks; OpenAPI drift; secret leakage in events; Artifact authz; thin-client violations (if client tests invent offline queues—don’t).
- Avoid LLM answer-quality evals and pixel-perfect Slack clone diffs as merge gates.

### Confirmed seams

#### Seam 1 — Control plane in-process (primary; existing)

Extend the existing in-process Worker control-plane harness (HTTP + SSE + auth + app wiring) with fake model, temp SQLite, temp workspace/skills roots, operator token.

**Covers Console-blocking Worker behavior:**

- Session list projection, get, create, title patch  
- Structured paged Transcript  
- Inbox projection contents and ordering invariants  
- Memory REST CRUD/search vs tool Policy still enforced on reduced origin  
- Skills list/get  
- Artifact create-from-tool (or test double) + authenticated get; unauthenticated deny  
- Tool events include summary/artifact refs suitable for viewers  
- Existing Run/Approval/Schedule/SSE contracts remain green  

This is the **primary merge-gate** confidence surface for the Worker half of Console 1.0 (prior art: `seam1_*` API tests).

#### Seam 4 — OpenAPI ↔ Dart API client (primary for Console client; new)

Checked-in OpenAPI document is the cross-stack contract. Pure Dart API package tests:

- Request/response mapping for Must resources  
- SSE event parsing for known taxonomy  
- Drift job: OpenAPI vs client types (generate or golden compare)  

No Flutter dependency at this seam. Highest client seam without dragging UI.

#### Secondary — Console widget / golden

- Composer idle vs Active affordances  
- Inbox item actions wiring (against faked repositories)  
- Chat list / thread / narrow navigation smoke  

Not a substitute for Seam 1 or Seam 4. Prefer fakes over live Worker in default CI.

#### Explicit non-seams for default CI

- Live Tailnet / Caddy Edge E2E  
- Live APNs/FCM  
- Live OpenAI/Grok  
- Full store-release signing pipelines on every PR  
- Android/Windows matrix (out of 1.0 Must)  
- Visual regression against Slack screenshots  

### Layers (ADR 0009 extended for Console)

| Layer | Gate |
|-------|------|
| L1 Domain/app (Worker) | Prefer Seam 1 |
| L2 Adapter contracts | Existing model/tool seams as today |
| L3 Control plane | Seam 1 |
| L3b OpenAPI client | Seam 4 |
| L4 UI presentation | Widget/golden secondary |
| L5 Live network / push / Edge | Opt-in / manual |

### Modules under test (by behavior)

- Worker API/app/storage: new Console resources, Artifact IO, Inbox projection, structured Transcript  
- OpenAPI artifact + Dart client  
- Flutter controllers: reconnect reload, SSE dispose, composer mode transitions  
- Widget: Inbox, composer  

---

## Out of Scope

For Console **1.0** (normative **Out** / non-goals):

- Console as a **Gateway** or default `gateway:*` Run origin  
- Client-side **agent loop** or second Transcript model  
- **Offline** Start Run / writeable local domain replica / CRDT sync  
- **WebSocket-first** control channel rewrite  
- **GraphQL/BFF** or **gRPC** product API for Console  
- Console as **model API key vault** or secret admin for providers  
- **OAuth/SSO**, multi-tenant IdP, device-paired tokens as ship gate (evolution allowed later)  
- **Skills Hub / marketplace**, full Skill CMS as Must  
- **Full remote-desktop** computer-use UI inside Console  
- **Web public** Console hosting  
- **Multi-window** desktop, custom theme marketplace, i18n  
- **Android/Windows** as Must platforms  
- **Pixel-faithful WhatsApp/Telegram/Slack** skin  
- Dual-rail operator cockpit as the default home (superseded by messaging IA)
- Replacing CLI/TUI or removing Gateways  
- Changing Worker hexagonal ownership of Policy, budgets, or cancel  

Related but separate specs: Worker v1 (0001), v2 agent OS (0002), MCP capabilities (0003). Console **depends on** control-plane completeness from those tracks but does not redefine Gateway or MCP product scope except where Console Must APIs require read surfaces.

---

## Further Notes

### ADR index (this session / Console track)

| ADR | Decision |
|-----|----------|
| 0013 | Console = primary control-plane operator surface |
| 0014 | ~~Dual-rail IA~~ → **superseded by 0031** |
| 0015 | Conversation + activity layers (reaffirmed for messenger) |
| 0016 | Explicit composer Run modes (Send refined by 0034) |
| 0017 | REST + SSE; push = Inbox wakeup only |
| 0018 | Flutter multi-platform |
| 0019 | Strict thin client |
| 0020 | ~~Slack principles shell~~ → **superseded by 0032** |
| 0021 | Operator token + secure storage + biometric |
| 0022 | Big-bang Console 1.0 |
| 0023 | Expand REST `/v1/*` + SSE (no BFF/gRPC) |
| 0024 | Monorepo `console/` + OpenAPI seam |
| 0025 | Structured compact Transcript |
| 0026 | Worker Artifacts under data dir |
| 0027 | Session list operator projection |
| 0028 | Inbox projection endpoint |
| 0029 | Memory control-plane API (one store) |
| 0030 | Skills read-mostly for 1.0 |
| 0031 | Messaging chat-list IA |
| 0032 | Messenger principles, not chat-app skin |
| 0033 | Approvals: Needs you + sticky in-thread |
| 0034 | New chat empty Session; idle Send starts Run |

### Success scenario

Operator on tailnet opens Console on Mac or iPhone → chat list home → multi-turn Session thread with live collapsible activity → clears Approvals via Needs you or sticky card → curates Memory from hub → manages Schedules → browses Skills → expands tool Artifacts → kills app → returns later with durable Worker state intact.

### Relationship to prior specs

- **0001 / 0002:** Worker remains the agent OS runtime; Console is the primary GUI client of that runtime’s control plane.  
- **ADR 0012** listed desktop app as future; **ADR 0013/0022** promote Console to an explicit product track with big-bang 1.0 scope.  
- Do not break operator token, loopback default, SQLite durability, or Session/Run vocabulary while adding Console APIs.

### Grill freeze

Product and architecture decisions above were locked in grilling (including 2026-07-28 messaging IA supersession of dual-rail/Slack shell). Implementation should not reopen role, IA, stack, auth model, thin-client rules, or DoD without a deliberate ADR supersession and spec edit.

---

## Test seam confirmation (for implementers)

Preferred seams (fewest high seams):

1. **Seam 1 — Control plane in-process** (existing `seam1_*` style) for all Worker/Console API behavior.  
2. **Seam 4 — OpenAPI ↔ pure Dart client** for Console consumption without Flutter.  

Widget/golden tests are secondary presentation coverage only.

If these seams are wrong for your workflow, adjust before large implementation—but default CI should not add Edge E2E or live push as merge gates.

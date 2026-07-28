# Console messaging shell — implementation spec

Status: **ready-for-agent**  
Aligned with: `CONTEXT.md`, ADRs **0031–0034** (messaging IA), **0015–0016** (layers + composer lifecycle), **0013/0019** (primary surface + thin client), product/design in `console/PRODUCT.md` and `console/DESIGN.md`, parent scope `docs/specs/0004-console-1.0.md`  
Test seam: **Flutter widget tests (primary)** — control-plane Seam 1 and OpenAPI Seam 4 only if an API gap is discovered  
Client: Flutter Console under `console/` · Worker remains system of record  

This document is the implementation PRD for revamping Console from a dual-rail operator cockpit to a **chat-thread-centric messaging client** optimized for agentic work. It synthesizes the 2026-07-28 grill freeze and ADRs 0031–0034. It does **not** redefine Worker domain law; it re-homes Console presentation and navigation.

---

## Problem Statement

I use Keryx Console as my day-to-day Principal surface for Sessions, Runs, Approvals, Memory, Schedules, and Skills. The current UI feels **goal-oriented and dual-rail**: Inbox and Sessions as peer rails, “Start Run” as the primary act, cockpit density that reads more like an operator dashboard than a place I chat with my agent.

I want Console to feel like **WhatsApp or Telegram for agents**—a chat list of threads, open a thread, type, Send—while remaining honest about agentic reality: one Active root Run per Session, Approvals that cannot be missed, tool-heavy activity that must not drown prose, Policy/Workspace configuration, and a Worker that stays system of record over Tailnet.

Without this revamp, daily use stays cognitively “drive a control plane” instead of “message my agent,” even though the domain model (Session, Run, Transcript, Approval) already supports a messaging metaphor.

---

## Solution

Revamp Console’s shell, navigation, thread, and composer so that:

1. **Home is a chat list** of Sessions (title, last preview, attention badges, Active Run hints), not dual-rail Inbox + Sessions.
2. **Needs you** is a thin **system row** in that list (Inbox projection)—not a permanent peer column.
3. Opening a Session shows a **thread**: Principal and agent **prose** as first-class messages; tools / Child Runs / status as **collapsible activity** in the same timeline.
4. **Single agent identity** (Worker + Soul branding) is the counterpart in every thread; Child Runs are not contacts.
5. **Idle Send** starts a root Run with the message as the goal (no primary “Start Run” CTA). **Active** composer exposes wait / cancel / cancel-and-re-run only—never a silent second root Run or client queue.
6. **Approvals** appear as a **sticky card** in the open thread when relevant, and always via **Needs you** for cross-Session attention.
7. **New chat** creates an empty Session under defaults; first Send starts the first Run—no mandatory wizard.
8. **Profile hub** holds Memory, Skills, Schedules, Settings; **Session info** holds per-Session title/Policy/Workspace (and optional contextual third pane on wide layouts).
9. Layout is **messenger master–detail** (list | thread on wide; stack on narrow).

Visual system remains original Keryx operator chrome (`console/DESIGN.md`)—messenger *principles*, not a WhatsApp/Telegram/Slack skin. Auth, thin client, REST+SSE, and control-plane APIs stay as today unless a true gap blocks the UX.

---

## User Stories

### Product metaphor and home

1. As a Principal, I want Console’s home to be a chat list of Sessions, so that day-to-day work feels like messaging my agent.
2. As a Principal, I want each Session to appear as one chat row (title, last-message preview, timestamps), so that I can scan work like Telegram/WhatsApp.
3. As a Principal, I want attention badges on chat rows for pending Approvals and meaningful Active work, so that I do not need a second rail to notice risk.
4. As a Principal, I want Active root Run state visible on the row (or a compact indicator), so that I know which threads are mid-work.
5. As a Principal, I want Workspace roots not presented as the primary navigation tree, so that Policy path jails are not confused with product folders.
6. As a Principal, I want a single agent identity as the counterpart of every Session chat, so that I am not managing a multi-agent contact list the domain does not own.
7. As a Principal, I want Child Runs never listed as separate people or DMs, so that delegation stays activity inside a thread.
8. As a Principal, I want Console to remain a control-plane Principal client (not a Gateway), so that Runs keep trusted origin and cancel/budget semantics.

### Needs you / Inbox

9. As a Principal, I want a Needs you system row at the top of (or pinned in) the chat list, so that cross-Session Approvals and failed/interrupted Runs are unmissable.
10. As a Principal, I want Needs you to show a count badge using the needs-you accent, so that attention is scannable at a glance.
11. As a Principal, I want opening Needs you to show the Inbox projection items (Approvals, failed Runs, similar alerts), so that I can process attention without a dual-rail home.
12. As a Principal, I want each Needs you item to deep-link into the source Session and/or Approval action, so that I always decide in context.
13. As a Principal, I want resolving an Approval to clear the corresponding attention without a separate “mark as read” model, so that Inbox remains a read projection over real records.
14. As a Principal, I want Needs you not to be a permanent second column on wide desktops, so that the messaging metaphor is not dual-rail in disguise.
15. As a Principal, I want failed or interrupted root Runs to appear in Needs you, so that overnight/Schedule failures surface.

### Thread timeline

16. As a Principal, I want durable Transcript prose (user and assistant) as first-class chat messages, so that the thread reads as a conversation.
17. As a Principal, I want tool and system activity as collapsible activity blocks in the same timeline, so that tool-heavy Runs stay readable.
18. As a Principal, I want not to see every Run event as a human-style bubble by default, so that the thread does not become unreadable spam.
19. As a Principal, I want not to flip between Chat and Activity tabs as the default experience, so that I can follow a Run without mode-switching.
20. As a Principal, I want expand-in-place for activity (tool name, status, summary, artifact links), so that detail is progressive disclosure.
21. As a Principal, I want Child Run linkage visible inside activity (read-only), so that delegated work is followable without group-chat avatars.
22. As a Principal, I want live model deltas to paint into the conversation layer while a Run is Active, so that answers feel live.
23. As a Principal, I want live tool started/finished events to update collapsed activity, so that progress is visible without JSON walls.
24. As a Principal, I want reopen-after-kill to restore conversation from Transcript, so that phone sleep is safe.
25. As a Principal, I want reverse-chronological paged Transcript (latest first, scroll up for history), so that large Sessions stay usable.
26. As a Principal, I want a compact streaming status strip while a Run is Active, so that I know work is ongoing without a job-console layout.

### Approvals in thread

27. As a Principal, when a Session has a pending Approval, I want a sticky action card above the composer, so that high-blast decisions are unmistakable in context.
28. As a Principal, I want Approve to use the needs-you accent and Deny to be secondary, so that the dangerous path is intentional.
29. As a Principal, I want approve/deny to hit the existing control-plane Approval APIs with Principal attribution, so that audit trails remain correct.
30. As a Principal, I want sticky Approval and Needs you to both work (dual surface), so that I can act from list attention or from the open thread.
31. As a Principal, I do not want every pending Approval for any Session to hard-block the entire app as the only pattern, so that I can finish reading another thread first.

### Composer and Run lifecycle

32. As a Principal, when a Session is idle, I want Send as the primary composer action, so that chat muscle memory starts work.
33. As a Principal, I want idle Send to start a root Run with my message text as the goal, so that intent maps to Worker law without a separate Start Run CTA.
34. As a Principal, I want optional provider/model selection still available (Settings or light composer meta), so that model choice remains under Policy without goal-cockpit chrome.
35. As a Principal, when a Session has an Active root Run, I want Send disabled or clearly non-starting, so that I cannot invent a second root Run.
36. As a Principal, when Active, I want explicit Cancel Run, so that runaway work stops.
37. As a Principal, when Active, I want explicit Cancel & re-run (with optional note), so that redirects are intentional.
38. As a Principal, I want no client-side follow-up queue, so that I never believe work was accepted when the Worker did not start it.
39. As a Principal, I want clear errors when start is rejected (Active present, caps, Policy), so that I can act without guessing.
40. As a Principal, I want composer hint copy to feel message-oriented when idle (not “describe a goal for a new Run” cockpit language), so that the metaphor is consistent.
41. As a Principal, I want empty Send refused with a clear error, so that blank Runs are not started.

### New chat

42. As a Principal, I want a New chat control on the chat list, so that I can start a fresh Session without CLI.
43. As a Principal, I want New chat to create an empty Session under operator defaults immediately, so that I am not forced through a wizard.
44. As a Principal, I want the first Send in an empty Session to start the first root Run, so that create and first message form one natural path.
45. As a Principal, I want to open Session info before or after the first Send to tighten Workspace/Policy, so that progressive disclosure covers high-risk work.
46. As a Principal, I want default titles derived from the first user message when I have not renamed, so that new chats are not anonymous UUIDs.
47. As a Principal, I want to rename a Session title from Session info (or inline), so that threads stay human-named.

### Session info and configuration

48. As a Principal, I want Session info reachable from the thread header, so that per-chat configuration is one tap/click away.
49. As a Principal, I want Session info to expose title edit and Policy/Workspace relevant fields the control plane already supports (or honest empty states if not yet exposed), so that constraints live on the Session not in global Settings soup.
50. As a Principal on wide layouts, I want Session info optionally as a contextual third pane, so that I can configure without losing the thread.
51. As a Principal on narrow layouts, I want Session info as a pushed screen, so that mobile remains full-screen chat first.

### Profile hub (global surfaces)

52. As a Principal, I want Memory, Skills, Schedules, and Settings under a profile/overflow hub, so that global tools do not compete with the chat list as peer bottom-nav destinations.
53. As a Principal, I want Worker connectivity status visible in the hub or header, so that Tailnet/Worker failures are obvious.
54. As a Principal, I want Memory search/curate behavior unchanged in capability (same control-plane APIs), so that the hub is navigation re-home not a feature cut.
55. As a Principal, I want Schedules list/create/pause/resume/delete unchanged in capability, so that unattended work remains operable.
56. As a Principal, I want Skills list/view unchanged in capability, so that procedures stay discoverable.
57. As a Principal, I want Settings (base URL, token, biometric lock, connectivity, provider/model defaults) reachable from the hub, so that auth and prefs stay findable.
58. As a Principal, I want logout to still delete secure token and local caches, so that device handoff stays safe.

### Layout and responsive behavior

59. As a Principal on a wide desktop, I want chat list and open thread side by side, so that messenger master–detail is usable daily.
60. As a Principal on medium width, I want list or thread (push navigation), so that density stays comfortable.
61. As a Principal on phone width, I want full-screen list then full-screen thread, so that chat is primary on mobile.
62. As a Principal, I want optional third pane only when Session info or an Artifact is open—not always-on activity IDE columns.
63. As a Principal, I want breakpoints consistent with the design system (~1100 wide, ~720 medium), so that layout shifts are predictable.
64. As a Principal, I want selection in the list to open/update the thread without losing composer draft for the selected Session when feasible, so that switching chats is not hostile.

### Visual system and chrome

65. As a Principal, I want restrained cool-slate chrome and a single needs-you accent, so that attention is obvious without neon AI cosplay.
66. As a Principal, I want system fonts and comfortable-compact density, so that desktop is efficient and mobile readable.
67. As a Principal, I want chat list selection via filled panel tint—not left accent stripes, so that anti-patterns stay banned.
68. As a Principal, I want empty states that teach New chat and Needs you, so that first-run messaging IA is learnable.
69. As a Principal, I want motion limited to short selection/expand transitions, so that the app stays calm.
70. As a Principal, I want message presentation that prioritizes operator readability (clear author/time; bubbles optional, not mandatory consumer cosplay), so that dense agent threads stay scannable.

### Artifacts and tools (presentation re-home)

71. As a Principal, I want Artifact viewers (diff, terminal, screenshot) still reachable from activity/artifact refs, so that tool outcomes remain inspectable.
72. As a Principal, I want Artifact open on wide layouts to prefer contextual third pane or modal—not a permanent dual-rail return.
73. As a Principal, I want authenticated Artifact fetch as Principal unchanged, so that blobs stay authorized.

### Thin client, reconnect, non-stories as user expectations

74. As a Principal, I want reconnect to reload durable Worker state then resubscribe SSE, so that the client is never system of record.
75. As a Principal, I want no offline Start Run queue, so that I never believe a Send worked without Worker acceptance.
76. As a Principal, I want model API keys never entered in Console, so that the Worker remains the secret vault.
77. As a Principal, I want Runs started from Console to keep control_plane origin, so that trusted operator power is not confused with Gateway chat.

### Explicit non-stories (as product owner)

78. As a product owner, I do not want multi-agent contact lists, agent group-chat participants, or Workspace-as-chat as 1.0 messaging IA.
79. As a product owner, I do not want dual-rail Inbox+Sessions restored as the default home.
80. As a product owner, I do not want pixel clones of WhatsApp/Telegram/Slack.
81. As a product owner, I do not want flat bubble spam of every tool event as the default timeline.
82. As a product owner, I do not want client-owned message queues or free chat without Runs.
83. As a product owner, I do not want Console to become a Gateway or host an agent loop.

---

## Implementation Decisions

### Scope and relationship to Console 1.0

1. This spec is the **implementation PRD for the messaging shell revamp**. Capability coverage of Console 1.0 (auth, Sessions, Transcript, Runs, Approvals, Memory, Schedules, Skills, Artifacts) remains governed by `docs/specs/0004-console-1.0.md` and ADRs 0013–0030, as amended by ADRs 0031–0034 and this document’s IA changes.
2. Prefer **UI/navigation/composer presentation changes** over Worker API changes. Expand control-plane contracts only if a Must UX cannot be built from existing list/Inbox/Transcript/Approval/Session endpoints.
3. Domain vocabulary from `CONTEXT.md` is normative in code comments, UI copy where domain terms appear, and tests.

### Architecture (non-negotiable)

4. **Worker is system of record.** Console remains a thin Principal client (REST + SSE). No offline mutation queue, no second Transcript model, no client agent loop.
5. **Console is not a Gateway.** Runs started from Console use control-plane origin and full Principal authority subject to Policy.
6. **One Active root Run per Session.** Composer and controllers must refuse silent second root starts (existing controller law stays).
7. **Transcript vs Run events.** Durable conversation truth is Transcript; Run events drive live activity and may rebuild debug views—not the default historical timeline alone (ADR 0015/0025 reaffirmed).

### Information architecture

8. **Replace dual-rail shell** with a **messaging shell**: primary surfaces are Chat list and Thread; Needs you is a system row / focused surface, not a peer permanent rail (ADR 0031).
9. **Chat list contents (top to bottom, conceptual):** optional sticky Needs you system row → Session chat rows (recency or server order). No Workspace tree.
10. **Profile hub** routes: Memory, Skills, Schedules, Settings (and connectivity). Not peer bottom-nav equals of Chats on phone if that recreates multi-rail cockpit; prefer avatar/menu entry to hub.
11. **Session info** is per-thread configuration (title; Policy/Workspace as available from API). Not dumped into Settings.
12. **Single agent identity** in headers and empty states (product name / Soul label if available later). No multi-persona contact model.

### Layout breakpoints

13. **Wide (≥ ~1100):** chat list | thread; optional third pane only when Session info or Artifact is explicitly open.
14. **Medium (~720–1099):** list *or* thread via push/replace navigation.
15. **Narrow (< ~720):** full-screen list → full-screen thread; hub via menu/avatar.
16. Remove permanent dual columns for Inbox + Sessions. Update any shell widget tests that assert simultaneous Inbox and Sessions rails.

### Chat list behavior

17. Session rows bind to existing Session list projection (title, timestamps, last message preview, active root Run summary, pending approval count when present).
18. Selecting a row sets selected Session and shows Thread (wide) or navigates to Thread (narrow/medium).
19. Needs you row uses Inbox projection; badge count = actionable item count (or equivalent). Opening expands/navigates to Needs you content; item actions approve/deny or jump to Session.
20. New chat calls existing create Session, selects it, focuses composer—no wizard screens.

### Thread / transcript presentation

21. **Layered timeline:** map Transcript user/assistant prose to message widgets; map tool/compact structured entries and live activity to collapsible activity blocks.
22. Prefer **operator message rows** (clear author, time, readable body) over mandatory consumer chat bubbles; bubbles are allowed if they preserve density and accessibility.
23. Sticky Approval card sits above composer when the open Session has a pending Approval (from Inbox projection filtered by Session, Session detail, or Approvals list—implementer picks the cheapest correct data path without inventing a Notification aggregate).
24. Live SSE continues to update streaming assistant text and activity; on reconnect, reload Transcript + Session + pending Approvals then resubscribe.

### Composer

25. **Idle primary CTA label/affordance: Send** (icon+Send or equivalent)—not “Start Run” as primary label (ADR 0034). Behavior remains start root Run with text as goal.
26. **Active:** show status that Run is in progress; Cancel; Cancel & re-run with note field; no start path.
27. Hint text should be message-oriented when idle (e.g. message the agent / describe what you need)—avoid pure goal-cockpit wording.
28. Provider/model: keep defaults from Settings/preferences; do not reintroduce dense Run-config chrome in the composer primary path.
29. Keyboard submit (Enter) on idle should Send/start when appropriate for platform conventions; do not start when Active.

### Approvals dual surface (ADR 0033)

30. List-level: Needs you system row + optional per-Session badge.
31. Thread-level: sticky Approve/Deny card.
32. Both call existing approve/deny APIs; refresh Inbox projection and Session state after decision.
33. Full-app modal interrupt is not the default sole pattern (optional later for lock-gate edge cases only—out of this revamp’s Must unless already present).

### Modules to build or modify (conceptual)

34. **App shell:** replace dual-rail navigation widget with messaging shell (list | thread | optional third pane; responsive stack).
35. **Chat list:** Session list UI restyled as messenger rows; integrate Needs you system row; New chat entry.
36. **Needs you surface:** re-home Inbox UI as system-chat / focused list content (same data: Inbox projection).
37. **Thread:** Session detail header (agent + Session title + info entry), layered transcript pane, sticky Approval, composer dock.
38. **Composer:** Send-first idle UX; keep Active lifecycle actions; align copy and tests.
39. **Session info:** new or extracted screen/pane for title (and Policy/Workspace as API allows).
40. **Profile hub:** single entry to Memory, Skills, Schedules, Settings (re-home overflow destinations).
41. **Theme / design tokens:** ensure DESIGN.md messaging IA breakpoints and components; no dual-rail layout tokens as defaults.
42. **Shared chrome widgets:** list rows, badges, empty states, sticky card patterns consistent with messenger density.
43. **Controllers:** Sessions selection, run lifecycle, Inbox fetch—behavior laws stay; navigation wiring changes. Prefer not to invent parallel state stores.
44. **API client package:** touch only if request shapes must change (not expected for Must).

### API contracts

45. **Expected existing endpoints (no change required for Must):** Session list/create/get/patch title; Transcript page; Run start/cancel/events; Approvals list/approve/deny; Inbox get; Memory/Skills/Schedules/Settings as today; Artifacts get.
46. **If Policy/Workspace cannot be edited from Console yet:** Session info shows honest read-only or “configure on Worker” empty state—do not block messaging shell on new Policy APIs unless already available.
47. **Do not** introduce GraphQL/BFF, WebSocket-first rewrite, or Console-local domain DB for this work.

### State machines (normative behavior)

48. Composer mode (refined ADR 0016/0034):

```
idle + non-empty Send → startRun(goal=text) → active
idle + empty Send → error, stay idle
active + Send → refused (UI disabled / error)
active + Cancel → cancelRun → idle (after terminal)
active + Cancel&re-run(note) → cancel then startRun(note or prior intent) per existing controller semantics
```

49. Shell selection:

```
no selection → list empty-state / prompt New chat or select thread
select Session → show thread for that Session
select Needs you → show attention items (not a Session thread)
New chat → create Session → select → empty thread + focused composer
```

### Copy and accessibility

50. Prefer domain terms in operator-facing chrome where already used (Session, Run, Approval); UI may say “chat” / “message” as labels for Session/Send without renaming domain aggregates.
51. Needs you / Approve must be reachable by keyboard/focus order on desktop; touch targets adequate on mobile.
52. Do not use needs-you color as general page chrome fill.

### Migration of existing dual-rail code

53. Dual-rail shell, dual-rail tests, and product copy referring to dual-rail as current IA are **deprecated by this work**. Delete or rewrite rather than leaving a dead dual-rail mode flag unless a temporary feature flag is required for dogfood—default is messaging shell on.
54. Update `console/PRODUCT.md` / `DESIGN.md` are already messaging-oriented; implementation should match them. Parent spec 0004 already amended for 0031–0034.

### Suggested implementation order (non-binding)

55. Messaging shell scaffold + responsive list|thread (replace dual-rail).
56. Chat list rows + New chat + selection.
57. Needs you system row + re-home Inbox actions.
58. Thread header + layered transcript presentation pass.
59. Composer Send-first + Active actions + tests.
60. Sticky Approval card wired to Approvals/Inbox.
61. Profile hub + Session info entry.
62. Artifact open in contextual pane/modal.
63. Empty states, polish, widget test suite update, dogfood.

---

## Testing Decisions

### What makes a good test

- Assert **external behavior** the Principal can observe: labels, presence of Send vs Cancel, layout regions at given widths, Needs you badge/row, refusal of second root Run, navigation from Needs you to thread.
- Prefer **highest seam that keeps failures local**—here, Flutter widget tests with faked credentials/auth and faked or stubbed API where needed.
- Do **not** assert private widget tree structure, Riverpod provider type names, or pixel-perfect WhatsApp clones.
- Do **not** add live Tailnet, live model providers, or Edge E2E as default merge gates for this revamp.
- Catch regressions: dual-rail returning as default home; Start Run as only idle CTA; silent second root Run; Approvals only buried in a Session with no list attention; flat tool spam if presentation helpers regress.

### Primary seam — Flutter widget tests

1. **Shell / layout:** wide width shows chat list + thread regions simultaneously; does **not** require dual permanent Inbox + Sessions rails. Narrow shows list-first navigation. Prior art: existing shell widget tests (rewrite expectations from dual-rail to messaging).
2. **Composer modes:** idle exposes Send; Active exposes Cancel / Cancel & re-run and does not offer a successful second start path. Prior art: composer/run state tests; extend for Send labeling and interactions.
3. **Needs you:** system row/badge appears when Inbox has items (with faked Inbox data); opening shows items.
4. **New chat:** control creates/selects empty Session (with faked API) and focuses thread/composer path.
5. **Sticky Approval:** when pending Approval for selected Session is present in faked state, Approve/Deny controls visible above composer.

### Seams explicitly not required (unless API gap)

6. **Control-plane Seam 1 (in-process Worker):** not required for pure shell/UX revamp. Use if implementer must add/fix an endpoint to support Session info Policy fields or richer pending-approval-by-session data.
7. **OpenAPI ↔ Dart client Seam 4:** not required unless client request/response mapping changes.

### Modules under test (by behavior)

- Messaging shell navigation and breakpoints  
- Chat list + Needs you presentation  
- Composer idle/Active lifecycle  
- Session run controller Active refusal (unit-level OK as today)  
- Sticky Approval wiring against fakes  

### Prior art

- Console widget tests for onboarding, shell, composer state  
- Spec 0004 testing decisions for thin-client and composer lifecycle  
- No new test framework; extend existing Flutter test suite  

---

## Out of Scope

- Multi-agent contact list, Soul marketplace, or agent-as-CRM product  
- Child Runs as group-chat participants / separate DMs  
- Workspace- or project-as-primary-chat-list  
- Dual-rail Inbox + Sessions as default home (superseded)  
- Pixel-faithful WhatsApp, Telegram, or Slack skins  
- Flat chat log of every tool/Run event as default  
- Client-side message queue, offline Start Run, free chat without Runs  
- Steer mid-Run unless control plane already supports it  
- Console as Gateway or `gateway:*` default origin  
- Client-side agent loop or second Transcript SoR  
- Model API key vault in Console  
- Web public hosting, OAuth/SSO multi-tenant, device-paired tokens as gates  
- Full remote-desktop computer-use UI  
- Skills Hub / marketplace  
- Mandatory redesign of Worker hexagonal internals  
- Changing Session/Run/Approval domain aggregates  
- Android/Windows as new Must platforms (unchanged from 0004)  
- i18n, multi-window desktop, custom theme marketplace  
- OS push notification implementation (still Should per 0004; not blocking this shell revamp)  
- Auto-derived advanced title ML; simple first-message/default title rules only  

---

## Further Notes

### Decision index (grill + ADRs)

| Decision | Source |
|----------|--------|
| Messaging-client product metaphor | Grill; ADR 0031 |
| Session = chat; thin system rows | Grill; ADR 0031; CONTEXT Session/Inbox |
| Single agent identity | Grill; ADR 0031 |
| Approvals: Needs you + sticky thread card | Grill; ADR 0033 |
| Layered prose + collapsible activity | Grill; ADR 0015 reaffirmed |
| Idle Send starts root Run; Active explicit only | Grill; ADR 0016 + 0034 |
| Profile hub + Session info | Grill; ADR 0031 |
| Master–detail messenger layout | Grill; ADR 0031/0032 |
| New chat = empty Session, no wizard | Grill; ADR 0034 |
| Messenger principles, not skin | ADR 0032 |
| Supersedes dual-rail / Slack shell IA | ADR 0014 → 0031; ADR 0020 → 0032 |

### Success scenario

Operator opens Console → sees chat list with Sessions and Needs you → New chat → types a message → Send starts a root Run → watches prose + collapsible tools in-thread → receives sticky Approval → approves → opens another Session from the list → later opens hub for Memory/Schedules → kills app → returns with Worker durable state intact.

### Relationship to other docs

- **0004 Console 1.0:** parent capability gate; IA sections already point at 0031–0034—this spec is the detailed implementation PRD for the shell/UX cut of that direction.  
- **PRODUCT.md / DESIGN.md:** visual and product principles for implementers.  
- **CONTEXT.md:** glossary; do not invent parallel product nouns for Session/Run/Approval/Inbox.

### Grill freeze

Product and architecture decisions above were locked in grilling (2026-07-28) and ADRs 0031–0034. Implementation should not reopen metaphor, dual-rail-vs-messaging, multi-agent contacts, Send lifecycle, or Approvals dual-surface without a deliberate ADR supersession and spec edit.

### Test seam confirmation

Primary seam agreed: **Flutter widget tests only** for this revamp, unless an API gap forces Seam 1 / Seam 4.

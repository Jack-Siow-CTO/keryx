# Research: Console 1.0 gap inventory vs daily-use bar

Status: **complete**  
Date: 2026-08-06  
Ticket: [#66](https://github.com/Jack-Siow-CTO/keryx/issues/66) · Parent map: [#64](https://github.com/Jack-Siow-CTO/keryx/issues/64)  
Branch: `research/console-1.0-gap`

## Purpose

Daily-use core locks **full Console 1.0** as the Console bar (not a thinner cut). This note inventories what the Flutter Console tree already satisfies versus the normative Definition of Done in `docs/specs/0004-console-1.0.md` and ADRs 0013–0034.

No product implementation in this ticket.

## Primary sources

| Source | Role |
|--------|------|
| `docs/specs/0004-console-1.0.md` | Normative DoD (rows 1–20), Must endpoints, test seams |
| `docs/specs/0005-console-messaging-shell.md` | Messaging IA implementation PRD |
| ADRs 0013–0034 under `docs/adr/` | Console product/architecture law |
| `console/app/lib/**` | Flutter Principal client |
| `console/packages/keryx_api/**` | Pure Dart control-plane client + Seam 4 tests |
| `docs/api/openapi.yaml` | Checked-in OpenAPI seam (ADR 0024) |
| `crates/api/src/routes.rs` | Worker routes actually mounted |
| `console/app/test/**`, `console/packages/keryx_api/test/**` | Console test seams |
| `console/PRODUCT.md`, `console/DESIGN.md`, `console/README.md` | Product/design register |

## Method

For each inventory area and each DoD Must row: classify **done** / **partial** / **missing** from code and OpenAPI as of this branch tip on `main` (research commit adds this file only).

Status meanings:

- **done** — capability is present end-to-end for the 1.0 Must bar (UI + client + Worker path where required).
- **partial** — skeleton or majority present; a Must slice is still thin, heuristic, undocumentable in OpenAPI, or not auto-wired.
- **missing** — no meaningful Console surface (or required contract) for the Must item.

Should/Out rows are listed for completeness but do not block the daily-use Console bar unless marked Must.

---

## Executive summary

Console 1.0 is **largely scaffolded as a real product surface**, not a stub. Messaging shell, auth/secure storage, Session list/thread/composer, Needs you + sticky Approvals, Memory/Schedules/Skills hub, Artifact basic viewers, provider picker, macOS/iOS targets, and a working `keryx_api` client exist under `console/`.

Gaps that still keep the **full 1.0 DoD** incomplete:

1. **Run status depth** — budgets/errors and a real Child Run tree are weak or heuristic.
2. **Skills “what a Run loaded”** indicators — missing.
3. **Session info Policy/Workspace** — rename only; Policy/Workspace honest empty state, not Worker-backed read/edit.
4. **Live reconnect / app-resume** — no lifecycle-driven reload + SSE resubscribe; refresh is open/manual.
5. **OpenAPI ↔ Dart seam** — Approvals paths absent from OpenAPI; several control-plane resources have path stubs without full schemas; drift tests are path-presence only.
6. **Tests** — API contract + widget tests exist; **golden** coverage for composer/Inbox is missing (DoD Must says widget/**golden**).

Daily-use implication for map #64: Console is not a greenfield. Critical path is **close remaining Must gaps + harden Seam 4**, not rebuild the shell.

---

## Inventory by ticket area

### 1. Auth, secure storage, Tailnet connectivity

| Item | Status | Evidence |
|------|--------|----------|
| Base URL + operator token onboarding | **done** | `console/app/lib/features/auth/onboarding_screen.dart`, `auth_controller.dart` |
| Token in OS secure storage (never plaintext prefs) | **done** | `credentials_store.dart` (`FlutterSecureStorage` for token; SharedPreferences for base URL / lock flag only); `auth_storage_test.dart` |
| Optional biometric / device credential lock | **done** | `device_lock.dart` (`LocalAuthDeviceLock`, fail-closed when unsupported); Settings + onboarding toggles |
| Logout clears secret + caches | **done** | `AuthController.logout` + Settings “Log out” |
| Connectivity/health: unreachable vs auth failure | **done** | `keryx_api` `checkConnectivity` (`/health` then `/v1/providers`); Settings banner |
| Fail closed on invalid token | **done** | `KeryxAuthException` on 401/403; authenticated probes |
| Types allow future per-device tokens | **done** | `KeryxApiConfig.operatorToken` optional; client does not hard-code single-device vault layout |
| Tailnet reachability in-app | **done** (OOB) | Spec: Tailnet join out-of-band; Console only stores base URL (HTTPS Edge or loopback) |
| Push payloads never include token | **done** (N/A) | No push implementation; no token in notifications |
| Model API keys in Console | **Out** | Not present (correct); providers catalog only |

**DoD #1** → **done**.

### 2. Messaging shell: Session list, thread, composer

| Item | Status | Evidence |
|------|--------|----------|
| Chat list home (Sessions as threads) | **done** | `messaging_shell.dart`, `sessions_list.dart` (`ChatListPane`) |
| Needs you system row (not peer dual-rail) | **done** | `_NeedsYouSystemRow` + count badge; ADR 0031/0033 |
| Wide list \| thread (+ optional third pane) | **done** | Breakpoints 1100 / 720 in `MessagingShell` |
| Narrow stack list → thread; hub via menu | **done** | `_stackedLayout` / `_pushSession`; profile hub push |
| New chat = empty Session, no wizard | **done** | `createSession` + open; ADR 0034 |
| Session row: title, time, preview, Active, Approval badge | **done** | `_ChatRow` |
| Rename Session title (durable) | **done** | `SessionInfoPane` → `patchSessionTitle` |
| Open thread with header chips | **done** | `SessionDetailPane` Active / pending Approval pills |
| Composer idle → Send starts root Run | **done** | `composer.dart`, `session_run_controller.dart` |
| Active → cancel / cancel-and-rerun (no silent second root) | **done** | UI + controller guards + `composer_modes_test.dart` / widget tests |
| Provider/model on Send | **done** | `run_preferences.dart` + Settings picker; passed into `startRun` |
| Per-Session drafts only (no offline Start Run) | **done** | In-memory drafts in composer; no offline queue |

**DoD #2, #4, #10** → **done**.

### 3. Transcript layering + live Run events

| Item | Status | Evidence |
|------|--------|----------|
| Durable Transcript as conversation SoR | **done** | `TranscriptPane` loads `GET .../transcript` newest-first, reverse for display |
| Prose messages first-class | **done** | `_ProseMessage` for user/assistant/system |
| Tools as collapsible activity (not bubble spam) | **done** | `_ActivityBlock` with expand + `ToolCompact` |
| Paged scroll-up history | **done** | `before` / `next_before` paging |
| Live model deltas into conversation layer | **partial** | SSE → `streamingText` strip above composer; not merged into Transcript until terminal reload |
| Live tool started/finished on collapsed cards | **partial** | Human lines in `activity` strip (`session_run_controller`); does not mutate Transcript tool rows live |
| Reload Transcript after Run terminal | **done** | `SessionConversationBody` listens Active→terminal → `reloadFromWorker` |
| Reconnect: reload durable state then resubscribe SSE | **partial** | Open Session / manual refresh reloads; no `AppLifecycle` resume observer for sleep/reconnect |
| Artifact refs on tool rows → viewer | **done** | ActionChip → shell artifact pane / push |
| Child Run as collapsible activity (read-only) | **partial** | Heuristic (`name`/`summary` contains “child”); not `parent_run_id` tree |

**DoD #3** → **partial**.

### 4. Needs you / Inbox + in-thread Approvals

| Item | Status | Evidence |
|------|--------|----------|
| Inbox projection API client | **done** | `listInbox` → `/v1/inbox` |
| Needs you pane: Approvals + failed Runs | **done** | `NeedsYouPane`; kinds `approval_pending` / failed |
| Approve / Deny from Needs you | **done** | `approveApproval` / `denyApproval` |
| Sticky in-thread Approval card | **done** | `StickyApprovalCard` (filters Inbox by Session) |
| Deep link Needs you item → Session | **done** | `open(sessionId)` + `onOpenSession` |
| Resolve Approval clears attention (no mark-as-read) | **done** | Refresh Inbox + Session list after decide |
| OS push + deep link | **Should** | **missing** (optional for 1.0) |
| Approvals in OpenAPI | **partial** | Worker mounts `/v1/approvals*`; Dart client implements them; **OpenAPI has Approvals tag only — no paths** |

**DoD #5** → **partial** (product UI mostly done; contract seam incomplete; push Should missing).

### 5. Memory, Schedules, Skills, profile hub

| Item | Status | Evidence |
|------|--------|----------|
| Profile hub entry (not dual-rail) | **done** | `ProfileHubPage` from shell app bar |
| Memory search + list + create/update/delete | **done** | `memory_screen.dart` + client CRUD (list/search/create/update/delete); no separate `getMemory` in client (not required for UI curate) |
| Schedules list/create/pause/resume/delete | **done** | `schedules_screen.dart` + client methods |
| Skills list + view package content | **done** | `skills_screen.dart` |
| Indicate Skills a Run loaded | **missing** | No chips/indicators on Run/Transcript for loaded Skills (DoD #9 Must) |
| Skill in-app edit | **Should** | Not present (correct optional) |
| Skills Hub / marketplace | **Out** | Not present (correct) |

**DoD #7, #8** → **done**.  
**DoD #9** → **partial** (list/view done; Run-loaded indicators missing).

### 6. Session info (Policy / Workspace)

| Item | Status | Evidence |
|------|--------|----------|
| Per-Session title rename | **done** | `session_info.dart` |
| Policy / Workspace visibility or edit | **partial** | Honest banner: not exposed; configure on Worker. No read projection of Policy/Workspace roots |
| Single agent identity copy | **done** | Explicit “Child Runs are not separate contacts” |

Not a separate DoD row; supports US 23b / ADR 0031 hub vs Session info. For daily-use “configure jail from Console,” still **partial**.

### 7. Artifacts

| Item | Status | Evidence |
|------|--------|----------|
| Authenticated get meta + bytes | **done** | `getArtifactMeta` / `getArtifactBytes` with bearer |
| Basic image viewer | **done** | `kind == image` → `Image.memory` |
| Basic terminal / diff / text / json | **partial** | Monospace `SelectableText` for non-image kinds (acceptable as “basic”; not side-by-side diff chrome) |
| Wide contextual pane + narrow push | **done** | `ArtifactViewerPane` / `ArtifactViewerPage` via shell |
| Full remote desktop in Console | **Out** | Not present (correct) |

**DoD #12** → **done** at basic bar (**partial** polish only).

### 8. Platforms (macOS, iOS, …)

| Item | Status | Evidence |
|------|--------|----------|
| Flutter targets macOS | **done** | `console/app/macos/**` |
| Flutter targets iOS | **done** | `console/app/ios/**` |
| Linux desktop | **Should** | **missing** (no `linux/` tree) |
| Android / Windows | **Out** unless free | **missing** (correct non-gate) |
| Store signing / release pipelines | not DoD Must | Not inventoried as ship-ready; scaffolding only |

**DoD #19** → **done** for Must platforms (targets present). Dogfood/packaging quality is outside this code inventory.

### 9. OpenAPI ↔ Dart client seam

| Item | Status | Evidence |
|------|--------|----------|
| Pure Dart package, no Flutter | **done** | `console/packages/keryx_api` |
| Client covers Sessions, Transcript, Runs, SSE, Inbox, Memory, Skills, Schedules, Artifacts, Providers, Approvals | **done** (client) | `client.dart` |
| Checked-in OpenAPI | **partial** | Paths exist for most Must areas; **Approvals paths missing**; Inbox/Memory/Skills/Artifacts/Runs/Schedules often stub responses without full schemas; Memory `{memory_id}` parameters incorrectly `$ref` SessionId |
| Seam 4 drift tests | **partial** | `openapi_drift_test.dart` asserts path keys only; not field-level schema ↔ model parity |
| Contract tests (health, session, SSE) | **partial** | `health_contract_test`, `session_contract_test`, `sse_contract_test` — not exhaustive for Memory/Schedules/Approvals/Artifacts |
| Worker routes vs OpenAPI | **partial** | `crates/api/src/routes.rs` has full Approvals/Memory/Skills/…; OpenAPI lags Approvals |

**DoD #20 (API client contracts)** → **partial**.

---

## Normative DoD matrix (`0004` § Definition of Done)

| # | Capability | Gate | Status | Notes / citations |
|---|------------|------|--------|-------------------|
| 1 | Settings: base URL + token (secure storage), biometric lock, connectivity/health | Must | **done** | ADR 0021; `credentials_store.dart`, `device_lock.dart`, `settings_screen.dart`, `auth_storage_test.dart` |
| 2 | Messaging shell: chat list + Needs you; new chat; open thread | Must | **done** | ADRs 0031, 0027, 0028, 0034; `messaging_shell.dart`, `sessions_list.dart` |
| 3 | Transcript conversation + live SSE activity (collapsed tools) | Must | **partial** | ADR 0015, 0025; Transcript + SSE stream work; live tools are status strip not live-collapsed cards; no app-resume reconnect pipeline |
| 4 | Composer: idle→Send; Active→cancel / cancel-and-rerun | Must | **done** | ADR 0016, 0034; `composer.dart`, `session_run_controller.dart`, tests |
| 5 | Approvals: Needs you + sticky; approve/deny; deep link | Must | **partial** | ADR 0033; UI done; OpenAPI Approvals paths missing; OS push **Should** missing |
| 6 | Run status: Active chip, budgets/errors, Child Run tree (read-only) | Must | **partial** | Active chip done; budget/error surface thin (`result`/activity line only); Child Run heuristic not tree (`parent_run_id` unused in UI) |
| 7 | Memory: search + read + write/curate | Must | **done** | ADR 0029; `memory_screen.dart` |
| 8 | Schedules: list/create/pause/resume/delete | Must | **done** | `schedules_screen.dart` + Worker routes |
| 9 | Skills: list/view; indicate what a Run loaded | Must | **partial** | ADR 0030; list/view done; **Run-loaded indicators missing** |
| 10 | Provider/model picker per Run | Must | **done** | Settings prefs + composer labels; secrets stay on Worker |
| 11 | Provider secret entry in Console | Out | **done** (absent) | Correct non-implementation |
| 12 | Rich tool viewers: diff, terminal, screenshot (basic) | Must | **done** | ADR 0026; basic monospace + image; not advanced diff UI |
| 13 | Soul + context file editor | Should | **missing** | No Console surface |
| 14 | MCP status / enable UI | Should | **missing** | No Console surface |
| 15 | Multi-window, custom themes, i18n | Out | **done** (absent) | |
| 16 | Offline write queue / local DB replica | Out | **done** (absent) | Thin client ADR 0019 |
| 17 | Device-paired tokens, OAuth | Out | **done** (absent) | |
| 18 | Web Console / public hosting | Out | **done** (absent) | |
| 19 | Platforms macOS + iOS Must; Linux Should | Must/Should | **done** / Linux **missing** | `macos/`, `ios/` present; no `linux/` |
| 20 | Tests: API contracts + widget/golden composer/Inbox | Must | **partial** | Contracts + widget tests present; **no golden files**; Inbox covered in widget tests not golden |

### Count (Must rows only)

| Status | DoD Must rows |
|--------|----------------|
| **done** | 1, 2, 4, 7, 8, 10, 12, 19 |
| **partial** | 3, 5, 6, 9, 20 |
| **missing** | (none fully missing among Must) |

Must capabilities are **not fully closed** until partial rows clear. No Must row is pure greenfield **missing**.

---

## Worker control-plane vs Console client (API readiness)

Illustrative Must endpoints from `0004` vs presence:

| Area | Worker (`routes.rs`) | OpenAPI | Dart client | Console UI |
|------|----------------------|---------|-------------|------------|
| Sessions list/get/create/patch | yes | yes (schemas solid) | yes | yes |
| Transcript page | yes | yes (schemas solid) | yes | yes |
| Runs start/get/cancel/SSE | yes | path stubs | yes | yes |
| Approvals list/approve/deny | yes | **no paths** | yes | yes (via Inbox + sticky) |
| Inbox projection | yes | path stub | yes | yes |
| Memory CRUD/search | yes | path stub; param bug | yes (no get-by-id) | yes |
| Skills list/get | yes | path stub | yes | yes (list/view) |
| Schedules CRUD lifecycle | yes | path stub | yes | yes |
| Artifacts get | yes | path stub | yes | yes |
| Providers list | yes | yes | yes | yes |
| Health | yes | yes | yes | yes |

Worker side for Console Must is largely present (prior Seam 1 suite under `crates/api/tests/seam1_*`). Console-blocking risk is **contract completeness and UI depth**, not “API does not exist.”

---

## Test seam inventory

| Seam | Spec intent | Present? |
|------|-------------|----------|
| Seam 1 control-plane in-process | Worker merge gate for Console APIs | Yes (`crates/api/tests/seam1_*`) — outside `console/` but healthy prior art |
| Seam 4 OpenAPI ↔ Dart | Drift failure before UI lies | Partial path checks only |
| Widget / golden composer + Inbox | Presentation regressions | Widget yes; **golden no** |
| Integration vs local Worker | Should | Not in default package tests |

App tests: `auth_storage_test`, `composer_modes_test`, `messaging_shell_test`, `shell_widget_test`.  
API tests: `health_contract_test`, `session_contract_test`, `sse_contract_test`, `openapi_drift_test`.

---

## Explicit non-gaps (correct absences)

Per Out / Should boundaries, do **not** treat these as daily-use Console blockers:

- Offline Start Run / local Transcript replica  
- Provider secret entry / OAuth / multi-tenant  
- Skills marketplace or full CMS  
- Full remote-desktop computer-use UI  
- Web public Console  
- Android/Windows as Must  
- Pixel clone of WhatsApp/Telegram/Slack  
- Dual-rail home (superseded by messaging IA; code uses messaging shell)  
- OS push (Should)  
- Soul editor / MCP admin (Should)  
- Steer mid-Run until control plane API exists  

---

## Critical path (for map #64 planning only)

Ordered by DoD leverage for a daily-use Console bar. Not an implementation ticket list.

1. **Close DoD #6** — surface budget/failure reasons; real Child Run linkage from Run/Transcript fields (not string heuristics).  
2. **Close DoD #9** — Run-loaded Skill indicators (needs Worker event/Transcript fields if not already emitted).  
3. **Harden DoD #3** — app-resume/reconnect: reload Session/Transcript/Inbox, resubscribe Active SSE; optionally fold live tool updates into activity cards.  
4. **Close DoD #5/#20 contract** — publish Approvals (+ fuller schemas) in `docs/api/openapi.yaml`; expand Seam 4 drift beyond path presence; golden tests for composer modes + Needs you.  
5. **Session info** — if daily-use needs Policy/Workspace visibility, add read projection; do not invent client-side Policy store.  
6. **Polish DoD #12** only if basic viewers prove unusable for terminal/diff review in dogfood.

Shell, auth, composer law, Memory/Schedules, and hub IA are **not** the critical rebuild path.

---

## ADR index (still binding)

| ADR | Decision | Inventory note |
|-----|----------|----------------|
| 0013 | Console = primary operator surface | Shell is primary client shape |
| 0014 → 0031 | Messaging chat-list IA | Implemented; dual-rail gone |
| 0015 | Conversation + activity layers | Implemented; live layer partial |
| 0016 / 0034 | Composer Run modes + new chat | Implemented + tested |
| 0017 | REST + SSE; push = Inbox wakeup | REST+SSE yes; push no |
| 0018 | Flutter multi-platform | macOS/iOS targets present |
| 0019 | Strict thin client | Observed (no offline queue) |
| 0020 → 0032 | Messenger principles, not skin | Theme/chrome present |
| 0021 | Secure token + biometric | Done |
| 0022 | Big-bang 1.0 | DoD partials still open |
| 0023–0030 | Control-plane expansions | Worker largely present; OpenAPI lag on Approvals |
| 0033 | Approvals dual surface | UI done |

---

## Conclusion

Against **full Console 1.0 as the daily-use Console bar**, the tree under `console/` is a **real messaging Principal client with most Must surfaces present**. Remaining work is **depth and contract hygiene** on five Must rows (3, 5, 6, 9, 20), not a new product skeleton.

**Gist:** Console 1.0 shell is mostly built; finish Run depth (budget/Child tree), Skill load indicators, reconnect, OpenAPI Approvals + golden/contract tests before calling the daily-use Console bar closed.

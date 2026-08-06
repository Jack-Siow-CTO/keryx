# Research: Worker gap inventory vs daily-use core

**Ticket:** [#65](https://github.com/Jack-Siow-CTO/keryx/issues/65)  
**Parent map:** [#64](https://github.com/Jack-Siow-CTO/keryx/issues/64)  
**Branch:** `research/worker-daily-use-gap`  
**Date:** 2026-08-06  
**Scope:** Rust Worker (control plane + app + adapters) vs **daily-use core** bar.  
**Sources:** `CONTEXT.md`, `crates/**`, `crates/api/tests/seam1_*.rs`, `docs/specs/0002-keryx-v2-agent-os.md`, ADRs (esp. 0012, 0029–0030, 0006–0007), `docs/api/openapi.yaml`.  
**Not this note:** Console Flutter product gaps, full ADR 0012 long-tail (browser, computer-use, Discord, execute_code, media zoo, multi-tenant, Skills Hub, external MCP life-stack as ship gate).

## Destination bar (daily-use core)

From map #64 / CONTEXT:

| Surface | Bar |
|---------|-----|
| Control plane | Sessions, root Runs, cancel, budgets, SSE; Child Runs; Policy + Run origin + Approvals; Memory + Soul/context; Skills full (packages + learning loop, trusted auto-apply only); Schedules; host work under Policy/Approval |
| Clients | Console 1.0 primary (Worker APIs that Console needs); CLI ops fallback (approve, schedule, doctor) |
| Away | Telegram Gateway **feature** parity under **reduced** Policy (capture, results, Approvals participation) — **not** Policy parity with Console |
| Deploy | Always-on `jack-agent-worker` + Tailnet Edge L4/L5 (topology assumed; not re-audited here) |
| Self-contained | MCP client **presence** only; external MCP servers not a ship gate |

Status legend: **done** | **partial** | **missing**.

---

## Gap matrix (summary)

| # | Daily-use area | Status | One-line |
|---|----------------|--------|----------|
| 1 | Session / root Run / cancel / budgets / SSE | **done** (small partials) | Spine ships; budget “nearing” and per-run budget override thin |
| 2 | Child Runs | **partial** | Core orchestration + Seam 1; no agent tool / HTTP spawn; depth-1 only |
| 3 | Policy, Run origin, Approvals | **done** (small partials) | Origin templates + Approval queue; no mid-run escalate API |
| 4 | Workspace + terminal + web tools | **partial** | FS/patch/search + terminal local/Docker + web extract; live `web_search` unconfigured; Docker cwd mount weak |
| 5 | Memory + FTS + Soul/context | **done** | Tools + REST + FTS5 + reduced-origin write deny + soft Soul/context attach |
| 6 | Skills packages + learning loop + trusted auto-apply | **partial** → **missing** core loop | Read-mostly GET only; no skill tools, no learning loop, no auto-apply |
| 7 | Schedules | **partial** | CRUD + durable fire semantics via explicit tick; **no Worker auto-ticker**; frozen `policy_tools` not applied on fire |
| 8 | Telegram Gateway (feature parity, reduced Policy) | **partial** | Capture + reduced origin + result reply; **no Approvals participation** |
| 9 | MCP client (presence) | **done** | Config-driven client + Policy + doctor; server export mock-only (not daily gate) |
| 10 | CLI ops fallback | **partial** | approve/deny + list schedules + thin doctor; no schedule mutate; SessionShow stub |

---

## 1. Session / root Run / cancel / budgets / SSE

### Status: **done** (small partials)

| Capability | Evidence | Status |
|------------|----------|--------|
| Create / list / get Session; patch title | `crates/api/src/routes.rs` list/get/patch/create; `seam1_session_list_projection.rs`, `seam1_hello_run.rs` | done |
| Start root Run (HTTP → `control_plane` origin) | `routes.rs` `start_run`; `service.rs` `start_run` / `start_run_with_origin` | done |
| One Active root per Session; global cap | `service.rs` `start_run_with_origin` ActiveRunRegistry; `seam1_concurrency_budgets_cancel.rs` | done |
| Cancel Active Run | `cancel_run` + cancel tree; seam1 cancel test | done |
| Budgets: time / tokens / tool calls (exceed → fail) | `limits.rs` `RunBudgets`; agent loop `service.rs` ~1140–1320; seam1 budget tests | done |
| Budget “nearing” signals (spec 0002 §27) | Only `RunBudget` on exceed | partial / missing |
| SSE taxonomy + reconnect via GET Run | `domain/events.rs`; `routes.rs` stream; `seam1_sse_run_events.rs`; approval/child events named | done |
| SQLite durability; Active → Interrupted on reopen / shutdown | `storage/sqlite.rs`; `worker/main.rs` interrupt on shutdown; `seam1_sqlite_durability.rs` | done |
| Paged structured Transcript | `get_transcript_page`; `seam1_transcript_page.rs` | done |
| Auth fail-closed | `api/auth.rs`; hello + many seam1 unauth tests | done |
| Health on loopback | `GET /health`; worker bind loopback config | done |

**OpenAPI:** `/v1/sessions`, runs, events — `docs/api/openapi.yaml`.

**Gap notes**

- Spec “budget nearing” is not emitted; only hard exceed.
- Default `RunBudgets::unlimited()` unless Worker configures limits (`limits.rs` default).
- Per-request budget fields on start-run are not in the HTTP contract (Worker default only).

---

## 2. Child Runs

### Status: **partial**

| Capability | Evidence | Status |
|------------|----------|--------|
| Domain: `parent_run_id`, `start_child`, `is_root` | `domain/run.rs` | done |
| Spawn under Active root; Policy freeze snapshot; budget carve | `service.rs` `spawn_child_run_inner` ~844–963; `Policy::subset_of`; `RunBudgets::carve_for_child` | done |
| Cancel root → children | `cancel_run` + registry cancel tree; `seam1_child_runs.rs` `cancel_root_cascades_to_children` | done |
| SSE `child_run.started` / `finished` | `domain/events.rs` 40–48, 73–74 | done |
| GET Run exposes `parent_run_id` + origin | `routes.rs` `RunResponse` 289–313; seam1 HTTP linkage test | done |
| One Active **root** while children run | `seam1_child_runs.rs` `session_still_one_active_root_while_child_runs` | done |
| Agent-callable spawn tool | No tool in `crates/tools`; no catalog entry | **missing** |
| HTTP spawn endpoint | Not in `router()` / OpenAPI | **missing** (internal `ControlPlaneService` only) |
| Nested Child Runs (child-of-child) | Explicit reject “only root may spawn” `service.rs` 857–861 | partial (depth-1 by design for now) |
| Isolated transcript **slice** | Comment claims isolation; loop still `append_transcript(session_id, …)` with `run_id` on messages — shared Session Transcript, not a separate store | partial |

**Seam 1:** `crates/api/tests/seam1_child_runs.rs` (spawn, exclusivity, cancel cascade, budget carve, HTTP parent linkage).

**Daily-use impact:** Child Run **runtime** exists for tests and internal callers. An operator-facing agent cannot delegate via tool today. Closing the bar needs at least a Policy-gated spawn tool (and likely HTTP only if Console/CLI must spawn without model).

---

## 3. Policy, Run origin, Approvals

### Status: **done** (small partials)

| Capability | Evidence | Status |
|------------|----------|--------|
| Run origin wire forms | `domain/origin.rs` (`control_plane`, `schedule`, `gateway:{platform}`) | done |
| Origin Policy templates (control_plane vs reduced) | `domain/policy.rs` `for_origin` / `control_plane_default` / `reduced`; unit tests 119–202 | done |
| HTTP start stamps `control_plane` | `seam1_run_origin_policy.rs` | done |
| Reduced gateway/schedule deny write / MCP by default | same seam1 + policy tests | done |
| Reduced: no local terminal (force docker) | `service.rs` 1338–1375; `seam1_terminal.rs` | done |
| High-blast → durable Approval wait; approve/deny; fail-closed deny/timeout | `approval_broker.rs`; `service.rs` `request_and_wait_approval`; `seam1_approvals.rs` | done |
| SSE `approval.waiting` / `resolved` | `domain/events.rs` | done |
| Inbox projection (pending Approvals + failed/interrupted) | `service.rs` `list_inbox`; `GET /v1/inbox`; ADR 0028; seam1 console | done |
| Config high-blast tool set + MCP high-blast | `with_high_blast_tools`; MCP config `high_blast` | done |
| Mid-run Policy escalate (trusted control-plane) | Spec 0002 §39; no API found | **missing** |
| Schedule frozen `policy_tools` applied at fire | Stored on create (`schedule.rs`, sqlite); **tick uses origin only** (`tick_schedules` 551–560) | **partial** |

**Seam 1:** `seam1_run_origin_policy.rs`, `seam1_approvals.rs`, MCP high-blast in `seam1_mcp.rs`.

---

## 4. Workspace + terminal (local/Docker) + web tools

### Status: **partial**

| Capability | Evidence | Status |
|------------|----------|--------|
| `read_file` / `write_file` path jail | `tools/workspace.rs`; `seam1_workspace_tools.rs` | done |
| `apply_patch` / `search_files` | same | done |
| `run_terminal` local + Docker backends | `tools/terminal.rs` `SystemExecRunner`; worker wires `TerminalTools` | done |
| Control-plane local → Approval | `is_high_blast_local_terminal`; `seam1_terminal.rs` | done |
| Reduced origin: local denied, docker allowed (double) | seam1 terminal tests | done |
| cwd jail under workspace roots | terminal + seam1 `cwd_escape_denied` | done |
| Docker volume/cwd map into container | `terminal.rs` 91: `let _ = cwd` — not mounted | partial |
| Process list/kill for agent processes | Spec 0002 §54 | **missing** |
| `web_extract` + SSRF private IP deny | `tools/web.rs`; `seam1_web_tools.rs` | done |
| `web_search` pluggable | Trait + Fixed double in tests | partial |
| Production `web_search` provider | Worker `build_tools` uses `UnconfiguredWebSearch` (`worker/main.rs` 461–462) | **partial** (tool present, live search fails closed until provider wired) |

**Daily-use impact:** Host file work and terminal under Policy/Approval are largely livable. Web search needs a real provider configuration path for “search the public web” days. Browser / computer-use remain **out of daily-use scope** (map #64).

---

## 5. Memory + FTS + Soul/context

### Status: **done**

| Capability | Evidence | Status |
|------------|----------|--------|
| Memory domain + tools (read/write/update/delete/search) | `domain/memory.rs`; `tools/memory.rs` | done |
| Control-plane Memory REST | `routes.rs` `/v1/memory*`; ADR 0029; `seam1_console_control_plane.rs`, `seam1_memory.rs` | done |
| SQLite FTS5 | `storage/sqlite.rs` `memory_fts`; `seam1_memory.rs` `sqlite_memory_tools_e2e_with_fts` | done |
| `session_search` over Transcripts | Memory tools + policy allowlist | done |
| Reduced origin denies memory write | Policy reduced; `seam1_memory.rs` `reduced_origin_denies_memory_write` | done |
| Provenance fields | `MemoryEntry` source_run / principal | done |
| Soul + Context attach (soft-missing) | `app/context.rs`; `service.rs` load once per root Run; `seam1_soul_context.rs` | done |
| High-blast protect Soul/context edits | `is_high_blast_soul_context_edit`; seam1 deny tests | done |
| Distinct labels Soul ≠ Memory ≠ Skill | soul context tests | done |

**Minor partials (not daily blockers):** Child Runs skip Soul re-attach by design (`spawn_child_run_inner` default context). No Console-specific Soul edit API (file-on-disk is enough for map “not yet specified”).

---

## 6. Skills packages + learning loop + trusted auto-apply

### Status: **partial** inventory surface; **missing** full Skills bar

| Capability | Evidence | Status |
|------------|----------|--------|
| Skills root config + doctor check | `KERYX_SKILLS_ROOT`; `worker/main.rs` doctor 305–310 | done |
| Console read-mostly list/get `SKILL.md` | `routes.rs` `list_skills` / `get_skill` ~708–762; ADR 0030; `seam1_console_control_plane.rs` `skills_list_and_get` | done (read-only) |
| Agent tools: skills_list / skill view / load / manage | **No** implementations under `crates/tools` | **missing** |
| Learning loop (draft create/improve from experience) | No module / loop hook in agent loop | **missing** |
| Trusted auto-apply only (control_plane + Policy + setting) | Spec 0002 §74–76; no auto-apply path | **missing** |
| Gateway draft/propose only for skill mutation | N/A without manage tools | **missing** |
| High-blast hook for `skill_manage` name | `service.rs` 1382: approval if name matches — **no tool registered** | dead hook |

**Daily-use impact:** This is the largest intentional product gap vs map #64 (“full Skills including learning loop with trusted auto-apply only”). Today the Worker only **exposes on-disk packages for Console browse**. Progressive disclosure tools and the always-on learning loop are not implemented.

---

## 7. Schedules

### Status: **partial**

| Capability | Evidence | Status |
|------------|----------|--------|
| Domain Schedule + pause/resume/delete/fire math | `domain/schedule.rs` (missed-fire single-step, no storm) | done |
| SQLite durability | `storage/sqlite.rs` schedules table; `seam1_schedules.rs` reopen | done |
| Control-plane CRUD + pause/resume/delete | `routes.rs` `/v1/schedules*`; OpenAPI | done |
| Fire with `origin=schedule` → reduced Policy | `tick_schedules` + seam1 `tick_fires_run_with_origin_schedule_reduced_policy` | done |
| Double-fire / overload skip | `tick_schedules` last_fired_at + ActiveRunExists skip | done |
| **Worker background ticker** | **No** `tokio::spawn` ticker in `worker/main.rs` serve path | **missing** for always-on trust |
| Explicit tick API | `POST /v1/schedules/tick` (deterministic `now` for tests) | done (ops/test path only) |
| Frozen `policy_tools` on fire | Stored; fire path ignores (uses `Policy::for_origin(Schedule)`) | **partial** |
| Gateway notify of schedule results | Spec §84; no notify wiring | **missing** (optional for daily if Console/Telegram poll) |
| CLI schedule create/pause | Only `SchedulesList` in `cli/main.rs` | partial (see §10) |

**Daily-use impact:** Schedule **data plane** and fire **semantics** are implemented and tested, but an always-on Worker will **not** fire Schedules unless something calls `/v1/schedules/tick` (or a future internal loop). That blocks “trustworthy Schedules” for unattended mornings until a ticker lands in the Worker composition root.

---

## 8. Telegram Gateway (feature parity under reduced Policy)

### Status: **partial**

Daily bar: capture, results, Approvals participation — **not** Policy parity with Console.

| Capability | Evidence | Status |
|------------|----------|--------|
| Live long-poll Gateway | `gateway/telegram_live.rs` `run_telegram_long_poll`; spawned from `worker/main.rs` when token set | done |
| Fail closed empty/invalid bot secret (runtime constructor) | `gateway/lib.rs` `GatewayRuntime::new` | done |
| Chat allowlist | `ChatAllowlist::from_env_csv` | done |
| Capture → Session continue + Run `gateway:telegram` | `handle_message_e2e` start_run_with_origin | done |
| Reduced Policy by origin | Domain Policy + origin tests | done |
| Ack + final result reply | `… working` then terminal result text | done |
| Multi-turn chat → Session map | `ChatSessionMap` | done |
| Seam 3 fixture parse + origin | `gateway/lib.rs` tests telegram_inbound_maps_to_gateway_origin | done |
| **Approvals participation** from Telegram | No list/approve/deny path in Gateway; e2e only polls `get_run` until terminal | **missing** |
| Clarify prompts to chat | No clarify tool / delivery path | missing (spec; daily optional) |
| Media / voice | Out of daily-use map | out of scope here |
| Discord live Gateway | Worker comment: not wired; daily map defers Discord | out of scope |

**Daily-use impact:** Away-from-desk **capture + answers** work under reduced Policy. If a Run blocks on Approval, Telegram will wait until timeout/cancel without letting the operator approve in-chat — **Approvals participation** is the main Telegram gap for the bar.

---

## 9. MCP client (presence only)

### Status: **done** (for daily-use presence)

| Capability | Evidence | Status |
|------------|----------|--------|
| Config load + stdio/remote client | `tools/mcp/*`; `build_mcp_runtimes` | done |
| Namespaced tools; connect ≠ allow | Policy extras / allowlist; `seam1_mcp.rs` | done |
| Reduced origin no MCP by default | policy + seam1 | done |
| High-blast MCP + disconnect fail-closed + secrets redaction | seam1_mcp | done |
| Worker doctor MCP lines | `worker/main.rs` doctor | done |
| MCP **server export** production bind | `McpServerExport` mock/auth unit only (`mcp/mock.rs`); no Worker serve mode | not required for daily bar |

External life-stack MCP is **not** a ship gate. Client presence is sufficient.

---

## 10. CLI ops fallback (approve, schedule, doctor)

### Status: **partial**

| Capability | Evidence | Status |
|------------|----------|--------|
| `keryx-cli` approve / deny / approvals list | `cli/main.rs` | done |
| Run start / show / cancel / events | same | done |
| Session create | same | done |
| Session show | Stub prints id only (comment: thin API); control plane **has** GET session | **partial** / stale |
| Schedules list | `SchedulesList` | done |
| Schedule create / pause / resume / delete / tick | not in CLI | **missing** |
| CLI `Doctor` | Health + providers only (`cli/main.rs` 217–232) | partial |
| Worker `keryx doctor` | Full readiness: bind, tokens, data dir, providers, workspace, Soul, skills, Telegram, Docker, MCP (`worker/main.rs` `doctor`) | **done** (binary, not CLI client) |
| Line TUI slash approve/cancel | `Commands::Tui` | partial (basic) |

**Daily-use impact:** Ops can approve high-blast work and list schedules over SSH via `keryx-cli`. Creating schedules and deep doctor checks prefer `curl` / `keryx doctor` on the host today.

---

## Cross-cutting: always-on Worker + Tailnet

| Item | Status | Note |
|------|--------|------|
| Long-running serve composition root | done | `worker/main.rs` `Serve` |
| Loopback bind default | done | config + doctor |
| Graceful interrupt Active Runs | done | shutdown path |
| Telegram optional task | done | env token |
| Schedule auto-fire loop | **missing** | see §7 |
| Tailnet Edge (Caddy etc.) | deploy docs exist (`docs/deploy/tailnet-edge.md`); not re-validated in code inventory | assume deploy path for L4/L5 |

---

## Largest gaps vs daily-use core (priority for map #64 critical path)

1. **Skills learning loop + agent skill tools + trusted auto-apply** (§6) — only read-mostly packages today.  
2. **Schedule always-on ticker** (+ apply frozen `policy_tools` on fire) (§7).  
3. **Telegram Approvals participation** (§8).  
4. **Child Run agent tool** (and optional HTTP) (§2).  
5. **CLI schedule mutate + SessionShow fix; richer CLI doctor or document `keryx doctor`** (§10).  
6. **Live `web_search` provider wiring** if public web is in the host-work floor (§4).  

Already strong for daily spine: Session/Run/cancel/SSE, Policy/origin/Approvals, Memory/FTS/Soul, workspace+terminal, MCP client presence, Telegram capture/results.

---

## What is explicitly not graded as daily-use missing

Per map #64 out of scope (do not treat as daily gap):

- Isolated browser / computer-use  
- Discord Gateway as release criterion  
- In-process `execute_code`  
- External MCP life-stack integrations as ship gates  
- Telegram **Policy** parity with Console  
- Multi-tenant Principal product  
- Skills Hub / vector DB / Honcho-class modeling  
- Media/TTS/vision zoo  

---

## Source index (primary)

| Kind | Paths |
|------|--------|
| Glossary | `CONTEXT.md` |
| Spec | `docs/specs/0002-keryx-v2-agent-os.md` |
| ADR product | `docs/adr/0012-v2-personal-agent-os.md`, 0027–0030, 0006–0007 |
| API | `docs/api/openapi.yaml`, `crates/api/src/routes.rs` |
| App | `crates/app/src/service.rs`, `limits.rs`, `context.rs`, `approval_broker.rs` |
| Domain | `crates/domain/src/{run,policy,origin,approval,schedule,memory,events}.rs` |
| Tools | `crates/tools/src/{workspace,terminal,web,memory,mcp}/**` |
| Storage | `crates/storage/src/sqlite.rs` |
| Gateway | `crates/gateway/src/{lib,telegram_live}.rs` |
| Worker / CLI | `crates/worker/src/main.rs`, `crates/cli/src/main.rs` |
| Seam 1 | `crates/api/tests/seam1_*.rs` (17 files) |

---

## Gist (one line for map)

Worker spine (Session/Run/SSE/Policy/Approvals/Memory/Soul/terminal/Telegram capture) is largely in place; largest daily-use holes are Skills learning loop + agent skill tools, Schedule auto-tick (and frozen policy apply), Telegram Approvals participation, and Child Run agent-facing spawn.

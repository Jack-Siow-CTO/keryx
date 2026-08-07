# Daily-use core as current ship bar

Status: **accepted** (grill 2026-08-06). **Companion** to [ADR 0012](0012-v2-personal-agent-os.md); does **not** supersede ADR 0012. Freezes the **current livable ship bar** for personal daily use. Full ADR 0012 breadth stays long-term ambition (later maps), not cancelled.

## Decision

Until a later ADR moves the bar, Keryx is **done enough for daily life** when **daily-use core** passes — not when every ADR 0012 capability ships.

**Daily-use core (in the bar):**

- **Console 1.0** as primary operator surface (full Console 1.0 Definition of Done).
- **Telegram** away-from-desk **feature** parity (capture, results, Approvals participation) under **reduced Policy** — not Policy parity with Console.
- Host work under Policy and **Approval**: Workspace **filesystem + terminal** (live web search/extract is not the host-work floor).
- **Child Runs** with **agent-facing spawn**.
- **Memory** + **Soul** / workspace context files.
- Full **Skills** (packages + always-on learning loop; trusted auto-apply only; factory auto-commit **OFF**; proposals are Approvals).
- Trustworthy **Schedules** (origin `schedule`, reduced Policy).
- Always-on **Worker** + Tailnet **Edge** (operator levels 4–5 on `jack-agent-worker`).
- Self-contained Keryx — no external life-stack MCP as a ship gate.

**Program carrier:**

- Whole daily-use program (sole open carrier): issue [Keryx v2 — daily-use core (ship bar)](https://github.com/Jack-Siow-CTO/keryx/issues/13).
- Console 1.0 Must carrier [Keryx Console 1.0](https://github.com/Jack-Siow-CTO/keryx/issues/37) is **closed** (collapsed into #13). Residual Console work is children of #13. Normative Console behavior remains `docs/specs/0004-console-1.0.md` + ADRs 0013–0034.

**Later (ADR 0012 ambition; not this ship gate):** isolated browser and computer-use; Discord Gateway; in-process `execute_code`; external life-stack MCP; Telegram Policy parity; multi-tenant / multi-human Principal; Skills Hub; vector DB / Honcho-class modeling; media/TTS/vision zoo; Funnel or public Edge; CLI/TUI completeness as Console fallback ship gate; live `web_search` / web extract as host-work floor.

## Why

ADR 0012 correctly set personal-agent-OS ambition. Treating the full surface as the next ship gate blocks a livable day: capture → host work → approve → remember → schedule on Console primary, Telegram away, always-on Worker. A companion ship-bar ADR freezes product truth for implementation without cancelling long-tail vision or inventing a second destination next to open v2 issues.

## Handoff for implementers

Ship **daily-use core** per this ADR and its checklist appendix. **Program carrier: issue #13 only** (Console Must carrier #37 closed; residual Console tickets are children of #13). Other product freezes (Telegram Approvals, Skills defaults, gap inventories, sequencing): [Wayfinder: Keryx daily-use core](https://github.com/Jack-Siow-CTO/keryx/issues/64) Decisions so far. Do **not** treat full ADR 0012 as the next-ship gate.

**Critical path** ([Grilling: Implementation critical path sequencing](https://github.com/Jack-Siow-CTO/keryx/issues/72)):

1. **Thin ops** on `jack-agent-worker`: refresh provider auth; prove Tailnet Edge `:8443` from a second tailnet node; rotate Telegram bot token; document Edge URL; stand up host CI job shell (may stay red).
2. **Worker product order:** Skills learning loop (first product slice; Console/CLI Approvals) → Approvals surfaces (OpenAPI + Telegram) → Schedule always-on ticker + frozen tools on fire → Child agent-facing spawn.
3. **Console:** contract-gated parallel under #13; daily-true when OpenAPI Approvals, Skill load indicators, Child/Run depth, reconnect, and required goldens are green (checklist line 1) — does not wait for Schedule ticker or Telegram.
4. Cut implementation tickets under #13 only (not under closed #37; not as new wayfinder children).

## Consequences

- ADR 0012 remains **accepted** ambition; readers must not require full 0012 includes for “daily-use reached.”
- Acceptance is **CI-only**: required GitHub Actions suite **and** required host job on `jack-agent-worker` both green on the production-truth branch (`main` unless renamed). Manual smoke may debug; it does not declare the bar.
- Normative checklist lives **only** in the appendix below. Deploy docs and issues may link; they must not hold a second authoritative table.

## Appendix: Daily-use acceptance checklist

Pass/fail only. Automation owns the claim.

### Shape

| Decision | Value |
|----------|--------|
| Grain | Hybrid: 8 capability lines × one named scenario each |
| Who declares pass | **CI/automated only** (manual smoke may debug; does not declare the bar) |
| Suites | **Split, one bar** — required **GHA** (product / control plane / Console contracts + goldens) + required **host job on jack-agent-worker** (L4/L5, Telegram E2E, schedule ticker under systemd) |
| Bar reached | Both required suites green on production-truth branch (`main` unless renamed) — no human checklist ceremony, no release-tag gate for the claim |

### Checklist body

| # | Line | Owner | Pass scenario |
|---|------|-------|----------------|
| 1 | Console 1.0 primary workflows | GHA | **Messaging day path** — auth → Session list → open thread → Send starts root Run → Transcript shows prose + collapsible activity → sticky Approval when pending → Needs you surfaces the same Approval; OpenAPI Approvals paths + goldens green |
| 2 | Telegram under reduced Policy | Host | **Away Approvals** — allowlisted chat: capture → Run under reduced Policy → high-blast: bot notifies + Approve/Deny; Approve continues without Policy escalate; Deny fails closed; non-allowlisted chat does nothing |
| 3 | Host work under Policy/Approval | GHA | **Gated tools** — control-plane Run may use Workspace **FS + terminal** under Policy; high-blast requires Approval before effect; deny/cancel leaves no unauthorized side effect. **Live web_search/extract not required** on this bar |
| 4 | Child Runs | GHA | **Spawn + budget** — **agent-facing spawn** (tool or equivalent product path); child has own budget; cancel/parent stop bounded; Console or API projection shows child under parent |
| 5 | Memory + Soul/context | GHA | **Survive restart** — write Memory + load Soul/context; restart Worker process; same path still sees Memory and Soul (SQLite durable fixture) |
| 6 | Skills load + learning loop | GHA | **Draft → Approve → load** — auto-commit OFF; Console-origin Run produces create/improve proposal → pending Approval → Approve writes package under skills root with `SKILL.md` → Skills list shows it → later Run loads it; Deny leaves root unchanged for that proposal (aligned with Skills learning-loop defaults) |
| 7 | Schedules | Both | **Fire with origin** — create Schedule; tick fires a Run with origin `schedule` and reduced Policy (frozen tools applied); GHA proves semantics; host job proves always-on ticker under systemd Worker |
| 8 | Always-on Worker + Tailnet | Host | **Always-on + Edge** — user systemd Worker active + health on loopback; Tailnet Edge `:8443` returns health/auth challenge from a **second** tailnet node (not only on-host); provider auth valid enough for one diagnostic Run. No Funnel; Mac/phone UI not required beyond second-node HTTPS proof |

### Explicit non-requirements (not on this checklist)

Isolated browser and computer-use; Discord Gateway; in-process `execute_code`; external life-stack MCP; Telegram **Policy** parity with Console; multi-tenant / multi-human Principal; Skills Hub marketplace; vector DB / Honcho-class modeling; media/TTS/vision zoo; Funnel or public Edge; mandatory skills seed pack; CLI/TUI as Console fallback ship gate; live `web_search` / web extract as host-work floor.

### Grounding

- Worker / Console / L4–L5 gap research ([#65](https://github.com/Jack-Siow-CTO/keryx/issues/65)–[#67](https://github.com/Jack-Siow-CTO/keryx/issues/67))
- Telegram Approvals participation ([#68](https://github.com/Jack-Siow-CTO/keryx/issues/68))
- Skills learning-loop defaults ([#69](https://github.com/Jack-Siow-CTO/keryx/issues/69))
- Checklist grill ([#70](https://github.com/Jack-Siow-CTO/keryx/issues/70))
- Critical path sequencing ([#72](https://github.com/Jack-Siow-CTO/keryx/issues/72))

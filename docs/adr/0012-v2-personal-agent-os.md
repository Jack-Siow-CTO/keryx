# v2: personal agent OS capability surface

Status: **accepted** (grill 2026-07-27). Supersedes the *deferred* surface of ADR 0005 for v2 direction; v1 runtime remains thin until v2 is implemented.

## Decision

Keryx v2 targets a **personal agent OS**, not feature-thin Worker-only scope and not a line-by-line Hermes port. The **control plane remains the system of record**. Messaging is a **Gateway** adapter family. Work stays **Session + Run**, with **Child Runs** for delegation (one Active **root** Run per Session).

**Trust:** single-operator; fail-closed Policy; high-blast actions need **Approval**; Runs with origin `gateway:*` or `schedule` get **reduced Policy** unless escalated.

**v2 includes:** local + Docker terminal; richer file tools (patch, search); pluggable web search/extract; isolated browser; isolated computer-use; curated Memory + FTS; document Skills with always-on learning loop (auto-apply only in trusted context); Schedules; MCP client + server (Policy-bound); Gateway for **Telegram + Discord**; vision/TTS/pluggable media; Soul + workspace context files; CLI + full TUI; in-process `execute_code` with hard RPC-only fence; todo/clarify/session_search; Approval APIs.

**v2 excludes / non-gates:** desktop app (future); 15–20 messaging platforms as release criteria; first-party HA/Spotify/IoT zoo (use MCP/Skills); vector DB / Honcho-class user modeling; Skills Hub client; research/RL/Atropos (optional trajectory export later); multi-Principal tenancy; mixture-of-agents as a special tool; attach-to-personal browser/desktop as default; SSH/Modal/Daytona as ship blockers (ports OK).

## Why

v1 proved a secure, durable Worker spine. Hermes-class personal use needs depth (exec, browser, memory, skills, messaging, schedule) on that spine—without becoming an unmaintainable clone of every Hermes plugin or lab feature. Control-plane canon preserves budgets, cancel, and origin-based Policy. In-process code execution is accepted for Hermes-like feel only with a hard fence; messaging never inherits full host power by default.

## Considered options (summary)

- Deepen Worker only / phased destination — rejected in favor of full agent-OS ambition for v2
- Gateway-peer or messaging-first rewrite — rejected; dual lifecycles rot cancel/budget
- Multi-Active-Run per Session — rejected; Child Runs under one root instead
- Six sandbox backends as gates — rejected; local + Docker ship, others port-ready
- Silent skill writes and unfenced in-process code — rejected as default (fences + origin rules)

## Consequences

- ADR 0005’s “no exec / no browser” is **v1**, not the v2 target.
- Glossary gains Gateway, Memory, Skill, Soul, Schedule, Child Run, Run origin, Approval (see `CONTEXT.md`).
- Implementation should land as a sequenced program of work; this ADR freezes *product decisions*, not a single mega-PR.

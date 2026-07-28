# Console 1.0 is a big-bang full agent OS GUI

Console’s first real release (1.0) is the full personal-agent-OS operator GUI—not a Sessions/Approvals-only vertical slice. In scope for 1.0: messaging chat-list shell (ADR 0031), Session conversation + collapsible activity, explicit composer/Run lifecycle (Send when idle), Approvals via Needs you + in-thread cards (ADR 0033), Memory, Schedules, Skills, provider/model selection (not Worker secret vault), and rich tool viewers sufficient for daily operation (Child Runs, terminal/file/browser outcomes as first-class surfaces). Phased “ship B then expand” and parallel unfinished epics were rejected as the release strategy: dogfood and external “real Console” labeling wait until this surface is coherent. (Earlier wording said dual-rail shell; IA is now ADR 0031.)

## Consequences

- Worker control-plane gaps (Session list/get, Transcript read, Memory/Skills/Schedule completeness, event payloads rich enough for tool viewers) are **release blockers**, not follow-ups.
- Longer time-to-first-daily-use; higher coordination between Rust API and Flutter UI before any milestone feels “the product.”
- Scope creep risk is high—**Definition of Done is frozen in** [`docs/specs/0004-console-1.0.md`](../specs/0004-console-1.0.md). Changes to Must/Should/Out require updating that spec deliberately, not silent expansion.

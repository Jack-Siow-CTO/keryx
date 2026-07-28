# Console uses dual-rail information architecture

Status: **superseded by ADR 0031** (messaging chat-list IA, grill 2026-07-28). Kept for history.

Console’s primary navigation is **Inbox + Session list** (dual-rail), not Session-only, Inbox-only, or filesystem Workspace-first. Sessions remain the durable “channel” for Transcript work; Inbox aggregates cross-Session attention (Approvals, failed/interrupted Runs, and similar alerts) as views over control-plane records. Chosen so time-critical Approvals are not buried under a single Session while multi-turn work still has a stable home. Workspace stays a Policy path jail, not a sidebar product container.


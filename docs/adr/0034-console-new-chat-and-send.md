# New chat is an empty Session; idle Send starts the root Run

Status: **accepted** (grill 2026-07-28). Refines ADR 0016 (explicit Run lifecycle) for messaging IA; does not relax one-Active-root-Run or invent queues.

**New chat** creates (or opens) an empty Session under operator defaults—no mandatory name/Workspace/Policy wizard. Progressive disclosure: Session info before or after first message for tighter Policy/Workspace. When the Session is **idle**, the primary composer action is **Send**, which posts the user message and starts a root Run with that text as the goal (not a separate goal-cockpit “Start Run” primary CTA). When a Session has an **Active** root Run, the composer exposes wait / cancel / cancel-and-re-run (steer only if the control plane supports it)—never a silent second root Run or auto-queue. Rejected: create wizards as default, free chat without Runs, and always-queue follow-ups.

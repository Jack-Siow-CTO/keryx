# Console composer is explicit about Run lifecycle

Status: **accepted**. Idle primary affordance refined by ADR 0034 (Send, not a separate Start Run CTA); lifecycle rules unchanged.

Console does not silently queue follow-ups or auto-steer an Active root Run. When a Session is idle, send starts a new root Run with the composer text as the goal. When a Session has an Active root Run, the composer exposes explicit actions (wait, cancel, cancel-and-re-run with note, and steer only if/when the control plane supports it)—never a hidden second root Run. Aligns UI with “one Active root Run per Session” and avoids inventing client-side queue or steer semantics the Worker does not own.


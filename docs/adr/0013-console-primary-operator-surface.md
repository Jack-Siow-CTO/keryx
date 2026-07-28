# Console is the primary control-plane operator surface

Keryx’s first-party mobile/desktop GUI (**Console**) is the primary day-to-day Principal client of the control plane—not a Gateway and not a host of the agent loop. Chosen over “companion only” (too thin for a Slack-style cockpit) and over treating the GUI as a messaging Gateway (would dual-lifecycle cancel/budget/Policy under `gateway:*` and blur ambient chat with trusted operator power). CLI/TUI remain power tools; Telegram/Discord Gateways remain ambient reduced-Policy channels.

## Consequences

- Console always uses control-plane auth and full Principal authority (subject to normal Policy), not `gateway:*` Run origin by default.
- Messaging-client layout (ADR 0031/0032) is a UX shell over Session/Run/Approval/Memory/Schedule—not a Gateway and not a second messaging product with dual cancel/budget lifecycles.
- ADR 0012’s “desktop app (future)” is now an explicit product track under the name Console.


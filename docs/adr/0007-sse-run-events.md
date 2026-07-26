# SSE Run events with milestones and model token deltas

Clients observe an Active Run via server-sent events with a small fixed taxonomy (run/model/tool/budget/terminal). Model token deltas are included when the provider streams; tool payloads are summarized and secrets redacted. Commands (start/cancel) stay request/response on the control plane. Chosen over status-only (poor UX), full raw provider dumps (unstable and leaky), and WebSocket-first (unnecessary bi-di complexity behind Caddy for v1).

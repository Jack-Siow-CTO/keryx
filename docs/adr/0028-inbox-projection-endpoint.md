# Inbox is a unified read projection, not a notification log

Console loads attention via `GET /v1/inbox`: a sorted, read-only projection of pending Approvals and recent failed/interrupted root Runs (and similar needs-you items). No durable Notification aggregate, no multi-human read/unread cursors. Actions remain on existing resources (approve/deny Approval, open Session). Client-side multi-GET merge and a full notification product were rejected for 1.0.

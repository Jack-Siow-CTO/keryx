# Session list is an operator projection, not bare ids

`GET /v1/sessions` returns a Console-oriented projection: title (operator override; default from first user goal), timestamps, optional active root Run summary, last message preview, and pending Approval count for that Session. Session gains durable title/updated_at (and related) fields on the Worker—not client-only nicknames. Rejected: UUID-only lists, embedding full Transcript/Policy in list rows, and multi-human unread cursors. Attention badges mean Approvals/Active work, not chat unread.

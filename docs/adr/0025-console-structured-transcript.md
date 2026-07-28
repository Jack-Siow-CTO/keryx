# Transcript is structured for Console; tools stay compact

Session Transcript messages gain stable identity and Run linkage (`id`, optional `run_id`, `created_at`) and keep user/assistant prose as first-class `content`. Tool participation is stored as compact structured fields (name, status, summary, artifact refs)—not full dumps as the default message body. Console’s main pane reads Transcript after reconnect; historical Run events remain available for debug/rebuild, not as the primary UI timeline. Rejected: fat tool-JSON transcripts, dual UI-vs-model transcripts, and SSE/event-only rebuild as the default Session view.

## Consequences

- Domain `TranscriptMessage` and `GET .../transcript` must grow beyond `{role, content}`.
- Rich expand-in-place viewers depend on artifact fetch (follow-on decision).
- Agent-loop prompt assembly may still bound/redact tool payloads independently of what Console can fetch under Policy.

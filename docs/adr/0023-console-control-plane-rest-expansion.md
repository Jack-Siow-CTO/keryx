# Console consumes an expanded REST + SSE control plane

Console 1.0 talks only to the Worker’s existing HTTP `/v1/*` control plane style, grown with first-class resources for Session list/get, paged Transcript, Memory, Skills, and artifact fetch as needed—not a GraphQL/BFF, not gRPC, and not UI state rebuilt solely from SSE. SSE remains live Run observation (ADR 0007); durable conversation truth stays Transcript. Tool events should carry viewer-friendly summaries and artifact references rather than unbounded raw payloads. No separate `/v2` API namespace for Console alone.

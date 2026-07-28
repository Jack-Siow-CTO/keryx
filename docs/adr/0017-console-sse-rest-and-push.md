# Console uses REST + SSE, with push only for Inbox wakeups

Console talks to the control plane with the existing HTTP API and SSE Run-event streams. On open and reconnect it reloads durable state (Sessions, Transcript, Approvals, Run records) then resubscribes—client buffers are not the system of record. OS push (APNs/FCM) is allowed only as a wakeup/deep-link for Inbox-class attention (e.g. pending Approvals), not as a parallel event log. Rejected: WebSocket-first rewrite (premature dual protocol), and local-first CRDT sync (fights Worker ownership of truth).

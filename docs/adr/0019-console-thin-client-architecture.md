# Console is a strict thin client of the control plane

Console holds presentation state and a small last-fetched cache (Session list, Transcript pages, Approvals, composer drafts). It does not mirror a writeable local domain DB, queue offline Run mutations, or re-derive a second Transcript model from events. Networking lives in a shared pure Dart API package; Flutter UI sits above session/inbox controllers. Mutations exist only after the control plane acknowledges them. Rejected: offline write replica and client-side agent assists that would compete with Worker ownership of truth.

# Operator token auth with Principal in the domain

Control-plane calls require a configured bearer token (operator token allowlist). Tailscale provides network reachability only; it is not application authorization. Every Session/Run is attributed to a Principal derived from the presenting token so per-device tokens and revocation can land later without rewriting the core. Rejected: Tailscale-identity-only auth, full multi-user accounts for v1, and anonymous loopback trust for a tool-capable agent.

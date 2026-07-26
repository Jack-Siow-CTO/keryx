# Loopback HTTP control plane with Tailnet HTTPS edge

Keryx's control plane binds only to loopback HTTP/JSON (with SSE or WebSocket for Run event streams). Remote Mac and phone clients reach it through a Tailnet-bound reverse proxy (Caddy on Tailscale IPs only), matching the jack-agent-worker T3 topology. Chosen over UDS-only (phone-hostile, forces SSH as data plane), public binds, queue-primary control, and gRPC-first (weaker multi-client ergonomics while the domain is still settling). Optional same-host UDS may be added later without changing the domain API.

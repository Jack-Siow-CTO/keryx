# Run Artifacts live on the Worker under the data dir

Large tool outcomes for Console viewers are stored as **Artifacts**: files under `KERYX_DATA_DIR` with SQLite metadata, fetched via authenticated `GET /v1/artifacts/{id}`. Transcript and events carry compact refs + summaries, not inline multi‑MB payloads. Rejected: inlining blobs in Transcript/SSE, external object stores for v1 Console, and Console-only caches as the durability story. Size quotas per artifact/Run apply; lifecycle is Session/Worker GC later, not multi-tenant ACL design.

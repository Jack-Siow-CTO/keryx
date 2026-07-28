# Memory is curated via control-plane API and tools against one store

Console 1.0 reads and writes Memory through REST on the control plane (list/search/get/create/update/delete) as a trusted Principal. Agent `memory_*` tools remain for in-Run use under origin Policy. Both paths share the same Memory entries; Console writes set principal provenance without requiring a synthetic Run. Rejected: tools-only curate UX, Console read-only, and a separate operator-notes database.

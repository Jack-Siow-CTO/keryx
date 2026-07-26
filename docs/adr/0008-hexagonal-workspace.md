# Small hexagonal Cargo workspace

Keryx is a multi-crate workspace: domain (Session/Run/Principal/Policy/tools/events), app (agent loop, concurrency, budgets), and adapters (storage, model providers, tools, HTTP/SSE API), composed only in the worker binary. Dependency direction is domain ← app ← adapters ← worker. Chosen over a single crate (boundary erosion) and over one-crate-per-plugin (premature fragmentation). Dynamic loading is out of scope; adapters compile in.

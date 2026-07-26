# v1 capability surface: thin core, OpenAI + Grok models, fs tools, no exec

Core owns the agent loop, Session/Run lifecycle, tool interface, policy enforcement, and Run event log.

**v1 Model providers (primary):** OpenAI and Grok (xAI) via **official API credentials** (API keys / base URLs / model IDs), using a shared OpenAI-compatible HTTP client shape where practical.

**v1 Model providers (optional secondary):** consumer web session adapters (`openai_web`, `grok_web`) using **operator-supplied** cookies/tokens from secret files/env — see ADR 0010. Not browser automation; not a substitute for control-plane auth. API keys remain preferred for reliability and CI.

**v1 tools:** workspace file read/write under allowlisted roots; shell/exec and browser tools are deferred. Memory is Session transcript only. Adapters compile in through registries rather than dynamic plugin loading. Chosen to keep the Worker minimal, testable, and fail-closed while remaining extensible at stable ports.

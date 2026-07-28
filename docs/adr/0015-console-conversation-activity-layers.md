# Console renders conversation and activity as separate layers

Status: **accepted**. Reaffirmed under messaging IA (ADR 0031/0032); Approvals placement detail in ADR 0033.

In a Session thread, Console treats durable **Transcript** prose (Principal and model) as first-class chat messages, and **Run events** (tools, budgets, Child Run linkage, system status) as collapsible activity in the same timeline—not a single flat chat log of every event, and not separate Chat vs Activity tabs as the default. Approvals surface as sticky action cards (with list-level Needs you per ADR 0033). Chosen so messenger readability survives tool-heavy Runs, and so Console respects the existing Transcript vs Run-event seam instead of inventing a merged log or a Run-only job history as the default home.


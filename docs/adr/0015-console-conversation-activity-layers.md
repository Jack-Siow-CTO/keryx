# Console renders conversation and activity as separate layers

In a Session main pane, Console treats durable **Transcript** prose (Principal and model) as first-class conversation messages, and **Run events** (tools, budgets, Child Run linkage, system status) as collapsible activity—not a single flat chat log of every event. Approvals surface as sticky action cards. Chosen so Slack-style readability survives tool-heavy Runs, and so Console respects the existing Transcript vs Run-event seam instead of inventing a merged log or a Run-only job history as the default home.

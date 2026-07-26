# Session-serial Runs with capped multi-Session parallelism

Within one Session, at most one Run executes at a time (Active Run). Across Sessions, the Worker may run multiple Runs in parallel up to a configured global cap. Chosen over global-serial-only for multi-stream personal/team use, and over fully parallel same-Session Runs to keep history and tool side effects coherent.

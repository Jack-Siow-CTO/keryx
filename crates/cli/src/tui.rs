//! Minimal full-TUI shell over the control plane (no shadow agent loop).
//!
//! Implemented as a line-oriented TUI for CI-friendly smoke: slash commands map
//! to the same HTTP surface as `keryx-cli`. A richer ratatui UI can replace this
//! module without changing control-plane contracts.

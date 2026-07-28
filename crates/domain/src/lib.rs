//! Keryx domain: pure types and rules for Sessions, Runs, Principals, Policy, Tools, and Run events.
//!
//! This crate has no I/O, HTTP, `SQLite`, or provider SDKs (ADR 0008).

mod approval;
mod events;
mod ids;
mod memory;
mod origin;
mod policy;
mod principal;
mod run;
mod schedule;
mod session;
mod transcript;

pub use approval::{Approval, ApprovalStatus};
pub use events::{RunEvent, RunEventKind};
pub use ids::{ApprovalId, RunId, SessionId};
pub use memory::{MemoryEntry, MemoryId};
pub use origin::{ParseRunOriginError, RunOrigin};
pub use policy::Policy;
pub use principal::{Principal, PrincipalId};
pub use run::{Run, RunStatus};
pub use schedule::{Schedule, ScheduleId, ScheduleStatus};
pub use session::{ActiveRootRunSummary, Session, SessionSummary};
pub use transcript::{MessageRole, Transcript, TranscriptMessage};

/// Workspace smoke: domain crate is loadable.
#[must_use]
pub fn crate_name() -> &'static str {
    "keryx-domain"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_smoke() {
        assert_eq!(crate_name(), "keryx-domain");
    }

    #[test]
    fn run_start_stamps_control_plane_origin() {
        let run = Run::start(SessionId::new(), PrincipalId::new("op"), "goal");
        assert_eq!(run.origin, RunOrigin::ControlPlane);
    }

    #[test]
    fn run_start_with_origin_gateway() {
        let run = Run::start_with_origin(
            SessionId::new(),
            PrincipalId::new("op"),
            "goal",
            RunOrigin::gateway("telegram"),
        );
        assert_eq!(run.origin.as_str(), "gateway:telegram");
        assert!(run.origin.is_reduced_trust());
        assert!(!Policy::for_origin(&run.origin).allows_tool("write_file"));
    }
}

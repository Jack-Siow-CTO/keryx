use async_trait::async_trait;
use keryx_domain::{
    Approval, ApprovalId, MemoryEntry, MemoryId, Run, RunId, Schedule, ScheduleId, Session,
    SessionId, Transcript, TranscriptMessage,
};

/// Persistence port for Sessions, Runs, Transcripts, and Approvals (in-memory or `SQLite`).
#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn create_session(&self, session: Session) -> Result<(), String>;
    async fn get_session(&self, id: SessionId) -> Result<Option<Session>, String>;
    async fn count_sessions(&self) -> Result<usize, String>;

    async fn create_run(&self, run: Run) -> Result<(), String>;
    async fn update_run(&self, run: Run) -> Result<(), String>;
    async fn get_run(&self, id: RunId) -> Result<Option<Run>, String>;
    async fn count_runs(&self) -> Result<usize, String>;

    async fn get_transcript(&self, session_id: SessionId) -> Result<Transcript, String>;
    async fn append_transcript(
        &self,
        session_id: SessionId,
        message: TranscriptMessage,
    ) -> Result<(), String>;

    /// Mark any Active Runs as interrupted (process reopen / crash recovery).
    async fn interrupt_active_runs(&self) -> Result<usize, String>;

    /// Persist a new pending Approval.
    async fn create_approval(&self, approval: Approval) -> Result<(), String>;
    async fn update_approval(&self, approval: Approval) -> Result<(), String>;
    /// Atomically transition a pending Approval; returns false if not pending / missing.
    async fn update_approval_if_pending(&self, approval: Approval) -> Result<bool, String>;
    async fn get_approval(&self, id: ApprovalId) -> Result<Option<Approval>, String>;
    async fn list_approvals(&self, pending_only: bool) -> Result<Vec<Approval>, String>;

    // --- Memory (curated facts; distinct from Transcript) ---
    async fn create_memory(&self, entry: MemoryEntry) -> Result<(), String>;
    async fn get_memory(&self, id: MemoryId) -> Result<Option<MemoryEntry>, String>;
    async fn update_memory(&self, entry: MemoryEntry) -> Result<(), String>;
    async fn delete_memory(&self, id: MemoryId) -> Result<(), String>;
    async fn list_memory(&self) -> Result<Vec<MemoryEntry>, String>;
    /// Full-text search over Memory content (SQLite FTS or in-memory substring).
    async fn search_memory(&self, query: &str, limit: usize) -> Result<Vec<MemoryEntry>, String>;
    /// Search Transcript text across Sessions (session_search).
    async fn search_transcripts(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<(SessionId, TranscriptMessage)>, String>;

    // --- Schedules ---
    async fn create_schedule(&self, schedule: Schedule) -> Result<(), String>;
    async fn update_schedule(&self, schedule: Schedule) -> Result<(), String>;
    async fn get_schedule(&self, id: ScheduleId) -> Result<Option<Schedule>, String>;
    async fn list_schedules(&self) -> Result<Vec<Schedule>, String>;
}

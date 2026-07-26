use async_trait::async_trait;
use keryx_domain::{Run, RunId, Session, SessionId, Transcript, TranscriptMessage};

/// Persistence port for Sessions, Runs, and Transcripts (in-memory or `SQLite`).
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
}

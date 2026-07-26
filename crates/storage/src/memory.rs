use async_trait::async_trait;
use keryx_app::SessionStore;
use keryx_domain::{Run, RunId, RunStatus, Session, SessionId, Transcript, TranscriptMessage};
use std::collections::HashMap;
use std::sync::Mutex;

/// Process-local Session/Run store for Seam 1 and early development.
#[derive(Debug, Default)]
pub struct InMemorySessionStore {
    sessions: Mutex<HashMap<SessionId, Session>>,
    runs: Mutex<HashMap<RunId, Run>>,
    transcripts: Mutex<HashMap<SessionId, Vec<TranscriptMessage>>>,
}

impl InMemorySessionStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl SessionStore for InMemorySessionStore {
    async fn create_session(&self, session: Session) -> Result<(), String> {
        let mut sessions = self.sessions.lock().map_err(|e| e.to_string())?;
        sessions.insert(session.id, session);
        Ok(())
    }

    async fn get_session(&self, id: SessionId) -> Result<Option<Session>, String> {
        let sessions = self.sessions.lock().map_err(|e| e.to_string())?;
        Ok(sessions.get(&id).cloned())
    }

    async fn count_sessions(&self) -> Result<usize, String> {
        let sessions = self.sessions.lock().map_err(|e| e.to_string())?;
        Ok(sessions.len())
    }

    async fn create_run(&self, run: Run) -> Result<(), String> {
        let mut runs = self.runs.lock().map_err(|e| e.to_string())?;
        runs.insert(run.id, run);
        Ok(())
    }

    async fn update_run(&self, run: Run) -> Result<(), String> {
        let mut runs = self.runs.lock().map_err(|e| e.to_string())?;
        if !runs.contains_key(&run.id) {
            return Err(format!("run {} not found", run.id));
        }
        runs.insert(run.id, run);
        Ok(())
    }

    async fn get_run(&self, id: RunId) -> Result<Option<Run>, String> {
        let runs = self.runs.lock().map_err(|e| e.to_string())?;
        Ok(runs.get(&id).cloned())
    }

    async fn count_runs(&self) -> Result<usize, String> {
        let runs = self.runs.lock().map_err(|e| e.to_string())?;
        Ok(runs.len())
    }

    async fn get_transcript(&self, session_id: SessionId) -> Result<Transcript, String> {
        let transcripts = self.transcripts.lock().map_err(|e| e.to_string())?;
        Ok(Transcript {
            session_id: Some(session_id),
            messages: transcripts.get(&session_id).cloned().unwrap_or_default(),
        })
    }

    async fn append_transcript(
        &self,
        session_id: SessionId,
        message: TranscriptMessage,
    ) -> Result<(), String> {
        let mut transcripts = self.transcripts.lock().map_err(|e| e.to_string())?;
        transcripts.entry(session_id).or_default().push(message);
        Ok(())
    }

    async fn interrupt_active_runs(&self) -> Result<usize, String> {
        let mut runs = self.runs.lock().map_err(|e| e.to_string())?;
        let mut count = 0;
        for run in runs.values_mut() {
            if run.status == RunStatus::Active {
                run.interrupt();
                count += 1;
            }
        }
        Ok(count)
    }
}

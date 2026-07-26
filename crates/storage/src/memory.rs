use async_trait::async_trait;
use keryx_app::SessionStore;
use keryx_domain::{Run, RunId, Session, SessionId};
use std::collections::HashMap;
use std::sync::Mutex;

/// Process-local Session/Run store for Seam 1 and early development.
#[derive(Debug, Default)]
pub struct InMemorySessionStore {
    sessions: Mutex<HashMap<SessionId, Session>>,
    runs: Mutex<HashMap<RunId, Run>>,
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
}

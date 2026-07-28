use async_trait::async_trait;
use keryx_app::SessionStore;
use keryx_domain::{
    Approval, ApprovalId, ApprovalStatus, MemoryEntry, MemoryId, Run, RunId, RunStatus, Schedule,
    ScheduleId, Session, SessionId, Transcript, TranscriptMessage,
};
use std::collections::HashMap;
use std::sync::Mutex;

/// Process-local Session/Run store for Seam 1 and early development.
#[derive(Debug, Default)]
pub struct InMemorySessionStore {
    sessions: Mutex<HashMap<SessionId, Session>>,
    runs: Mutex<HashMap<RunId, Run>>,
    transcripts: Mutex<HashMap<SessionId, Vec<TranscriptMessage>>>,
    approvals: Mutex<HashMap<ApprovalId, Approval>>,
    memory: Mutex<HashMap<MemoryId, MemoryEntry>>,
    schedules: Mutex<HashMap<ScheduleId, Schedule>>,
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

    async fn update_session(&self, session: Session) -> Result<(), String> {
        let mut sessions = self.sessions.lock().map_err(|e| e.to_string())?;
        if !sessions.contains_key(&session.id) {
            return Err(format!("session {} not found", session.id));
        }
        sessions.insert(session.id, session);
        Ok(())
    }

    async fn list_sessions(&self) -> Result<Vec<Session>, String> {
        let sessions = self.sessions.lock().map_err(|e| e.to_string())?;
        let mut list: Vec<Session> = sessions.values().cloned().collect();
        list.sort_by_key(|b| std::cmp::Reverse(b.updated_at));
        Ok(list)
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

    async fn list_runs_for_session(&self, session_id: SessionId) -> Result<Vec<Run>, String> {
        let runs = self.runs.lock().map_err(|e| e.to_string())?;
        Ok(runs
            .values()
            .filter(|r| r.session_id == session_id)
            .cloned()
            .collect())
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

    async fn get_transcript_page(
        &self,
        session_id: SessionId,
        limit: usize,
        before: Option<&str>,
    ) -> Result<(Vec<TranscriptMessage>, Option<String>), String> {
        let transcripts = self.transcripts.lock().map_err(|e| e.to_string())?;
        let all = transcripts.get(&session_id).cloned().unwrap_or_default();
        // Chronological storage → reverse for newest-first.
        let mut rev: Vec<TranscriptMessage> = all.into_iter().rev().collect();
        if let Some(before_id) = before {
            if let Some(pos) = rev.iter().position(|m| m.id == before_id) {
                rev = rev.split_off(pos + 1);
            }
        }
        let limit = limit.max(1);
        let page: Vec<TranscriptMessage> = rev.into_iter().take(limit).collect();
        let next_before = if page.len() == limit {
            page.last().map(|m| m.id.clone())
        } else {
            None
        };
        // If more remain after this page, next_before is the oldest id on this page.
        Ok((page, next_before))
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

    async fn create_approval(&self, approval: Approval) -> Result<(), String> {
        let mut approvals = self.approvals.lock().map_err(|e| e.to_string())?;
        approvals.insert(approval.id, approval);
        Ok(())
    }

    async fn update_approval(&self, approval: Approval) -> Result<(), String> {
        let mut approvals = self.approvals.lock().map_err(|e| e.to_string())?;
        if !approvals.contains_key(&approval.id) {
            return Err(format!("approval {} not found", approval.id));
        }
        approvals.insert(approval.id, approval);
        Ok(())
    }

    async fn update_approval_if_pending(&self, approval: Approval) -> Result<bool, String> {
        let mut approvals = self.approvals.lock().map_err(|e| e.to_string())?;
        match approvals.get(&approval.id) {
            Some(existing) if existing.status == ApprovalStatus::Pending => {
                approvals.insert(approval.id, approval);
                Ok(true)
            }
            Some(_) => Ok(false),
            None => Ok(false),
        }
    }

    async fn get_approval(&self, id: ApprovalId) -> Result<Option<Approval>, String> {
        let approvals = self.approvals.lock().map_err(|e| e.to_string())?;
        Ok(approvals.get(&id).cloned())
    }

    async fn list_approvals(&self, pending_only: bool) -> Result<Vec<Approval>, String> {
        let approvals = self.approvals.lock().map_err(|e| e.to_string())?;
        let mut out: Vec<_> = approvals
            .values()
            .filter(|a| !pending_only || a.status == ApprovalStatus::Pending)
            .cloned()
            .collect();
        out.sort_by_key(|a| a.id.to_string());
        Ok(out)
    }

    async fn create_memory(&self, entry: MemoryEntry) -> Result<(), String> {
        let mut memory = self.memory.lock().map_err(|e| e.to_string())?;
        memory.insert(entry.id, entry);
        Ok(())
    }

    async fn get_memory(&self, id: MemoryId) -> Result<Option<MemoryEntry>, String> {
        let memory = self.memory.lock().map_err(|e| e.to_string())?;
        Ok(memory.get(&id).cloned())
    }

    async fn update_memory(&self, entry: MemoryEntry) -> Result<(), String> {
        let mut memory = self.memory.lock().map_err(|e| e.to_string())?;
        if !memory.contains_key(&entry.id) {
            return Err(format!("memory {} not found", entry.id));
        }
        memory.insert(entry.id, entry);
        Ok(())
    }

    async fn delete_memory(&self, id: MemoryId) -> Result<(), String> {
        let mut memory = self.memory.lock().map_err(|e| e.to_string())?;
        if memory.remove(&id).is_none() {
            return Err(format!("memory {id} not found"));
        }
        Ok(())
    }

    async fn list_memory(&self) -> Result<Vec<MemoryEntry>, String> {
        let memory = self.memory.lock().map_err(|e| e.to_string())?;
        let mut out: Vec<_> = memory.values().cloned().collect();
        out.sort_by_key(|e| e.id.to_string());
        Ok(out)
    }

    async fn search_memory(&self, query: &str, limit: usize) -> Result<Vec<MemoryEntry>, String> {
        let memory = self.memory.lock().map_err(|e| e.to_string())?;
        let q = query.to_ascii_lowercase();
        let mut out: Vec<_> = memory
            .values()
            .filter(|e| {
                e.content.to_ascii_lowercase().contains(&q)
                    || e.label
                        .as_ref()
                        .is_some_and(|l| l.to_ascii_lowercase().contains(&q))
            })
            .cloned()
            .collect();
        out.truncate(limit.max(1));
        Ok(out)
    }

    async fn search_transcripts(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<(SessionId, TranscriptMessage)>, String> {
        let transcripts = self.transcripts.lock().map_err(|e| e.to_string())?;
        let q = query.to_ascii_lowercase();
        let mut out = Vec::new();
        for (sid, msgs) in transcripts.iter() {
            for m in msgs {
                if m.content.to_ascii_lowercase().contains(&q) {
                    out.push((*sid, m.clone()));
                    if out.len() >= limit.max(1) {
                        return Ok(out);
                    }
                }
            }
        }
        Ok(out)
    }

    async fn create_schedule(&self, schedule: Schedule) -> Result<(), String> {
        let mut schedules = self.schedules.lock().map_err(|e| e.to_string())?;
        schedules.insert(schedule.id, schedule);
        Ok(())
    }

    async fn update_schedule(&self, schedule: Schedule) -> Result<(), String> {
        let mut schedules = self.schedules.lock().map_err(|e| e.to_string())?;
        if !schedules.contains_key(&schedule.id) {
            return Err(format!("schedule {} not found", schedule.id));
        }
        schedules.insert(schedule.id, schedule);
        Ok(())
    }

    async fn get_schedule(&self, id: ScheduleId) -> Result<Option<Schedule>, String> {
        let schedules = self.schedules.lock().map_err(|e| e.to_string())?;
        Ok(schedules.get(&id).cloned())
    }

    async fn list_schedules(&self) -> Result<Vec<Schedule>, String> {
        let schedules = self.schedules.lock().map_err(|e| e.to_string())?;
        let mut out: Vec<_> = schedules
            .values()
            .filter(|s| s.status != keryx_domain::ScheduleStatus::Deleted)
            .cloned()
            .collect();
        out.sort_by_key(|s| s.id.to_string());
        Ok(out)
    }
}

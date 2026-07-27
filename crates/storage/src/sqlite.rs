use async_trait::async_trait;
use keryx_app::SessionStore;
use keryx_domain::{
    Approval, ApprovalId, ApprovalStatus, MemoryEntry, MemoryId, MessageRole, PrincipalId, Run,
    RunId, RunOrigin, RunStatus, Schedule, ScheduleId, ScheduleStatus, Session, SessionId,
    Transcript, TranscriptMessage,
};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Mutex;

/// Durable `SQLite` store for Sessions, Transcripts, and Run records (ADR 0006).
pub struct SqliteSessionStore {
    path: PathBuf,
    conn: Mutex<Connection>,
}

impl SqliteSessionStore {
    /// Open (or create) a store at `data_dir/keryx.db`, migrate, and interrupt Active Runs.
    pub fn open(data_dir: impl AsRef<Path>) -> Result<Self, String> {
        let data_dir = data_dir.as_ref();
        std::fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
        let path = data_dir.join("keryx.db");
        let conn = Connection::open(&path).map_err(|e| e.to_string())?;
        let store = Self {
            path,
            conn: Mutex::new(conn),
        };
        store.migrate()?;
        // Crash recovery: Active Runs do not resume mid-loop.
        store.interrupt_active_runs_blocking()?;
        Ok(store)
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn migrate(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute_batch(
            r"
            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY NOT NULL,
                principal_id TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS runs (
                id TEXT PRIMARY KEY NOT NULL,
                session_id TEXT NOT NULL,
                principal_id TEXT NOT NULL,
                goal TEXT NOT NULL,
                status TEXT NOT NULL,
                result TEXT,
                origin TEXT NOT NULL DEFAULT 'control_plane',
                parent_run_id TEXT,
                FOREIGN KEY(session_id) REFERENCES sessions(id)
            );
            CREATE TABLE IF NOT EXISTS transcript_messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                FOREIGN KEY(session_id) REFERENCES sessions(id)
            );
            CREATE TABLE IF NOT EXISTS approvals (
                id TEXT PRIMARY KEY NOT NULL,
                run_id TEXT NOT NULL,
                action TEXT NOT NULL,
                summary TEXT NOT NULL,
                status TEXT NOT NULL,
                requested_by TEXT NOT NULL,
                decided_by TEXT,
                FOREIGN KEY(run_id) REFERENCES runs(id)
            );
            CREATE TABLE IF NOT EXISTS memory_entries (
                id TEXT PRIMARY KEY NOT NULL,
                content TEXT NOT NULL,
                label TEXT,
                source_run_id TEXT,
                source_principal_id TEXT
            );
            CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(
                id UNINDEXED,
                content,
                label
            );
            CREATE TABLE IF NOT EXISTS schedules (
                id TEXT PRIMARY KEY NOT NULL,
                principal_id TEXT NOT NULL,
                session_id TEXT,
                goal TEXT NOT NULL,
                interval_secs INTEGER NOT NULL,
                status TEXT NOT NULL,
                next_fire_at INTEGER NOT NULL,
                policy_tools TEXT NOT NULL,
                last_fired_at INTEGER
            );
            ",
        )
        .map_err(|e| e.to_string())?;

        // v2: Run origin column for stores created before origin was introduced.
        let has_origin: bool = {
            let mut stmt = conn
                .prepare("PRAGMA table_info(runs)")
                .map_err(|e| e.to_string())?;
            let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
            let mut found = false;
            while let Some(row) = rows.next().map_err(|e| e.to_string())? {
                let name: String = row.get(1).map_err(|e| e.to_string())?;
                if name == "origin" {
                    found = true;
                    break;
                }
            }
            found
        };
        if !has_origin {
            conn.execute(
                "ALTER TABLE runs ADD COLUMN origin TEXT NOT NULL DEFAULT 'control_plane'",
                [],
            )
            .map_err(|e| e.to_string())?;
        }

        let has_parent: bool = {
            let mut stmt = conn
                .prepare("PRAGMA table_info(runs)")
                .map_err(|e| e.to_string())?;
            let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
            let mut found = false;
            while let Some(row) = rows.next().map_err(|e| e.to_string())? {
                let name: String = row.get(1).map_err(|e| e.to_string())?;
                if name == "parent_run_id" {
                    found = true;
                    break;
                }
            }
            found
        };
        if !has_parent {
            conn.execute("ALTER TABLE runs ADD COLUMN parent_run_id TEXT", [])
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn interrupt_active_runs_blocking(&self) -> Result<usize, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let n = conn
            .execute(
                "UPDATE runs SET status = ?1, result = ?2 WHERE status = ?3",
                params!["interrupted", "interrupted", "active"],
            )
            .map_err(|e| e.to_string())?;
        Ok(n)
    }
}

fn parse_session_id(s: &str) -> Result<SessionId, String> {
    SessionId::from_str(s).map_err(|e| e.to_string())
}

fn parse_run_id(s: &str) -> Result<RunId, String> {
    RunId::from_str(s).map_err(|e| e.to_string())
}

fn status_to_str(status: RunStatus) -> &'static str {
    match status {
        RunStatus::Active => "active",
        RunStatus::Completed => "completed",
        RunStatus::Failed => "failed",
        RunStatus::Cancelled => "cancelled",
        RunStatus::Interrupted => "interrupted",
    }
}

fn status_from_str(s: &str) -> Result<RunStatus, String> {
    match s {
        "active" => Ok(RunStatus::Active),
        "completed" => Ok(RunStatus::Completed),
        "failed" => Ok(RunStatus::Failed),
        "cancelled" => Ok(RunStatus::Cancelled),
        "interrupted" => Ok(RunStatus::Interrupted),
        other => Err(format!("unknown run status: {other}")),
    }
}

fn role_to_str(role: &MessageRole) -> &'static str {
    match role {
        MessageRole::System => "system",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::Tool => "tool",
    }
}

fn role_from_str(s: &str) -> Result<MessageRole, String> {
    match s {
        "system" => Ok(MessageRole::System),
        "user" => Ok(MessageRole::User),
        "assistant" => Ok(MessageRole::Assistant),
        "tool" => Ok(MessageRole::Tool),
        other => Err(format!("unknown message role: {other}")),
    }
}

fn parse_run_origin(s: &str) -> Result<RunOrigin, String> {
    RunOrigin::from_str(s).map_err(|e| e.to_string())
}

fn row_to_run(
    id: String,
    session_id: String,
    principal_id: String,
    goal: String,
    status: String,
    result: Option<String>,
    origin: String,
    parent_run_id: Option<String>,
) -> Result<Run, String> {
    Ok(Run {
        id: parse_run_id(&id)?,
        session_id: parse_session_id(&session_id)?,
        principal_id: PrincipalId::new(principal_id),
        goal,
        status: status_from_str(&status)?,
        origin: parse_run_origin(&origin)?,
        parent_run_id: parent_run_id
            .filter(|s| !s.is_empty())
            .map(|s| parse_run_id(&s))
            .transpose()?,
        result,
    })
}

#[async_trait]
impl SessionStore for SqliteSessionStore {
    async fn create_session(&self, session: Session) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO sessions (id, principal_id) VALUES (?1, ?2)",
            params![session.id.to_string(), session.principal_id.to_string()],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    async fn get_session(&self, id: SessionId) -> Result<Option<Session>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT id, principal_id FROM sessions WHERE id = ?1")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query(params![id.to_string()])
            .map_err(|e| e.to_string())?;
        match rows.next().map_err(|e| e.to_string())? {
            Some(row) => {
                let sid: String = row.get(0).map_err(|e| e.to_string())?;
                let principal: String = row.get(1).map_err(|e| e.to_string())?;
                Ok(Some(Session {
                    id: parse_session_id(&sid)?,
                    principal_id: PrincipalId::new(principal),
                }))
            }
            None => Ok(None),
        }
    }

    async fn count_sessions(&self) -> Result<usize, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))
            .map_err(|e| e.to_string())?;
        Ok(n as usize)
    }

    async fn create_run(&self, run: Run) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO runs (id, session_id, principal_id, goal, status, result, origin, parent_run_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                run.id.to_string(),
                run.session_id.to_string(),
                run.principal_id.to_string(),
                run.goal,
                status_to_str(run.status),
                run.result,
                run.origin.as_str(),
                run.parent_run_id.map(|id| id.to_string()),
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    async fn update_run(&self, run: Run) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let n = conn
            .execute(
                "UPDATE runs SET session_id = ?1, principal_id = ?2, goal = ?3, status = ?4, result = ?5, origin = ?6, parent_run_id = ?7
                 WHERE id = ?8",
                params![
                    run.session_id.to_string(),
                    run.principal_id.to_string(),
                    run.goal,
                    status_to_str(run.status),
                    run.result,
                    run.origin.as_str(),
                    run.parent_run_id.map(|id| id.to_string()),
                    run.id.to_string(),
                ],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err(format!("run {} not found", run.id));
        }
        Ok(())
    }

    async fn get_run(&self, id: RunId) -> Result<Option<Run>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, session_id, principal_id, goal, status, result, origin, parent_run_id FROM runs WHERE id = ?1",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query(params![id.to_string()])
            .map_err(|e| e.to_string())?;
        match rows.next().map_err(|e| e.to_string())? {
            Some(row) => {
                let run = row_to_run(
                    row.get(0).map_err(|e| e.to_string())?,
                    row.get(1).map_err(|e| e.to_string())?,
                    row.get(2).map_err(|e| e.to_string())?,
                    row.get(3).map_err(|e| e.to_string())?,
                    row.get(4).map_err(|e| e.to_string())?,
                    row.get(5).map_err(|e| e.to_string())?,
                    row.get(6).map_err(|e| e.to_string())?,
                    row.get(7).map_err(|e| e.to_string())?,
                )?;
                Ok(Some(run))
            }
            None => Ok(None),
        }
    }

    async fn count_runs(&self) -> Result<usize, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM runs", [], |row| row.get(0))
            .map_err(|e| e.to_string())?;
        Ok(n as usize)
    }

    async fn get_transcript(&self, session_id: SessionId) -> Result<Transcript, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT role, content FROM transcript_messages
                 WHERE session_id = ?1 ORDER BY id ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![session_id.to_string()], |row| {
                let role: String = row.get(0)?;
                let content: String = row.get(1)?;
                Ok((role, content))
            })
            .map_err(|e| e.to_string())?;

        let mut messages = Vec::new();
        for row in rows {
            let (role, content) = row.map_err(|e| e.to_string())?;
            messages.push(TranscriptMessage {
                role: role_from_str(&role)?,
                content,
            });
        }
        Ok(Transcript {
            session_id: Some(session_id),
            messages,
        })
    }

    async fn append_transcript(
        &self,
        session_id: SessionId,
        message: TranscriptMessage,
    ) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO transcript_messages (session_id, role, content) VALUES (?1, ?2, ?3)",
            params![
                session_id.to_string(),
                role_to_str(&message.role),
                message.content,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    async fn interrupt_active_runs(&self) -> Result<usize, String> {
        self.interrupt_active_runs_blocking()
    }

    async fn create_approval(&self, approval: Approval) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO approvals (id, run_id, action, summary, status, requested_by, decided_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                approval.id.to_string(),
                approval.run_id.to_string(),
                approval.action,
                approval.summary,
                approval_status_to_str(approval.status),
                approval.requested_by.to_string(),
                approval.decided_by.as_ref().map(ToString::to_string),
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    async fn update_approval(&self, approval: Approval) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let n = conn
            .execute(
                "UPDATE approvals SET run_id = ?1, action = ?2, summary = ?3, status = ?4,
                 requested_by = ?5, decided_by = ?6 WHERE id = ?7",
                params![
                    approval.run_id.to_string(),
                    approval.action,
                    approval.summary,
                    approval_status_to_str(approval.status),
                    approval.requested_by.to_string(),
                    approval.decided_by.as_ref().map(ToString::to_string),
                    approval.id.to_string(),
                ],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err(format!("approval {} not found", approval.id));
        }
        Ok(())
    }

    async fn update_approval_if_pending(&self, approval: Approval) -> Result<bool, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let n = conn
            .execute(
                "UPDATE approvals SET run_id = ?1, action = ?2, summary = ?3, status = ?4,
                 requested_by = ?5, decided_by = ?6 WHERE id = ?7 AND status = 'pending'",
                params![
                    approval.run_id.to_string(),
                    approval.action,
                    approval.summary,
                    approval_status_to_str(approval.status),
                    approval.requested_by.to_string(),
                    approval.decided_by.as_ref().map(ToString::to_string),
                    approval.id.to_string(),
                ],
            )
            .map_err(|e| e.to_string())?;
        Ok(n > 0)
    }

    async fn get_approval(&self, id: ApprovalId) -> Result<Option<Approval>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, run_id, action, summary, status, requested_by, decided_by
                 FROM approvals WHERE id = ?1",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query(params![id.to_string()])
            .map_err(|e| e.to_string())?;
        match rows.next().map_err(|e| e.to_string())? {
            Some(row) => Ok(Some(row_to_approval(
                row.get(0).map_err(|e| e.to_string())?,
                row.get(1).map_err(|e| e.to_string())?,
                row.get(2).map_err(|e| e.to_string())?,
                row.get(3).map_err(|e| e.to_string())?,
                row.get(4).map_err(|e| e.to_string())?,
                row.get(5).map_err(|e| e.to_string())?,
                row.get(6).map_err(|e| e.to_string())?,
            )?)),
            None => Ok(None),
        }
    }

    async fn list_approvals(&self, pending_only: bool) -> Result<Vec<Approval>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let sql = if pending_only {
            "SELECT id, run_id, action, summary, status, requested_by, decided_by
             FROM approvals WHERE status = 'pending' ORDER BY id ASC"
        } else {
            "SELECT id, run_id, action, summary, status, requested_by, decided_by
             FROM approvals ORDER BY id ASC"
        };
        let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            let (id, run_id, action, summary, status, requested_by, decided_by) =
                row.map_err(|e| e.to_string())?;
            out.push(row_to_approval(
                id,
                run_id,
                action,
                summary,
                status,
                requested_by,
                decided_by,
            )?);
        }
        Ok(out)
    }

    async fn create_memory(&self, entry: MemoryEntry) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute_batch("BEGIN IMMEDIATE;")
            .map_err(|e| e.to_string())?;
        let result = (|| {
            conn.execute(
                "INSERT INTO memory_entries (id, content, label, source_run_id, source_principal_id)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    entry.id.to_string(),
                    entry.content,
                    entry.label,
                    entry.source_run_id.map(|id| id.to_string()),
                    entry.source_principal_id.as_ref().map(ToString::to_string),
                ],
            )
            .map_err(|e| e.to_string())?;
            conn.execute(
                "INSERT INTO memory_fts (id, content, label) VALUES (?1, ?2, ?3)",
                params![
                    entry.id.to_string(),
                    entry.content,
                    entry.label.clone().unwrap_or_default(),
                ],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                conn.execute_batch("COMMIT;").map_err(|e| e.to_string())?;
                Ok(())
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK;");
                Err(e)
            }
        }
    }

    async fn get_memory(&self, id: MemoryId) -> Result<Option<MemoryEntry>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, content, label, source_run_id, source_principal_id
                 FROM memory_entries WHERE id = ?1",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query(params![id.to_string()])
            .map_err(|e| e.to_string())?;
        match rows.next().map_err(|e| e.to_string())? {
            Some(row) => Ok(Some(row_to_memory(
                row.get(0).map_err(|e| e.to_string())?,
                row.get(1).map_err(|e| e.to_string())?,
                row.get(2).map_err(|e| e.to_string())?,
                row.get(3).map_err(|e| e.to_string())?,
                row.get(4).map_err(|e| e.to_string())?,
            )?)),
            None => Ok(None),
        }
    }

    async fn update_memory(&self, entry: MemoryEntry) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute_batch("BEGIN IMMEDIATE;")
            .map_err(|e| e.to_string())?;
        let result = (|| {
            let n = conn
                .execute(
                    "UPDATE memory_entries SET content = ?1, label = ?2, source_run_id = ?3,
                     source_principal_id = ?4 WHERE id = ?5",
                    params![
                        entry.content,
                        entry.label,
                        entry.source_run_id.map(|id| id.to_string()),
                        entry.source_principal_id.as_ref().map(ToString::to_string),
                        entry.id.to_string(),
                    ],
                )
                .map_err(|e| e.to_string())?;
            if n == 0 {
                return Err(format!("memory {} not found", entry.id));
            }
            conn.execute(
                "DELETE FROM memory_fts WHERE id = ?1",
                params![entry.id.to_string()],
            )
            .map_err(|e| e.to_string())?;
            conn.execute(
                "INSERT INTO memory_fts (id, content, label) VALUES (?1, ?2, ?3)",
                params![
                    entry.id.to_string(),
                    entry.content,
                    entry.label.clone().unwrap_or_default(),
                ],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                conn.execute_batch("COMMIT;").map_err(|e| e.to_string())?;
                Ok(())
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK;");
                Err(e)
            }
        }
    }

    async fn delete_memory(&self, id: MemoryId) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute_batch("BEGIN IMMEDIATE;")
            .map_err(|e| e.to_string())?;
        let result = (|| {
            conn.execute(
                "DELETE FROM memory_fts WHERE id = ?1",
                params![id.to_string()],
            )
            .map_err(|e| e.to_string())?;
            let n = conn
                .execute(
                    "DELETE FROM memory_entries WHERE id = ?1",
                    params![id.to_string()],
                )
                .map_err(|e| e.to_string())?;
            if n == 0 {
                return Err(format!("memory {id} not found"));
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                conn.execute_batch("COMMIT;").map_err(|e| e.to_string())?;
                Ok(())
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK;");
                Err(e)
            }
        }
    }

    async fn list_memory(&self) -> Result<Vec<MemoryEntry>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, content, label, source_run_id, source_principal_id
                 FROM memory_entries ORDER BY id ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            let (id, content, label, source_run_id, source_principal_id) =
                row.map_err(|e| e.to_string())?;
            out.push(row_to_memory(
                id,
                content,
                label,
                source_run_id,
                source_principal_id,
            )?);
        }
        Ok(out)
    }

    async fn search_memory(&self, query: &str, limit: usize) -> Result<Vec<MemoryEntry>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        // FTS5: quote simple token queries; fall back to LIKE if MATCH fails.
        let fts_q = format!("\"{}\"", query.replace('"', ""));
        let mut stmt = conn
            .prepare(
                "SELECT m.id, m.content, m.label, m.source_run_id, m.source_principal_id
                 FROM memory_fts
                 JOIN memory_entries m ON m.id = memory_fts.id
                 WHERE memory_fts MATCH ?1
                 LIMIT ?2",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![fts_q, limit.max(1) as i64], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            });
        match rows {
            Ok(rows) => {
                let mut out = Vec::new();
                for row in rows {
                    let (id, content, label, source_run_id, source_principal_id) =
                        row.map_err(|e| e.to_string())?;
                    out.push(row_to_memory(
                        id,
                        content,
                        label,
                        source_run_id,
                        source_principal_id,
                    )?);
                }
                Ok(out)
            }
            Err(_) => {
                // Fallback substring search
                let like = format!("%{}%", query);
                let mut stmt = conn
                    .prepare(
                        "SELECT id, content, label, source_run_id, source_principal_id
                         FROM memory_entries
                         WHERE content LIKE ?1 OR IFNULL(label,'') LIKE ?1
                         LIMIT ?2",
                    )
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map(params![like, limit.max(1) as i64], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, Option<String>>(2)?,
                            row.get::<_, Option<String>>(3)?,
                            row.get::<_, Option<String>>(4)?,
                        ))
                    })
                    .map_err(|e| e.to_string())?;
                let mut out = Vec::new();
                for row in rows {
                    let (id, content, label, source_run_id, source_principal_id) =
                        row.map_err(|e| e.to_string())?;
                    out.push(row_to_memory(
                        id,
                        content,
                        label,
                        source_run_id,
                        source_principal_id,
                    )?);
                }
                Ok(out)
            }
        }
    }

    async fn search_transcripts(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<(SessionId, TranscriptMessage)>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let like = format!("%{}%", query);
        let mut stmt = conn
            .prepare(
                "SELECT session_id, role, content FROM transcript_messages
                 WHERE content LIKE ?1 ORDER BY id ASC LIMIT ?2",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![like, limit.max(1) as i64], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            let (sid, role, content) = row.map_err(|e| e.to_string())?;
            out.push((
                parse_session_id(&sid)?,
                TranscriptMessage {
                    role: role_from_str(&role)?,
                    content,
                },
            ));
        }
        Ok(out)
    }

    async fn create_schedule(&self, schedule: Schedule) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let tools = serde_json::to_string(&schedule.policy_tools).map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO schedules (id, principal_id, session_id, goal, interval_secs, status,
             next_fire_at, policy_tools, last_fired_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                schedule.id.to_string(),
                schedule.principal_id.to_string(),
                schedule.session_id.map(|id| id.to_string()),
                schedule.goal,
                schedule.interval_secs as i64,
                schedule_status_to_str(schedule.status),
                schedule.next_fire_at,
                tools,
                schedule.last_fired_at,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    async fn update_schedule(&self, schedule: Schedule) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let tools = serde_json::to_string(&schedule.policy_tools).map_err(|e| e.to_string())?;
        let n = conn
            .execute(
                "UPDATE schedules SET principal_id = ?1, session_id = ?2, goal = ?3, interval_secs = ?4,
                 status = ?5, next_fire_at = ?6, policy_tools = ?7, last_fired_at = ?8 WHERE id = ?9",
                params![
                    schedule.principal_id.to_string(),
                    schedule.session_id.map(|id| id.to_string()),
                    schedule.goal,
                    schedule.interval_secs as i64,
                    schedule_status_to_str(schedule.status),
                    schedule.next_fire_at,
                    tools,
                    schedule.last_fired_at,
                    schedule.id.to_string(),
                ],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err(format!("schedule {} not found", schedule.id));
        }
        Ok(())
    }

    async fn get_schedule(&self, id: ScheduleId) -> Result<Option<Schedule>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, principal_id, session_id, goal, interval_secs, status, next_fire_at,
                 policy_tools, last_fired_at FROM schedules WHERE id = ?1",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query(params![id.to_string()])
            .map_err(|e| e.to_string())?;
        match rows.next().map_err(|e| e.to_string())? {
            Some(row) => Ok(Some(row_to_schedule(
                row.get(0).map_err(|e| e.to_string())?,
                row.get(1).map_err(|e| e.to_string())?,
                row.get(2).map_err(|e| e.to_string())?,
                row.get(3).map_err(|e| e.to_string())?,
                row.get(4).map_err(|e| e.to_string())?,
                row.get(5).map_err(|e| e.to_string())?,
                row.get(6).map_err(|e| e.to_string())?,
                row.get(7).map_err(|e| e.to_string())?,
                row.get(8).map_err(|e| e.to_string())?,
            )?)),
            None => Ok(None),
        }
    }

    async fn list_schedules(&self) -> Result<Vec<Schedule>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, principal_id, session_id, goal, interval_secs, status, next_fire_at,
                 policy_tools, last_fired_at FROM schedules WHERE status != 'deleted' ORDER BY id ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, Option<i64>>(8)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            let (id, principal_id, session_id, goal, interval_secs, status, next_fire_at, tools, last) =
                row.map_err(|e| e.to_string())?;
            out.push(row_to_schedule(
                id,
                principal_id,
                session_id,
                goal,
                interval_secs,
                status,
                next_fire_at,
                tools,
                last,
            )?);
        }
        Ok(out)
    }
}

fn schedule_status_to_str(s: ScheduleStatus) -> &'static str {
    match s {
        ScheduleStatus::Active => "active",
        ScheduleStatus::Paused => "paused",
        ScheduleStatus::Deleted => "deleted",
    }
}

fn schedule_status_from_str(s: &str) -> Result<ScheduleStatus, String> {
    match s {
        "active" => Ok(ScheduleStatus::Active),
        "paused" => Ok(ScheduleStatus::Paused),
        "deleted" => Ok(ScheduleStatus::Deleted),
        other => Err(format!("unknown schedule status: {other}")),
    }
}

fn parse_schedule_id(s: &str) -> Result<ScheduleId, String> {
    ScheduleId::from_str(s).map_err(|e| e.to_string())
}

fn row_to_schedule(
    id: String,
    principal_id: String,
    session_id: Option<String>,
    goal: String,
    interval_secs: i64,
    status: String,
    next_fire_at: i64,
    policy_tools: String,
    last_fired_at: Option<i64>,
) -> Result<Schedule, String> {
    let tools: Vec<String> = serde_json::from_str(&policy_tools).unwrap_or_default();
    Ok(Schedule {
        id: parse_schedule_id(&id)?,
        principal_id: PrincipalId::new(principal_id),
        session_id: session_id
            .filter(|s| !s.is_empty())
            .map(|s| parse_session_id(&s))
            .transpose()?,
        goal,
        interval_secs: interval_secs.max(1) as u64,
        status: schedule_status_from_str(&status)?,
        next_fire_at,
        policy_tools: tools,
        last_fired_at,
    })
}

fn parse_memory_id(s: &str) -> Result<MemoryId, String> {
    MemoryId::from_str(s).map_err(|e| e.to_string())
}

fn row_to_memory(
    id: String,
    content: String,
    label: Option<String>,
    source_run_id: Option<String>,
    source_principal_id: Option<String>,
) -> Result<MemoryEntry, String> {
    Ok(MemoryEntry {
        id: parse_memory_id(&id)?,
        content,
        label,
        source_run_id: source_run_id.map(|s| parse_run_id(&s)).transpose()?,
        source_principal_id: source_principal_id.map(PrincipalId::new),
    })
}

fn approval_status_to_str(s: ApprovalStatus) -> &'static str {
    match s {
        ApprovalStatus::Pending => "pending",
        ApprovalStatus::Approved => "approved",
        ApprovalStatus::Denied => "denied",
    }
}

fn approval_status_from_str(s: &str) -> Result<ApprovalStatus, String> {
    match s {
        "pending" => Ok(ApprovalStatus::Pending),
        "approved" => Ok(ApprovalStatus::Approved),
        "denied" => Ok(ApprovalStatus::Denied),
        other => Err(format!("unknown approval status: {other}")),
    }
}

fn parse_approval_id(s: &str) -> Result<ApprovalId, String> {
    ApprovalId::from_str(s).map_err(|e| e.to_string())
}

fn row_to_approval(
    id: String,
    run_id: String,
    action: String,
    summary: String,
    status: String,
    requested_by: String,
    decided_by: Option<String>,
) -> Result<Approval, String> {
    Ok(Approval {
        id: parse_approval_id(&id)?,
        run_id: parse_run_id(&run_id)?,
        action,
        summary,
        status: approval_status_from_str(&status)?,
        requested_by: PrincipalId::new(requested_by),
        decided_by: decided_by.map(PrincipalId::new),
    })
}

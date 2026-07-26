use async_trait::async_trait;
use keryx_app::SessionStore;
use keryx_domain::{
    MessageRole, PrincipalId, Run, RunId, RunStatus, Session, SessionId, Transcript,
    TranscriptMessage,
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
                FOREIGN KEY(session_id) REFERENCES sessions(id)
            );
            CREATE TABLE IF NOT EXISTS transcript_messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                FOREIGN KEY(session_id) REFERENCES sessions(id)
            );
            ",
        )
        .map_err(|e| e.to_string())
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
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::Tool => "tool",
    }
}

fn role_from_str(s: &str) -> Result<MessageRole, String> {
    match s {
        "user" => Ok(MessageRole::User),
        "assistant" => Ok(MessageRole::Assistant),
        "tool" => Ok(MessageRole::Tool),
        other => Err(format!("unknown message role: {other}")),
    }
}

fn row_to_run(
    id: String,
    session_id: String,
    principal_id: String,
    goal: String,
    status: String,
    result: Option<String>,
) -> Result<Run, String> {
    Ok(Run {
        id: parse_run_id(&id)?,
        session_id: parse_session_id(&session_id)?,
        principal_id: PrincipalId::new(principal_id),
        goal,
        status: status_from_str(&status)?,
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
            "INSERT INTO runs (id, session_id, principal_id, goal, status, result)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                run.id.to_string(),
                run.session_id.to_string(),
                run.principal_id.to_string(),
                run.goal,
                status_to_str(run.status),
                run.result,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    async fn update_run(&self, run: Run) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let n = conn
            .execute(
                "UPDATE runs SET session_id = ?1, principal_id = ?2, goal = ?3, status = ?4, result = ?5
                 WHERE id = ?6",
                params![
                    run.session_id.to_string(),
                    run.principal_id.to_string(),
                    run.goal,
                    status_to_str(run.status),
                    run.result,
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
                "SELECT id, session_id, principal_id, goal, status, result FROM runs WHERE id = ?1",
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
}

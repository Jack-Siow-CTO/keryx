use async_trait::async_trait;
use keryx_app::SessionStore;
use keryx_domain::{
    Approval, ApprovalId, ApprovalStatus, ArtifactId, ArtifactKind, ArtifactMeta, MemoryEntry,
    MemoryId, MessageRole, PrincipalId, Run, RunId, RunOrigin, RunStatus, Schedule, ScheduleId,
    ScheduleStatus, Session, SessionId, Transcript, TranscriptMessage,
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
                principal_id TEXT NOT NULL,
                title TEXT,
                created_at INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL DEFAULT 0
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
                message_id TEXT NOT NULL DEFAULT '',
                session_id TEXT NOT NULL,
                run_id TEXT,
                created_at INTEGER NOT NULL DEFAULT 0,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                tool_name TEXT,
                tool_status TEXT,
                tool_summary TEXT,
                artifact_refs TEXT,
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
            CREATE TABLE IF NOT EXISTS artifacts (
                id TEXT PRIMARY KEY NOT NULL,
                kind TEXT NOT NULL,
                media_type TEXT NOT NULL,
                byte_len INTEGER NOT NULL,
                created_at INTEGER NOT NULL,
                run_id TEXT,
                session_id TEXT,
                summary TEXT NOT NULL,
                content_text TEXT
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

        // Console: Session list projection fields (ADR 0027).
        Self::ensure_column(
            &conn,
            "sessions",
            "title",
            "ALTER TABLE sessions ADD COLUMN title TEXT",
        )?;
        Self::ensure_column(
            &conn,
            "sessions",
            "created_at",
            "ALTER TABLE sessions ADD COLUMN created_at INTEGER NOT NULL DEFAULT 0",
        )?;
        Self::ensure_column(
            &conn,
            "sessions",
            "updated_at",
            "ALTER TABLE sessions ADD COLUMN updated_at INTEGER NOT NULL DEFAULT 0",
        )?;
        // Structured Transcript (ADR 0025).
        Self::ensure_column(
            &conn,
            "transcript_messages",
            "message_id",
            "ALTER TABLE transcript_messages ADD COLUMN message_id TEXT NOT NULL DEFAULT ''",
        )?;
        Self::ensure_column(
            &conn,
            "transcript_messages",
            "run_id",
            "ALTER TABLE transcript_messages ADD COLUMN run_id TEXT",
        )?;
        Self::ensure_column(
            &conn,
            "transcript_messages",
            "created_at",
            "ALTER TABLE transcript_messages ADD COLUMN created_at INTEGER NOT NULL DEFAULT 0",
        )?;
        Self::ensure_column(
            &conn,
            "transcript_messages",
            "tool_name",
            "ALTER TABLE transcript_messages ADD COLUMN tool_name TEXT",
        )?;
        Self::ensure_column(
            &conn,
            "transcript_messages",
            "tool_status",
            "ALTER TABLE transcript_messages ADD COLUMN tool_status TEXT",
        )?;
        Self::ensure_column(
            &conn,
            "transcript_messages",
            "tool_summary",
            "ALTER TABLE transcript_messages ADD COLUMN tool_summary TEXT",
        )?;
        Self::ensure_column(
            &conn,
            "transcript_messages",
            "artifact_refs",
            "ALTER TABLE transcript_messages ADD COLUMN artifact_refs TEXT",
        )?;
        Self::ensure_column(
            &conn,
            "artifacts",
            "content_text",
            "ALTER TABLE artifacts ADD COLUMN content_text TEXT",
        )?;
        Ok(())
    }

    fn ensure_column(
        conn: &Connection,
        table: &str,
        column: &str,
        alter_sql: &str,
    ) -> Result<(), String> {
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({table})"))
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
        let mut found = false;
        while let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let name: String = row.get(1).map_err(|e| e.to_string())?;
            if name == column {
                found = true;
                break;
            }
        }
        if !found {
            conn.execute(alter_sql, []).map_err(|e| e.to_string())?;
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

fn row_to_transcript_message(row: &rusqlite::Row<'_>) -> Result<TranscriptMessage, String> {
    use keryx_domain::ToolCompact;
    let row_id: i64 = row.get(0).map_err(|e| e.to_string())?;
    let message_id: String = row.get(1).map_err(|e| e.to_string())?;
    let run_id: Option<String> = row.get(2).map_err(|e| e.to_string())?;
    let created_at: i64 = row.get(3).map_err(|e| e.to_string())?;
    let role: String = row.get(4).map_err(|e| e.to_string())?;
    let content: String = row.get(5).map_err(|e| e.to_string())?;
    let tool_name: Option<String> = row.get(6).map_err(|e| e.to_string())?;
    let tool_status: Option<String> = row.get(7).map_err(|e| e.to_string())?;
    let tool_summary: Option<String> = row.get(8).map_err(|e| e.to_string())?;
    let artifact_refs_raw: Option<String> = row.get(9).map_err(|e| e.to_string())?;

    let id = if message_id.is_empty() {
        format!("row-{row_id}")
    } else {
        message_id
    };
    let tool = match (tool_name, tool_status, tool_summary) {
        (Some(name), status, summary) => Some(ToolCompact {
            name,
            status: status.unwrap_or_else(|| "unknown".into()),
            summary: summary.unwrap_or_default(),
            artifact_refs: artifact_refs_raw
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default(),
        }),
        _ => None,
    };
    Ok(TranscriptMessage {
        id,
        run_id: run_id
            .filter(|s| !s.is_empty())
            .map(|s| parse_run_id(&s))
            .transpose()?,
        created_at,
        role: role_from_str(&role)?,
        content,
        tool,
    })
}

fn row_to_session(row: &rusqlite::Row<'_>) -> Result<Session, String> {
    let sid: String = row.get(0).map_err(|e| e.to_string())?;
    let principal: String = row.get(1).map_err(|e| e.to_string())?;
    let title: Option<String> = row.get(2).map_err(|e| e.to_string())?;
    let created_at: i64 = row.get(3).map_err(|e| e.to_string())?;
    let updated_at: i64 = row.get(4).map_err(|e| e.to_string())?;
    Ok(Session {
        id: parse_session_id(&sid)?,
        principal_id: PrincipalId::new(principal),
        title: title.filter(|s| !s.is_empty()),
        created_at,
        updated_at,
    })
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

#[allow(clippy::too_many_arguments)] // row mapper mirrors SQL columns
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
            "INSERT INTO sessions (id, principal_id, title, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                session.id.to_string(),
                session.principal_id.to_string(),
                session.title,
                session.created_at,
                session.updated_at,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    async fn get_session(&self, id: SessionId) -> Result<Option<Session>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, principal_id, title, created_at, updated_at FROM sessions WHERE id = ?1",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query(params![id.to_string()])
            .map_err(|e| e.to_string())?;
        match rows.next().map_err(|e| e.to_string())? {
            Some(row) => Ok(Some(row_to_session(row)?)),
            None => Ok(None),
        }
    }

    async fn update_session(&self, session: Session) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let n = conn
            .execute(
                "UPDATE sessions SET principal_id = ?1, title = ?2, created_at = ?3, updated_at = ?4
                 WHERE id = ?5",
                params![
                    session.principal_id.to_string(),
                    session.title,
                    session.created_at,
                    session.updated_at,
                    session.id.to_string(),
                ],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err(format!("session {} not found", session.id));
        }
        Ok(())
    }

    async fn list_sessions(&self) -> Result<Vec<Session>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, principal_id, title, created_at, updated_at FROM sessions
                 ORDER BY updated_at DESC, id DESC",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().map_err(|e| e.to_string())? {
            out.push(row_to_session(row)?);
        }
        Ok(out)
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

    async fn list_runs_for_session(&self, session_id: SessionId) -> Result<Vec<Run>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, session_id, principal_id, goal, status, result, origin, parent_run_id
                 FROM runs WHERE session_id = ?1",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query(params![session_id.to_string()])
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().map_err(|e| e.to_string())? {
            out.push(row_to_run(
                row.get(0).map_err(|e| e.to_string())?,
                row.get(1).map_err(|e| e.to_string())?,
                row.get(2).map_err(|e| e.to_string())?,
                row.get(3).map_err(|e| e.to_string())?,
                row.get(4).map_err(|e| e.to_string())?,
                row.get(5).map_err(|e| e.to_string())?,
                row.get(6).map_err(|e| e.to_string())?,
                row.get(7).map_err(|e| e.to_string())?,
            )?);
        }
        Ok(out)
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
                "SELECT id, message_id, run_id, created_at, role, content,
                        tool_name, tool_status, tool_summary, artifact_refs
                 FROM transcript_messages
                 WHERE session_id = ?1 ORDER BY id ASC",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query(params![session_id.to_string()])
            .map_err(|e| e.to_string())?;
        let mut messages = Vec::new();
        while let Some(row) = rows.next().map_err(|e| e.to_string())? {
            messages.push(row_to_transcript_message(row)?);
        }
        Ok(Transcript {
            session_id: Some(session_id),
            messages,
        })
    }

    async fn get_transcript_page(
        &self,
        session_id: SessionId,
        limit: usize,
        before: Option<&str>,
    ) -> Result<(Vec<TranscriptMessage>, Option<String>), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let limit = limit.max(1) as i64;
        // `before` is message_id of oldest on previous page; map to autoincrement id.
        let before_row_id: Option<i64> = if let Some(before_id) = before {
            conn.query_row(
                "SELECT id FROM transcript_messages WHERE session_id = ?1 AND (message_id = ?2 OR CAST(id AS TEXT) = ?2) LIMIT 1",
                params![session_id.to_string(), before_id],
                |row| row.get(0),
            )
            .ok()
        } else {
            None
        };

        let sql = if before_row_id.is_some() {
            "SELECT id, message_id, run_id, created_at, role, content,
                    tool_name, tool_status, tool_summary, artifact_refs
             FROM transcript_messages
             WHERE session_id = ?1 AND id < ?2
             ORDER BY id DESC LIMIT ?3"
        } else {
            "SELECT id, message_id, run_id, created_at, role, content,
                    tool_name, tool_status, tool_summary, artifact_refs
             FROM transcript_messages
             WHERE session_id = ?1
             ORDER BY id DESC LIMIT ?2"
        };

        let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
        let mut messages = Vec::new();
        if let Some(brid) = before_row_id {
            let mut rows = stmt
                .query(params![session_id.to_string(), brid, limit])
                .map_err(|e| e.to_string())?;
            while let Some(row) = rows.next().map_err(|e| e.to_string())? {
                messages.push(row_to_transcript_message(row)?);
            }
        } else {
            let mut rows = stmt
                .query(params![session_id.to_string(), limit])
                .map_err(|e| e.to_string())?;
            while let Some(row) = rows.next().map_err(|e| e.to_string())? {
                messages.push(row_to_transcript_message(row)?);
            }
        }

        let next_before = if messages.len() as i64 == limit {
            messages.last().map(|m| m.id.clone())
        } else {
            None
        };
        Ok((messages, next_before))
    }

    async fn append_transcript(
        &self,
        session_id: SessionId,
        message: TranscriptMessage,
    ) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let artifact_refs = message
            .tool
            .as_ref()
            .map(|t| serde_json::to_string(&t.artifact_refs).unwrap_or_else(|_| "[]".into()));
        conn.execute(
            "INSERT INTO transcript_messages
             (message_id, session_id, run_id, created_at, role, content,
              tool_name, tool_status, tool_summary, artifact_refs)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                message.id,
                session_id.to_string(),
                message.run_id.map(|id| id.to_string()),
                message.created_at,
                role_to_str(&message.role),
                message.content,
                message.tool.as_ref().map(|t| t.name.clone()),
                message.tool.as_ref().map(|t| t.status.clone()),
                message.tool.as_ref().map(|t| t.summary.clone()),
                artifact_refs,
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
        let rows = stmt.query_map(params![fts_q, limit.max(1) as i64], |row| {
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
                "SELECT id, message_id, run_id, created_at, role, content,
                        tool_name, tool_status, tool_summary, artifact_refs, session_id
                 FROM transcript_messages
                 WHERE content LIKE ?1 ORDER BY id ASC LIMIT ?2",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query(params![like, limit.max(1) as i64])
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let sid: String = row.get(10).map_err(|e| e.to_string())?;
            // Reuse row_to which expects cols 0..9
            let msg = row_to_transcript_message(row)?;
            out.push((parse_session_id(&sid)?, msg));
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
            let (
                id,
                principal_id,
                session_id,
                goal,
                interval_secs,
                status,
                next_fire_at,
                tools,
                last,
            ) = row.map_err(|e| e.to_string())?;
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

    async fn create_artifact_meta(&self, meta: ArtifactMeta) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO artifacts (id, kind, media_type, byte_len, created_at, run_id, session_id, summary, content_text)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                meta.id.to_string(),
                meta.kind.as_str(),
                meta.media_type,
                meta.byte_len as i64,
                meta.created_at,
                meta.run_id,
                meta.session_id,
                meta.summary,
                meta.content_text,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    async fn get_artifact_meta(&self, id: ArtifactId) -> Result<Option<ArtifactMeta>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, kind, media_type, byte_len, created_at, run_id, session_id, summary, content_text
                 FROM artifacts WHERE id = ?1",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query(params![id.to_string()])
            .map_err(|e| e.to_string())?;
        match rows.next().map_err(|e| e.to_string())? {
            Some(row) => {
                let id_s: String = row.get(0).map_err(|e| e.to_string())?;
                let kind_s: String = row.get(1).map_err(|e| e.to_string())?;
                let media_type: String = row.get(2).map_err(|e| e.to_string())?;
                let byte_len: i64 = row.get(3).map_err(|e| e.to_string())?;
                let created_at: i64 = row.get(4).map_err(|e| e.to_string())?;
                let run_id: Option<String> = row.get(5).map_err(|e| e.to_string())?;
                let session_id: Option<String> = row.get(6).map_err(|e| e.to_string())?;
                let summary: String = row.get(7).map_err(|e| e.to_string())?;
                let content_text: Option<String> = row.get(8).map_err(|e| e.to_string())?;
                Ok(Some(ArtifactMeta {
                    id: ArtifactId::from_str(&id_s).map_err(|e| e.to_string())?,
                    kind: ArtifactKind::parse(&kind_s)
                        .ok_or_else(|| format!("unknown artifact kind: {kind_s}"))?,
                    media_type,
                    byte_len: byte_len as u64,
                    created_at,
                    run_id,
                    session_id,
                    summary,
                    content_text,
                }))
            }
            None => Ok(None),
        }
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

#[allow(clippy::too_many_arguments)] // row mapper mirrors SQL columns
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

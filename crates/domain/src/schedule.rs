use crate::{PrincipalId, SessionId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Durable identifier for a Schedule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ScheduleId(Uuid);

impl ScheduleId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ScheduleId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ScheduleId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::str::FromStr for ScheduleId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

/// Lifecycle of a Schedule trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleStatus {
    Active,
    Paused,
    Deleted,
}

/// Durable trigger that starts Runs on a cadence with frozen Policy and origin `schedule`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Schedule {
    pub id: ScheduleId,
    pub principal_id: PrincipalId,
    /// Optional Session to continue; when None a new Session is created per fire.
    pub session_id: Option<SessionId>,
    pub goal: String,
    /// Interval between fires in seconds (minimum 1).
    pub interval_secs: u64,
    pub status: ScheduleStatus,
    /// Unix epoch seconds of next planned fire (deterministic clock in tests).
    pub next_fire_at: i64,
    /// Frozen tool allowlist snapshot at authoring time (JSON list of tool names).
    pub policy_tools: Vec<String>,
    /// Last fire time (epoch secs), if any.
    pub last_fired_at: Option<i64>,
}

impl Schedule {
    #[must_use]
    pub fn new(
        principal_id: PrincipalId,
        goal: impl Into<String>,
        interval_secs: u64,
        next_fire_at: i64,
        policy_tools: Vec<String>,
    ) -> Self {
        Self {
            id: ScheduleId::new(),
            principal_id,
            session_id: None,
            goal: goal.into(),
            interval_secs: interval_secs.max(1),
            status: ScheduleStatus::Active,
            next_fire_at,
            policy_tools,
            last_fired_at: None,
        }
    }

    pub fn pause(&mut self) {
        self.status = ScheduleStatus::Paused;
    }

    pub fn resume(&mut self, now: i64) {
        if self.status == ScheduleStatus::Paused {
            self.status = ScheduleStatus::Active;
            // Missed fires: jump next_fire_at forward to now if in the past (no storm).
            if self.next_fire_at < now {
                self.next_fire_at = now;
            }
        }
    }

    pub fn mark_deleted(&mut self) {
        self.status = ScheduleStatus::Deleted;
    }

    /// Record a successful fire and schedule the next occurrence.
    ///
    /// **Missed-fire policy:** if `now` is past `next_fire_at`, advance by
    /// `interval_secs` from the *scheduled* time once (not catch-up loop), so a
    /// Worker downtime does not multi-fire. Documented + tested at Seam 1.
    pub fn record_fire(&mut self, now: i64) {
        self.last_fired_at = Some(now);
        let interval = self.interval_secs as i64;
        if self.next_fire_at <= now {
            // Single step forward from scheduled slot (or now if far behind).
            let base = self.next_fire_at.max(now - interval);
            self.next_fire_at = base + interval;
            if self.next_fire_at <= now {
                self.next_fire_at = now + interval;
            }
        } else {
            self.next_fire_at += interval;
        }
    }

    #[must_use]
    pub fn is_due(&self, now: i64) -> bool {
        self.status == ScheduleStatus::Active && self.next_fire_at <= now
    }
}

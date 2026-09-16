use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rusqlite::{params, Connection};

use harboria_core::domain::{
    EventId, Goal, GoalId, GoalProgress, GoalStatus, InterventionOutcome, InterventionRecord,
    InterventionRecordId, ProgressRecord, StoredEvent, TaskLogEntry, TaskOutcome,
    UserState,
};
use harboria_core::ports::{
    EventStore, GoalStore, InterventionRecordStore, MemoryStore, ProgressRecordStore,
    ProgressStore, TaskLogStore, UserStateStore,
};


pub struct SqliteBackend {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteBackend {
    pub fn open(path: impl AsRef<std::path::Path>) -> anyhow::Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn open_in_memory() -> anyhow::Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS goals (
    id   TEXT PRIMARY KEY,
    data TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS goal_progress (
    goal_id TEXT PRIMARY KEY,
    data    TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS user_state (
    id   INTEGER PRIMARY KEY CHECK (id = 1),
    data TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS memory (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    content    TEXT NOT NULL,
    tags       TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS events (
    id             TEXT PRIMARY KEY,
    event_type     TEXT NOT NULL,
    payload        TEXT NOT NULL,
    occurred_at    TEXT NOT NULL,
    correlation_id TEXT NOT NULL,
    causation_id   TEXT
);

CREATE TABLE IF NOT EXISTS progress_records (
    id      INTEGER PRIMARY KEY AUTOINCREMENT,
    goal_id TEXT NOT NULL,
    data    TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS task_log (
    id      TEXT PRIMARY KEY,
    goal_id TEXT NOT NULL,
    data    TEXT NOT NULL,
    occurred_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS intervention_records (
    id          TEXT PRIMARY KEY,
    goal_id     TEXT,
    occurred_at TEXT NOT NULL,
    data        TEXT NOT NULL,
    has_outcome INTEGER NOT NULL DEFAULT 0
);
"#;

// ---------------------------------------------------------------------------
// GoalStore
// ---------------------------------------------------------------------------

#[async_trait]
impl GoalStore for SqliteBackend {
    async fn create(&self, goal: Goal) -> anyhow::Result<()> {
        self.update(goal).await
    }

    async fn get(&self, id: GoalId) -> anyhow::Result<Option<Goal>> {
        let conn = self.conn.clone();
        let id_str = id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");
            let mut stmt = conn.prepare("SELECT data FROM goals WHERE id = ?1")?;
            let mut rows = stmt.query(params![id_str])?;
            if let Some(row) = rows.next()? {
                let data: String = row.get(0)?;
                let goal: Goal = serde_json::from_str(&data)?;
                Ok::<_, anyhow::Error>(Some(goal))
            } else {
                Ok(None)
            }
        })
        .await?
    }


    async fn list_active(&self) -> anyhow::Result<Vec<Goal>> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");
            let mut stmt = conn.prepare("SELECT data FROM goals")?;
            let rows = stmt.query_map([], |row| {
                let data: String = row.get(0)?;
                Ok(data)
            })?;
            let mut goals = Vec::new();
            for row in rows {
                let goal: Goal = serde_json::from_str(&row?)?;
                if matches!(goal.status, GoalStatus::Active | GoalStatus::AtRisk) {
                    goals.push(goal);
                }
            }
            Ok::<_, anyhow::Error>(goals)
        })
        .await?
    }

    async fn update(&self, goal: Goal) -> anyhow::Result<()> {
        let conn = self.conn.clone();
        let id_str = goal.id.to_string();
        let json = serde_json::to_string(&goal)?;
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");
            conn.execute(
                "INSERT INTO goals (id, data) VALUES (?1, ?2)
                 ON CONFLICT(id) DO UPDATE SET data = excluded.data",
                params![id_str, json],
            )?;
            Ok::<_, anyhow::Error>(())
        })
        .await?
    }
}

// ---------------------------------------------------------------------------
// ProgressStore
// ---------------------------------------------------------------------------

#[async_trait]
impl ProgressStore for SqliteBackend {
    async fn latest(&self, goal_id: GoalId) -> anyhow::Result<Option<GoalProgress>> {
        let conn = self.conn.clone();
        let id_str = goal_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");
            let mut stmt = conn.prepare("SELECT data FROM goal_progress WHERE goal_id = ?1")?;
            let mut rows = stmt.query(params![id_str])?;
            if let Some(row) = rows.next()? {
                let data: String = row.get(0)?;
                let progress: GoalProgress = serde_json::from_str(&data)?;
                Ok::<_, anyhow::Error>(Some(progress))
            } else {
                Ok(None)
            }
        })
        .await?
    }

    async fn upsert(&self, progress: GoalProgress) -> anyhow::Result<()> {
        let conn = self.conn.clone();
        let id_str = progress.goal_id.to_string();
        let json = serde_json::to_string(&progress)?;
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");
            conn.execute(
                "INSERT INTO goal_progress (goal_id, data) VALUES (?1, ?2)
                 ON CONFLICT(goal_id) DO UPDATE SET data = excluded.data",
                params![id_str, json],
            )?;
            Ok::<_, anyhow::Error>(())
        })
        .await?
    }
}

// ---------------------------------------------------------------------------
// UserStateStore
// ---------------------------------------------------------------------------

#[async_trait]
impl UserStateStore for SqliteBackend {
    async fn get(&self) -> anyhow::Result<UserState> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");
            let mut stmt = conn.prepare("SELECT data FROM user_state WHERE id = 1")?;
            let mut rows = stmt.query([])?;
            if let Some(row) = rows.next()? {
                let data: String = row.get(0)?;
                let state: UserState = serde_json::from_str(&data)?;
                Ok::<_, anyhow::Error>(state)
            } else {
                Ok(UserState {
                    stable: Default::default(),
                    temporary: Vec::new(),
                })
            }
        })
        .await?
    }

    async fn put(&self, state: UserState) -> anyhow::Result<()> {
        let conn = self.conn.clone();
        let json = serde_json::to_string(&state)?;
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");
            conn.execute(
                "INSERT INTO user_state (id, data) VALUES (1, ?1)
                 ON CONFLICT(id) DO UPDATE SET data = excluded.data",
                params![json],
            )?;
            Ok::<_, anyhow::Error>(())
        })
        .await?
    }
}

// ---------------------------------------------------------------------------
// ProgressRecordStore
// ---------------------------------------------------------------------------

#[async_trait]
impl ProgressRecordStore for SqliteBackend {
    async fn append(&self, record: ProgressRecord) -> anyhow::Result<()> {
        let conn = self.conn.clone();
        let goal_id = record.goal_id.to_string();
        let json = serde_json::to_string(&record)?;
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");
            conn.execute(
                "INSERT INTO progress_records (goal_id, data) VALUES (?1, ?2)",
                params![goal_id, json],
            )?;
            Ok::<_, anyhow::Error>(())
        })
        .await?
    }

    async fn list_for_goal(&self, goal_id: GoalId) -> anyhow::Result<Vec<ProgressRecord>> {
        let conn = self.conn.clone();
        let goal_id = goal_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");
            let mut stmt =
                conn.prepare("SELECT data FROM progress_records WHERE goal_id = ?1 ORDER BY id ASC")?;
            let rows = stmt.query_map(params![goal_id], |row| row.get::<_, String>(0))?;
            let mut out = Vec::new();
            for row in rows {
                out.push(serde_json::from_str(&row?)?);
            }
            Ok::<_, anyhow::Error>(out)
        })
        .await?
    }
}

// ---------------------------------------------------------------------------
// TaskLogStore
// ---------------------------------------------------------------------------

#[async_trait]
impl TaskLogStore for SqliteBackend {
    async fn append(&self, entry: TaskLogEntry) -> anyhow::Result<()> {
        let conn = self.conn.clone();
        let id = entry.id.to_string();
        let goal_id = entry.goal_id.to_string();
        let occurred_at = entry.occurred_at.to_rfc3339();
        let json = serde_json::to_string(&entry)?;
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");
            conn.execute(
                "INSERT INTO task_log (id, goal_id, data, occurred_at) VALUES (?1, ?2, ?3, ?4)",
                params![id, goal_id, json, occurred_at],
            )?;
            Ok::<_, anyhow::Error>(())
        })
        .await?
    }

    async fn postponed_since(
        &self,
        since: chrono::DateTime<chrono::Utc>,
    ) -> anyhow::Result<Vec<TaskLogEntry>> {
        let conn = self.conn.clone();
        let since = since.to_rfc3339();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");
            let mut stmt =
                conn.prepare("SELECT data FROM task_log WHERE occurred_at >= ?1 ORDER BY occurred_at ASC")?;
            let rows = stmt.query_map(params![since], |row| row.get::<_, String>(0))?;
            let mut out = Vec::new();
            for row in rows {
                let entry: TaskLogEntry = serde_json::from_str(&row?)?;
                if matches!(entry.outcome, TaskOutcome::Postponed) {
                    out.push(entry);
                }
            }
            Ok::<_, anyhow::Error>(out)
        })
        .await?
    }
}

// ---------------------------------------------------------------------------
// InterventionRecordStore
// ---------------------------------------------------------------------------

#[async_trait]
impl InterventionRecordStore for SqliteBackend {
    async fn append(&self, record: InterventionRecord) -> anyhow::Result<()> {
        let conn = self.conn.clone();
        let id = record.id.to_string();
        let goal_id = record.goal_id.map(|g| g.to_string());
        let occurred_at = record.occurred_at.to_rfc3339();
        let has_outcome = record.outcome.is_some() as i64;
        let json = serde_json::to_string(&record)?;
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");
            conn.execute(
                "INSERT INTO intervention_records (id, goal_id, occurred_at, data, has_outcome)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, goal_id, occurred_at, json, has_outcome],
            )?;
            Ok::<_, anyhow::Error>(())
        })
        .await?
    }

    async fn recent(&self, since: chrono::DateTime<chrono::Utc>) -> anyhow::Result<Vec<InterventionRecord>> {
        let conn = self.conn.clone();
        let since = since.to_rfc3339();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");
            let mut stmt = conn.prepare(
                "SELECT data FROM intervention_records WHERE occurred_at >= ?1 ORDER BY occurred_at ASC",
            )?;
            let rows = stmt.query_map(params![since], |row| row.get::<_, String>(0))?;
            let mut out = Vec::new();
            for row in rows {
                out.push(serde_json::from_str(&row?)?);
            }
            Ok::<_, anyhow::Error>(out)
        })
        .await?
    }

    async fn latest_pending_for_goal(
        &self,
        goal_id: GoalId,
    ) -> anyhow::Result<Option<InterventionRecord>> {
        let conn = self.conn.clone();
        let goal_id_str = goal_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");
            let mut stmt = conn.prepare(
                "SELECT data FROM intervention_records
                 WHERE goal_id = ?1 AND has_outcome = 0
                 ORDER BY occurred_at DESC LIMIT 1",
            )?;
            let mut rows = stmt.query(params![goal_id_str])?;
            if let Some(row) = rows.next()? {
                let data: String = row.get(0)?;
                Ok::<_, anyhow::Error>(Some(serde_json::from_str(&data)?))
            } else {
                Ok(None)
            }
        })
        .await?
    }

    async fn record_outcome(
        &self,
        id: InterventionRecordId,
        outcome: InterventionOutcome,
    ) -> anyhow::Result<()> {
        let conn = self.conn.clone();
        let id_str = id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");
            let mut stmt = conn.prepare("SELECT data FROM intervention_records WHERE id = ?1")?;
            let mut rows = stmt.query(params![id_str.clone()])?;
            let data: String = match rows.next()? {
                Some(row) => row.get(0)?,
                None => return Err(anyhow::anyhow!("intervention record not found: {id_str}")),
            };
            drop(rows);
            let mut record: InterventionRecord = serde_json::from_str(&data)?;
            record.outcome = Some(outcome);
            let json = serde_json::to_string(&record)?;
            conn.execute(
                "UPDATE intervention_records SET data = ?1, has_outcome = 1 WHERE id = ?2",
                params![json, id_str],
            )?;
            Ok::<_, anyhow::Error>(())
        })
        .await?
    }
}



#[async_trait]
impl MemoryStore for SqliteBackend {
    async fn append(&self, content: String, tags: Vec<String>) -> anyhow::Result<()> {
        let conn = self.conn.clone();
        let tags_joined = tags.join(",");
        let now = chrono::Utc::now().to_rfc3339();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");
            conn.execute(
                "INSERT INTO memory (content, tags, created_at) VALUES (?1, ?2, ?3)",
                params![content, tags_joined, now],
            )?;
            Ok::<_, anyhow::Error>(())
        })
        .await?
    }

    async fn query(&self, tag: &str, limit: usize) -> anyhow::Result<Vec<String>> {
        let conn = self.conn.clone();
        let tag = tag.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");
            let mut stmt = conn.prepare(
                "SELECT content FROM memory WHERE ',' || tags || ',' LIKE ?1
                 ORDER BY created_at DESC LIMIT ?2",
            )?;
            let pattern = format!("%,{tag},%");
            let rows = stmt.query_map(params![pattern, limit as i64], |row| row.get::<_, String>(0))?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok::<_, anyhow::Error>(out)
        })
        .await?
    }
}

// ---------------------------------------------------------------------------
// EventStore
// ---------------------------------------------------------------------------

#[async_trait]
impl EventStore for SqliteBackend {
    async fn append(&self, event: StoredEvent) -> anyhow::Result<EventId> {
        let conn = self.conn.clone();
        let id_str = event.id.to_string();
        let event_type = event_type_name(&event.event);
        let payload = serde_json::to_string(&event.event)?;
        let occurred_at = event.occurred_at.to_rfc3339();
        let correlation_id = event.correlation_id.to_string();
        let causation_id = event.causation_id.map(|c| c.to_string());
        let returned_id = event.id;
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");
            conn.execute(
                "INSERT INTO events (id, event_type, payload, occurred_at, correlation_id, causation_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![id_str, event_type, payload, occurred_at, correlation_id, causation_id],
            )?;
            Ok::<_, anyhow::Error>(())
        })
        .await??;
        Ok(returned_id)
    }

    async fn load_after(&self, after: Option<EventId>) -> anyhow::Result<Vec<StoredEvent>> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");

            let (sql, after_rowid) = match after {
                Some(ref after_id) => (
                    "SELECT rowid, id, payload, occurred_at, correlation_id, causation_id
                     FROM events WHERE rowid > (SELECT rowid FROM events WHERE id = ?1)
                     ORDER BY rowid ASC",
                    Some(after_id.to_string()),
                ),
                None => (
                    "SELECT rowid, id, payload, occurred_at, correlation_id, causation_id
                     FROM events ORDER BY rowid ASC",
                    None,
                ),
            };
            let mut stmt = conn.prepare(sql)?;
            let map_row = |row: &rusqlite::Row| -> rusqlite::Result<(String, String, String, String, Option<String>)> {
                let id: String = row.get(1)?;
                let payload: String = row.get(2)?;
                let occurred_at: String = row.get(3)?;
                let correlation_id: String = row.get(4)?;
                let causation_id: Option<String> = row.get(5)?;
                Ok((id, payload, occurred_at, correlation_id, causation_id))
            };

            let rows: Vec<(String, String, String, String, Option<String>)> = if let Some(after_rowid) = after_rowid
            {
                stmt.query_map(params![after_rowid], map_row)?
                    .collect::<Result<_, _>>()?
            } else {
                stmt.query_map([], map_row)?.collect::<Result<_, _>>()?
            };

            let mut out = Vec::new();
            for (id, payload, occurred_at, correlation_id, causation_id) in rows {
                out.push(StoredEvent {
                    id: parse_event_id(&id)?,
                    event: serde_json::from_str(&payload)?,
                    occurred_at: chrono::DateTime::parse_from_rfc3339(&occurred_at)?.with_timezone(&chrono::Utc),
                    correlation_id: parse_event_id(&correlation_id)?,
                    causation_id: causation_id.map(|c| parse_event_id(&c)).transpose()?,
                });
            }
            Ok::<_, anyhow::Error>(out)
        })
        .await?
    }
}

fn parse_event_id(s: &str) -> anyhow::Result<EventId> {
    Ok(EventId(uuid::Uuid::parse_str(s)?))
}

fn event_type_name(event: &harboria_core::domain::AssistantEvent) -> &'static str {
    use harboria_core::domain::AssistantEvent::*;
    match event {
        UserInteraction { .. } => "UserInteraction",
        UserStateChanged => "UserStateChanged",
        GoalCreated { .. } => "GoalCreated",
        GoalUpdated { .. } => "GoalUpdated",
        ProgressRecordAdded { .. } => "ProgressRecordAdded",
        TaskStarted { .. } => "TaskStarted",
        TaskCompleted { .. } => "TaskCompleted",
        UserBecameAvailable => "UserBecameAvailable",
        UserBecameBusy => "UserBecameBusy",
        OpportunityDetected { .. } => "OpportunityDetected",
        DecisionMade { .. } => "DecisionMade",
        InterventionOutcomeRecorded { .. } => "InterventionOutcomeRecorded",
        TimerTriggered => "TimerTriggered",
    }
}

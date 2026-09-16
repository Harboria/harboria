use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::ids::{GoalId, TaskLogId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskOutcome {
    Completed,
    Postponed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskLogEntry {
    pub id: TaskLogId,
    pub goal_id: GoalId,
    pub description: String,
    pub outcome: TaskOutcome,
    pub occurred_at: DateTime<Utc>,
}

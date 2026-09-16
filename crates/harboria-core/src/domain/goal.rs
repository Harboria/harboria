use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::ids::GoalId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    pub id: GoalId,
    pub title: String,
    pub description: String,
    pub why: Option<String>,
    pub desired_outcome: String,
    pub goal_type: GoalType,
    pub assistance_mode: AssistanceMode,
    pub created_at: DateTime<Utc>,
    pub deadline: Option<DateTime<Utc>>,
    pub priority: Priority,
    pub status: GoalStatus,
    pub constraints: Vec<Constraint>,
    pub success_criteria: Vec<SuccessCriterion>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GoalType {
    Outcome,
    Project,
    Habit,
    Maintenance,
}

/// Goal-level intervention permission; the Hard Filter reads this field directly.
/// Even if rules generate an Opportunity for a `Passive` Goal, it is dropped during
/// the filtering stage (except at the Critical level, e.g. on the deadline day).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssistanceMode {
    Passive,
    Balanced,
    Proactive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Priority {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GoalStatus {
    Draft,
    Active,
    AtRisk,
    Blocked,
    Paused,
    Completed,
    Abandoned,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Constraint {
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuccessCriterion {
    pub description: String,
}

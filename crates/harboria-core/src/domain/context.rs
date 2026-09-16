use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::goal::Goal;
use super::ids::GoalId;
use super::intervention::InterventionRecord;
use super::progress::GoalProgress;
use super::user_state::{UserOpportunityContext, UserState};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TriggerReason {
    UserBecameAvailable,
    GoalStateChanged,
    UserStateChanged,
    TimerTriggered,
    ManualTick,
}

/// A read-only snapshot assembled on the fly for each evaluation cycle. It is not
/// persisted as long-term state — this avoids two competing sources of truth
/// ("Context database" vs. "real data"). It only leaves a trace in the Decision Log
/// via correlation_id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionContext {
    pub as_of: DateTime<Utc>,
    pub user_state: UserState,
    pub opportunity_context: UserOpportunityContext,
    pub goals: Vec<(Goal, GoalProgress)>,
    pub recent_interventions: Vec<InterventionRecord>,
    /// `(goal_id, occurred_at)`, containing only `TaskOutcome::Postponed` records,
    /// already truncated to the time window defined by
    /// `policies::defaults::DEFAULT_AVOIDANCE_RULE.window`.
    /// This is placed here rather than letting `services::evaluation::evaluate` query
    /// TaskLogStore itself, in order to keep `evaluate` a pure function that performs no IO,
    /// making it easier to unit test.
    pub recent_postponements: Vec<(GoalId, DateTime<Utc>)>,
    pub trigger: TriggerReason,
}

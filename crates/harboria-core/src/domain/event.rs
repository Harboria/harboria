use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::decision::Decision;
use super::ids::{EventId, GoalId};
use super::intervention::InterventionOutcome;
use super::opportunity::Opportunity;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AssistantEvent {
    UserInteraction {
        text: String,
    },
    UserStateChanged,
    GoalCreated {
        goal_id: GoalId,
    },
    GoalUpdated {
        goal_id: GoalId,
    },
    ProgressRecordAdded {
        goal_id: GoalId,
    },
    TaskStarted {
        goal_id: GoalId,
    },
    TaskCompleted {
        goal_id: GoalId,
        succeeded: bool,
    },
    UserBecameAvailable,
    UserBecameBusy,
    OpportunityDetected {
        opportunity: Opportunity,
    },
    DecisionMade {
        decision: Decision,
    },
    InterventionOutcomeRecorded {
        goal_id: Option<GoalId>,
        outcome: InterventionOutcome,
    },
    TimerTriggered,
}

/// Persisted event wrapper: `correlation_id` runs through the entire chain of
/// "one intervention decision" (OpportunityDetected -> DecisionMade -> TaskCompleted),
/// and the Decision Log uses it to string together a complete chain directly.
/// `causation_id` only adds the field in the first version, without enforcing
/// population; the value is allowed to be empty for now.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredEvent {
    pub id: EventId,
    pub event: AssistantEvent,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: EventId,
    pub causation_id: Option<EventId>,
}

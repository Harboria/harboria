use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::decision::Decision;
use super::ids::GoalId;

/// Proactive interruption (consumes global budget) vs. user-initiated (does not consume)
/// vs. small talk (independent channel) vs. high priority (barely constrained by budget)
/// — these cannot share the same counter, otherwise two different suppression signals —
/// "the user has already been bothered once today" and "this Goal was just mentioned
/// half an hour ago" — would get mixed together.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InteractionOrigin {
    Proactive,
    UserRequested,
    Companion,
    Critical,
}

/// The user's feedback on an intervention — this is the highest-value signal in
/// "self-evolution" and should start being accumulated from the first version, even
/// if there is not yet a UserModelEngine to consume it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InterventionOutcome {
    Accepted,
    Rejected,
    Ignored,
    ExplicitFeedback(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterventionRecord {
    pub id: super::ids::InterventionRecordId,
    pub goal_id: Option<GoalId>,
    pub origin: InteractionOrigin,
    pub decision: Decision,
    pub occurred_at: DateTime<Utc>,
    pub outcome: Option<InterventionOutcome>,
}

/// Global fatigue level: only counts `InteractionOrigin::Proactive`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InterventionBudget {
    pub proactive_count_today: u32,
    pub last_proactive_at: Option<DateTime<Utc>>,
}

/// Per-Goal cooldown and the global budget are two independent dimensions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GoalInterventionState {
    pub goal_id: Option<GoalId>,
    pub last_intervention_at: Option<DateTime<Utc>>,
    pub cooldown_until: Option<DateTime<Utc>>,
}

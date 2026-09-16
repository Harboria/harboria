use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::ids::{GoalId, OpportunityId};
use super::progress::EvidenceRef;

/// The MVP is a closed enum, entirely sourced from the Deterministic Evaluation rule
/// layer, with no escape hatch like `Other(String)` — when a new pattern is discovered
/// (e.g. RepeatedAvoidance), a new variant is explicitly added and the version is bumped,
/// rather than letting the LLM bypass the rule layer and improvise freely.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpportunityKind {
    ScheduledWindowMatch,
    DeadlineApproaching,
    PlanMismatchDetected,
    UserBecameAvailable,
    /// The thresholds (how many times, over what time window) are not hardcoded here;
    /// they are centralized in `policies::defaults::AvoidanceRule`, and can have
    /// different parameters per GoalType.
    RepeatedAvoidance,
}

/// Does not pretend that urgency/relevance/interruption_cost are precise 0..1 floats —
/// the first version has no real data to support such precision, so discrete levels are
/// used to avoid "false precision".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ScoreBand {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpportunityScore {
    pub urgency: ScoreBand,
    pub relevance: ScoreBand,
    pub interruption_cost: ScoreBand,
}

/// An Opportunity only represents "there is an opportunity here worth the Assistant's
/// consideration"; it does not carry the final recommended action — "whether to suggest
/// and how to phrase it" is the job of Decision, and the two are not mixed.
///
/// `candidate_goals` can be multiple (e.g. UserBecameAvailable may make
/// Exercise/English/Product all candidates at once), or empty (not belonging to any Goal,
/// going through the Companion/Interaction Loop and not entering Arbitration).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Opportunity {
    pub id: OpportunityId,
    pub detected_at: DateTime<Utc>,
    pub kind: OpportunityKind,
    pub candidate_goals: Vec<GoalId>,
    pub score: OpportunityScore,
    pub rationale: String,
    pub evidence: Vec<EvidenceRef>,
}

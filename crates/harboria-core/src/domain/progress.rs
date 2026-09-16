use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::ids::{EventId, GoalId};

/// Points to the concrete evidence supporting a given evaluation/inference, running
/// through Progress / Preference / Opportunity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceRef {
    pub event_id: EventId,
    pub note: Option<String>,
}

/// Different Goals may well use different units of measurement (count/duration/
/// distance/milestone/custom); no attempt is made to force them into one generic
/// mathematical formula (such as "1 - coefficient of variation").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProgressMetric {
    Count,
    Duration,
    Distance,
    Milestone,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricValue {
    pub amount: f64,
    pub unit: Option<String>,
}

/// A single concrete progress record; it is the raw input of GoalProgress.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressRecord {
    pub goal_id: GoalId,
    pub observed_at: DateTime<Utc>,
    pub metric: ProgressMetric,
    pub value: MetricValue,
    pub source: EvidenceRef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProgressAssessment {
    OnTrack,
    AtRisk,
    Blocked,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deviation {
    pub description: String,
}

/// Computed by `progress_service` based on the history of `ProgressRecord`s; it is not
/// a persisted raw field. How exactly to compute the assessment from a series of
/// ProgressRecords is, in the first version, dispatched by GoalType to different
/// simplest-possible implementations (see services::progress), without pursuing a
/// unified algorithm.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalProgress {
    pub goal_id: GoalId,
    pub as_of: DateTime<Utc>,
    pub assessment: ProgressAssessment,
    pub deviation: Option<Deviation>,
    pub evidence: Vec<EvidenceRef>,
}

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::ids::PreferenceId;
use super::progress::EvidenceRef;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserState {
    pub stable: StableProfile,
    pub temporary: Vec<TemporaryStateItem>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StableProfile {
    pub routines: Vec<Routine>,
    pub preferences: Vec<Preference>,
    pub interaction_style: InteractionStyle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Routine {
    pub description: String,
    pub usual_hour: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvidenceSource {
    ExplicitStatement,
    ObservedBehavior,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preference {
    pub id: PreferenceId,
    pub statement: String,
    pub confidence: f32,
    pub source: EvidenceSource,
    pub evidence: Vec<EvidenceRef>,
    pub created_at: DateTime<Utc>,
    pub last_reinforced_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InteractionStyle {
    pub prefers_brevity: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporaryStateItem {
    pub kind: TemporaryStateKind,
    pub confidence: f32,
    pub source: EvidenceSource,
    pub observed_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TemporaryStateKind {
    Tired,
    Busy,
    LowMood,
    InMeeting,
    HighStress,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Availability {
    Available,
    Busy,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserOpportunityContext {
    pub availability: Availability,
    pub available_duration: Option<std::time::Duration>,
}

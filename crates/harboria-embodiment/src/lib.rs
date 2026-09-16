//! `harboria-embodiment`
//!
//! The semantic protocol layer between Assistant Core and a concrete physical/virtual body.
//!
//! Design principles (finalized after five rounds of discussion):
//! - `Observation` is the Assistant's worldview, not a mirror of any specific hardware IPC
//!   protocol (such as MicroDuck's JSON-RPC `robot.state`). The work of flattening/mapping
//!   concrete hardware fields is entirely encapsulated in the corresponding adapter; neither
//!   this crate nor Core should ever know names like `robotd` / `mediad` / `tofd`.
//! - `subscribe()` is not a mandatory part of the base trait: not every embodiment has
//!   event-push capability (some may only support polling). Those that do implement it
//!   separately via `EmbodimentEventSource`; Core probes for this capability through
//!   `as_event_source()`, and if it's absent, falls back to polling `observe()`.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EmbodimentError {
    #[error("embodiment unavailable: {0}")]
    Unavailable(String),
    #[error("embodiment action rejected: {0}")]
    ActionRejected(String),
    #[error("embodiment transport error: {0}")]
    Transport(String),
}

pub type Result<T> = std::result::Result<T, EmbodimentError>;

// ---------------------------------------------------------------------------
// Capabilities / State
// ---------------------------------------------------------------------------


#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Capabilities {
    pub can_speak: bool,
    pub can_listen: bool,
    pub can_express: bool,
    pub can_attend: bool,
    pub has_event_source: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeviceState {
    Offline,
    Connecting,
    Idle,
    Active,
    Error(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbodimentState {
    pub device: DeviceState,
    pub as_of: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Observation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub timestamp: DateTime<Utc>,
    /// 这个事实是谁提供的，为多 embodiment / 多信息源并存留缝。
    pub source: ObservationSource,
    pub presence: Option<PresenceObservation>,
    pub interaction: Option<InteractionObservation>,
    pub environment: Option<EnvironmentObservation>,
    pub device: DeviceObservation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ObservationSource {
    Embodiment,
    Assistant,
    External,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresenceObservation {
    pub user_present: Option<bool>,
    pub confidence: Option<f32>,
}


#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InteractionObservation {
    pub last_user_utterance: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EnvironmentObservation {
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceObservation {
    pub connected: bool,
    pub state: DeviceState,
}

// ---------------------------------------------------------------------------
// EmbodiedAction
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EmbodiedAction {
    Speak { text: String, tone: Option<Tone> },
    Attention { target: AttentionTarget },
    Express { expression: Expression },
    Idle,
    Wake,
    Sleep,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Tone {
    Neutral,
    Warm,
    Playful,
    Calm,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AttentionTarget {
    User,
    Away,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Expression {
    Happy,
    Curious,
    Concerned,
    Neutral,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    pub accepted: bool,
    pub detail: Option<String>,
}

// ---------------------------------------------------------------------------
// EmbodimentEvent
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EmbodimentEvent {
    UserApproached,
    UserLeft,
    WakeWordDetected,
    DeviceStateChanged(DeviceState),
}


pub type EmbodimentEventStream = tokio::sync::mpsc::Receiver<EmbodimentEvent>;

// ---------------------------------------------------------------------------
// trait
// ---------------------------------------------------------------------------

#[async_trait]
pub trait Embodiment: Send + Sync {
    async fn capabilities(&self) -> Capabilities;
    async fn state(&self) -> EmbodimentState;
    async fn observe(&self) -> Result<Observation>;
    async fn perform(&self, action: EmbodiedAction) -> Result<ActionResult>;

    fn as_event_source(&self) -> Option<&dyn EmbodimentEventSource> {
        None
    }
}

#[async_trait]
pub trait EmbodimentEventSource: Send + Sync {
    async fn subscribe(&self) -> Result<EmbodimentEventStream>;
}

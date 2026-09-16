//! All boundaries that Core knows about from the outside are traits.
//!
//! Dependency inversion rule: traits are defined here (because the domain concepts
//! belong to Core), while concrete implementations (SQLite / LLM API / rule-based
//! arbitration) live in other crates and depend on harboria-core in reverse.
//! `harboria-core` itself never `use`s any concrete implementation crate.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::domain::{
    Decision, DecisionContext, EventId, Goal, GoalId, GoalProgress, InterventionRecord,
    InterventionRecordId, InterventionOutcome, Opportunity, ProgressRecord, StoredEvent,
    TaskLogEntry, UserState,
};

// ---------------------------------------------------------------------------
// Storage
// ---------------------------------------------------------------------------

#[async_trait]
pub trait GoalStore: Send + Sync {
    async fn create(&self, goal: Goal) -> anyhow::Result<()>;
    async fn get(&self, id: GoalId) -> anyhow::Result<Option<Goal>>;
    async fn list_active(&self) -> anyhow::Result<Vec<Goal>>;
    async fn update(&self, goal: Goal) -> anyhow::Result<()>;
}

#[async_trait]
pub trait ProgressStore: Send + Sync {
    async fn latest(&self, goal_id: GoalId) -> anyhow::Result<Option<GoalProgress>>;
    async fn upsert(&self, progress: GoalProgress) -> anyhow::Result<()>;
}


#[async_trait]
pub trait ProgressRecordStore: Send + Sync {
    async fn append(&self, record: ProgressRecord) -> anyhow::Result<()>;
    async fn list_for_goal(&self, goal_id: GoalId) -> anyhow::Result<Vec<ProgressRecord>>;
}

#[async_trait]
pub trait TaskLogStore: Send + Sync {
    async fn append(&self, entry: TaskLogEntry) -> anyhow::Result<()>;

    async fn postponed_since(&self, since: DateTime<Utc>) -> anyhow::Result<Vec<TaskLogEntry>>;
}


#[async_trait]
pub trait InterventionRecordStore: Send + Sync {
    async fn append(&self, record: InterventionRecord) -> anyhow::Result<()>;
    async fn recent(&self, since: DateTime<Utc>) -> anyhow::Result<Vec<InterventionRecord>>;
    async fn latest_pending_for_goal(
        &self,
        goal_id: GoalId,
    ) -> anyhow::Result<Option<InterventionRecord>>;
    async fn record_outcome(
        &self,
        id: InterventionRecordId,
        outcome: InterventionOutcome,
    ) -> anyhow::Result<()>;
}

#[async_trait]
pub trait UserStateStore: Send + Sync {
    async fn get(&self) -> anyhow::Result<UserState>;
    async fn put(&self, state: UserState) -> anyhow::Result<()>;
}


#[async_trait]
pub trait MemoryStore: Send + Sync {
    async fn append(&self, content: String, tags: Vec<String>) -> anyhow::Result<()>;
    async fn query(&self, tag: &str, limit: usize) -> anyhow::Result<Vec<String>>;
}

#[async_trait]
pub trait EventStore: Send + Sync {
    async fn append(&self, event: StoredEvent) -> anyhow::Result<EventId>;
    async fn load_after(&self, after: Option<EventId>) -> anyhow::Result<Vec<StoredEvent>>;
}


pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}



#[derive(Debug, Clone, Serialize)]
pub struct RankedOpportunity {
    pub opportunity: Opportunity,
    pub rank: u32,
    pub rationale: String,
}

#[async_trait]
pub trait ArbitrationEngine: Send + Sync {
    async fn arbitrate(
        &self,
        opportunities: Vec<Opportunity>,
        ctx: &DecisionContext,
    ) -> Vec<RankedOpportunity>;
}



#[async_trait]
pub trait ReasoningProvider: Send + Sync {
    async fn decide(
        &self,
        context: &DecisionContext,
        opportunities: &[RankedOpportunity],
    ) -> anyhow::Result<Decision>;
}



#[async_trait]
pub trait ConversationProvider: Send + Sync {
    async fn respond(
        &self,
        user_text: &str,
        user_state: &UserState,
        recent_memory: &[String],
    ) -> anyhow::Result<String>;
}

use std::sync::Arc;

use tokio::sync::RwLock;

use harboria_embodiment::{EmbodiedAction, Embodiment, Tone};
use harboria_execution::{ExecutionEngine, ExecutionRequest, RiskLevel, TaskSpec};

use crate::domain::{
    AssistantEvent, Availability, Decision, DecisionContext, EventId, GoalId, GoalProgress,
    InteractionOrigin, InterventionRecord, InterventionRecordId, ProgressAssessment,
    TriggerReason, UserOpportunityContext,
};
use crate::policies::defaults::DEFAULT_AVOIDANCE_RULE;
use crate::policies::{InterventionPolicy, PolicyOutcome};
use crate::ports::{
    ArbitrationEngine, Clock, GoalStore, InterventionRecordStore, ProgressStore, ReasoningProvider,
    TaskLogStore, UserStateStore,
};
use crate::services::evaluation;

use super::event_bus::EventBus;


const ARBITRATION_TOP_K: usize = 3;


pub struct AssistantRuntime {
    pub goal_store: Arc<dyn GoalStore>,
    pub progress_store: Arc<dyn ProgressStore>,
    pub user_state_store: Arc<dyn UserStateStore>,
    pub intervention_store: Arc<dyn InterventionRecordStore>,
    pub task_log_store: Arc<dyn TaskLogStore>,
    pub arbitration: Arc<dyn ArbitrationEngine>,
    pub reasoning: Arc<dyn ReasoningProvider>,
    pub embodiment: Arc<dyn Embodiment>,
    pub execution: Arc<dyn ExecutionEngine>,
    pub clock: Arc<dyn Clock>,
    pub policy: InterventionPolicy,
    pub event_bus: EventBus,
    pub availability: Arc<RwLock<(Availability, Option<std::time::Duration>)>>,
}

impl AssistantRuntime {
    pub async fn run_assistant_loop(&self, trigger: TriggerReason) -> anyhow::Result<Decision> {
        let ctx = self.build_context(trigger).await?;

        let candidates = evaluation::evaluate(&ctx);
        if candidates.is_empty() {
            return self
                .finish_with(Decision::DoNothing {
                    reason: "no opportunity detected by deterministic evaluation".into(),
                })
                .await;
        }

        let filtered = self.policy.filter_opportunities(candidates, &ctx);
        if filtered.is_empty() {
            return self
                .finish_with(Decision::DoNothing {
                    reason: "all candidates dropped by hard policy filter".into(),
                })
                .await;
        }

        let ranked = self.arbitration.arbitrate(filtered, &ctx).await;
        let top_k: Vec<_> = ranked.into_iter().take(ARBITRATION_TOP_K).collect();

        for r in &top_k {
            self.event_bus
                .publish(
                    AssistantEvent::OpportunityDetected {
                        opportunity: r.opportunity.clone(),
                    },
                    EventId::new(),
                    None,
                )
                .await?;
        }


        let decision = match self.reasoning.decide(&ctx, &top_k).await {
            Ok(decision) => decision,
            Err(err) => {
                tracing::warn!(error = %err, "reasoning provider failed, falling back to DoNothing");
                Decision::DoNothing {
                    reason: format!("reasoning provider error: {err}"),
                }
            }
        };

        match self.policy.validate_decision(&decision, &ctx) {
            PolicyOutcome::Approved => {
                self.apply_decision(&decision).await?;
                self.record_intervention(&decision).await?;
                self.finish_with(decision).await
            }
            PolicyOutcome::RequiresConfirmation(reason) => {
                tracing::info!(%reason, "decision requires confirmation, not auto-applying in MVP");
                self.finish_with(decision).await
            }
            PolicyOutcome::Rejected(reason) => {
                tracing::warn!(%reason, "decision rejected by policy");
                self.finish_with(Decision::DoNothing {
                    reason: format!("policy rejected decision: {reason}"),
                })
                .await
            }
        }
    }

    async fn build_context(&self, trigger: TriggerReason) -> anyhow::Result<DecisionContext> {
        let now = self.clock.now();
        let active_goals = self.goal_store.list_active().await?;

        let mut goals = Vec::with_capacity(active_goals.len());
        for goal in active_goals {
            let progress = self
                .progress_store
                .latest(goal.id)
                .await?
                .unwrap_or_else(|| GoalProgress {
                    goal_id: goal.id,
                    as_of: now,
                    assessment: ProgressAssessment::Unknown,
                    deviation: None,
                    evidence: Vec::new(),
                });
            goals.push((goal, progress));
        }

        let user_state = self.user_state_store.get().await?;


        let observed_presence = self
            .embodiment
            .observe()
            .await
            .ok()
            .and_then(|o| o.presence)
            .and_then(|p| p.user_present);

        let opportunity_context = if let Some(present) = observed_presence {
            UserOpportunityContext {
                availability: if present {
                    Availability::Available
                } else {
                    Availability::Busy
                },
                available_duration: None,
            }
        } else {
            let (availability, available_duration) = *self.availability.read().await;
            UserOpportunityContext {
                availability,
                available_duration,
            }
        };


        let recent_interventions = self
            .intervention_store
            .recent(now - chrono::Duration::hours(24))
            .await?;


        let recent_postponements: Vec<(GoalId, chrono::DateTime<chrono::Utc>)> = self
            .task_log_store
            .postponed_since(now - DEFAULT_AVOIDANCE_RULE.window)
            .await?
            .into_iter()
            .map(|entry| (entry.goal_id, entry.occurred_at))
            .collect();

        Ok(DecisionContext {
            as_of: now,
            user_state,
            opportunity_context,
            goals,
            recent_interventions,
            recent_postponements,
            trigger,
        })
    }

    async fn apply_decision(&self, decision: &Decision) -> anyhow::Result<()> {
        match decision {
            Decision::DoNothing { .. } | Decision::Wait { .. } => {}
            Decision::Companion { message } => {
                if let Some(text) = message {
                    let _ = self
                        .embodiment
                        .perform(EmbodiedAction::Speak {
                            text: text.clone(),
                            tone: None,
                        })
                        .await;
                }
            }
            Decision::Mention { message, .. } => {
                let _ = self
                    .embodiment
                    .perform(EmbodiedAction::Speak {
                        text: message.clone(),
                        tone: Some(Tone::Neutral),
                    })
                    .await;
            }
            Decision::Ask { question, .. } => {
                let _ = self
                    .embodiment
                    .perform(EmbodiedAction::Speak {
                        text: question.text.clone(),
                        tone: Some(Tone::Neutral),
                    })
                    .await;
            }
            Decision::Suggest { suggestion, .. } => {
                let _ = self
                    .embodiment
                    .perform(EmbodiedAction::Speak {
                        text: suggestion.text.clone(),
                        tone: Some(Tone::Warm),
                    })
                    .await;
            }
            Decision::Intervene { plan, .. } => {
                let _ = self
                    .embodiment
                    .perform(EmbodiedAction::Speak {
                        text: plan.summary.clone(),
                        tone: Some(Tone::Warm),
                    })
                    .await;
            }
            Decision::Execute { goal_id, task } => {
                let request = ExecutionRequest {
                    goal_ref: goal_id.as_execution_ref(),
                    task: TaskSpec {
                        description: task.description.clone(),
                        tool_name: task.tool_name.clone(),
                        args: task.args.clone(),
                    },

                    risk_level: RiskLevel::Write,
                };
                let _ = self.execution.execute(request).await;
            }
        }
        Ok::<(), anyhow::Error>(())
    }


    async fn record_intervention(&self, decision: &Decision) -> anyhow::Result<()> {
        if matches!(decision, Decision::DoNothing { .. } | Decision::Wait { .. }) {

            return Ok(());
        }

        let goal_id = extract_goal_id(decision);
        if goal_id.is_none() && !matches!(decision, Decision::Companion { .. }) {
            return Ok(());
        }

        let record = InterventionRecord {
            id: InterventionRecordId::new(),
            goal_id,
            origin: InteractionOrigin::Proactive,
            decision: decision.clone(),
            occurred_at: self.clock.now(),
            outcome: None,
        };
        self.intervention_store.append(record).await
    }

    async fn finish_with(&self, decision: Decision) -> anyhow::Result<Decision> {
        self.event_bus
            .publish(
                AssistantEvent::DecisionMade {
                    decision: decision.clone(),
                },
                EventId::new(),
                None,
            )
            .await?;
        Ok(decision)
    }


    pub async fn set_availability(
        &self,
        availability: Availability,
        available_duration: Option<std::time::Duration>,
    ) -> anyhow::Result<()> {
        *self.availability.write().await = (availability, available_duration);
        let event = match availability {
            Availability::Available => AssistantEvent::UserBecameAvailable,
            Availability::Busy => AssistantEvent::UserBecameBusy,
            Availability::Unknown => return Ok(()),
        };
        self.event_bus.publish(event, EventId::new(), None).await?;
        Ok(())
    }
}

fn extract_goal_id(decision: &Decision) -> Option<GoalId> {
    match decision {
        Decision::Mention { goal_id, .. }
        | Decision::Ask { goal_id, .. }
        | Decision::Suggest { goal_id, .. }
        | Decision::Intervene { goal_id, .. }
        | Decision::Execute { goal_id, .. } => Some(*goal_id),
        Decision::DoNothing { .. } | Decision::Wait { .. } | Decision::Companion { .. } => None,
    }
}

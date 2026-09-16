use chrono::{DateTime, Utc};

use crate::domain::{
    AssistanceMode, Decision, DecisionContext, GoalId, GoalStatus, InteractionOrigin, Priority,
};

use super::defaults::{DEFAULT_GOAL_COOLDOWN, DEFAULT_MAX_PROACTIVE_PER_DAY};

#[derive(Debug, Clone)]
pub enum PolicyOutcome {
    Approved,
    RequiresConfirmation(String),
    Rejected(String),
}

pub struct InterventionPolicy;

impl Default for InterventionPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl InterventionPolicy {
    pub fn new() -> Self {
        Self
    }

    /// After candidates are generated and before entering Arbitration/Reasoning:
    /// drop the candidates corresponding to goals that are paused/passive/in cooldown.
    pub fn filter_opportunities(
        &self,
        opportunities: Vec<crate::domain::Opportunity>,
        ctx: &DecisionContext,
    ) -> Vec<crate::domain::Opportunity> {
        opportunities
            .into_iter()
            .filter_map(|mut opp| {
                let had_candidates = !opp.candidate_goals.is_empty();
                let surviving: Vec<GoalId> = opp
                    .candidate_goals
                    .iter()
                    .copied()
                    .filter(|gid| self.goal_passes_hard_filter(*gid, ctx, ctx.as_of))
                    .collect();
                if had_candidates && surviving.is_empty() {
                    None
                } else {
                    opp.candidate_goals = surviving;
                    Some(opp)
                }
            })
            .collect()
    }

    /// The final validation after the LLM produces a Decision and before Execution/Embodiment.
    pub fn validate_decision(&self, decision: &Decision, ctx: &DecisionContext) -> PolicyOutcome {
        match decision {
            Decision::DoNothing { .. } | Decision::Wait { .. } | Decision::Companion { .. } => {
                PolicyOutcome::Approved
            }
            Decision::Mention { goal_id, .. }
            | Decision::Ask { goal_id, .. }
            | Decision::Suggest { goal_id, .. }
            | Decision::Intervene { goal_id, .. } => {
                if !self.goal_passes_hard_filter(*goal_id, ctx, ctx.as_of) {
                    return PolicyOutcome::Rejected(
                        "goal is paused / passive / cooling down".to_string(),
                    );
                }
                if self.proactive_count_today(ctx) >= DEFAULT_MAX_PROACTIVE_PER_DAY {
                    return PolicyOutcome::Rejected(
                        "daily proactive intervention budget exhausted".to_string(),
                    );
                }
                PolicyOutcome::Approved
            }
            Decision::Execute { goal_id, .. } => {
                if !self.goal_passes_hard_filter(*goal_id, ctx, ctx.as_of) {
                    return PolicyOutcome::Rejected("goal not eligible for execution".to_string());
                }
                // The External/Dangerous classification is determined by ToolDefinition.risk
                // in the Execution layer; in the first version, manual confirmation is
                // uniformly required here, with no automatic approval.
                PolicyOutcome::RequiresConfirmation(
                    "execution actions require confirmation in MVP".to_string(),
                )
            }
        }
    }

    fn goal_passes_hard_filter(
        &self,
        goal_id: GoalId,
        ctx: &DecisionContext,
        now: DateTime<Utc>,
    ) -> bool {
        let Some((goal, _progress)) = ctx.goals.iter().find(|(g, _)| g.id == goal_id) else {
            return false;
        };

        if !matches!(goal.status, GoalStatus::Active | GoalStatus::AtRisk) {
            return false;
        }

        if goal.assistance_mode == AssistanceMode::Passive && goal.priority != Priority::Critical {
            return false;
        }

        if let Some(rec) = ctx
            .recent_interventions
            .iter()
            .filter(|r| r.goal_id == Some(goal_id))
            .max_by_key(|r| r.occurred_at)
        {
            let cooldown_until = rec.occurred_at
                + chrono::Duration::from_std(DEFAULT_GOAL_COOLDOWN)
                    .expect("DEFAULT_GOAL_COOLDOWN fits in chrono::Duration");
            if now < cooldown_until {
                return false;
            }
        }

        true
    }

    fn proactive_count_today(&self, ctx: &DecisionContext) -> u32 {
        ctx.recent_interventions
            .iter()
            .filter(|r| {
                matches!(r.origin, InteractionOrigin::Proactive)
                    && r.occurred_at.date_naive() == ctx.as_of.date_naive()
            })
            .count() as u32
    }
}

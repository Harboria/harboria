use async_trait::async_trait;

use crate::domain::{DecisionContext, Goal, Opportunity, OpportunityKind, Priority, ScoreBand};
use crate::ports::{ArbitrationEngine, RankedOpportunity};


#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct RankKey {
    deadline_critical: bool,
    priority: Priority,
    urgency: ScoreBand,
    relevance: ScoreBand,
}

pub struct RuleBasedArbitration;

impl RuleBasedArbitration {
    pub fn new() -> Self {
        Self
    }

    fn max_priority(&self, opp: &Opportunity, goals: &[(Goal, crate::domain::GoalProgress)]) -> Priority {
        opp.candidate_goals
            .iter()
            .filter_map(|gid| goals.iter().find(|(g, _)| g.id == *gid))
            .map(|(g, _)| g.priority)
            .max()
            .unwrap_or(Priority::Low)
    }

    fn rank_key(&self, opp: &Opportunity, ctx: &DecisionContext) -> RankKey {
        RankKey {
            deadline_critical: matches!(opp.kind, OpportunityKind::DeadlineApproaching),
            priority: self.max_priority(opp, &ctx.goals),
            urgency: opp.score.urgency,
            relevance: opp.score.relevance,
        }
    }
}

impl Default for RuleBasedArbitration {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ArbitrationEngine for RuleBasedArbitration {
    async fn arbitrate(
        &self,
        opportunities: Vec<Opportunity>,
        ctx: &DecisionContext,
    ) -> Vec<RankedOpportunity> {
        let mut scored: Vec<(Opportunity, RankKey)> = opportunities
            .into_iter()
            .map(|opp| {
                let key = self.rank_key(&opp, ctx);
                (opp, key)
            })
            .collect();

        scored.sort_by(|a, b| b.1.cmp(&a.1));

        scored
            .into_iter()
            .enumerate()
            .map(|(idx, (opportunity, key))| RankedOpportunity {
                rationale: format!(
                    "deadline_critical={} priority={:?} urgency={:?} relevance={:?}",
                    key.deadline_critical, key.priority, key.urgency, key.relevance
                ),
                opportunity,
                rank: idx as u32,
            })
            .collect()
    }
}

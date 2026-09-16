use chrono::Timelike;
use std::collections::HashMap;

use crate::domain::{
    DecisionContext, GoalId, GoalStatus, GoalType, Opportunity, OpportunityId, OpportunityKind,
    OpportunityScore, ProgressAssessment, ScoreBand, TriggerReason,
};
use crate::policies::defaults::DEFAULT_AVOIDANCE_RULE;

pub fn evaluate(ctx: &DecisionContext) -> Vec<Opportunity> {
    let mut opportunities = Vec::new();

    let mut deadline_candidates = Vec::new();
    let mut habit_window_candidates = Vec::new();
    let mut available_candidates = Vec::new();

    for (goal, progress) in &ctx.goals {
        if !matches!(goal.status, GoalStatus::Active | GoalStatus::AtRisk) {
            continue;
        }

        if let Some(deadline) = goal.deadline {
            let hours_left = (deadline - ctx.as_of).num_hours();
            if hours_left <= 24 && !matches!(progress.assessment, ProgressAssessment::OnTrack) {
                deadline_candidates.push(goal.id);
            }
        }

        if goal.goal_type == GoalType::Habit {
            let now_hour = ctx.as_of.hour() as u8;
            let routine_matches = ctx
                .user_state
                .stable
                .routines
                .iter()
                .any(|r| r.usual_hour == Some(now_hour));
            if routine_matches {
                habit_window_candidates.push(goal.id);
            }
        }

        if matches!(ctx.trigger, TriggerReason::UserBecameAvailable)
            && matches!(progress.assessment, ProgressAssessment::AtRisk)
        {
            available_candidates.push(goal.id);
        }
    }

    push_if_any(
        &mut opportunities,
        deadline_candidates,
        OpportunityKind::DeadlineApproaching,
        ScoreBand::High,
        "goal deadline is within 24 hours and progress is not on track",
        ctx,
    );

    push_if_any(
        &mut opportunities,
        habit_window_candidates,
        OpportunityKind::ScheduledWindowMatch,
        ScoreBand::Medium,
        "current hour matches a known user routine window",
        ctx,
    );

    push_if_any(
        &mut opportunities,
        available_candidates,
        OpportunityKind::UserBecameAvailable,
        ScoreBand::Medium,
        "user just became available and an at-risk goal is overdue for attention",
        ctx,
    );

    let mut postponement_counts: HashMap<GoalId, u32> = HashMap::new();
    for (goal_id, _) in &ctx.recent_postponements {
        *postponement_counts.entry(*goal_id).or_insert(0) += 1;
    }
    let avoidance_candidates: Vec<GoalId> = postponement_counts
        .into_iter()
        .filter(|(_, count)| *count >= DEFAULT_AVOIDANCE_RULE.min_occurrences)
        .map(|(goal_id, _)| goal_id)
        .filter(|goal_id| {
            ctx.goals.iter().any(|(g, _)| {
                g.id == *goal_id && matches!(g.status, GoalStatus::Active | GoalStatus::AtRisk)
            })
        })
        .collect();

    push_if_any(
        &mut opportunities,
        avoidance_candidates,
        OpportunityKind::RepeatedAvoidance,
        ScoreBand::Medium,
        "this goal's tasks have been postponed repeatedly in the recent window",
        ctx,
    );

    opportunities
}

fn push_if_any(
    out: &mut Vec<Opportunity>,
    candidates: Vec<GoalId>,
    kind: OpportunityKind,
    urgency: ScoreBand,
    rationale: &str,
    ctx: &DecisionContext,
) {
    if candidates.is_empty() {
        return;
    }
    out.push(Opportunity {
        id: OpportunityId::new(),
        detected_at: ctx.as_of,
        kind,
        candidate_goals: candidates,
        score: OpportunityScore {
            urgency,
            relevance: ScoreBand::Medium,
            interruption_cost: interruption_cost(ctx),
        },
        rationale: rationale.to_string(),
        evidence: Vec::new(),
    });
}

fn interruption_cost(ctx: &DecisionContext) -> ScoreBand {
    let negative_temporary_states = ctx
        .user_state
        .temporary
        .iter()
        .filter(|s| {
            matches!(
                s.kind,
                crate::domain::TemporaryStateKind::Tired
                    | crate::domain::TemporaryStateKind::LowMood
                    | crate::domain::TemporaryStateKind::HighStress
            )
        })
        .count();

    match negative_temporary_states {
        0 => ScoreBand::Low,
        1 => ScoreBand::Medium,
        _ => ScoreBand::High,
    }
}

use chrono::{DateTime, Utc};

use crate::domain::{
    Goal, GoalId, GoalProgress, GoalType, ProgressAssessment, ProgressRecord,
};

pub fn assess(goal: &Goal, records: &[ProgressRecord], now: DateTime<Utc>) -> GoalProgress {
    let relevant: Vec<&ProgressRecord> = records.iter().filter(|r| r.goal_id == goal.id).collect();

    let assessment = match goal.goal_type {
        GoalType::Habit => assess_habit(&relevant, now),
        GoalType::Project => assess_project(&relevant),
        GoalType::Outcome => assess_outcome(&relevant, goal),
        GoalType::Maintenance => assess_maintenance(&relevant, now),
    };

    GoalProgress {
        goal_id: goal.id,
        as_of: now,
        assessment,
        deviation: None,
        evidence: relevant.iter().map(|r| r.source.clone()).collect(),
    }
}

fn assess_habit(records: &[&ProgressRecord], now: DateTime<Utc>) -> ProgressAssessment {
    let window_days = 7;
    let cutoff = now - chrono::Duration::days(window_days);
    let recent = records.iter().filter(|r| r.observed_at >= cutoff).count();
    if records.is_empty() {
        ProgressAssessment::Unknown
    } else if recent >= (window_days as usize) / 2 {
        ProgressAssessment::OnTrack
    } else if recent > 0 {
        ProgressAssessment::AtRisk
    } else {
        ProgressAssessment::Blocked
    }
}

fn assess_project(records: &[&ProgressRecord]) -> ProgressAssessment {
    if records.is_empty() {
        ProgressAssessment::Unknown
    } else {
        ProgressAssessment::OnTrack
    }
}

fn assess_outcome(records: &[&ProgressRecord], goal: &Goal) -> ProgressAssessment {
    if records.is_empty() {
        return ProgressAssessment::Unknown;
    }
    match goal.deadline {
        Some(_) => ProgressAssessment::OnTrack,
        None => ProgressAssessment::Unknown,
    }
}

fn assess_maintenance(records: &[&ProgressRecord], now: DateTime<Utc>) -> ProgressAssessment {
    let cutoff = now - chrono::Duration::days(30);
    if records.iter().any(|r| r.observed_at >= cutoff) {
        ProgressAssessment::OnTrack
    } else {
        ProgressAssessment::AtRisk
    }
}


pub async fn recompute_and_store(
    goal_id: GoalId,
    goal_store: &dyn crate::ports::GoalStore,
    progress_store: &dyn crate::ports::ProgressStore,
    records: &[ProgressRecord],
    now: DateTime<Utc>,
) -> anyhow::Result<()> {
    if let Some(goal) = goal_store.get(goal_id).await? {
        let progress = assess(&goal, records, now);
        progress_store.upsert(progress).await?;
    }
    Ok(())
}

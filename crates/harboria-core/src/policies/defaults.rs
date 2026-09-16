use std::time::Duration;

#[derive(Debug, Clone, Copy)]
pub struct AvoidanceRule {
    pub min_occurrences: u32,
    pub window: Duration,
}

pub const DEFAULT_AVOIDANCE_RULE: AvoidanceRule = AvoidanceRule {
    min_occurrences: 3,
    window: Duration::from_secs(14 * 24 * 3600), // 两周窗口内出现 >= 3 次
};

/// Global proactive interruption budget
pub const DEFAULT_MAX_PROACTIVE_PER_DAY: u32 = 3;

/// Per-Goal cooldown: after a Goal has been Intervened on, it will not be triggered
/// again for at least this long.
pub const DEFAULT_GOAL_COOLDOWN: Duration = Duration::from_secs(4 * 3600);

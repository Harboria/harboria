use serde::{Deserialize, Serialize};
use std::time::Duration;

use super::ids::GoalId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Question {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suggestion {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterventionPlan {
    pub summary: String,
    /// For concrete adjustments such as "reduce intensity instead of canceling",
    /// free text is used in the first version; structured Plan-diff is deferred until
    /// Adaptation is actually implemented.
    pub adjustment: Option<String>,
}

/// `Decision::Execute` uses this instead of directly referencing
/// `harboria_execution::ExecutionRequest`: risk_level is determined by Policy based on
/// the `ToolDefinition.risk` corresponding to `tool_name`. The LLM should not declare
/// on its own whether "this action is dangerous" — that would amount to letting the
/// model grant itself permissions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionTask {
    pub description: String,
    pub tool_name: String,
    pub args: serde_json::Value,
}

/// The only legal output of the LLM (`ReasoningProvider::decide`). The LLM is forbidden
/// from writing to the database directly — every variant must first pass through
/// `DecisionPolicy::validate_decision` before it can be Applied.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Decision {
    DoNothing {
        reason: String,
    },
    Wait {
        reevaluate_after: Duration,
    },
    Companion {
        message: Option<String>,
    },
    Mention {
        goal_id: GoalId,
        message: String,
    },
    Ask {
        goal_id: GoalId,
        question: Question,
    },
    Suggest {
        goal_id: GoalId,
        suggestion: Suggestion,
    },
    Intervene {
        goal_id: GoalId,
        plan: InterventionPlan,
    },
    Execute {
        goal_id: GoalId,
        task: ExecutionTask,
    },
}

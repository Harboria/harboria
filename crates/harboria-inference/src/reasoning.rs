//! `ReasoningProvider` 的三个实现：
//!
//! - `NoOpReasoningProvider`：永远 `DoNothing`，纯调试用。
//! - `HeuristicReasoningProvider`：纯规则，不调用任何模型。
//! - `AnthropicReasoningProvider`：真正调用 Anthropic API。
//!


use std::collections::HashSet;

use async_trait::async_trait;
use serde::Deserialize;

use harboria_core::domain::{
    Decision, DecisionContext, GoalId, InterventionPlan, OpportunityKind, Question, ScoreBand,
    Suggestion,
};
use harboria_core::ports::{RankedOpportunity, ReasoningProvider};

use crate::client::AnthropicClient;

pub struct NoOpReasoningProvider;

#[async_trait]
impl ReasoningProvider for NoOpReasoningProvider {
    async fn decide(
        &self,
        _context: &DecisionContext,
        _opportunities: &[RankedOpportunity],
    ) -> anyhow::Result<Decision> {
        tracing::debug!("NoOpReasoningProvider invoked; no LLM backend wired");
        Ok(Decision::DoNothing {
            reason: "no ReasoningProvider backend configured".to_string(),
        })
    }
}


pub struct HeuristicReasoningProvider;

#[async_trait]
impl ReasoningProvider for HeuristicReasoningProvider {
    async fn decide(
        &self,
        _context: &DecisionContext,
        opportunities: &[RankedOpportunity],
    ) -> anyhow::Result<Decision> {
        let Some(top) = opportunities.first() else {
            return Ok(Decision::DoNothing {
                reason: "no ranked opportunities to reason about".to_string(),
            });
        };
        let Some(goal_id) = top.opportunity.candidate_goals.first().copied() else {
            return Ok(Decision::DoNothing {
                reason: "top opportunity has no eligible candidate goal after filtering"
                    .to_string(),
            });
        };

        let gentle = matches!(
            top.opportunity.score.interruption_cost,
            ScoreBand::High | ScoreBand::Critical
        );

        let text = match (top.opportunity.kind, gentle) {
            (OpportunityKind::DeadlineApproaching, true) => {
                "This goal is approaching its deadline. If you're not feeling great right now, you could start with a small step.".to_string()
            }
            (OpportunityKind::DeadlineApproaching, false) => {
                "This goal is approaching its deadline. Do you want to make some progress on it now?".to_string()
            }
            (OpportunityKind::ScheduledWindowMatch, true) => {
                "It's your usual habit time. If you're fairly tired, you could do a lighter version first.".to_string()
            }
            (OpportunityKind::ScheduledWindowMatch, false) => {
                "It's your usual habit time. Do you want to start now?".to_string()
            }
            (OpportunityKind::UserBecameAvailable, _) => {
                "You're free right now, and this goal is falling a bit behind. Do you want to make some progress while you have the chance?".to_string()
            }
            (OpportunityKind::PlanMismatchDetected, _) => {
                "The plan for this goal doesn't quite seem to match reality. Do you want to take a look together at how to adjust it?".to_string()
            }
            (OpportunityKind::RepeatedAvoidance, _) => {
                "This goal has been pushed back several times recently. Do you want to talk about whether the plan itself needs adjusting?".to_string()
            }
        };

        Ok(Decision::Suggest {
            goal_id,
            suggestion: Suggestion { text },
        })
    }
}


#[derive(Debug, Deserialize)]
struct LlmDecisionOutput {
    action: String,
    #[serde(default)]
    goal_id: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    reason: Option<String>,
}

const REASONING_SYSTEM_PROMPT: &str = "You are the reasoning component of a long-term personal \
assistant runtime. You are given a small, pre-filtered set of \"opportunities\" that a \
deterministic rules engine has already identified as worth considering, plus the user's \
current state. Your ONLY job is to decide how to respond to these already-identified \
opportunities.\n\n\
Hard rules:\n\
- You must NOT invent a new opportunity that is not in the given list.\n\
- You must NOT target a goal_id that is not among the candidate_goals of the opportunities \
you were given. Any other goal_id will be rejected and this whole response discarded.\n\
- Respond with a single JSON object and nothing else: no markdown code fences, no prose \
before or after.\n\n\
The JSON object must have exactly these fields:\n\
{\"action\": \"do_nothing\" | \"wait\" | \"mention\" | \"ask\" | \"suggest\" | \"intervene\", \
\"goal_id\": \"<uuid string, or null for do_nothing/wait>\", \
\"message\": \"<string to show the user, or null>\", \
\"reason\": \"<short internal-facing reason, or null>\"}\n\n\
Guidance: keep \"message\" short (one or two sentences), warm, and in the same language the \
user's data appears to be written in. If the user's current state suggests they are tired, \
stressed, or low on energy, prefer a gentler framing (e.g. suggest a lighter version of the \
task) rather than dropping the opportunity entirely, unless declining outright is clearly \
better for the user.";

pub struct AnthropicReasoningProvider {
    client: AnthropicClient,
}

impl AnthropicReasoningProvider {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            client: AnthropicClient::from_env()?,
        })
    }
}

#[async_trait]
impl ReasoningProvider for AnthropicReasoningProvider {
    async fn decide(
        &self,
        context: &DecisionContext,
        opportunities: &[RankedOpportunity],
    ) -> anyhow::Result<Decision> {
        if opportunities.is_empty() {
            return Ok(Decision::DoNothing {
                reason: "no ranked opportunities".to_string(),
            });
        }

        let candidate_goal_ids: HashSet<GoalId> = opportunities
            .iter()
            .flat_map(|r| r.opportunity.candidate_goals.iter().copied())
            .collect();

        let user_prompt = serde_json::json!({
            "top_ranked_opportunities": opportunities,
            "user_state": context.user_state,
            "opportunity_context": context.opportunity_context,
        })
        .to_string();

        let raw = self
            .client
            .complete(REASONING_SYSTEM_PROMPT, &user_prompt, 512)
            .await?;
        let cleaned = strip_code_fence(&raw);
        let output: LlmDecisionOutput = serde_json::from_str(cleaned).map_err(|e| {
            anyhow::anyhow!(
                "failed to parse LLM output as the expected JSON shape: {e}; raw output: {cleaned}"
            )
        })?;

        map_llm_output(output, &candidate_goal_ids)
    }
}

fn strip_code_fence(s: &str) -> &str {
    let s = s.trim();
    let s = s
        .strip_prefix("```json")
        .or_else(|| s.strip_prefix("```"))
        .unwrap_or(s);
    s.strip_suffix("```").unwrap_or(s).trim()
}

fn map_llm_output(
    output: LlmDecisionOutput,
    candidates: &HashSet<GoalId>,
) -> anyhow::Result<Decision> {
    let resolve_goal_id = |raw: Option<String>| -> anyhow::Result<GoalId> {
        let raw = raw.ok_or_else(|| anyhow::anyhow!("this action requires a goal_id but none was given"))?;
        let uuid = uuid::Uuid::parse_str(&raw)
            .map_err(|e| anyhow::anyhow!("goal_id '{raw}' is not a valid uuid: {e}"))?;
        let gid = GoalId(uuid);
        if !candidates.contains(&gid) {
            anyhow::bail!(
                "LLM targeted goal_id {gid} which was not among the candidate goals it was given"
            );
        }
        Ok(gid)
    };

    Ok(match output.action.as_str() {
        "do_nothing" => Decision::DoNothing {
            reason: output.reason.unwrap_or_else(|| "LLM chose to do nothing".to_string()),
        },
        "wait" => Decision::Wait {
            reevaluate_after: std::time::Duration::from_secs(30 * 60),
        },
        "mention" => Decision::Mention {
            goal_id: resolve_goal_id(output.goal_id)?,
            message: output.message.unwrap_or_default(),
        },
        "ask" => Decision::Ask {
            goal_id: resolve_goal_id(output.goal_id)?,
            question: Question {
                text: output.message.unwrap_or_default(),
            },
        },
        "suggest" => Decision::Suggest {
            goal_id: resolve_goal_id(output.goal_id)?,
            suggestion: Suggestion {
                text: output.message.unwrap_or_default(),
            },
        },
        "intervene" => Decision::Intervene {
            goal_id: resolve_goal_id(output.goal_id)?,
            plan: InterventionPlan {
                summary: output.message.unwrap_or_default(),
                adjustment: None,
            },
        },
        other => anyhow::bail!("LLM returned an unrecognized action: '{other}'"),
    })
}

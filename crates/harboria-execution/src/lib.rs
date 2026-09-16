//! `harboria-execution`
//!
//! The Execution layer is not "reusing some execution library"; rather, it is a collection of
//! in-process capabilities that all implement the same `Tool` trait. `ExecutionEngine` is the
//! only boundary that Core knows about; `ToolRegistry` and the concrete `Tool` implementations
//! are its internal details, of which Core is completely unaware.
//!
//! Key division of responsibilities (finalized after five rounds of discussion):
//! - Core decides "whether to do it, and which Goal-related thing to do" (goal-level reasoning).
//! - `ExecutionEngine` decides "how many tools this task actually needs to call, and in what
//!   order" (task-level reasoning). These two layers of reasoning should not be mixed into the
//!   same LLM call. For the first version, `ExecutionEngine` may start as simple single-tool
//!   dispatch, with task-level reasoning added later.

use async_trait::async_trait;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ExecutionError {
    #[error("tool not found: {0}")]
    ToolNotFound(String),
    #[error("tool invocation failed: {0}")]
    ToolFailed(String),
    #[error("execution rejected by policy: {0}")]
    PolicyRejected(String),
}

pub type Result<T> = std::result::Result<T, ExecutionError>;


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GoalRef(pub Uuid);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RiskLevel {
    Read,
    Write,
    External,
    Dangerous,
}

#[derive(Debug, Clone, Default)]
pub struct SideEffects {
    pub network: bool,
    pub filesystem: bool,
    pub irreversible: bool,
}


#[derive(Debug, Clone)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: JsonValue,
    pub risk: RiskLevel,
    pub side_effects: SideEffects,
}

pub type ToolArgs = JsonValue;

#[derive(Debug, Clone)]
pub struct ToolResult {
    pub output: JsonValue,
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn definition(&self) -> ToolDefinition;
    async fn invoke(&self, args: ToolArgs) -> Result<ToolResult>;
}

#[derive(Default)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.definition().name.clone(), tool);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|t| t.definition()).collect()
    }
}

// ---------------------------------------------------------------------------
// ExecutionEngine
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct TaskSpec {
    pub description: String,
    pub tool_name: String,
    pub args: ToolArgs,
}

#[derive(Debug, Clone)]
pub struct ExecutionRequest {
    pub goal_ref: GoalRef,
    pub task: TaskSpec,
    pub risk_level: RiskLevel,
}

#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub goal_ref: GoalRef,
    pub succeeded: bool,
    pub output: Option<JsonValue>,
    pub error: Option<String>,
}

#[async_trait]
pub trait ExecutionEngine: Send + Sync {
    async fn execute(&self, request: ExecutionRequest) -> Result<ExecutionResult>;
}

pub struct SingleToolExecutionEngine {
    registry: ToolRegistry,
}

impl SingleToolExecutionEngine {
    pub fn new(registry: ToolRegistry) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl ExecutionEngine for SingleToolExecutionEngine {
    async fn execute(&self, request: ExecutionRequest) -> Result<ExecutionResult> {
        let tool = self
            .registry
            .get(&request.task.tool_name)
            .ok_or_else(|| ExecutionError::ToolNotFound(request.task.tool_name.clone()))?;

        match tool.invoke(request.task.args.clone()).await {
            Ok(result) => Ok(ExecutionResult {
                goal_ref: request.goal_ref,
                succeeded: true,
                output: Some(result.output),
                error: None,
            }),
            Err(err) => Ok(ExecutionResult {
                goal_ref: request.goal_ref,
                succeeded: false,
                output: None,
                error: Some(err.to_string()),
            }),
        }
    }
}

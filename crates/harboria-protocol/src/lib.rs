//! `harboria-protocol`
//!
//! `harboriad` and `harboria-cli` exchange JSON-RPC 2.0 messages over a Unix Domain Socket
//! using NDJSON. This crate only contains wire types, not any domain models.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

pub const JSONRPC_VERSION: &str = "2.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: JsonValue,
}

impl RpcRequest {
    pub fn new(id: u64, method: impl Into<String>, params: JsonValue) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id,
            method: method.into(),
            params,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcResponse {
    pub jsonrpc: String,
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

impl RpcResponse {
    pub fn ok(id: u64, result: JsonValue) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: u64, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id,
            result: None,
            error: Some(RpcError {
                code,
                message: message.into(),
            }),
        }
    }
}

pub mod methods {
    pub const GOAL_CREATE: &str = "goal.create";
    pub const GOAL_LIST: &str = "goal.list";
    pub const GOAL_GET: &str = "goal.get";
    pub const STATE_GET: &str = "state.get";
    pub const STATE_ADD_ROUTINE: &str = "state.add_routine";
    pub const STATE_ADD_TEMPORARY: &str = "state.add_temporary";
    pub const STATE_SET_AVAILABILITY: &str = "state.set_availability";
    pub const PROGRESS_ADD: &str = "progress.add";
    pub const TASK_LOG: &str = "task.log";
    pub const INTERVENTION_RESPOND: &str = "intervention.respond";
    pub const CHAT: &str = "chat";
    pub const TICK: &str = "tick";
}

pub const DEFAULT_SOCKET_PATH: &str = "/home/buyan/.cache/duck-sim/duck-a.sock";

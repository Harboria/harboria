use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use harboria_protocol::{methods, RpcRequest, RpcResponse, DEFAULT_SOCKET_PATH};


#[derive(Parser)]
#[command(name = "harboria")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// 创建一个新 Goal
    GoalCreate {
        title: String,
        #[arg(long, default_value = "")]
        description: String,
        #[arg(long, default_value = "")]
        desired_outcome: String,
        /// RFC3339 时间戳，例如 2026-09-20T00:00:00Z
        #[arg(long)]
        deadline: Option<String>,
        /// outcome(默认) / habit / project / maintenance
        #[arg(long)]
        goal_type: Option<String>,
        /// balanced(默认) / passive / proactive
        #[arg(long)]
        assistance_mode: Option<String>,
    },
    /// 列出所有仍在被管理的 Goal(Active/AtRisk)
    GoalList,
    /// 查看单个 Goal
    GoalGet { id: String },
    /// 查看当前 UserState
    State,
    /// 给 UserState 加一条惯常时间(比如"通常8点跑步")
    StateAddRoutine {
        /// 0-23
        usual_hour: u8,
        description: String,
    },
    /// 给 UserState 加一条临时状态(tired/busy/lowmood/inmeeting/highstress)
    StateAddTemporary {
        kind: String,
        /// 0.0-1.0
        confidence: f32,
        #[arg(long)]
        expires_in_minutes: Option<i64>,
    },
    /// 记一条 Progress(count/duration/distance/milestone/自定义名字)
    ProgressAdd {
        goal_id: String,
        metric: String,
        amount: f64,
        #[arg(long)]
        unit: Option<String>,
    },
    /// 记一条 Task 日志(completed/postponed)，是 RepeatedAvoidance 的事实依据
    TaskLog {
        goal_id: String,
        description: String,
        outcome: String,
    },
    /// 回应最近一次(还没有 outcome 的)干预：accepted/rejected/ignored/feedback
    InterventionRespond {
        goal_id: String,
        outcome: String,
        #[arg(long)]
        feedback: Option<String>,
    },
    /// 设置 availability 信号，会触发一次自动评估
    StateSetAvailability {
        /// true/false/yes/no
        available: String,
        #[arg(long)]
        duration_minutes: Option<i64>,
    },
    /// 和 Assistant 闲聊一句(Interaction Loop，不影响任何 Goal)
    Chat { text: String },
    /// 手动触发一次 Assistant Loop 评估(调试用，绕过定时器)
    Tick,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let (method, params) = match cli.command {
        Command::GoalCreate {
            title,
            description,
            desired_outcome,
            deadline,
            goal_type,
            assistance_mode,
        } => (
            methods::GOAL_CREATE,
            json!({
                "title": title,
                "description": description,
                "desired_outcome": desired_outcome,
                "deadline": deadline,
                "goal_type": goal_type,
                "assistance_mode": assistance_mode,
            }),
        ),
        Command::GoalList => (methods::GOAL_LIST, json!({})),
        Command::GoalGet { id } => (methods::GOAL_GET, json!({ "id": id })),
        Command::State => (methods::STATE_GET, json!({})),
        Command::StateAddRoutine {
            usual_hour,
            description,
        } => (
            methods::STATE_ADD_ROUTINE,
            json!({ "usual_hour": usual_hour, "description": description }),
        ),
        Command::StateAddTemporary {
            kind,
            confidence,
            expires_in_minutes,
        } => (
            methods::STATE_ADD_TEMPORARY,
            json!({ "kind": kind, "confidence": confidence, "expires_in_minutes": expires_in_minutes }),
        ),
        Command::ProgressAdd {
            goal_id,
            metric,
            amount,
            unit,
        } => (
            methods::PROGRESS_ADD,
            json!({ "goal_id": goal_id, "metric": metric, "amount": amount, "unit": unit }),
        ),
        Command::TaskLog {
            goal_id,
            description,
            outcome,
        } => (
            methods::TASK_LOG,
            json!({ "goal_id": goal_id, "description": description, "outcome": outcome }),
        ),
        Command::InterventionRespond {
            goal_id,
            outcome,
            feedback,
        } => (
            methods::INTERVENTION_RESPOND,
            json!({ "goal_id": goal_id, "outcome": outcome, "feedback": feedback }),
        ),
        Command::StateSetAvailability {
            available,
            duration_minutes,
        } => {
            let available = matches!(available.to_lowercase().as_str(), "true" | "yes" | "1");
            (
                methods::STATE_SET_AVAILABILITY,
                json!({ "available": available, "duration_minutes": duration_minutes }),
            )
        }
        Command::Chat { text } => (methods::CHAT, json!({ "text": text })),
        Command::Tick => (methods::TICK, json!({})),
    };

    let response = call(method, params).await?;
    println!("{}", serde_json::to_string_pretty(&response)?);
    Ok(())
}

async fn call(method: &str, params: serde_json::Value) -> Result<RpcResponse> {
    let stream = UnixStream::connect(DEFAULT_SOCKET_PATH)
        .await
        .with_context(|| format!("failed to connect to harboriad at {DEFAULT_SOCKET_PATH}; is it running?"))?;
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    let req = RpcRequest::new(1, method, params);
    let mut line = serde_json::to_string(&req)?;
    line.push('\n');
    writer.write_all(line.as_bytes()).await?;

    let response_line = lines
        .next_line()
        .await?
        .context("harboriad closed the connection without a response")?;
    let response: RpcResponse = serde_json::from_str(&response_line)?;
    Ok(response)
}

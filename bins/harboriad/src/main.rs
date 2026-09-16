use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use serde::Deserialize;
use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::RwLock;

use harboria_core::domain::{
    AssistanceMode, AssistantEvent, Availability, EvidenceRef, EvidenceSource, Goal, GoalId,
    GoalStatus, GoalType, InterventionOutcome, InterventionRecordId, MetricValue,
    Priority, ProgressMetric, ProgressRecord, Routine, TaskLogEntry, TaskLogId, TaskOutcome,
    TemporaryStateItem, TemporaryStateKind, TriggerReason,
};
use harboria_core::policies::{InterventionPolicy, RuleBasedArbitration};
use harboria_core::ports::{
    Clock, ConversationProvider, EventStore, GoalStore, InterventionRecordStore, MemoryStore,
    ProgressRecordStore, ProgressStore, ReasoningProvider, SystemClock, TaskLogStore,
    UserStateStore,
};
use harboria_core::runtime::{AssistantRuntime, EventBus};
use harboria_core::services;
use harboria_execution::{SingleToolExecutionEngine, ToolRegistry};
use harboria_inference::{
    AnthropicConversationProvider, AnthropicReasoningProvider, EchoConversationProvider,
    HeuristicReasoningProvider,
};
use harboria_protocol::{methods, RpcError, RpcRequest, RpcResponse, DEFAULT_SOCKET_PATH};
use harboria_storage::SqliteBackend;
use microduck_adapter::MicroDuckAdapter;

const DB_PATH: &str = "/tmp/harboriad/data.sqlite3";

const DEFAULT_TICK_INTERVAL_SECS: u64 = 300;

struct AppContext {
    runtime: Arc<AssistantRuntime>,
    progress_record_store: Arc<dyn ProgressRecordStore>,
    memory_store: Arc<dyn MemoryStore>,
    conversation: Arc<dyn ConversationProvider>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let storage = Arc::new(SqliteBackend::open(DB_PATH)?);


    let goal_store: Arc<dyn GoalStore> = storage.clone();
    let progress_store: Arc<dyn ProgressStore> = storage.clone();
    let progress_record_store: Arc<dyn ProgressRecordStore> = storage.clone();
    let user_state_store: Arc<dyn UserStateStore> = storage.clone();
    let event_store: Arc<dyn EventStore> = storage.clone();
    let intervention_store: Arc<dyn InterventionRecordStore> = storage.clone();
    let task_log_store: Arc<dyn TaskLogStore> = storage.clone();
    let memory_store: Arc<dyn MemoryStore> = storage.clone();

    let embodiment = Arc::new(
        MicroDuckAdapter::connect_simulator("simulator://localhost")
            .await
            .map_err(|e| anyhow::anyhow!("failed to construct embodiment adapter: {e}"))?,
    );
    let execution = Arc::new(SingleToolExecutionEngine::new(ToolRegistry::new()));


    let reasoning: Arc<dyn ReasoningProvider> = match AnthropicReasoningProvider::from_env() {
        Ok(provider) => {
            tracing::info!("using AnthropicReasoningProvider (ANTHROPIC_API_KEY is set)");
            Arc::new(provider)
        }
        Err(err) => {
            tracing::warn!(
                reason = %err,
                "ANTHROPIC_API_KEY not usable, falling back to HeuristicReasoningProvider"
            );
            Arc::new(HeuristicReasoningProvider)
        }
    };
    let conversation: Arc<dyn ConversationProvider> = match AnthropicConversationProvider::from_env()
    {
        Ok(provider) => Arc::new(provider),
        Err(err) => {
            tracing::warn!(
                reason = %err,
                "ANTHROPIC_API_KEY not usable, falling back to EchoConversationProvider"
            );
            Arc::new(EchoConversationProvider)
        }
    };

    let arbitration = Arc::new(RuleBasedArbitration::new());
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let event_bus = EventBus::new(event_store);


    spawn_goal_loop(
        event_bus.subscribe(),
        goal_store.clone(),
        progress_store.clone(),
        progress_record_store.clone(),
    );

    let runtime = Arc::new(AssistantRuntime {
        goal_store,
        progress_store,
        user_state_store,
        intervention_store,
        task_log_store,
        arbitration,
        reasoning,
        embodiment,
        execution,
        clock,
        policy: InterventionPolicy::new(),
        event_bus,
        availability: Arc::new(RwLock::new((Availability::Unknown, None))),
    });


    spawn_periodic_ticker(runtime.clone());
    spawn_trigger_listener(runtime.clone());

    let ctx = Arc::new(AppContext {
        runtime,
        progress_record_store,
        memory_store,
        conversation,
    });

    serve_uds(ctx).await
}


fn spawn_periodic_ticker(runtime: Arc<AssistantRuntime>) {
    let interval_secs = std::env::var("HARBORIA_TICK_INTERVAL_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_TICK_INTERVAL_SECS);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
        loop {
            interval.tick().await;
            if let Err(err) = runtime
                .event_bus
                .publish(
                    AssistantEvent::TimerTriggered,
                    harboria_core::domain::EventId::new(),
                    None,
                )
                .await
            {
                tracing::warn!(error = %err, "periodic ticker: failed to publish TimerTriggered");
            }
        }
    });
}


fn spawn_trigger_listener(runtime: Arc<AssistantRuntime>) {
    let mut rx = runtime.event_bus.subscribe();
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(stored) => {
                    let trigger = match stored.event {
                        AssistantEvent::TimerTriggered => Some(TriggerReason::TimerTriggered),
                        AssistantEvent::UserBecameAvailable => {
                            Some(TriggerReason::UserBecameAvailable)
                        }
                        _ => None,
                    };
                    if let Some(trigger) = trigger {
                        if let Err(err) = runtime.run_assistant_loop(trigger).await {
                            tracing::warn!(error = %err, "trigger listener: run_assistant_loop failed");
                        }
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(skipped = n, "trigger listener: event bus receiver lagged");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

fn spawn_goal_loop(
    mut rx: tokio::sync::broadcast::Receiver<harboria_core::domain::StoredEvent>,
    goal_store: Arc<dyn GoalStore>,
    progress_store: Arc<dyn ProgressStore>,
    progress_record_store: Arc<dyn ProgressRecordStore>,
) {
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(stored) => {
                    if let AssistantEvent::ProgressRecordAdded { goal_id } = stored.event {
                        let records = match progress_record_store.list_for_goal(goal_id).await {
                            Ok(r) => r,
                            Err(err) => {
                                tracing::warn!(error = %err, "goal loop: failed to load progress records");
                                continue;
                            }
                        };
                        if let Err(err) = services::progress::recompute_and_store(
                            goal_id,
                            goal_store.as_ref(),
                            progress_store.as_ref(),
                            &records,
                            chrono::Utc::now(),
                        )
                        .await
                        {
                            tracing::warn!(error = %err, "goal loop: failed to recompute progress");
                        } else {
                            tracing::debug!(%goal_id, "goal loop: progress recomputed");
                        }
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(skipped = n, "goal loop: event bus receiver lagged");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

#[derive(Deserialize)]
struct CreateGoalParams {
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    desired_outcome: String,
    deadline: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    goal_type: Option<String>,
    #[serde(default)]
    assistance_mode: Option<String>,
}

#[derive(Deserialize)]
struct GoalIdParams {
    id: String,
}

#[derive(Deserialize)]
struct AddRoutineParams {
    usual_hour: u8,
    description: String,
}

#[derive(Deserialize)]
struct AddTemporaryParams {
    kind: String,
    confidence: f32,
    #[serde(default)]
    expires_in_minutes: Option<i64>,
}

#[derive(Deserialize)]
struct ProgressAddParams {
    goal_id: String,
    metric: String,
    amount: f64,
    #[serde(default)]
    unit: Option<String>,
}

#[derive(Deserialize)]
struct TaskLogParams {
    goal_id: String,
    description: String,
    outcome: String,
}

#[derive(Deserialize)]
struct InterventionRespondParams {
    goal_id: String,
    outcome: String,
    #[serde(default)]
    feedback: Option<String>,
}

#[derive(Deserialize)]
struct SetAvailabilityParams {
    available: bool,
    #[serde(default)]
    duration_minutes: Option<i64>,
}

#[derive(Deserialize)]
struct ChatParams {
    text: String,
}

async fn serve_uds(ctx: Arc<AppContext>) -> Result<()> {
    let _ = std::fs::remove_file(DEFAULT_SOCKET_PATH);
    let listener = UnixListener::bind(DEFAULT_SOCKET_PATH)?;
    tracing::info!(socket = DEFAULT_SOCKET_PATH, "harboriad listening");

    loop {
        let (stream, _addr) = listener.accept().await?;
        let ctx = ctx.clone();
        tokio::spawn(async move {
            if let Err(err) = handle_connection(stream, ctx).await {
                tracing::warn!(error = %err, "connection ended with error");
            }
        });
    }
}

async fn handle_connection(stream: tokio::net::UnixStream, ctx: Arc<AppContext>) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<RpcRequest>(&line) {
            Ok(req) => handle_request(req, &ctx).await,
            Err(err) => RpcResponse {
                jsonrpc: "2.0".to_string(),
                id: 0,
                result: None,
                error: Some(RpcError {
                    code: -32700,
                    message: format!("parse error: {err}"),
                }),
            },
        };
        let mut out = serde_json::to_string(&response)?;
        out.push('\n');
        writer.write_all(out.as_bytes()).await?;
    }
    Ok(())
}


async fn handle_request(req: RpcRequest, ctx: &Arc<AppContext>) -> RpcResponse {
    let id = req.id;
    let runtime = &ctx.runtime;
    let result = match req.method.as_str() {
        methods::GOAL_CREATE => create_goal(req, runtime).await,
        methods::GOAL_LIST => list_goals(runtime).await,
        methods::GOAL_GET => get_goal(req, runtime).await,
        methods::STATE_GET => get_state(runtime).await,
        methods::STATE_ADD_ROUTINE => add_routine(req, runtime).await,
        methods::STATE_ADD_TEMPORARY => add_temporary(req, runtime).await,
        methods::PROGRESS_ADD => progress_add(req, runtime, &ctx.progress_record_store).await,
        methods::TASK_LOG => task_log(req, runtime).await,
        methods::INTERVENTION_RESPOND => intervention_respond(req, runtime).await,
        methods::TICK => tick(runtime).await,
        methods::STATE_SET_AVAILABILITY => set_availability(req, runtime).await,
        methods::CHAT => chat(req, ctx).await,
        other => Err(anyhow::anyhow!("unknown method: {other}")),
    };

    match result {
        Ok(value) => RpcResponse::ok(id, value),
        Err(err) => RpcResponse::err(id, -32000, err.to_string()),
    }
}

async fn create_goal(req: RpcRequest, runtime: &Arc<AssistantRuntime>) -> anyhow::Result<serde_json::Value> {
    let params: CreateGoalParams = serde_json::from_value(req.params)?;

    let goal_type = match params.goal_type.as_deref() {
        Some("habit") => GoalType::Habit,
        Some("project") => GoalType::Project,
        Some("maintenance") => GoalType::Maintenance,
        _ => GoalType::Outcome,
    };
    let assistance_mode = match params.assistance_mode.as_deref() {
        Some("passive") => AssistanceMode::Passive,
        Some("proactive") => AssistanceMode::Proactive,
        _ => AssistanceMode::Balanced,
    };

    let goal = Goal {
        id: GoalId::new(),
        title: params.title,
        description: params.description,
        why: None,
        desired_outcome: params.desired_outcome,
        goal_type,
        assistance_mode,
        created_at: chrono::Utc::now(),
        deadline: params.deadline,
        priority: Priority::Medium,
        status: GoalStatus::Active,
        constraints: Vec::new(),
        success_criteria: Vec::new(),
    };
    runtime.goal_store.create(goal.clone()).await?;
    Ok(json!({ "id": goal.id.to_string() }))
}

async fn list_goals(runtime: &Arc<AssistantRuntime>) -> anyhow::Result<serde_json::Value> {
    let goals = runtime.goal_store.list_active().await?;
    Ok(serde_json::to_value(goals)?)
}

async fn get_goal(req: RpcRequest, runtime: &Arc<AssistantRuntime>) -> anyhow::Result<serde_json::Value> {
    let params: GoalIdParams = serde_json::from_value(req.params)?;
    let id = GoalId(uuid::Uuid::parse_str(&params.id)?);
    let goal = runtime.goal_store.get(id).await?;
    Ok(serde_json::to_value(goal)?)
}

async fn get_state(runtime: &Arc<AssistantRuntime>) -> anyhow::Result<serde_json::Value> {
    let state = runtime.user_state_store.get().await?;
    Ok(serde_json::to_value(state)?)
}

async fn add_routine(req: RpcRequest, runtime: &Arc<AssistantRuntime>) -> anyhow::Result<serde_json::Value> {
    let params: AddRoutineParams = serde_json::from_value(req.params)?;
    let mut state = runtime.user_state_store.get().await?;
    state.stable.routines.push(Routine {
        description: params.description,
        usual_hour: Some(params.usual_hour),
    });
    runtime.user_state_store.put(state).await?;
    Ok(json!({ "ok": true }))
}

async fn add_temporary(req: RpcRequest, runtime: &Arc<AssistantRuntime>) -> anyhow::Result<serde_json::Value> {
    let params: AddTemporaryParams = serde_json::from_value(req.params)?;
    let kind = match params.kind.as_str() {
        "tired" => TemporaryStateKind::Tired,
        "busy" => TemporaryStateKind::Busy,
        "lowmood" | "low_mood" => TemporaryStateKind::LowMood,
        "inmeeting" | "in_meeting" => TemporaryStateKind::InMeeting,
        "highstress" | "high_stress" => TemporaryStateKind::HighStress,
        other => return Err(anyhow::anyhow!("unknown temporary state kind: {other}")),
    };
    let now = chrono::Utc::now();
    let mut state = runtime.user_state_store.get().await?;
    state.temporary.push(TemporaryStateItem {
        kind,
        confidence: params.confidence,
        source: EvidenceSource::ExplicitStatement,
        observed_at: now,
        expires_at: params
            .expires_in_minutes
            .map(|m| now + chrono::Duration::minutes(m)),
    });
    runtime.user_state_store.put(state).await?;
    Ok(json!({ "ok": true }))
}

async fn progress_add(
    req: RpcRequest,
    runtime: &Arc<AssistantRuntime>,
    progress_record_store: &Arc<dyn ProgressRecordStore>,
) -> anyhow::Result<serde_json::Value> {
    let params: ProgressAddParams = serde_json::from_value(req.params)?;
    let goal_id = GoalId(uuid::Uuid::parse_str(&params.goal_id)?);
    let metric = match params.metric.as_str() {
        "count" => ProgressMetric::Count,
        "duration" => ProgressMetric::Duration,
        "distance" => ProgressMetric::Distance,
        "milestone" => ProgressMetric::Milestone,
        other => ProgressMetric::Custom(other.to_string()),
    };
    let record = ProgressRecord {
        goal_id,
        observed_at: chrono::Utc::now(),
        metric,
        value: MetricValue {
            amount: params.amount,
            unit: params.unit,
        },
        source: EvidenceRef {
            event_id: harboria_core::domain::EventId::new(),
            note: Some("recorded via progress.add RPC".to_string()),
        },
    };
    progress_record_store.append(record).await?;
    runtime
        .event_bus
        .publish(
            AssistantEvent::ProgressRecordAdded { goal_id },
            harboria_core::domain::EventId::new(),
            None,
        )
        .await?;
    Ok(json!({ "ok": true }))
}

async fn task_log(req: RpcRequest, runtime: &Arc<AssistantRuntime>) -> anyhow::Result<serde_json::Value> {
    let params: TaskLogParams = serde_json::from_value(req.params)?;
    let goal_id = GoalId(uuid::Uuid::parse_str(&params.goal_id)?);
    let outcome = match params.outcome.as_str() {
        "completed" => TaskOutcome::Completed,
        "postponed" => TaskOutcome::Postponed,
        other => return Err(anyhow::anyhow!("unknown task outcome: {other}")),
    };
    runtime
        .task_log_store
        .append(TaskLogEntry {
            id: TaskLogId::new(),
            goal_id,
            description: params.description,
            outcome,
            occurred_at: chrono::Utc::now(),
        })
        .await?;
    Ok(json!({ "ok": true }))
}

async fn intervention_respond(
    req: RpcRequest,
    runtime: &Arc<AssistantRuntime>,
) -> anyhow::Result<serde_json::Value> {
    let params: InterventionRespondParams = serde_json::from_value(req.params)?;
    let goal_id = GoalId(uuid::Uuid::parse_str(&params.goal_id)?);
    let pending = runtime
        .intervention_store
        .latest_pending_for_goal(goal_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no pending intervention found for this goal"))?;

    let outcome = match params.outcome.as_str() {
        "accepted" => InterventionOutcome::Accepted,
        "rejected" => InterventionOutcome::Rejected,
        "ignored" => InterventionOutcome::Ignored,
        "feedback" => InterventionOutcome::ExplicitFeedback(
            params
                .feedback
                .ok_or_else(|| anyhow::anyhow!("outcome=feedback requires a feedback string"))?,
        ),
        other => return Err(anyhow::anyhow!("unknown intervention outcome: {other}")),
    };

    let record_id: InterventionRecordId = pending.id;
    runtime
        .intervention_store
        .record_outcome(record_id, outcome.clone())
        .await?;
    runtime
        .event_bus
        .publish(
            AssistantEvent::InterventionOutcomeRecorded {
                goal_id: Some(goal_id),
                outcome,
            },
            harboria_core::domain::EventId::new(),
            None,
        )
        .await?;
    Ok(json!({ "ok": true }))
}

async fn set_availability(
    req: RpcRequest,
    runtime: &Arc<AssistantRuntime>,
) -> anyhow::Result<serde_json::Value> {
    let params: SetAvailabilityParams = serde_json::from_value(req.params)?;
    let availability = if params.available {
        Availability::Available
    } else {
        Availability::Busy
    };
    let duration = params
        .duration_minutes
        .map(|m| Duration::from_secs((m.max(0) as u64) * 60));
    runtime.set_availability(availability, duration).await?;
    Ok(json!({ "ok": true }))
}


async fn chat(req: RpcRequest, ctx: &Arc<AppContext>) -> anyhow::Result<serde_json::Value> {
    let params: ChatParams = serde_json::from_value(req.params)?;
    let runtime = &ctx.runtime;

    let user_state = runtime.user_state_store.get().await?;
    let recent_memory = ctx.memory_store.query("conversation", 10).await.unwrap_or_default();

    ctx.memory_store
        .append(format!("User: {}", params.text), vec!["conversation".to_string()])
        .await?;

    let reply = match ctx
        .conversation
        .respond(&params.text, &user_state, &recent_memory)
        .await
    {
        Ok(reply) => reply,
        Err(err) => {
            tracing::warn!(error = %err, "conversation provider failed");
            "Sorry, I can't give you a proper response right now. Please try again later.".to_string()
        }
    };

    ctx.memory_store
        .append(
            format!("Assistant: {reply}"),
            vec!["conversation".to_string()],
        )
        .await?;


    let _ = runtime
        .embodiment
        .perform(harboria_embodiment::EmbodiedAction::Speak {
            text: reply.clone(),
            tone: Some(harboria_embodiment::Tone::Warm),
        })
        .await;

    runtime
        .event_bus
        .publish(
            AssistantEvent::UserInteraction { text: params.text },
            harboria_core::domain::EventId::new(),
            None,
        )
        .await?;

    Ok(json!({ "reply": reply }))
}

async fn tick(runtime: &Arc<AssistantRuntime>) -> anyhow::Result<serde_json::Value> {
    let decision = runtime
        .run_assistant_loop(TriggerReason::ManualTick)
        .await?;
    Ok(serde_json::to_value(decision)?)
}

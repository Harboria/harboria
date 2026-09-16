// //! `microduck-adapter`
// //!
// //! 真实的 JSON-RPC 2.0 / NDJSON 客户端，连接 MicroDuck 的 `robotd`。
// //!
// //! # 方法名的确认状态
// //!
// //! **已在 simulator 上验证**（本轮实验）：
// //! - `robot.health` / `robot.state` / `robot.subscribe` / `robot.mode` /
// //!   `robot.policies` / `robot.skills` / `robot.do {skill}` /
// //!   `robot.move [forward, lateral, turn]` / `robot.stop`
// //!
// //! **名字来自官方方法清单，但参数结构未在真机验证**：
// //! - `robot.sound` / `robot.look` / `robot.enable` / `robot.relax` /
// //!   `robot.init` / `robot.mouth` / `robot.setMode`
// //!
// //! 接真机前，请用探测程序逐个核对参数结构——只需改本文件里的字符串和
// //! JSON 参数，不需要动连接/调用逻辑。

// use std::path::PathBuf;
// use std::sync::atomic::{AtomicU64, Ordering};

// use async_trait::async_trait;
// use chrono::Utc;
// use serde_json::{json, Value};
// use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
// use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
// use tokio::net::UnixStream;
// use tokio::sync::Mutex;

// use harboria_embodiment::{
//     ActionResult, AttentionTarget, Capabilities, DeviceObservation, DeviceState, EmbodiedAction,
//     Embodiment, EmbodimentError, EmbodimentState, Expression, Observation, ObservationSource,
//     Result as EmbodimentResult, Tone,
// };

// #[derive(Debug, Clone)]
// pub struct RobotdMethods {
//     pub health: String,
//     pub state: String,
//     pub subscribe: String,
//     pub r#move: String,
//     pub head: String,
//     pub look: String,
//     pub sound: String,
//     pub stop: String,
//     pub mode: String,
//     pub set_mode: String,
//     pub relax: String,
//     pub enable: String,
//     pub init: String,
//     pub do_: String,
//     pub mouth: String,
// }

// impl Default for RobotdMethods {
//     fn default() -> Self {
//         Self {
//             // ---- 已在 simulator 上验证 ----
//             health: "robot.health".to_string(),
//             state: "robot.state".to_string(),
//             subscribe: "robot.subscribe".to_string(),
//             r#move: "robot.move".to_string(),
//             stop: "robot.stop".to_string(),
//             mode: "robot.mode".to_string(),       // 只读查询
//             do_: "robot.do".to_string(),
//             // ---- 名字来自官方清单，参数结构待真机验证 ----
//             head: "robot.head".to_string(),
//             look: "robot.look".to_string(),
//             sound: "robot.sound".to_string(),
//             set_mode: "robot.setMode".to_string(),
//             relax: "robot.relax".to_string(),
//             enable: "robot.enable".to_string(),
//             init: "robot.init".to_string(),
//             mouth: "robot.mouth".to_string(),
//         }
//     }
// }

// #[derive(Debug, Clone)]
// pub struct RobotdClientConfig {
//     pub socket_path: PathBuf,
//     pub methods: RobotdMethods,
// }

// impl RobotdClientConfig {
//     pub fn hardware_default() -> Self {
//         Self {
//             socket_path: PathBuf::from("/run/robotd.sock"),
//             methods: RobotdMethods::default(),
//         }
//     }

//     pub fn with_socket_path(mut self, path: impl Into<PathBuf>) -> Self {
//         self.socket_path = path.into();
//         self
//     }
// }

// struct Connection {
//     reader: Lines<BufReader<OwnedReadHalf>>,
//     writer: OwnedWriteHalf,
// }

// impl Connection {
//     async fn connect(path: &std::path::Path) -> std::io::Result<Self> {
//         let stream = UnixStream::connect(path).await?;
//         let (r, w) = stream.into_split();
//         Ok(Self {
//             reader: BufReader::new(r).lines(),
//             writer: w,
//         })
//     }

//     async fn call(&mut self, id: u64, method: &str, params: Value) -> anyhow::Result<Value> {
//         let request = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
//         let mut line = serde_json::to_string(&request)?;
//         line.push('\n');
//         self.writer.write_all(line.as_bytes()).await?;

//         loop {
//             let line = self
//                 .reader
//                 .next_line()
//                 .await?
//                 .ok_or_else(|| anyhow::anyhow!("robotd closed the connection"))?;
//             if line.trim().is_empty() {
//                 continue;
//             }
//             let value: Value = serde_json::from_str(&line)?;
//             if value.get("id").and_then(|v| v.as_u64()) == Some(id) {
//                 if let Some(err) = value.get("error") {
//                     anyhow::bail!("robotd returned an error: {err}");
//                 }
//                 return Ok(value.get("result").cloned().unwrap_or(Value::Null));
//             }
//             // 不是我们在等的响应 —— 丢弃继续读。
//         }
//     }
// }

// pub struct MicroDuckAdapter {
//     config: RobotdClientConfig,
//     conn: Mutex<Option<Connection>>,
//     next_id: AtomicU64,
// }

// impl MicroDuckAdapter {
//     pub async fn connect_simulator(socket_path: &str) -> EmbodimentResult<Self> {
//         Self::connect_with_config(RobotdClientConfig {
//             socket_path: PathBuf::from(socket_path),
//             methods: RobotdMethods::default(),
//         })
//         .await
//     }

//     pub async fn connect_hardware() -> EmbodimentResult<Self> {
//         Self::connect_with_config(RobotdClientConfig::hardware_default()).await
//     }

//     pub async fn connect_with_config(config: RobotdClientConfig) -> EmbodimentResult<Self> {
//         Ok(Self {
//             config,
//             conn: Mutex::new(None),
//             next_id: AtomicU64::new(1),
//         })
//     }

//     async fn call(&self, method: &str, params: Value) -> EmbodimentResult<Value> {
//         let mut guard = self.conn.lock().await;
//         if guard.is_none() {
//             match Connection::connect(&self.config.socket_path).await {
//                 Ok(c) => *guard = Some(c),
//                 Err(e) => {
//                     return Err(EmbodimentError::Unavailable(format!(
//                         "cannot reach robotd at {}: {e}",
//                         self.config.socket_path.display()
//                     )))
//                 }
//             }
//         }
//         let id = self.next_id.fetch_add(1, Ordering::Relaxed);
//         let result = guard.as_mut().unwrap().call(id, method, params).await;
//         match result {
//             Ok(v) => Ok(v),
//             Err(e) => {
//                 *guard = None;
//                 Err(EmbodimentError::Transport(e.to_string()))
//             }
//         }
//     }
// }

// #[async_trait]
// impl Embodiment for MicroDuckAdapter {
//     async fn capabilities(&self) -> Capabilities {
//         Capabilities {
//             // 严格来说 MicroDuck 不能"说任意文本"，只能播预设音效。
//             // 保持 true 让上层不跳过 Speak 决策，但 perform 里的 detail
//             // 会明确说明"文本未被播报"。
//             can_speak: true,
//             can_listen: false,
//             can_express: true,
//             can_attend: true,
//             has_event_source: false, // 订阅还没接
//         }
//     }

//     async fn state(&self) -> EmbodimentState {
//         match self.call(&self.config.methods.health, json!({})).await {
//             Ok(value) => {
//                 let healthy = value
//                     .get("healthy")
//                     .and_then(|v| v.as_bool())
//                     .unwrap_or(true);
//                 EmbodimentState {
//                     device: if healthy {
//                         DeviceState::Active
//                     } else {
//                         DeviceState::Error("robot.health reported unhealthy".to_string())
//                     },
//                     as_of: Utc::now(),
//                 }
//             }
//             Err(e) => EmbodimentState {
//                 device: DeviceState::Offline,
//                 as_of: Utc::now(),
//             }
//             .tap_log_unavailable(&e),
//         }
//     }

//     async fn observe(&self) -> EmbodimentResult<Observation> {
//         // 注意：`robot.state` 是 server→client 通知名，不能直接作为请求发。
//         // 这里改用 `robot.health` 做连接性探测——它确认可以作为请求，
//         // 响应也有 `healthy: bool` 字段。语义上只是"能不能连上"，
//         // 与之前用 robot.state 探测的目的相同。
//         let probe_result = self.call(&self.config.methods.health, json!({})).await;

//         let (connected, device_state) = match &probe_result {
//             Ok(_) => (true, DeviceState::Active),
//             Err(EmbodimentError::Unavailable(_)) => (false, DeviceState::Offline),
//             Err(e) => (false, DeviceState::Error(e.to_string())),
//         };

//         Ok(Observation {
//             timestamp: Utc::now(),
//             source: ObservationSource::Embodiment,
//             // presence 的真正数据源是 mediad 的感知特征流，不是 robotd。
//             // 第一版还没接 mediad，诚实地留 None。
//             presence: None,
//             interaction: None,
//             environment: None,
//             device: DeviceObservation {
//                 connected,
//                 state: device_state,
//             },
//         })
//     }

//     async fn perform(&self, action: EmbodiedAction) -> EmbodimentResult<ActionResult> {
//         match action {
//             EmbodiedAction::Speak { text: _, tone } => {
//                 // 文本被丢弃 —— MicroDuck 没有 TTS。
//                 // 映射到一个预设音效名。可用音效：chirp / greet / coo / wheee。
//                 let sound_name = match tone {
//                     Some(Tone::Warm) | Some(Tone::Playful) => "chirp",
//                     Some(Tone::Calm) => "coo",
//                     _ => "chirp",
//                 };
//                 let result = self
//                     .call(&self.config.methods.sound, json!({ "sound": sound_name }))
//                     .await?;
//                 Ok(ActionResult {
//                     accepted: true,
//                     detail: Some(format!(
//                         "played sound '{sound_name}'; text was NOT spoken \
//                          (MicroDuck has no TTS) — raw result: {result}"
//                     )),
//                 })
//             }
//             EmbodiedAction::Attention { target } => {
//                 // `robot.look` 接受躯干坐标系里的一个三维点，daemon 自己跑 gaze IK。
//                 // 这里用两个粗略点近似"看向用户"和"看向别处"。
//                 // 单位米，坐标轴方向需在真机验证（假设 x 向前、y 向左、z 向上）。
//                 let point = match target {
//                     AttentionTarget::User => json!([0.5, 0.0, 0.12]),
//                     AttentionTarget::Away => json!([-0.5, 0.0, 0.12]),
//                 };
//                 let result = self.call(&self.config.methods.look, point).await?;
//                 Ok(ActionResult {
//                     accepted: true,
//                     detail: Some(result.to_string()),
//                 })
//             }
//             EmbodiedAction::Express { expression } => {
//                 let sound_name = match expression {
//                     Expression::Happy | Expression::Curious => "chirp",
//                     Expression::Concerned => "coo",
//                     Expression::Neutral => "chirp",
//                 };
//                 let result = self
//                     .call(&self.config.methods.sound, json!({ "sound": sound_name }))
//                     .await?;
//                 Ok(ActionResult {
//                     accepted: true,
//                     detail: Some(result.to_string()),
//                 })
//             }
//             EmbodiedAction::Idle => {
//                 // `robot.mode` 是只读查询，不能用来"设置"模式。
//                 // Idle 的语义更接近"停止当前移动" —— 用 robot.stop。
//                 let result = self.call(&self.config.methods.stop, json!({})).await?;
//                 Ok(ActionResult {
//                     accepted: true,
//                     detail: Some(result.to_string()),
//                 })
//             }
//             EmbodiedAction::Wake => {
//                 // `robot.enable` 带 enabled: true 打开策略执行。
//                 // 参数结构未在真机验证；若是空对象被拒，去掉这个字段即可。
//                 let result = self
//                     .call(&self.config.methods.enable, json!({ "enabled": true }))
//                     .await?;
//                 Ok(ActionResult {
//                     accepted: true,
//                     detail: Some(result.to_string()),
//                 })
//             }
//             EmbodiedAction::Sleep => {
//                 // `robot.relax` 的语义就是"断电、瘫倒"，与 Sleep 对应。
//                 let result = self.call(&self.config.methods.relax, json!({})).await?;
//                 Ok(ActionResult {
//                     accepted: true,
//                     detail: Some(result.to_string()),
//                 })
//             }
//         }
//     }
// }

// trait TapLogUnavailable {
//     fn tap_log_unavailable(self, err: &EmbodimentError) -> Self;
// }
// impl TapLogUnavailable for EmbodimentState {
//     fn tap_log_unavailable(self, err: &EmbodimentError) -> Self {
//         tracing::debug!(error = %err, "microduck-adapter: robotd unreachable");
//         self
//     }
// }


//! `microduck-adapter`
//!
//! 真实的 JSON-RPC 2.0 / NDJSON 客户端，连接 MicroDuck 的 `robotd`。
//!
//! # 方法名的确认状态
//!
//! **已在 simulator 上验证**（本轮实验）：
//! - `robot.health` / `robot.state` / `robot.subscribe` / `robot.mode` /
//!   `robot.policies` / `robot.skills` / `robot.do {skill}` /
//!   `robot.move [forward, lateral, turn]` / `robot.stop`
//!
//! **名字来自官方方法清单，但参数结构未在真机验证**：
//! - `robot.sound` / `robot.look` / `robot.enable` / `robot.relax` /
//!   `robot.init` / `robot.mouth` / `robot.setMode`
//!
//! 接真机前，请用探测程序逐个核对参数结构——只需改本文件里的字符串和
//! JSON 参数，不需要动连接/调用逻辑。

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use chrono::Utc;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::UnixStream;
use tokio::sync::Mutex;

use harboria_embodiment::{
    ActionResult, AttentionTarget, Capabilities, DeviceObservation, DeviceState, EmbodiedAction,
    Embodiment, EmbodimentError, EmbodimentState, Expression, Observation, ObservationSource,
    Result as EmbodimentResult, Tone,
};

#[derive(Debug, Clone)]
pub struct RobotdMethods {
    pub health: String,
    pub state: String,
    pub subscribe: String,
    pub r#move: String,
    pub head: String,
    pub look: String,
    pub sound: String,
    pub stop: String,
    pub mode: String,
    pub set_mode: String,
    pub relax: String,
    pub enable: String,
    pub init: String,
    pub do_: String,
    pub mouth: String,
}

impl Default for RobotdMethods {
    fn default() -> Self {
        Self {
            // ---- 已在 simulator 上验证 ----
            health: "robot.health".to_string(),
            state: "robot.state".to_string(),
            subscribe: "robot.subscribe".to_string(),
            r#move: "robot.move".to_string(),
            stop: "robot.stop".to_string(),
            mode: "robot.mode".to_string(),       // 只读查询
            do_: "robot.do".to_string(),
            // ---- 名字来自官方清单，参数结构待真机验证 ----
            head: "robot.head".to_string(),
            look: "robot.look".to_string(),
            sound: "robot.sound".to_string(),
            set_mode: "robot.setMode".to_string(),
            relax: "robot.relax".to_string(),
            enable: "robot.enable".to_string(),
            init: "robot.init".to_string(),
            mouth: "robot.mouth".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RobotdClientConfig {
    pub socket_path: PathBuf,
    pub methods: RobotdMethods,
}

impl RobotdClientConfig {
    pub fn hardware_default() -> Self {
        Self {
            socket_path: PathBuf::from("/run/robotd.sock"),
            methods: RobotdMethods::default(),
        }
    }

    pub fn with_socket_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.socket_path = path.into();
        self
    }
}

struct Connection {
    reader: Lines<BufReader<OwnedReadHalf>>,
    writer: OwnedWriteHalf,
}

impl Connection {
    async fn connect(path: &std::path::Path) -> std::io::Result<Self> {
        let stream = UnixStream::connect(path).await?;
        let (r, w) = stream.into_split();
        Ok(Self {
            reader: BufReader::new(r).lines(),
            writer: w,
        })
    }

    async fn call(&mut self, id: u64, method: &str, params: Value) -> anyhow::Result<Value> {
        let request = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        let mut line = serde_json::to_string(&request)?;
        line.push('\n');
        self.writer.write_all(line.as_bytes()).await?;

        loop {
            let line = self
                .reader
                .next_line()
                .await?
                .ok_or_else(|| anyhow::anyhow!("robotd closed the connection"))?;
            if line.trim().is_empty() {
                continue;
            }
            let value: Value = serde_json::from_str(&line)?;
            if value.get("id").and_then(|v| v.as_u64()) == Some(id) {
                if let Some(err) = value.get("error") {
                    anyhow::bail!("robotd returned an error: {err}");
                }
                return Ok(value.get("result").cloned().unwrap_or(Value::Null));
            }
            // 不是我们在等的响应 —— 丢弃继续读。
        }
    }
}

pub struct MicroDuckAdapter {
    config: RobotdClientConfig,
    conn: Mutex<Option<Connection>>,
    next_id: AtomicU64,
}

impl MicroDuckAdapter {
    pub async fn connect_simulator(socket_path: &str) -> EmbodimentResult<Self> {
        Self::connect_with_config(RobotdClientConfig {
            socket_path: PathBuf::from(socket_path),
            methods: RobotdMethods::default(),
        })
        .await
    }

    pub async fn connect_hardware() -> EmbodimentResult<Self> {
        Self::connect_with_config(RobotdClientConfig::hardware_default()).await
    }

    pub async fn connect_with_config(config: RobotdClientConfig) -> EmbodimentResult<Self> {
        Ok(Self {
            config,
            conn: Mutex::new(None),
            next_id: AtomicU64::new(1),
        })
    }

    async fn call(&self, method: &str, params: Value) -> EmbodimentResult<Value> {
        let mut guard = self.conn.lock().await;
        if guard.is_none() {
            match Connection::connect(&self.config.socket_path).await {
                Ok(c) => *guard = Some(c),
                Err(e) => {
                    return Err(EmbodimentError::Unavailable(format!(
                        "cannot reach robotd at {}: {e}",
                        self.config.socket_path.display()
                    )))
                }
            }
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let result = guard.as_mut().unwrap().call(id, method, params).await;
        match result {
            Ok(v) => Ok(v),
            Err(e) => {
                *guard = None;
                Err(EmbodimentError::Transport(e.to_string()))
            }
        }
    }
}

#[async_trait]
impl Embodiment for MicroDuckAdapter {
    async fn capabilities(&self) -> Capabilities {
        Capabilities {
            // 严格来说 MicroDuck 不能"说任意文本"，只能播预设音效。
            // 保持 true 让上层不跳过 Speak 决策，但 perform 里的 detail
            // 会明确说明"文本未被播报"。
            can_speak: true,
            can_listen: false,
            can_express: true,
            can_attend: true,
            has_event_source: false, // 订阅还没接
        }
    }

    async fn state(&self) -> EmbodimentState {
        match self.call(&self.config.methods.health, json!({})).await {
            Ok(value) => {
                let healthy = value
                    .get("healthy")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                EmbodimentState {
                    device: if healthy {
                        DeviceState::Active
                    } else {
                        DeviceState::Error("robot.health reported unhealthy".to_string())
                    },
                    as_of: Utc::now(),
                }
            }
            Err(e) => EmbodimentState {
                device: DeviceState::Offline,
                as_of: Utc::now(),
            }
            .tap_log_unavailable(&e),
        }
    }

    async fn observe(&self) -> EmbodimentResult<Observation> {
        // 注意：`robot.state` 是 server→client 通知名，不能直接作为请求发。
        // 这里改用 `robot.health` 做连接性探测——它确认可以作为请求，
        // 响应也有 `healthy: bool` 字段。语义上只是"能不能连上"，
        // 与之前用 robot.state 探测的目的相同。
        let probe_result = self.call(&self.config.methods.health, json!({})).await;

        let (connected, device_state) = match &probe_result {
            Ok(_) => (true, DeviceState::Active),
            Err(EmbodimentError::Unavailable(_)) => (false, DeviceState::Offline),
            Err(e) => (false, DeviceState::Error(e.to_string())),
        };

        Ok(Observation {
            timestamp: Utc::now(),
            source: ObservationSource::Embodiment,
            // presence 的真正数据源是 mediad 的感知特征流，不是 robotd。
            // 第一版还没接 mediad，诚实地留 None。
            presence: None,
            interaction: None,
            environment: None,
            device: DeviceObservation {
                connected,
                state: device_state,
            },
        })
    }

    async fn perform(&self, action: EmbodiedAction) -> EmbodimentResult<ActionResult> {
        match action {
            EmbodiedAction::Speak { text: _, tone } => {
                // 文本被丢弃 —— MicroDuck 没有 TTS。
                // 映射到一个预设音效名。可用音效：chirp / greet / coo / wheee。
                let sound_name = match tone {
                    Some(Tone::Warm) | Some(Tone::Playful) => "chirp",
                    Some(Tone::Calm) => "coo",
                    _ => "chirp",
                };
                let result = self
                    .call(&self.config.methods.sound, json!({ "sound": sound_name }))
                    .await?;
                Ok(ActionResult {
                    accepted: true,
                    detail: Some(format!(
                        "played sound '{sound_name}'; text was NOT spoken \
                         (MicroDuck has no TTS) — raw result: {result}"
                    )),
                })
            }
            EmbodiedAction::Attention { target } => {
                // `robot.look` 接受躯干坐标系里的一个三维点，daemon 自己跑 gaze IK。
                // 这里用两个粗略点近似"看向用户"和"看向别处"。
                // 单位米，坐标轴方向需在真机验证（假设 x 向前、y 向左、z 向上）。
                let point = match target {
                    AttentionTarget::User => json!([0.5, 0.0, 0.12]),
                    AttentionTarget::Away => json!([-0.5, 0.0, 0.12]),
                };
                let result = self.call(&self.config.methods.look, point).await?;
                Ok(ActionResult {
                    accepted: true,
                    detail: Some(result.to_string()),
                })
            }
            EmbodiedAction::Express { expression } => {
                let sound_name = match expression {
                    Expression::Happy | Expression::Curious => "chirp",
                    Expression::Concerned => "coo",
                    Expression::Neutral => "chirp",
                };
                let result = self
                    .call(&self.config.methods.sound, json!({ "sound": sound_name }))
                    .await?;
                Ok(ActionResult {
                    accepted: true,
                    detail: Some(result.to_string()),
                })
            }
            EmbodiedAction::Idle => {
                // `robot.mode` 是只读查询，不能用来"设置"模式。
                // Idle 的语义更接近"停止当前移动" —— 用 robot.stop。
                let result = self.call(&self.config.methods.stop, json!({})).await?;
                Ok(ActionResult {
                    accepted: true,
                    detail: Some(result.to_string()),
                })
            }
            EmbodiedAction::Wake => {
                // `robot.enable` 带 enabled: true 打开策略执行。
                // 参数结构未在真机验证；若是空对象被拒，去掉这个字段即可。
                let result = self
                    .call(&self.config.methods.enable, json!({ "enabled": true }))
                    .await?;
                Ok(ActionResult {
                    accepted: true,
                    detail: Some(result.to_string()),
                })
            }
            EmbodiedAction::Sleep => {
                // `robot.relax` 的语义就是"断电、瘫倒"，与 Sleep 对应。
                let result = self.call(&self.config.methods.relax, json!({})).await?;
                Ok(ActionResult {
                    accepted: true,
                    detail: Some(result.to_string()),
                })
            }
        }
    }
}

trait TapLogUnavailable {
    fn tap_log_unavailable(self, err: &EmbodimentError) -> Self;
}
impl TapLogUnavailable for EmbodimentState {
    fn tap_log_unavailable(self, err: &EmbodimentError) -> Self {
        tracing::debug!(error = %err, "microduck-adapter: robotd unreachable");
        self
    }
}

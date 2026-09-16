//! Among the three loops — Goal Loop / Assistant Loop / Interaction Loop — `AssistantRuntime`
//! implements only the main pipeline of the Assistant Loop. Goal Loop (Progress re-evaluation)
//! and Interaction Loop (Companion dialogue) are independent EventBus subscribers and are not
//! defined here. They communicate by emitting events and do not call AssistantRuntime's methods
//! directly.

pub mod engine;
pub mod event_bus;

pub use engine::AssistantRuntime;
pub use event_bus::EventBus;

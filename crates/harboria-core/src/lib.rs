//! `harboria-core`
//!
//! Domain models for Goal / UserState / Memory / Opportunity / Intervention / Decision,
//! along with the only traits Core knows about from the outside (`ports`), the default
//! implementations of pure rules (`policies`), the rule-based evaluation services
//! (`services`), and the main pipeline that wires everything together (`runtime`).
//!
//! Module boundaries:
//! - `domain`   —— data, no behavior
//! - `ports`    —— all traits that Core knows through dependency inversion
//! - `policies` —— hard rules (InterventionPolicy is always rule-based) + the MVP default
//!                 RuleBasedArbitration (pluggable, replaceable by an LLM version)
//! - `services` —— Deterministic Evaluation / Progress evaluation, mostly pure functions
//! - `runtime`  —— EventBus + AssistantRuntime, running all of the above through the
//!                 pipeline finalized in the five rounds of discussion

pub mod domain;
pub mod policies;
pub mod ports;
pub mod runtime;
pub mod services;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! define_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub struct $name(pub Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

define_id!(GoalId);
define_id!(OpportunityId);
define_id!(PreferenceId);
define_id!(EventId);
define_id!(InterventionRecordId);
define_id!(TaskLogId);

impl GoalId {
    pub fn as_execution_ref(&self) -> harboria_execution::GoalRef {
        harboria_execution::GoalRef(self.0)
    }
}

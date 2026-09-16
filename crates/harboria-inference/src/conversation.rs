//! The two implementations of `ConversationProvider`, used by the Interaction Loop (chit-chat).
//! Deliberately kept separate from `ReasoningProvider`: chit-chat does not produce a `Decision`
//! and does not need to go through `DecisionPolicy` validation, because it does not modify any
//! Goal and does not execute any task—it just returns a single reply.

use async_trait::async_trait;

use harboria_core::domain::UserState;
use harboria_core::ports::ConversationProvider;

use crate::client::AnthropicClient;


pub struct EchoConversationProvider;

#[async_trait]
impl ConversationProvider for EchoConversationProvider {
    async fn respond(
        &self,
        user_text: &str,
        _user_state: &UserState,
        _recent_memory: &[String],
    ) -> anyhow::Result<String> {
        Ok(format!(
            "I received: \"{user_text}\". No real conversation model is configured yet, so this is a placeholder reply."
        ))
    }
}

const CONVERSATION_SYSTEM_PROMPT: &str = "You are a warm, concise personal companion embedded in \
a long-running personal assistant runtime. Keep replies short (one to three sentences), \
conversational, and in the same language the user writes in. You are not the goal-management \
system itself and you cannot create, modify, or check off any goal in this conversation -- \
just have a natural, friendly exchange. If the user brings up wanting to work on a goal, \
acknowledge it warmly but note that goal changes happen through the assistant's normal \
goal-tracking flow, not through this chat.";

pub struct AnthropicConversationProvider {
    client: AnthropicClient,
}

impl AnthropicConversationProvider {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            client: AnthropicClient::from_env()?,
        })
    }
}

#[async_trait]
impl ConversationProvider for AnthropicConversationProvider {
    async fn respond(
        &self,
        user_text: &str,
        user_state: &UserState,
        recent_memory: &[String],
    ) -> anyhow::Result<String> {
        let mut prompt = String::new();

        if !recent_memory.is_empty() {
            prompt.push_str("Recent conversation memory (most recent last):\n");
            for m in recent_memory {
                prompt.push_str("- ");
                prompt.push_str(m);
                prompt.push('\n');
            }
            prompt.push('\n');
        }

        if !user_state.stable.preferences.is_empty() {
            prompt.push_str("Known stable preferences about the user:\n");
            for p in &user_state.stable.preferences {
                prompt.push_str("- ");
                prompt.push_str(&p.statement);
                prompt.push('\n');
            }
            prompt.push('\n');
        }

        prompt.push_str("User says: ");
        prompt.push_str(user_text);

        self.client
            .complete(CONVERSATION_SYSTEM_PROMPT, &prompt, 300)
            .await
    }
}

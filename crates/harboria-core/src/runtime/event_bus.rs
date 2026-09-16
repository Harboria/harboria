use std::sync::Arc;
use tokio::sync::broadcast;

use crate::domain::{AssistantEvent, EventId, StoredEvent};
use crate::ports::EventStore;

#[derive(Clone)]
pub struct EventBus {
    storage: Arc<dyn EventStore>,
    tx: broadcast::Sender<StoredEvent>,
}

impl EventBus {
    pub fn new(storage: Arc<dyn EventStore>) -> Self {
        let (tx, _rx) = broadcast::channel(1024);
        Self { storage, tx }
    }

    pub async fn publish(
        &self,
        event: AssistantEvent,
        correlation_id: EventId,
        causation_id: Option<EventId>,
    ) -> anyhow::Result<EventId> {
        let stored = StoredEvent {
            id: EventId::new(),
            event,
            occurred_at: chrono::Utc::now(),
            correlation_id,
            causation_id,
        };
        let id = self.storage.append(stored.clone()).await?;
        let _ = self.tx.send(stored);
        Ok(id)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<StoredEvent> {
        self.tx.subscribe()
    }

    pub async fn replay(&self, after: Option<EventId>) -> anyhow::Result<Vec<StoredEvent>> {
        self.storage.load_after(after).await
    }
}

//! WebSocket event broadcaster for real-time GUI communication.

use nautilus_state_encoder::SextantEvent;
use tokio::sync::broadcast;

/// Wrapper for events broadcast over WebSocket.
#[derive(Clone, Debug, serde::Serialize)]
pub struct StreamEvent {
    pub timestamp_ns: u64,
    pub event: SextantEvent,
}

/// Shared event broadcaster. Callers clone the sender to broadcast events.
pub struct EventBroadcaster {
    tx: broadcast::Sender<StreamEvent>,
}

impl EventBroadcaster {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    pub fn sender(&self) -> broadcast::Sender<StreamEvent> {
        self.tx.clone()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<StreamEvent> {
        self.tx.subscribe()
    }

    /// Broadcast a SextantEvent to all connected clients.
    pub fn broadcast(&self, event: SextantEvent) {
        let stream_event = StreamEvent {
            timestamp_ns: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64,
            event,
        };
        // Ignore error if no receivers
        let _ = self.tx.send(stream_event);
    }
}

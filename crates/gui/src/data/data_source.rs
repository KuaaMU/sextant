//! Trait abstraction for data sources.

use nautilus_state_encoder::ContextWindow;

/// A data source that produces ContextWindow snapshots.
pub trait DataSource: Send {
    /// Try to read the latest snapshot. Returns None if no new data.
    fn try_read(&mut self) -> Option<ContextWindow>;

    /// Drain pending log entries since last call.
    fn drain_logs(&mut self) -> Vec<LogEntry> {
        Vec::new()
    }
}

/// A log entry from the data source (agent decision, event, etc.).
pub struct LogEntry {
    pub timestamp: String,
    pub agent_type: LogAgentType,
    pub latency_ms: u64,
    pub summary: String,
}

#[derive(Clone, Copy, Debug)]
pub enum LogAgentType {
    Perception,
    Strategy,
    Risk,
    Execution,
}

impl LogAgentType {
    pub fn label(self) -> &'static str {
        match self {
            Self::Perception => "PERCEPTION",
            Self::Strategy => "STRATEGY",
            Self::Risk => "RISK",
            Self::Execution => "EXECUTION",
        }
    }
}

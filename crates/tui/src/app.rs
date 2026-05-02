//! Application state and panel routing.

use nautilus_state_encoder::ContextWindow;
use ratatui::widgets::Borders;

use crate::data::simulator::Simulator;

/// Which panel is currently focused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivePanel {
    Market,
    AgentLog,
    Risk,
    Orders,
    Research,
    Memory,
}

impl ActivePanel {
    /// All panels in order.
    pub const ALL: [ActivePanel; 6] = [
        ActivePanel::Market,
        ActivePanel::AgentLog,
        ActivePanel::Risk,
        ActivePanel::Orders,
        ActivePanel::Research,
        ActivePanel::Memory,
    ];

    /// Tab to next panel.
    pub fn next(self) -> Self {
        match self {
            Self::Market => Self::AgentLog,
            Self::AgentLog => Self::Risk,
            Self::Risk => Self::Orders,
            Self::Orders => Self::Research,
            Self::Research => Self::Memory,
            Self::Memory => Self::Market,
        }
    }

    /// Tab to previous panel.
    pub fn prev(self) -> Self {
        match self {
            Self::Market => Self::Memory,
            Self::AgentLog => Self::Market,
            Self::Risk => Self::AgentLog,
            Self::Orders => Self::Risk,
            Self::Research => Self::Orders,
            Self::Memory => Self::Research,
        }
    }

    /// Panel label for the tab bar.
    pub fn label(self) -> &'static str {
        match self {
            Self::Market => "[1] Market",
            Self::AgentLog => "[2] AgentLog",
            Self::Risk => "[3] Risk",
            Self::Orders => "[4] Orders",
            Self::Research => "[5] Research",
            Self::Memory => "[6] Memory",
        }
    }

    /// Panel index (1-based).
    pub fn index(self) -> usize {
        match self {
            Self::Market => 1,
            Self::AgentLog => 2,
            Self::Risk => 3,
            Self::Orders => 4,
            Self::Research => 5,
            Self::Memory => 6,
        }
    }

    /// Create from index (1-based).
    pub fn from_index(i: usize) -> Option<Self> {
        match i {
            1 => Some(Self::Market),
            2 => Some(Self::AgentLog),
            3 => Some(Self::Risk),
            4 => Some(Self::Orders),
            5 => Some(Self::Research),
            6 => Some(Self::Memory),
            _ => None,
        }
    }
}

/// Price history for sparkline rendering (ring buffer).
pub struct PriceHistory {
    pub prices: Vec<f64>,
    pub max_len: usize,
}

impl PriceHistory {
    pub fn new(max_len: usize) -> Self {
        Self {
            prices: Vec::with_capacity(max_len),
            max_len,
        }
    }

    pub fn push(&mut self, price: f64) {
        if self.prices.len() >= self.max_len {
            self.prices.remove(0);
        }
        self.prices.push(price);
    }
}

/// Application state.
pub struct App {
    /// Currently active panel.
    pub active: ActivePanel,
    /// Is the app running?
    pub running: bool,
    /// Is data collection paused?
    pub paused: bool,
    /// Show help overlay?
    pub show_help: bool,
    /// Zen mode (fullscreen active panel)?
    pub zen_mode: bool,
    /// Frame counter for status display.
    pub frame_count: u64,
    /// Market data simulator.
    pub simulator: Simulator,
    /// Latest context window from simulator.
    pub context: Option<ContextWindow>,
    /// Price history for sparkline.
    pub price_history: PriceHistory,
    /// Simulated agent log entries.
    pub log_entries: Vec<LogEntry>,
}

/// A simulated agent log entry.
pub struct LogEntry {
    pub timestamp: String,
    pub agent_type: AgentType,
    pub latency_ms: u64,
    pub summary: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentType {
    Perception,
    Strategy,
    Risk,
    Execution,
}

impl AgentType {
    pub fn label(self) -> &'static str {
        match self {
            Self::Perception => "PERCEPTION",
            Self::Strategy => "STRATEGY",
            Self::Risk => "RISK",
            Self::Execution => "EXECUTION",
        }
    }
}

impl App {
    pub fn new() -> Self {
        Self {
            active: ActivePanel::Market,
            running: true,
            paused: false,
            show_help: false,
            zen_mode: false,
            frame_count: 0,
            simulator: Simulator::new(150.0),
            context: None,
            price_history: PriceHistory::new(60),
            log_entries: Vec::new(),
        }
    }

    /// Tick the simulator and update state.
    pub fn tick(&mut self) {
        if self.paused {
            return;
        }

        self.simulator.tick();
        self.context = self.simulator.read();

        if self.context.is_some() {
            self.price_history.push(self.simulator.price());

            // Generate simulated log entries every 10 ticks
            if self.frame_count % 10 == 0 {
                let tick = self.simulator.tick_count();
                let entries = generate_log_entries(tick, self.simulator.price());
                for entry in entries {
                    self.log_entries.push(entry);
                    // Keep last 100 entries
                    if self.log_entries.len() > 100 {
                        self.log_entries.remove(0);
                    }
                }
            }
        }
    }

    /// Get border style for a panel — highlighted if active.
    pub fn panel_borders(&self, panel: ActivePanel) -> Borders {
        if self.active == panel {
            Borders::ALL
        } else {
            Borders::ALL
        }
    }

    /// Should this panel be rendered?
    pub fn should_render(&self, panel: ActivePanel) -> bool {
        if self.zen_mode {
            self.active == panel
        } else {
            true
        }
    }
}

fn generate_log_entries(tick: u64, price: f64) -> Vec<LogEntry> {
    let ts = format!("11:{:02}:{:02}", (tick / 60) % 60, tick % 60);
    vec![
        LogEntry {
            timestamp: ts.clone(),
            agent_type: AgentType::Perception,
            latency_ms: 8 + (tick % 10),
            summary: format!("regime:trend  confidence:0.{:02}", 70 + tick % 20),
        },
        LogEntry {
            timestamp: ts.clone(),
            agent_type: AgentType::Strategy,
            latency_ms: 5 + (tick % 8),
            summary: format!("intent:TrendFollow  conf:0.{:02}", 65 + tick % 25),
        },
        LogEntry {
            timestamp: ts.clone(),
            agent_type: AgentType::Risk,
            latency_ms: 1,
            summary: format!("gradient:{:.2}  pass", 0.2 + (tick % 10) as f64 * 0.03),
        },
        LogEntry {
            timestamp: ts,
            agent_type: AgentType::Execution,
            latency_ms: 2 + (tick % 5),
            summary: format!("BUY 10 SOL @ {:.2}  slip:{:.1}bps", price, 1.5 + (tick % 3) as f64),
        },
    ]
}

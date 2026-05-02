//! Application state and egui_dock TabViewer implementation.

use std::sync::mpsc;
use std::time::Duration;

use egui::{Color32, FontFamily, FontId, RichText};
use egui_dock::{DockState, NodeIndex, TabViewer};

use nautilus_state_encoder::ContextWindow;

use crate::data;
use crate::panels;
use crate::theme::SextantTheme;
use crate::DataPayload;

/// Which panel tab is active.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Tab {
    Market,
    AgentLog,
    Risk,
    Orders,
    Research,
    Memory,
}

impl Tab {
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
}

/// Price history ring buffer.
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

/// GUI-side agent log entry.
pub struct LogEntry {
    pub timestamp: String,
    pub agent_type: AgentType,
    pub latency_ms: u64,
    pub summary: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum AgentType {
    Perception,
    Strategy,
    Risk,
    Execution,
}

impl AgentType {
    pub fn color(self) -> Color32 {
        match self {
            Self::Perception => Color32::from_rgb(0xc0, 0x60, 0xff),
            Self::Strategy => SextantTheme::YELLOW,
            Self::Risk => SextantTheme::RED,
            Self::Execution => SextantTheme::CYAN,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Perception => "PERCEPTION",
            Self::Strategy => "STRATEGY",
            Self::Risk => "RISK",
            Self::Execution => "EXECUTION",
        }
    }
}

impl From<data::LogAgentType> for AgentType {
    fn from(t: data::LogAgentType) -> Self {
        match t {
            data::LogAgentType::Perception => Self::Perception,
            data::LogAgentType::Strategy => Self::Strategy,
            data::LogAgentType::Risk => Self::Risk,
            data::LogAgentType::Execution => Self::Execution,
        }
    }
}

impl From<data::LogEntry> for LogEntry {
    fn from(e: data::LogEntry) -> Self {
        Self {
            timestamp: e.timestamp,
            agent_type: e.agent_type.into(),
            latency_ms: e.latency_ms,
            summary: e.summary,
        }
    }
}

/// Shared GUI state updated each frame.
pub struct GuiState {
    pub context: Option<ContextWindow>,
    pub price_history: PriceHistory,
    pub log_entries: Vec<LogEntry>,
    pub current_price: f64,
    pub last_version: u64,
}

impl Default for GuiState {
    fn default() -> Self {
        Self {
            context: None,
            price_history: PriceHistory::new(600),
            log_entries: Vec::new(),
            current_price: 150.0,
            last_version: 0,
        }
    }
}

impl GuiState {
    pub fn update(&mut self, payload: DataPayload) {
        self.current_price =
            parse_mid_price(payload.context.market_state_str()).unwrap_or(self.current_price);
        self.price_history.push(self.current_price);
        self.last_version = payload.context.version;
        self.context = Some(payload.context);

        // Append log entries, bounded to last 200
        for entry in payload.logs {
            self.log_entries.push(entry.into());
        }
        if self.log_entries.len() > 200 {
            let drain = self.log_entries.len() - 200;
            self.log_entries.drain(..drain);
        }
    }
}

fn parse_mid_price(state: &str) -> Option<f64> {
    let bid_part = state.strip_prefix("bid:")?;
    let bid_str = bid_part
        .split(|c: char| !c.is_ascii_digit() && c != '.')
        .next()?;
    let bid: f64 = bid_str.parse().ok()?;
    let ask_marker = state.find("ask:")?;
    let ask_part = &state[ask_marker + 4..];
    let ask_str = ask_part
        .split(|c: char| !c.is_ascii_digit() && c != '.')
        .next()?;
    let ask: f64 = ask_str.parse().ok()?;
    Some((bid + ask) / 2.0)
}

/// The main application.
pub struct SextantApp {
    pub dock_state: DockState<Tab>,
    pub state: GuiState,
    pub rx: mpsc::Receiver<DataPayload>,
    pub dock_style: egui_dock::Style,
}

impl SextantApp {
    pub fn new(rx: mpsc::Receiver<DataPayload>) -> Self {
        let mut dock_state = DockState::new(vec![Tab::Market]);
        let [left, _right] =
            dock_state
                .main_surface_mut()
                .split_left(NodeIndex::root(), 0.3, vec![Tab::AgentLog]);
        let [top_right, _bottom_right] =
            dock_state
                .main_surface_mut()
                .split_right(left, 0.6, vec![Tab::Risk]);
        let [mid_right, _bot_right] =
            dock_state
                .main_surface_mut()
                .split_below(top_right, 0.5, vec![Tab::Orders]);
        dock_state
            .main_surface_mut()
            .split_below(mid_right, 0.5, vec![Tab::Research]);

        Self {
            dock_state,
            state: GuiState::default(),
            rx,
            dock_style: SextantTheme::dock_style(),
        }
    }
}

impl eframe::App for SextantApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Drain all pending data from the background thread
        while let Ok(payload) = self.rx.try_recv() {
            self.state.update(payload);
        }

        SextantTheme::apply(ctx);

        // Render dock with Cyberpunk-Terminal style
        egui_dock::DockArea::new(&mut self.dock_state)
            .style(self.dock_style.clone())
            .show(ctx, &mut TabViewerImpl { state: &self.state });

        // Request repaint to keep polling data
        ctx.request_repaint();
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, "dock_state", &self.dock_state);
    }

    fn auto_save_interval(&self) -> Duration {
        Duration::from_secs(30)
    }
}

struct TabViewerImpl<'a> {
    state: &'a GuiState,
}

impl<'a> TabViewer for TabViewerImpl<'a> {
    type Tab = Tab;

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        RichText::new(tab.label())
            .font(FontId::new(12.0, FontFamily::Monospace))
            .into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        match tab {
            Tab::Market => panels::market::render(ui, self.state),
            Tab::AgentLog => panels::agent_log::render(ui, self.state),
            Tab::Risk => panels::risk::render(ui, self.state),
            Tab::Orders => panels::orders::render(ui, self.state),
            Tab::Research => panels::research::render(ui, self.state),
            Tab::Memory => panels::memory::render(ui, self.state),
        }
    }
}

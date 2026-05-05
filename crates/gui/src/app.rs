//! Application state and egui_dock TabViewer implementation.

use std::sync::mpsc;
use std::time::Duration;

use egui::{Color32, FontFamily, FontId, RichText};
use egui_dock::{DockState, NodeIndex, TabViewer};

use nautilus_state_encoder::{ContextWindow, SextantEvent};

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

    fn from_number(n: u8) -> Option<Self> {
        match n {
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

/// OHLCV bar accumulator — groups N price ticks into one candlestick bar.
pub struct OhlcvAccumulator {
    pub bars: Vec<OhlcvBar>,
    pub max_len: usize,
    pub group_size: usize,
    // Current bar state
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
    tick_count: usize,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct OhlcvBar {
    pub time: chrono::DateTime<chrono::Utc>,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

impl OhlcvAccumulator {
    pub fn new(group_size: usize, max_len: usize) -> Self {
        Self {
            bars: Vec::with_capacity(max_len),
            max_len,
            group_size,
            open: 0.0,
            high: f64::NEG_INFINITY,
            low: f64::INFINITY,
            close: 0.0,
            volume: 0.0,
            tick_count: 0,
        }
    }

    /// Feed a new price tick. Returns `Some(bar)` when a bar is completed.
    pub fn push(&mut self, price: f64, volume: f64) -> Option<OhlcvBar> {
        if self.tick_count == 0 {
            self.open = price;
            self.high = price;
            self.low = price;
        } else {
            self.high = self.high.max(price);
            self.low = self.low.min(price);
        }
        self.close = price;
        self.volume += volume;
        self.tick_count += 1;

        if self.tick_count >= self.group_size {
            let bar = OhlcvBar {
                time: chrono::Utc::now(),
                open: self.open,
                high: self.high,
                low: self.low,
                close: self.close,
                volume: self.volume,
            };
            // Ring buffer eviction
            if self.bars.len() >= self.max_len {
                self.bars.remove(0);
            }
            self.bars.push(bar.clone());
            self.tick_count = 0;
            Some(bar)
        } else {
            None
        }
    }

    /// Get the current incomplete bar (open/high/low/close so far).
    #[allow(dead_code)]
    pub fn current(&self) -> Option<OhlcvBar> {
        if self.tick_count > 0 {
            Some(OhlcvBar {
                time: chrono::Utc::now(),
                open: self.open,
                high: self.high,
                low: self.low,
                close: self.close,
                volume: self.volume,
            })
        } else {
            None
        }
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
    pub ohlcv: OhlcvAccumulator,
    pub log_entries: Vec<LogEntry>,
    pub current_price: f64,
    pub last_version: u64,
    // Extended events (from --events mmap)
    pub order_events: Vec<SextantEvent>,
    pub research_events: Vec<SextantEvent>,
    pub risk_events: Vec<SextantEvent>,
    pub intent_events: Vec<SextantEvent>,
}

impl Default for GuiState {
    fn default() -> Self {
        Self {
            context: None,
            price_history: PriceHistory::new(600),
            ohlcv: OhlcvAccumulator::new(10, 200), // 10 ticks per bar, 200 bars max
            log_entries: Vec::new(),
            current_price: 150.0,
            last_version: 0,
            order_events: Vec::new(),
            research_events: Vec::new(),
            risk_events: Vec::new(),
            intent_events: Vec::new(),
        }
    }
}

impl GuiState {
    pub fn update(&mut self, payload: DataPayload) {
        self.current_price =
            parse_mid_price(payload.context.market_state_str()).unwrap_or(self.current_price);
        self.price_history.push(self.current_price);
        self.ohlcv.push(self.current_price, 1.0);
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

        // Sort extended events by type, bounded to last 256 each
        for event in payload.events {
            match &event {
                SextantEvent::OrderSubmitted { .. }
                | SextantEvent::OrderFilled { .. }
                | SextantEvent::OrderRejected { .. } => {
                    self.order_events.push(event);
                }
                SextantEvent::AutoresearchResult { .. } => {
                    self.research_events.push(event);
                }
                SextantEvent::RiskAlert { .. } => {
                    self.risk_events.push(event);
                }
                SextantEvent::IntentGenerated { .. }
                | SextantEvent::IntentApproved { .. }
                | SextantEvent::IntentRejected { .. } => {
                    self.intent_events.push(event);
                }
            }
        }
        // Bound each event list
        for list in [
            &mut self.order_events,
            &mut self.research_events,
            &mut self.risk_events,
            &mut self.intent_events,
        ] {
            if list.len() > 256 {
                let drain = list.len() - 256;
                list.drain(..drain);
            }
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
    pub fonts_loaded: bool,
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
            fonts_loaded: false,
        }
    }

    /// Load system fonts on first frame.
    fn setup_fonts(&self, ctx: &egui::Context) {
        let mut fonts = egui::FontDefinitions::default();

        // Try loading Consolas (Windows monospace)
        if let Ok(data) = std::fs::read("C:\\Windows\\Fonts\\consola.ttf") {
            fonts.font_data.insert(
                "consolas".to_owned(),
                std::sync::Arc::new(egui::FontData::from_owned(data)),
            );
            if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
                family.push("consolas".to_owned());
            }
        }

        // Try loading Segoe UI (Windows proportional)
        if let Ok(data) = std::fs::read("C:\\Windows\\Fonts\\segoeui.ttf") {
            fonts.font_data.insert(
                "segoeui".to_owned(),
                std::sync::Arc::new(egui::FontData::from_owned(data)),
            );
            if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                family.push("segoeui".to_owned());
            }
        }

        ctx.set_fonts(fonts);
    }

    /// Switch to a specific tab by number (1-6).
    fn switch_to_tab(&mut self, tab: Tab) {
        // Find which (surface, node) contains this tab
        let found = self
            .dock_state
            .iter_all_tabs()
            .find(|(_, t)| **t == tab)
            .map(|((s, n), _)| (s, n));

        if let Some((si, ni)) = found {
            // Get the node from the surface's tree to find tab index
            if let Some(tree) = self.dock_state.get_surface(si).and_then(|s| s.node_tree()) {
                let node = &tree[ni];
                if let Some(tabs) = node.tabs() {
                    for (idx, t) in tabs.iter().enumerate() {
                        if *t == tab {
                            self.dock_state
                                .set_active_tab((si, ni, egui_dock::TabIndex(idx)));
                            self.dock_state
                                .set_focused_node_and_surface((si, ni));
                            return;
                        }
                    }
                }
            }
        }
    }
}

impl eframe::App for SextantApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Load fonts once on first frame
        if !self.fonts_loaded {
            self.setup_fonts(ctx);
            self.fonts_loaded = true;
        }

        // Drain all pending data from the background thread
        while let Ok(payload) = self.rx.try_recv() {
            self.state.update(payload);
        }

        SextantTheme::apply(ctx);

        // Keyboard shortcuts
        ctx.input(|i| {
            // Number keys 1-6 switch tabs
            for n in 1..=6u8 {
                let key = match n {
                    1 => egui::Key::Num1,
                    2 => egui::Key::Num2,
                    3 => egui::Key::Num3,
                    4 => egui::Key::Num4,
                    5 => egui::Key::Num5,
                    6 => egui::Key::Num6,
                    _ => unreachable!(),
                };
                if i.key_pressed(key) {
                    if let Some(tab) = Tab::from_number(n) {
                        self.switch_to_tab(tab);
                    }
                }
            }

            // Ctrl+R: reset layout to default
            if i.key_pressed(egui::Key::R) && i.modifiers.ctrl {
                let mut dock = DockState::new(vec![Tab::Market]);
                let [left, _] = dock.main_surface_mut().split_left(
                    NodeIndex::root(),
                    0.3,
                    vec![Tab::AgentLog],
                );
                let [top_right, _] = dock
                    .main_surface_mut()
                    .split_right(left, 0.6, vec![Tab::Risk]);
                let [mid_right, _] = dock
                    .main_surface_mut()
                    .split_below(top_right, 0.5, vec![Tab::Orders]);
                dock.main_surface_mut()
                    .split_below(mid_right, 0.5, vec![Tab::Research]);
                self.dock_state = dock;
            }
        });

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

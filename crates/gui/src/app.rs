//! Application state — bridge viewport layout.
//!
//! Layout:
//!   Top:    PnL strip (position, PnL, command input)
//!   Center: Sea chart (flowing liquidity/volatility)
//!   Right:  Intent card slot (empty or card)
//!   Bottom: Crew status bar (agent cells)
//!
//! Secondary panels (Orders, Research, Memory, Hull Integrity)
//! are available as drawers, toggled by keyboard shortcuts.

use std::collections::HashMap;
use std::sync::mpsc;
use std::time::Duration;

use egui::{Color32, CornerRadius, Vec2};

use nautilus_state_encoder::{ContextWindow, SextantEvent};

use crate::data;
use crate::panels;
use crate::theme::SextantTheme;
use crate::DataPayload;

/// Which drawer is open (if any).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drawer {
    Orders,
    Research,
    Memory,
    HullIntegrity,
    StrategyCenter,
    ExecutionLog,
    ResearchLab,
}

impl Drawer {
    pub fn label(self) -> &'static str {
        match self {
            Self::Orders => "Orders",
            Self::Research => "Research",
            Self::Memory => "Memory",
            Self::HullIntegrity => "Hull Integrity",
            Self::StrategyCenter => "策略工坊",
            Self::ExecutionLog => "航海日志",
            Self::ResearchLab => "研究室",
        }
    }
}

/// Autonomy level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Autonomy {
    Manual,
    Assisted,
    Auto,
}

impl Autonomy {
    pub fn label(self) -> &'static str {
        match self {
            Self::Manual => "MANUAL",
            Self::Assisted => "ASSISTED",
            Self::Auto => "AUTO",
        }
    }

    pub fn color(self) -> Color32 {
        match self {
            Self::Manual => SextantTheme::TEXT_SECONDARY,
            Self::Assisted => SextantTheme::YELLOW,
            Self::Auto => SextantTheme::GREEN,
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

/// OHLCV bar accumulator.
pub struct OhlcvAccumulator {
    pub bars: Vec<OhlcvBar>,
    pub max_len: usize,
    pub group_size: usize,
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
}

/// GUI-side agent log entry.
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
    pub fn color(self) -> Color32 {
        match self {
            Self::Perception => SextantTheme::CYAN,
            Self::Strategy => SextantTheme::YELLOW,
            Self::Risk => SextantTheme::RED,
            Self::Execution => SextantTheme::GREEN,
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

/// Agent state for the crew status bar.
#[derive(Clone, Debug)]
pub struct AgentState {
    pub id: String,
    pub agent_type: AgentType,
    pub status: AgentStatus,
    pub task: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentStatus {
    Active,  // green
    Busy,    // yellow
    Alert,   // red
    Idle,    // muted
}

impl AgentStatus {
    pub fn color(self) -> Color32 {
        match self {
            Self::Active => SextantTheme::GREEN,
            Self::Busy => SextantTheme::YELLOW,
            Self::Alert => SextantTheme::RED,
            Self::Idle => SextantTheme::TEXT_MUTED,
        }
    }
}

/// Intent card — agent wants to trade.
#[derive(Clone, Debug)]
pub struct IntentCard {
    pub intent_id: String,
    pub agent_id: String,
    pub title: String,
    pub reasoning: String,
    pub side: String,      // "BUY" / "SELL"
    pub quantity: f64,
    pub price: Option<f64>,
    pub confidence: f64,
    pub confidence_label: String,
}

/// Commands sent from GUI back to the engine.
#[derive(Clone, Debug)]
pub enum GuiCommand {
    ApproveIntent { intent_id: String },
    RejectIntent { intent_id: String, reason: String },
    SetEStop(bool),
    SetAutonomy(Autonomy),
}

/// Execution log sub-tab state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionLogTab {
    Execution,
    Evolution,
    Backtest,
}

/// Shared GUI state updated each frame.
pub struct GuiState {
    pub context: Option<ContextWindow>,
    pub price_history: PriceHistory,
    pub ohlcv: OhlcvAccumulator,
    pub log_entries: Vec<LogEntry>,
    pub current_price: f64,
    pub last_version: u64,
    // Extended events
    pub order_events: Vec<SextantEvent>,
    pub research_events: Vec<SextantEvent>,
    pub risk_events: Vec<SextantEvent>,
    pub intent_events: Vec<SextantEvent>,
    // Crew state
    pub agents: Vec<AgentState>,
    // Intent cards
    pub intent_cards: Vec<IntentCard>,
    // Autonomy
    pub autonomy: Autonomy,
    // E-Stop
    pub e_stopped: bool,
    // Open drawer
    pub open_drawer: Option<Drawer>,
    // GUI → engine command channel
    pub cmd_tx: Option<mpsc::Sender<GuiCommand>>,
    // Persistent strategy parameters (agent_id → param_name → value)
    pub strategy_params: HashMap<String, HashMap<String, f64>>,
    // Execution log sub-tab
    pub execution_log_tab: ExecutionLogTab,
    // Monotonic intent ID counter
    next_intent_id: u64,
}

impl Default for GuiState {
    fn default() -> Self {
        Self {
            context: None,
            price_history: PriceHistory::new(600),
            ohlcv: OhlcvAccumulator::new(10, 200),
            log_entries: Vec::new(),
            current_price: 150.0,
            last_version: 0,
            order_events: Vec::new(),
            research_events: Vec::new(),
            risk_events: Vec::new(),
            intent_events: Vec::new(),
            agents: vec![
                AgentState {
                    id: "momentum-01".into(),
                    agent_type: AgentType::Strategy,
                    status: AgentStatus::Idle,
                    task: "Waiting for data...".into(),
                },
                AgentState {
                    id: "mean-rev-01".into(),
                    agent_type: AgentType::Strategy,
                    status: AgentStatus::Idle,
                    task: "Waiting for data...".into(),
                },
                AgentState {
                    id: "risk-01".into(),
                    agent_type: AgentType::Risk,
                    status: AgentStatus::Idle,
                    task: "Monitoring...".into(),
                },
                AgentState {
                    id: "perception".into(),
                    agent_type: AgentType::Perception,
                    status: AgentStatus::Idle,
                    task: "Idle".into(),
                },
            ],
            intent_cards: Vec::new(),
            autonomy: Autonomy::Assisted,
            e_stopped: false,
            open_drawer: None,
            cmd_tx: None,
            strategy_params: HashMap::new(),
            execution_log_tab: ExecutionLogTab::Execution,
            next_intent_id: 0,
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

        // Sort extended events by type
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
                SextantEvent::IntentGenerated {
                    agent_id,
                    intent_type,
                    title,
                    reasoning,
                    confidence,
                    confidence_label,
                    ..
                } => {
                    let side = if intent_type.to_lowercase().contains("sell")
                        || intent_type.to_lowercase().contains("short")
                    {
                        "SELL"
                    } else {
                        "BUY"
                    };
                    let id = format!("INT-{}", self.next_intent_id);
                    self.next_intent_id += 1;
                    self.intent_cards.push(IntentCard {
                        intent_id: id,
                        agent_id: agent_id.clone(),
                        title: title.clone(),
                        reasoning: reasoning.clone(),
                        side: side.into(),
                        quantity: 0.01,
                        price: None,
                        confidence: *confidence,
                        confidence_label: confidence_label.clone(),
                    });
                    self.intent_events.push(event);
                }
                SextantEvent::IntentApproved { intent_id, .. } => {
                    self.intent_cards.retain(|c| c.intent_id != *intent_id);
                    self.intent_events.push(event);
                }
                SextantEvent::IntentRejected { intent_id, .. } => {
                    self.intent_cards.retain(|c| c.intent_id != *intent_id);
                    self.intent_events.push(event);
                }
            }
        }
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

        // Update crew status based on latest events
        self.update_crew_status();
    }

    fn update_crew_status(&mut self) {
        // Update agent states based on recent activity
        for agent in &mut self.agents {
            if self.last_version > 0 {
                agent.status = AgentStatus::Active;
                match agent.agent_type {
                    AgentType::Strategy => {
                        agent.task = format!("Watching {:.0}", self.current_price);
                    }
                    AgentType::Risk => {
                        let risk = self.context.as_ref()
                            .map(|c| c.risk_potential)
                            .unwrap_or(0.0);
                        if risk > 0.8 {
                            agent.status = AgentStatus::Alert;
                            agent.task = format!("Risk: {:.0}%", risk * 100.0);
                        } else if risk > 0.5 {
                            agent.status = AgentStatus::Busy;
                            agent.task = format!("Risk: {:.0}%", risk * 100.0);
                        } else {
                            agent.task = "Nominal".into();
                        }
                    }
                    AgentType::Perception => {
                        agent.task = "Processing quotes".into();
                    }
                    _ => {}
                }
            }
        }
    }

    pub fn dismiss_intent(&mut self, idx: usize) {
        if idx < self.intent_cards.len() {
            self.intent_cards.remove(idx);
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
    pub state: GuiState,
    pub rx: mpsc::Receiver<DataPayload>,
    pub fonts_loaded: bool,
}

impl SextantApp {
    pub fn new(rx: mpsc::Receiver<DataPayload>) -> Self {
        Self {
            state: GuiState::default(),
            rx,
            fonts_loaded: false,
        }
    }

    fn setup_fonts(&self, ctx: &egui::Context) {
        let mut fonts = egui::FontDefinitions::default();

        if let Ok(data) = std::fs::read("C:\\Windows\\Fonts\\consola.ttf") {
            fonts.font_data.insert(
                "consolas".to_owned(),
                std::sync::Arc::new(egui::FontData::from_owned(data)),
            );
            if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
                family.push("consolas".to_owned());
            }
        }

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
}

impl eframe::App for SextantApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.fonts_loaded {
            self.setup_fonts(ctx);
            self.fonts_loaded = true;
        }

        // Drain all pending data
        while let Ok(payload) = self.rx.try_recv() {
            self.state.update(payload);
        }

        SextantTheme::apply(ctx);

        // Keyboard shortcuts
        ctx.input(|i| {
            if i.key_pressed(egui::Key::Num1) {
                self.state.open_drawer = match self.state.open_drawer {
                    Some(Drawer::Orders) => None,
                    _ => Some(Drawer::Orders),
                };
            }
            if i.key_pressed(egui::Key::Num2) {
                self.state.open_drawer = match self.state.open_drawer {
                    Some(Drawer::Research) => None,
                    _ => Some(Drawer::Research),
                };
            }
            if i.key_pressed(egui::Key::Num3) {
                self.state.open_drawer = match self.state.open_drawer {
                    Some(Drawer::Memory) => None,
                    _ => Some(Drawer::Memory),
                };
            }
            if i.key_pressed(egui::Key::Num4) {
                self.state.open_drawer = match self.state.open_drawer {
                    Some(Drawer::HullIntegrity) => None,
                    _ => Some(Drawer::HullIntegrity),
                };
            }
            if i.key_pressed(egui::Key::Num5) {
                self.state.open_drawer = match self.state.open_drawer {
                    Some(Drawer::StrategyCenter) => None,
                    _ => Some(Drawer::StrategyCenter),
                };
            }
            if i.key_pressed(egui::Key::Num6) {
                self.state.open_drawer = match self.state.open_drawer {
                    Some(Drawer::ExecutionLog) => None,
                    _ => Some(Drawer::ExecutionLog),
                };
            }
            if i.key_pressed(egui::Key::Num7) {
                self.state.open_drawer = match self.state.open_drawer {
                    Some(Drawer::ResearchLab) => None,
                    _ => Some(Drawer::ResearchLab),
                };
            }
            if i.key_pressed(egui::Key::Escape) {
                self.state.open_drawer = None;
            }
        });

        // Layout
        let screen = ctx.screen_rect();

        // ── Top: PnL strip ────────────────────────────────────
        let pnl_height = 48.0;
        let pnl_rect = egui::Rect::from_min_size(
            screen.min,
            Vec2::new(screen.width(), pnl_height),
        );

        // ── Bottom: Crew status bar ───────────────────────────
        let crew_height = 52.0;
        let crew_rect = egui::Rect::from_min_size(
            egui::pos2(screen.min.x, screen.max.y - crew_height),
            Vec2::new(screen.width(), crew_height),
        );

        // ── Right: Intent card slot ───────────────────────────
        let intent_width = if self.state.intent_cards.is_empty() || self.state.open_drawer.is_some() {
            0.0
        } else {
            320.0
        };
        let intent_rect = if intent_width > 0.0 {
            egui::Rect::from_min_size(
                egui::pos2(screen.max.x - intent_width, pnl_height),
                Vec2::new(intent_width, screen.height() - pnl_height - crew_height),
            )
        } else {
            egui::Rect::ZERO
        };

        // ── Center: Sea chart ─────────────────────────────────
        let chart_rect = egui::Rect::from_min_size(
            egui::pos2(screen.min.x, pnl_height),
            Vec2::new(
                screen.width() - intent_width,
                screen.height() - pnl_height - crew_height,
            ),
        );

        // ── Drawer (overlay from right) ───────────────────────
        let drawer_width = 480.0;
        let drawer_rect = if self.state.open_drawer.is_some() {
            egui::Rect::from_min_size(
                egui::pos2(screen.max.x - drawer_width, pnl_height),
                Vec2::new(drawer_width, screen.height() - pnl_height - crew_height),
            )
        } else {
            egui::Rect::ZERO
        };

        // Render PnL strip
        egui::Area::new(egui::Id::new("pnl_strip"))
            .fixed_pos(pnl_rect.min)
            .show(ctx, |ui| {
                ui.set_min_size(pnl_rect.size());
                ui.set_max_size(pnl_rect.size());
                panels::pnl_strip::render(ui, &self.state);
            });

        // Render sea chart
        egui::Area::new(egui::Id::new("sea_chart"))
            .fixed_pos(chart_rect.min)
            .show(ctx, |ui| {
                ui.set_min_size(chart_rect.size());
                ui.set_max_size(chart_rect.size());
                panels::sea_chart::render(ui, &self.state);
            });

        // Render crew status bar
        egui::Area::new(egui::Id::new("crew_status"))
            .fixed_pos(crew_rect.min)
            .show(ctx, |ui| {
                ui.set_min_size(crew_rect.size());
                ui.set_max_size(crew_rect.size());
                panels::crew_status::render(ui, &mut self.state);
            });

        // Render intent cards (hidden when drawer is open)
        if !self.state.intent_cards.is_empty() && self.state.open_drawer.is_none() {
            egui::Area::new(egui::Id::new("intent_cards"))
                .fixed_pos(intent_rect.min)
                .show(ctx, |ui| {
                    ui.set_min_size(intent_rect.size());
                    ui.set_max_size(intent_rect.size());
                    panels::intent_card::render(ui, &mut self.state);
                });
        }

        // Render drawer overlay
        if let Some(drawer) = self.state.open_drawer {
            // Clickable background — click to close drawer
            let bg_rect = egui::Rect::from_min_size(
                egui::pos2(screen.min.x, pnl_height),
                Vec2::new(
                    screen.width() - drawer_width,
                    screen.height() - pnl_height - crew_height,
                ),
            );
            let bg_response = egui::Area::new(egui::Id::new("drawer_bg"))
                .fixed_pos(bg_rect.min)
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    ui.set_min_size(bg_rect.size());
                    ui.set_max_size(bg_rect.size());
                    ui.painter().rect_filled(
                        bg_rect,
                        CornerRadius::ZERO,
                        Color32::from_rgba_premultiplied(0x0f, 0x11, 0x17, 0x80),
                    );
                });
            if bg_response.response.clicked() {
                self.state.open_drawer = None;
            }

            egui::Area::new(egui::Id::new("drawer"))
                .fixed_pos(drawer_rect.min)
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    ui.set_min_size(drawer_rect.size());
                    ui.set_max_size(drawer_rect.size());
                    panels::drawer::render(ui, &mut self.state, drawer);
                });
        }

        ctx.request_repaint();
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        // Nothing to persist in the new layout
        let _ = storage;
    }

    fn auto_save_interval(&self) -> Duration {
        Duration::from_secs(30)
    }
}

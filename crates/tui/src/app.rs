//! Application state and panel routing.

use ratatui::widgets::Borders;

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

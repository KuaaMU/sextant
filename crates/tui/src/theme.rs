//! Color theme for the Sextant TUI.

use ratatui::style::Color;

/// Sextant TUI color theme — dark terminal with green/red trading colors.
pub struct Theme;

impl Theme {
    // Status colors
    pub const OK: Color = Color::Green;
    pub const WARN: Color = Color::Yellow;
    pub const ALERT: Color = Color::Red;
    pub const INFO: Color = Color::Blue;

    // Panel colors
    pub const PANEL_BORDER: Color = Color::DarkGray;
    pub const PANEL_BORDER_ACTIVE: Color = Color::Cyan;
    pub const PANEL_TITLE: Color = Color::White;

    // Text
    pub const TEXT: Color = Color::White;
    pub const TEXT_DIM: Color = Color::Gray;
    pub const TEXT_BRIGHT: Color = Color::White;

    // Trading
    pub const BUY: Color = Color::Green;
    pub const SELL: Color = Color::Red;
    pub const FILL: Color = Color::Green;
    pub const PARTIAL: Color = Color::Cyan;
    pub const PENDING: Color = Color::Yellow;
    pub const DENY: Color = Color::Red;
    pub const ACK: Color = Color::Blue;

    // Background highlights
    pub const HIGHLIGHT_BG: Color = Color::DarkGray;
    pub const SELECTED_BG: Color = Color::Rgb(40, 40, 60);
}

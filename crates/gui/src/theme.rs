//! Cyberpunk-Terminal theme for Sextant GUI.
//!
//! Three-layer background depth, neon accents, font hierarchy,
//! styled dock tabs, capsule badges, glowing borders.

use egui::{Color32, CornerRadius, FontFamily, FontId, Margin, Stroke, Vec2};

pub struct SextantTheme;

// ── Color Palette ────────────────────────────────────────────────

#[allow(dead_code)]
impl SextantTheme {
    // Background layers (deep → surface)
    pub const BG_DEEP: Color32 = Color32::from_rgb(0x0d, 0x0d, 0x1a);
    pub const BG_BASE: Color32 = Color32::from_rgb(0x12, 0x12, 0x1f);
    pub const BG_SURFACE: Color32 = Color32::from_rgb(0x1a, 0x1a, 0x2e);
    pub const BG_ELEVATED: Color32 = Color32::from_rgb(0x23, 0x23, 0x38);

    // Neon accents
    pub const CYAN: Color32 = Color32::from_rgb(0x00, 0xf0, 0xff);
    pub const MAGENTA: Color32 = Color32::from_rgb(0xff, 0x00, 0x6e);
    pub const YELLOW: Color32 = Color32::from_rgb(0xff, 0xbe, 0x0b);
    pub const GREEN: Color32 = Color32::from_rgb(0x06, 0xff, 0xa5);
    pub const RED: Color32 = Color32::from_rgb(0xff, 0x38, 0x64);

    // Semantic aliases
    pub const BUY: Color32 = Self::CYAN;
    pub const SELL: Color32 = Self::MAGENTA;
    pub const OK: Color32 = Self::GREEN;
    pub const WARN: Color32 = Self::YELLOW;
    pub const ALERT: Color32 = Self::RED;
    pub const INFO: Color32 = Self::CYAN;

    // Order status
    pub const FILL: Color32 = Self::GREEN;
    pub const PARTIAL: Color32 = Self::CYAN;
    pub const ACK: Color32 = Self::YELLOW;
    pub const PENDING: Color32 = Self::TEXT_MUTED;
    pub const DENY: Color32 = Self::RED;

    // Text hierarchy
    pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(0xe8, 0xe8, 0xf0);
    pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(0x8b, 0x8b, 0xa7);
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x5a, 0x5a, 0x7a);
    #[allow(dead_code)]
    pub const TEXT_DISABLED: Color32 = Color32::from_rgb(0x3a, 0x3a, 0x5a);

    // Borders
    pub const BORDER_SUBTLE: Color32 = Color32::from_rgb(0x2a, 0x2a, 0x4a);
    pub const BORDER_GLOW: Color32 = Color32::from_rgba_premultiplied(0x00, 0x3c, 0x3f, 0x30);

    // Backwards-compat aliases for panel code
    pub const TEXT: Color32 = Self::TEXT_PRIMARY;
    pub const TEXT_DIM: Color32 = Self::TEXT_MUTED;
    pub const BG: Color32 = Self::BG_SURFACE;
    pub const SURFACE: Color32 = Self::BG_BASE;
    pub const PANEL: Color32 = Self::BG_ELEVATED;
}

// ── Font System ──────────────────────────────────────────────────

impl SextantTheme {
    /// Monospace — data, numbers, code
    pub const FONT_MONO: FontId = FontId::new(13.0, FontFamily::Monospace);
    /// Sans-serif — UI labels, navigation
    pub const FONT_SANS: FontId = FontId::new(13.0, FontFamily::Proportional);
    /// Display — large prices, hero numbers
    #[allow(dead_code)]
    pub const FONT_DISPLAY: FontId = FontId::new(22.0, FontFamily::Proportional);
    /// Small — badges, footnotes
    pub const FONT_SMALL: FontId = FontId::new(10.0, FontFamily::Monospace);
}

// ── Theme Application ────────────────────────────────────────────

impl SextantTheme {
    /// Apply the Cyberpunk-Terminal dark theme to an egui context.
    pub fn apply(ctx: &egui::Context) {
        let mut style = (*ctx.style()).clone();

        style.visuals.dark_mode = true;

        // Background layers
        style.visuals.panel_fill = Self::BG_BASE;
        style.visuals.window_fill = Self::BG_SURFACE;
        style.visuals.extreme_bg_color = Self::BG_DEEP;
        style.visuals.faint_bg_color = Self::BG_SURFACE;

        // Override text color
        style.visuals.override_text_color = Some(Self::TEXT_PRIMARY);

        // Selection
        style.visuals.selection.bg_fill = Color32::from_rgba_premultiplied(0x00, 0x3c, 0x3f, 0x60);
        style.visuals.selection.stroke = Stroke::new(1.0, Self::CYAN);

        // Widget styling — all 5 states
        let r = CornerRadius::same(4);

        style.visuals.widgets.noninteractive.bg_fill = Self::BG_SURFACE;
        style.visuals.widgets.noninteractive.weak_bg_fill = Self::BG_SURFACE;
        style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Self::BORDER_SUBTLE);
        style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, Self::TEXT_PRIMARY);
        style.visuals.widgets.noninteractive.corner_radius = r;

        style.visuals.widgets.inactive.bg_fill = Self::BG_ELEVATED;
        style.visuals.widgets.inactive.weak_bg_fill = Self::BG_ELEVATED;
        style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Self::BORDER_SUBTLE);
        style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, Self::TEXT_SECONDARY);
        style.visuals.widgets.inactive.corner_radius = r;

        style.visuals.widgets.hovered.bg_fill = Self::BG_ELEVATED;
        style.visuals.widgets.hovered.weak_bg_fill = Self::BG_ELEVATED;
        style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Self::CYAN);
        style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, Self::TEXT_PRIMARY);
        style.visuals.widgets.hovered.corner_radius = r;

        style.visuals.widgets.active.bg_fill = Color32::from_rgba_premultiplied(0x00, 0x3c, 0x3f, 0x40);
        style.visuals.widgets.active.weak_bg_fill = Self::BG_ELEVATED;
        style.visuals.widgets.active.bg_stroke = Stroke::new(1.5, Self::CYAN);
        style.visuals.widgets.active.fg_stroke = Stroke::new(1.0, Self::CYAN);
        style.visuals.widgets.active.corner_radius = r;

        style.visuals.widgets.open.bg_fill = Self::BG_ELEVATED;
        style.visuals.widgets.open.weak_bg_fill = Self::BG_ELEVATED;
        style.visuals.widgets.open.bg_stroke = Stroke::new(1.0, Self::CYAN);
        style.visuals.widgets.open.fg_stroke = Stroke::new(1.0, Self::TEXT_PRIMARY);
        style.visuals.widgets.open.corner_radius = r;

        // Spacing — breathing room
        style.spacing.item_spacing = Vec2::new(8.0, 6.0);
        style.spacing.button_padding = Vec2::new(8.0, 4.0);
        style.spacing.window_margin = Margin::same(12);
        style.spacing.indent = 20.0;

        ctx.set_style(style);
    }

    /// Build a Cyberpunk-Terminal style for egui_dock.
    pub fn dock_style() -> egui_dock::Style {
        let mut s = egui_dock::Style::from_egui(&egui::Style::default());

        // Tab bar
        s.tab_bar.bg_fill = Self::BG_DEEP;
        s.tab_bar.height = 28.0;
        s.tab_bar.corner_radius = CornerRadius::same(0);
        s.tab_bar.hline_color = Self::BORDER_SUBTLE;
        s.tab_bar.fill_tab_bar = true;

        // Active tab
        s.tab.active.corner_radius = CornerRadius {
            nw: 4,
            ne: 4,
            sw: 0,
            se: 0,
        };
        s.tab.active.bg_fill = Self::BG_ELEVATED;
        s.tab.active.text_color = Self::CYAN;
        s.tab.active.outline_color = Self::CYAN;

        // Inactive tab
        s.tab.inactive.corner_radius = CornerRadius {
            nw: 4,
            ne: 4,
            sw: 0,
            se: 0,
        };
        s.tab.inactive.bg_fill = Self::BG_DEEP;
        s.tab.inactive.text_color = Self::TEXT_MUTED;
        s.tab.inactive.outline_color = Self::BORDER_SUBTLE;

        // Focused tab
        s.tab.focused.corner_radius = s.tab.active.corner_radius;
        s.tab.focused.bg_fill = Self::BG_SURFACE;
        s.tab.focused.text_color = Self::TEXT_PRIMARY;
        s.tab.focused.outline_color = Self::CYAN;

        // Hovered tab
        s.tab.hovered.corner_radius = s.tab.active.corner_radius;
        s.tab.hovered.bg_fill = Self::BG_SURFACE;
        s.tab.hovered.text_color = Self::TEXT_PRIMARY;
        s.tab.hovered.outline_color = Self::BORDER_SUBTLE;

        // Tab body
        s.tab.tab_body.inner_margin = Margin::same(8);
        s.tab.tab_body.bg_fill = Self::BG_SURFACE;
        s.tab.tab_body.corner_radius = CornerRadius::same(0);
        s.tab.tab_body.stroke = Stroke::NONE;

        // Separator
        s.separator.width = 2.0;
        s.separator.color_idle = Self::BORDER_SUBTLE;
        s.separator.color_hovered = Self::CYAN;
        s.separator.color_dragged = Self::CYAN;

        // Dock area padding
        s.dock_area_padding = Some(Margin::same(0));

        // Main surface border
        s.main_surface_border_stroke = Stroke::new(1.0, Self::BORDER_SUBTLE);
        s.main_surface_border_rounding = CornerRadius::same(0);

        s
    }
}

// ── Reusable Components ──────────────────────────────────────────

impl SextantTheme {
    /// Styled frame with background, rounded corners, subtle border.
    pub fn panel_frame() -> egui::Frame {
        egui::Frame::new()
            .fill(Self::BG_SURFACE)
            .corner_radius(CornerRadius::same(6))
            .stroke(Stroke::new(1.0, Self::BORDER_SUBTLE))
            .inner_margin(Margin::same(12))
    }

    /// Elevated frame (hovered/selected state).
    pub fn elevated_frame() -> egui::Frame {
        egui::Frame::new()
            .fill(Self::BG_ELEVATED)
            .corner_radius(CornerRadius::same(6))
            .stroke(Stroke::new(1.0, Self::BORDER_SUBTLE))
            .inner_margin(Margin::same(12))
    }

    /// Header frame with colored left accent stripe.
    pub fn accent_frame(accent: Color32) -> egui::Frame {
        // egui doesn't support per-side strokes, so we use a full border with accent color
        egui::Frame::new()
            .fill(Self::BG_ELEVATED)
            .corner_radius(CornerRadius::same(4))
            .stroke(Stroke::new(1.0, accent))
            .inner_margin(Margin::symmetric(10, 6))
    }

    /// Draw a capsule badge (rounded pill shape with text).
    pub fn badge(ui: &mut egui::Ui, text: &str, color: Color32) {
        let galley = ui.painter().layout_no_wrap(
            text.to_string(),
            Self::FONT_SMALL,
            color,
        );
        let text_size = galley.size();
        let padding = Vec2::new(8.0, 3.0);
        let total = text_size + padding * 2.0;

        let (rect, _response) = ui.allocate_exact_size(total, egui::Sense::hover());

        // Background: color at 15% opacity
        let bg = Color32::from_rgba_premultiplied(
            color.r(),
            color.g(),
            color.b(),
            (255.0 * 0.15) as u8,
        );
        ui.painter()
            .rect_filled(rect, CornerRadius::same(10), bg);

        // Text centered
        ui.painter().galley(
            rect.center() - text_size / 2.0,
            galley,
            color,
        );
    }

    /// Color for latency values: green <50ms, yellow <200ms, red >=200ms.
    pub fn latency_color(ms: u64) -> Color32 {
        if ms < 50 {
            Self::GREEN
        } else if ms < 200 {
            Self::YELLOW
        } else {
            Self::RED
        }
    }
}

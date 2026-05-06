//! Sextant bridge viewport theme — nautical deep-ocean palette.
//!
//! Deep navy backgrounds, ocean cyan accent, calm emerald/rose data colors.
//! Inspired by TradingView dark + Tailwind slate + nautical identity.

use egui::{Color32, CornerRadius, FontFamily, FontId, Margin, Stroke, Vec2};

pub struct SextantTheme;

#[allow(dead_code)]
impl SextantTheme {
    // Background layers — deep ocean navy (not gray)
    pub const BG_DEEP: Color32 = Color32::from_rgb(0x0B, 0x11, 0x20);   // #0B1120 deep ocean
    pub const BG_BASE: Color32 = Color32::from_rgb(0x11, 0x18, 0x27);   // #111827 dark slate
    pub const BG_SURFACE: Color32 = Color32::from_rgb(0x1F, 0x29, 0x37); // #1F2937 card bg
    pub const BG_ELEVATED: Color32 = Color32::from_rgb(0x37, 0x41, 0x51); // #374151 hover/popover

    // Accent — ocean cyan + AI violet
    pub const CYAN: Color32 = Color32::from_rgb(0x06, 0xB6, 0xD4);     // #06B6D4 ocean/primary accent
    pub const VIOLET: Color32 = Color32::from_rgb(0x8B, 0x5C, 0xF6);   // #8B5CF6 AI/agent accent
    pub const MAGENTA: Color32 = Color32::from_rgb(0xD4, 0x7B, 0x9a);  // rose (kept)
    pub const YELLOW: Color32 = Color32::from_rgb(0xF5, 0x9E, 0x0B);   // #F59E0B amber warning
    pub const GREEN: Color32 = Color32::from_rgb(0x10, 0xB9, 0x81);    // #10B981 emerald positive
    pub const RED: Color32 = Color32::from_rgb(0xEF, 0x44, 0x44);      // #EF4444 red negative
    pub const BLUE: Color32 = Color32::from_rgb(0x3B, 0x82, 0xF6);     // #3B82F6 blue info

    // Semantic
    pub const BUY: Color32 = Self::GREEN;
    pub const SELL: Color32 = Self::RED;
    pub const OK: Color32 = Self::GREEN;
    pub const WARN: Color32 = Self::YELLOW;
    pub const ALERT: Color32 = Self::RED;
    pub const INFO: Color32 = Self::BLUE;

    // Order status
    pub const FILL: Color32 = Self::GREEN;
    pub const PARTIAL: Color32 = Self::CYAN;
    pub const ACK: Color32 = Self::YELLOW;
    pub const PENDING: Color32 = Self::TEXT_MUTED;
    pub const DENY: Color32 = Self::RED;

    // Text hierarchy — high contrast on deep navy
    pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(0xF9, 0xFA, 0xFB);   // #F9FAFB near-white
    pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(0x9C, 0xA3, 0xAF); // #9CA3AF gray-400
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x6B, 0x72, 0x80);     // #6B7280 gray-500
    pub const TEXT_DISABLED: Color32 = Color32::from_rgb(0x4B, 0x55, 0x63);  // #4B5563 gray-600

    // Borders
    pub const BORDER_SUBTLE: Color32 = Color32::from_rgb(0x37, 0x41, 0x51); // #374151 matches elevated
    pub const BORDER_GLOW: Color32 = Color32::from_rgba_premultiplied(0x06, 0xB6, 0xD4, 0x30);

    // Sea chart specific
    pub const SEA_WARM: Color32 = Color32::from_rgb(0xEF, 0x44, 0x44);   // high vol
    pub const SEA_COOL: Color32 = Color32::from_rgb(0x06, 0xB6, 0xD4);   // low vol (cyan)
    pub const SEA_CURRENT: Color32 = Color32::from_rgb(0x3B, 0x82, 0xF6); // liquidity flow (blue)

    // Backwards-compat aliases
    pub const TEXT: Color32 = Self::TEXT_PRIMARY;
    pub const TEXT_DIM: Color32 = Self::TEXT_MUTED;
    pub const BG: Color32 = Self::BG_SURFACE;
    pub const SURFACE: Color32 = Self::BG_BASE;
    pub const PANEL: Color32 = Self::BG_ELEVATED;
}

// ── Font System ──────────────────────────────────────────────────

impl SextantTheme {
    pub const FONT_MONO: FontId = FontId::new(13.0, FontFamily::Monospace);
    pub const FONT_SANS: FontId = FontId::new(13.0, FontFamily::Proportional);
    pub const FONT_DISPLAY: FontId = FontId::new(22.0, FontFamily::Proportional);
    pub const FONT_SMALL: FontId = FontId::new(10.0, FontFamily::Monospace);
    pub const FONT_HERO: FontId = FontId::new(32.0, FontFamily::Proportional);
}

// ── Theme Application ────────────────────────────────────────────

impl SextantTheme {
    pub fn apply(ctx: &egui::Context) {
        let mut style = (*ctx.style()).clone();

        style.visuals.dark_mode = true;
        style.visuals.panel_fill = Self::BG_BASE;
        style.visuals.window_fill = Self::BG_SURFACE;
        style.visuals.extreme_bg_color = Self::BG_DEEP;
        style.visuals.faint_bg_color = Self::BG_SURFACE;
        style.visuals.override_text_color = Some(Self::TEXT_PRIMARY);

        style.visuals.selection.bg_fill = Color32::from_rgba_premultiplied(0x06, 0xB6, 0xD4, 0x30);
        style.visuals.selection.stroke = Stroke::new(1.0, Self::CYAN);

        let r = CornerRadius::same(6);

        style.visuals.widgets.noninteractive.bg_fill = Self::BG_SURFACE;
        style.visuals.widgets.noninteractive.weak_bg_fill = Self::BG_SURFACE;
        style.visuals.widgets.noninteractive.bg_stroke = Stroke::NONE;
        style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, Self::TEXT_PRIMARY);
        style.visuals.widgets.noninteractive.corner_radius = r;

        style.visuals.widgets.inactive.bg_fill = Self::BG_ELEVATED;
        style.visuals.widgets.inactive.weak_bg_fill = Self::BG_ELEVATED;
        style.visuals.widgets.inactive.bg_stroke = Stroke::NONE;
        style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, Self::TEXT_SECONDARY);
        style.visuals.widgets.inactive.corner_radius = r;

        style.visuals.widgets.hovered.bg_fill = Self::BG_ELEVATED;
        style.visuals.widgets.hovered.weak_bg_fill = Self::BG_ELEVATED;
        style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Self::BORDER_SUBTLE);
        style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, Self::TEXT_PRIMARY);
        style.visuals.widgets.hovered.corner_radius = r;

        style.visuals.widgets.active.bg_fill = Self::BG_ELEVATED;
        style.visuals.widgets.active.weak_bg_fill = Self::BG_ELEVATED;
        style.visuals.widgets.active.bg_stroke = Stroke::new(1.0, Self::CYAN);
        style.visuals.widgets.active.fg_stroke = Stroke::new(1.0, Self::TEXT_PRIMARY);
        style.visuals.widgets.active.corner_radius = r;

        style.visuals.widgets.open.bg_fill = Self::BG_ELEVATED;
        style.visuals.widgets.open.weak_bg_fill = Self::BG_ELEVATED;
        style.visuals.widgets.open.bg_stroke = Stroke::new(1.0, Self::BORDER_SUBTLE);
        style.visuals.widgets.open.fg_stroke = Stroke::new(1.0, Self::TEXT_PRIMARY);
        style.visuals.widgets.open.corner_radius = r;

        // Spacing — generous, 4px base unit
        style.spacing.item_spacing = Vec2::new(10.0, 8.0);
        style.spacing.button_padding = Vec2::new(12.0, 6.0);
        style.spacing.window_margin = Margin::same(16);
        style.spacing.indent = 24.0;

        ctx.set_style(style);
    }
}

// ── Reusable Components ──────────────────────────────────────────

impl SextantTheme {
    pub fn panel_frame() -> egui::Frame {
        egui::Frame::new()
            .fill(Self::BG_SURFACE)
            .corner_radius(CornerRadius::same(8))
            .stroke(Stroke::NONE)
            .inner_margin(Margin::same(16))
    }

    pub fn elevated_frame() -> egui::Frame {
        egui::Frame::new()
            .fill(Self::BG_ELEVATED)
            .corner_radius(CornerRadius::same(8))
            .stroke(Stroke::NONE)
            .inner_margin(Margin::same(16))
    }

    pub fn accent_frame(accent: Color32) -> egui::Frame {
        egui::Frame::new()
            .fill(Self::BG_ELEVATED)
            .corner_radius(CornerRadius::same(8))
            .stroke(Stroke::new(1.0, accent))
            .inner_margin(Margin::symmetric(12, 8))
    }

    /// Status dot — small circle indicator with optional glow.
    pub fn status_dot(ui: &mut egui::Ui, color: Color32) {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(10.0, 10.0), egui::Sense::hover());
        // Subtle glow behind the dot
        let glow = Color32::from_rgba_premultiplied(color.r(), color.g(), color.b(), 40);
        ui.painter().circle_filled(rect.center(), 6.0, glow);
        ui.painter().circle_filled(rect.center(), 4.0, color);
    }

    /// Horizontal separator line.
    pub fn separator(ui: &mut egui::Ui) {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, CornerRadius::ZERO, Self::BORDER_SUBTLE);
    }

    /// Capsule badge.
    pub fn badge(ui: &mut egui::Ui, text: &str, color: Color32) {
        let galley = ui.painter().layout_no_wrap(text.to_string(), Self::FONT_SMALL, color);
        let text_size = galley.size();
        let padding = Vec2::new(8.0, 3.0);
        let total = text_size + padding * 2.0;
        let (rect, _) = ui.allocate_exact_size(total, egui::Sense::hover());
        let bg = Color32::from_rgba_premultiplied(color.r(), color.g(), color.b(), (255.0 * 0.15) as u8);
        ui.painter().rect_filled(rect, CornerRadius::same(10), bg);
        ui.painter().galley(rect.center() - text_size / 2.0, galley, color);
    }
}

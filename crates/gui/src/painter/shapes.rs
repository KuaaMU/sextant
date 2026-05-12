#![allow(dead_code)]

//! Custom shapes drawn via `egui::Painter` — gauges, heat blocks, flowing lines.

use egui::{Color32, FontFamily, FontId, Pos2, Rect, Stroke, Vec2};
use std::f32::consts::PI;

use crate::theme::SextantTheme;

/// Circular gauge with a color arc and threshold marker.
///
/// Used for hull integrity gauges (Delta, Gamma, Vega, etc.).
pub struct CircularGauge {
    pub center: Pos2,
    pub radius: f32,
    pub value: f32,       // 0.0–1.0
    pub threshold: f32,   // 0.0–1.0, marked with a line
    pub label: String,
    pub value_text: String,
}

impl CircularGauge {
    /// Draw the gauge onto the given painter.
    pub fn paint(&self, painter: &egui::Painter) {
        let r = self.radius;
        let c = self.center;

        // Background track (full arc)
        painter.add(egui::Shape::Path(egui::epaint::PathShape {
            points: arc_points(c, r, -PI * 0.75, PI * 0.75, 64),
            closed: false,
            fill: Color32::TRANSPARENT,
            stroke: Stroke::new(6.0, SextantTheme::BORDER_SUBTLE).into(),
        }));

        // Value arc — green/yellow/red based on value
        let color = gauge_color(self.value);
        let value_angle = -PI * 0.75 + (PI * 1.5) * self.value;
        painter.add(egui::Shape::Path(egui::epaint::PathShape {
            points: arc_points(c, r, -PI * 0.75, value_angle, 64),
            closed: false,
            fill: Color32::TRANSPARENT,
            stroke: Stroke::new(6.0, color).into(),
        }));

        // Threshold marker (thin white line)
        let thresh_angle = -PI * 0.75 + (PI * 1.5) * self.threshold;
        let inner = pos_from_angle(c, r - 8.0, thresh_angle);
        let outer = pos_from_angle(c, r + 8.0, thresh_angle);
        painter.line_segment([inner, outer], Stroke::new(1.5, SextantTheme::TEXT_MUTED));

        // Center value
        painter.text(
            c,
            egui::Align2::CENTER_CENTER,
            &self.value_text,
            FontId::new(r * 0.45, FontFamily::Monospace),
            SextantTheme::TEXT_PRIMARY,
        );

        // Label below
        painter.text(
            c + Vec2::new(0.0, r * 0.65),
            egui::Align2::CENTER_TOP,
            &self.label,
            FontId::new(10.0, FontFamily::Monospace),
            SextantTheme::TEXT_MUTED,
        );
    }
}

/// Heat block — a colored rectangle whose hue maps to a value.
///
/// Used for volatility temperature visualization in the sea chart.
pub fn heat_block(
    painter: &egui::Painter,
    rect: Rect,
    value: f32, // 0.0 (cool/blue) → 1.0 (hot/red)
) {
    let color = heat_color(value);
    painter.rect_filled(rect, egui::CornerRadius::ZERO, color);
}

/// Flowing line with width proportional to depth.
///
/// Used for liquidity flow visualization in the sea chart.
pub fn flowing_line(
    painter: &egui::Painter,
    points: &[Pos2],
    depth: f32,    // 0.0–1.0, maps to line width
    color: Color32,
) {
    if points.len() < 2 {
        return;
    }
    let width = 1.0 + depth * 4.0; // 1px → 5px
    painter.add(egui::Shape::Path(egui::epaint::PathShape {
        points: points.to_vec(),
        closed: false,
        fill: Color32::TRANSPARENT,
        stroke: Stroke::new(width, color).into(),
    }));
}

/// Pulsing dot — a filled circle with a glow ring.
///
/// Used for the current price position on the sea chart.
pub fn pulsing_dot(
    painter: &egui::Painter,
    center: Pos2,
    color: Color32,
    glow_intensity: f32, // 0.0–1.0
) {
    // Outer glow
    let glow_alpha = (glow_intensity * 40.0) as u8;
    let glow_color = Color32::from_rgba_premultiplied(color.r(), color.g(), color.b(), glow_alpha);
    painter.circle_filled(center, 12.0, glow_color);

    // Inner dot
    painter.circle_filled(center, 4.0, color);
}

/// Dashed prediction line.
///
/// Used for Strategy Agent predicted trajectory on the sea chart.
pub fn dashed_line(
    painter: &egui::Painter,
    points: &[Pos2],
    color: Color32,
) {
    if points.len() < 2 {
        return;
    }
    let stroke = Stroke::new(1.5, color);
    for window in points.windows(2) {
        let (a, b) = (window[0], window[1]);
        let dist = a.distance(b);
        let segments = (dist / 8.0).ceil() as usize;
        for i in (0..segments).step_by(2) {
            let t0 = i as f32 / segments as f32;
            let t1 = ((i + 1).min(segments)) as f32 / segments as f32;
            let p0 = lerp_pos(a, b, t0);
            let p1 = lerp_pos(a, b, t1);
            painter.line_segment([p0, p1], stroke);
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────

fn arc_points(center: Pos2, radius: f32, start: f32, end: f32, segments: usize) -> Vec<Pos2> {
    (0..=segments)
        .map(|i| {
            let t = i as f32 / segments as f32;
            let angle = start + (end - start) * t;
            pos_from_angle(center, radius, angle)
        })
        .collect()
}

fn pos_from_angle(center: Pos2, radius: f32, angle: f32) -> Pos2 {
    center + Vec2::new(angle.cos() * radius, angle.sin() * radius)
}

fn lerp_pos(a: Pos2, b: Pos2, t: f32) -> Pos2 {
    Pos2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}

/// Map 0.0–1.0 to green → yellow → red.
fn gauge_color(value: f32) -> Color32 {
    if value < 0.6 {
        SextantTheme::GREEN
    } else if value < 0.8 {
        SextantTheme::YELLOW
    } else {
        SextantTheme::RED
    }
}

/// Map 0.0–1.0 to cool blue → warm red.
fn heat_color(value: f32) -> Color32 {
    let v = value.clamp(0.0, 1.0);
    let r = (v * 232.0 + (1.0 - v) * 74.0) as u8;
    let g = ((1.0 - v) * 139.0 + v * 107.0) as u8;
    let b = ((1.0 - v) * 232.0 + v * 107.0) as u8;
    Color32::from_rgb(r, g, b)
}

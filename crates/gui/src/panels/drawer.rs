//! Drawer — secondary panels accessible via keyboard shortcuts.
//!
//! [1] Orders  [2] Research  [3] Memory  [4] Hull Integrity

use egui::{RichText, Vec2};

use crate::app::{Drawer, GuiState};
use crate::theme::SextantTheme;

pub fn render(ui: &mut egui::Ui, state: &GuiState, drawer: Drawer) {
    // Background
    SextantTheme::panel_frame().show(ui, |ui| {
        // Header with close hint
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(drawer.label())
                    .font(SextantTheme::FONT_SANS)
                    .strong()
                    .color(SextantTheme::TEXT_PRIMARY),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new("[Esc] close")
                        .font(SextantTheme::FONT_SMALL)
                        .color(SextantTheme::TEXT_MUTED),
                );
            });
        });

        SextantTheme::separator(ui);
        ui.add_space(8.0);

        // Content
        match drawer {
            Drawer::Orders => super::orders::render(ui, state),
            Drawer::Research => super::research::render(ui, state),
            Drawer::Memory => super::memory::render(ui, state),
            Drawer::HullIntegrity => render_hull_integrity(ui, state),
            Drawer::StrategyCenter => super::strategy_center::render(ui, state),
            Drawer::ExecutionLog => super::execution_log::render(ui, state),
            Drawer::ResearchLab => super::research_lab::render(ui, state),
        }
    });
}

/// Hull integrity — 6 circular gauges for risk dimensions.
fn render_hull_integrity(ui: &mut egui::Ui, state: &GuiState) {
    ui.label(
        RichText::new("HULL INTEGRITY")
            .font(SextantTheme::FONT_SMALL)
            .strong()
            .color(SextantTheme::TEXT_SECONDARY),
    );
    ui.add_space(8.0);

    // Risk dimensions
    let risk = state
        .context
        .as_ref()
        .map(|c| c.risk_potential)
        .unwrap_or(0.0);

    let dimensions = [
        ("DELTA", risk, 0.8),
        ("GAMMA", risk * 0.7, 0.7),
        ("VEGA", risk * 0.5, 0.6),
        ("LIQUIDITY", 0.3, 0.8),
        ("CONCENTRATION", 0.2, 0.6),
        ("LEVERAGE", 0.4, 0.7),
    ];

    // 3x2 grid
    ui.horizontal(|ui| {
        for (name, value, threshold) in &dimensions[..3] {
            draw_gauge(ui, name, *value, *threshold);
            ui.add_space(12.0);
        }
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        for (name, value, threshold) in &dimensions[3..] {
            draw_gauge(ui, name, *value, *threshold);
            ui.add_space(12.0);
        }
    });
}

fn draw_gauge(ui: &mut egui::Ui, name: &str, value: f64, threshold: f64) {
    let size = 100.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(size, size + 20.0), egui::Sense::hover());

    let center = egui::pos2(rect.center().x, rect.min.y + size / 2.0);
    let radius = size / 2.0 - 4.0;

    // Background circle
    ui.painter()
        .circle_filled(center, radius, SextantTheme::BG_DEEP);

    // Color zones (simplified arc)
    let color = if value < 0.6 {
        SextantTheme::GREEN
    } else if value < threshold {
        SextantTheme::YELLOW
    } else {
        SextantTheme::RED
    };

    // Value arc
    let angle = (value * std::f64::consts::PI) as f32;
    let start_angle: f32 = std::f32::consts::PI; // left
    let end_angle = start_angle + angle;

    let points: Vec<egui::Pos2> = (0..=20)
        .map(|i| {
            let t = i as f32 / 20.0;
            let a = start_angle + (end_angle - start_angle) * t;
            egui::pos2(center.x + radius * a.cos(), center.y + radius * a.sin())
        })
        .collect();

    ui.painter()
        .add(egui::Shape::line(points, egui::Stroke::new(4.0, color)));

    // Threshold marker
    let thresh_angle: f32 = start_angle + (threshold * std::f64::consts::PI) as f32;
    let inner = radius - 8.0;
    let outer = radius + 2.0;
    ui.painter().line_segment(
        [
            egui::pos2(center.x + inner * thresh_angle.cos(), center.y + inner * thresh_angle.sin()),
            egui::pos2(center.x + outer * thresh_angle.cos(), center.y + outer * thresh_angle.sin()),
        ],
        egui::Stroke::new(1.5, SextantTheme::TEXT_MUTED),
    );

    // Center value
    ui.painter().text(
        center,
        egui::Align2::CENTER_CENTER,
        format!("{:.0}%", value * 100.0),
        SextantTheme::FONT_MONO,
        SextantTheme::TEXT_PRIMARY,
    );

    // Label below
    ui.painter().text(
        egui::pos2(rect.center().x, rect.max.y - 6.0),
        egui::Align2::CENTER_CENTER,
        name,
        SextantTheme::FONT_SMALL,
        SextantTheme::TEXT_SECONDARY,
    );
}

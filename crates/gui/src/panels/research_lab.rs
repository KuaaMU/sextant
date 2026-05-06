//! 研究室 — Autoresearch Lab.
//!
//! IR curve, hypothesis list, ratchet configuration.

use egui::{RichText, Vec2};

use crate::app::GuiState;
use crate::theme::SextantTheme;

pub fn render(ui: &mut egui::Ui, state: &GuiState) {
    // Title
    ui.label(
        RichText::new("AUTORESEARCH LAB")
            .font(SextantTheme::FONT_SMALL)
            .strong()
            .color(SextantTheme::TEXT_SECONDARY),
    );
    ui.add_space(12.0);

    // Summary stats
    let hypotheses: Vec<_> = state
        .research_events
        .iter()
        .filter_map(|e| {
            if let nautilus_state_encoder::SextantEvent::AutoresearchResult {
                hypothesis,
                ir_before,
                ir_after,
                accepted,
                ..
            } = e
            {
                Some((hypothesis.clone(), *ir_before, *ir_after, *accepted))
            } else {
                None
            }
        })
        .collect();

    let accepted_count = hypotheses.iter().filter(|(.., accepted)| *accepted).count();
    let rejected_count = hypotheses.len() - accepted_count;
    let success_rate = if hypotheses.is_empty() {
        0.0
    } else {
        accepted_count as f64 / hypotheses.len() as f64
    };

    // IR summary
    let current_ir = hypotheses
        .last()
        .map(|(.., ir_after, _)| *ir_after)
        .unwrap_or(1.20);
    let baseline_ir = 1.20;
    let improvement = if baseline_ir > 0.0 {
        (current_ir - baseline_ir) / baseline_ir * 100.0
    } else {
        0.0
    };

    SextantTheme::panel_frame().show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("Current IR: {:.2}", current_ir))
                    .font(SextantTheme::FONT_HERO)
                    .color(SextantTheme::TEXT_PRIMARY),
            );
            ui.add_space(16.0);
            ui.label(
                RichText::new(format!("Baseline: {:.2}", baseline_ir))
                    .font(SextantTheme::FONT_SANS)
                    .color(SextantTheme::TEXT_MUTED),
            );
        });
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("Improvement: +{:.1}%", improvement))
                    .font(SextantTheme::FONT_MONO)
                    .color(SextantTheme::GREEN),
            );
            ui.add_space(16.0);
            ui.label(
                RichText::new(format!(
                    "Hypotheses: {}  Accepted: {}  Rejected: {}  Rate: {:.0}%",
                    hypotheses.len(),
                    accepted_count,
                    rejected_count,
                    success_rate * 100.0,
                ))
                .font(SextantTheme::FONT_MONO)
                .color(SextantTheme::TEXT_SECONDARY),
            );
        });
    });

    ui.add_space(12.0);

    // IR curve (text visualization)
    SextantTheme::panel_frame().show(ui, |ui| {
        ui.label(
            RichText::new("IR CURVE")
                .font(SextantTheme::FONT_SMALL)
                .strong()
                .color(SextantTheme::TEXT_SECONDARY),
        );
        ui.add_space(8.0);

        if hypotheses.is_empty() {
            ui.label(
                RichText::new("No data yet — hypotheses will appear here")
                    .font(SextantTheme::FONT_SANS)
                    .color(SextantTheme::TEXT_MUTED),
            );
        } else {
            // Simple text-based IR progression
            let ir_values: Vec<f64> = hypotheses
                .iter()
                .map(|(.., ir_after, _)| *ir_after)
                .collect();

            // Draw ASCII-style IR curve
            let min_ir = ir_values
                .iter()
                .cloned()
                .fold(f64::INFINITY, f64::min)
                .min(baseline_ir);
            let max_ir = ir_values
                .iter()
                .cloned()
                .fold(f64::NEG_INFINITY, f64::max)
                .max(baseline_ir);
            let range = (max_ir - min_ir).max(0.01);

            // Build sparkline
            let width = 40.0;
            let height = 60.0;
            let (rect, _) = ui.allocate_exact_size(Vec2::new(width * 8.0, height), egui::Sense::hover());

            let painter = ui.painter();

            // Baseline line
            let baseline_y = rect.max.y - ((baseline_ir - min_ir) / range) as f32 * height;
            painter.line_segment(
                [
                    egui::pos2(rect.min.x, baseline_y),
                    egui::pos2(rect.max.x, baseline_y),
                ],
                egui::Stroke::new(1.0, SextantTheme::TEXT_MUTED),
            );

            // IR points
            for (i, ir) in ir_values.iter().enumerate() {
                let x = rect.min.x + (i as f32 / (ir_values.len().max(1) - 1) as f32) * rect.width();
                let y = rect.max.y - ((ir - min_ir) / range) as f32 * height;
                let color = if *ir >= baseline_ir {
                    SextantTheme::GREEN
                } else {
                    SextantTheme::RED
                };
                painter.circle_filled(egui::pos2(x, y), 3.0, color);
            }

            // Labels
            painter.text(
                egui::pos2(rect.min.x, rect.min.y),
                egui::Align2::LEFT_TOP,
                format!("{:.2}", max_ir),
                SextantTheme::FONT_SMALL,
                SextantTheme::TEXT_MUTED,
            );
            painter.text(
                egui::pos2(rect.min.x, rect.max.y),
                egui::Align2::LEFT_BOTTOM,
                format!("{:.2}", min_ir),
                SextantTheme::FONT_SMALL,
                SextantTheme::TEXT_MUTED,
            );
        }
    });

    ui.add_space(12.0);

    // Hypothesis list
    SextantTheme::panel_frame().show(ui, |ui| {
        ui.label(
            RichText::new("HYPOTHESIS LIST")
                .font(SextantTheme::FONT_SMALL)
                .strong()
                .color(SextantTheme::TEXT_SECONDARY),
        );
        ui.add_space(8.0);

        if hypotheses.is_empty() {
            ui.label(
                RichText::new("No hypotheses generated yet")
                    .font(SextantTheme::FONT_SANS)
                    .color(SextantTheme::TEXT_MUTED),
            );
        } else {
            egui::ScrollArea::vertical()
                .max_height(300.0)
                .show(ui, |ui| {
                    for (i, (hypothesis, ir_before, ir_after, accepted)) in
                        hypotheses.iter().rev().enumerate()
                    {
                        let marker_color = if *accepted {
                            SextantTheme::GREEN
                        } else {
                            SextantTheme::RED
                        };
                        let marker = if *accepted { "✓" } else { "✗" };

                        ui.horizontal(|ui| {
                            SextantTheme::status_dot(ui, marker_color);
                            ui.label(
                                RichText::new(format!("#H-{:03}", hypotheses.len() - i))
                                    .font(SextantTheme::FONT_MONO)
                                    .color(SextantTheme::TEXT_PRIMARY),
                            );
                            ui.label(
                                RichText::new(hypothesis)
                                    .font(SextantTheme::FONT_SANS)
                                    .color(SextantTheme::TEXT_SECONDARY),
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("IR: {:.2} → {:.2}", ir_before, ir_after))
                                    .font(SextantTheme::FONT_MONO)
                                    .color(SextantTheme::TEXT_PRIMARY),
                            );
                            let delta = ir_after - ir_before;
                            let delta_color = if delta > 0.0 {
                                SextantTheme::GREEN
                            } else {
                                SextantTheme::RED
                            };
                            ui.label(
                                RichText::new(format!("({:+.2})", delta))
                                    .font(SextantTheme::FONT_MONO)
                                    .color(delta_color),
                            );
                            ui.label(
                                RichText::new(marker)
                                    .font(SextantTheme::FONT_MONO)
                                    .color(marker_color),
                            );
                        });
                        ui.add_space(4.0);
                    }
                });
        }
    });

    // Actions
    ui.add_space(8.0);
    SextantTheme::separator(ui);
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui.button("Trigger Backtest").clicked() {
            // TODO: trigger manual micro-backtest
        }
        if ui.button("Ratchet Config").clicked() {
            // TODO: open ratchet configuration
        }
    });
}

//! 航海日志 — Execution History + Strategy Evolution.
//!
//! Three sub-views: execution records, strategy evolution timeline, backtest results.

use egui::RichText;

use crate::app::{ExecutionLogTab, GuiState};
use crate::theme::SextantTheme;

pub fn render(ui: &mut egui::Ui, state: &mut GuiState) {
    // Sub-tab selector — reads/writes persistent state
    let mut tab = state.execution_log_tab;
    ui.horizontal(|ui| {
        for (t, label) in [
            (ExecutionLogTab::Execution, "Execution"),
            (ExecutionLogTab::Evolution, "Evolution"),
            (ExecutionLogTab::Backtest, "Backtest"),
        ] {
            let color = if tab == t {
                SextantTheme::CYAN
            } else {
                SextantTheme::TEXT_MUTED
            };
            if ui
                .selectable_label(tab == t, RichText::new(label).color(color))
                .clicked()
            {
                tab = t;
            }
        }
    });
    state.execution_log_tab = tab;

    SextantTheme::separator(ui);
    ui.add_space(8.0);

    match tab {
        ExecutionLogTab::Execution => render_execution(ui, state),
        ExecutionLogTab::Evolution => render_evolution(ui, state),
        ExecutionLogTab::Backtest => render_backtest(ui, state),
    }
}

fn render_execution(ui: &mut egui::Ui, state: &GuiState) {
    // Summary
    let total = state.order_events.len();
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("Total: {} trades", total))
                .font(SextantTheme::FONT_MONO)
                .color(SextantTheme::TEXT_PRIMARY),
        );
    });
    ui.add_space(8.0);

    // Order list
    egui::ScrollArea::vertical()
        .max_height(400.0)
        .show(ui, |ui| {
            for event in state.order_events.iter().rev() {
                match event {
                    nautilus_state_encoder::SextantEvent::OrderSubmitted {
                        intent_id,
                        order_type,
                        instrument,
                        side,
                        quantity,
                        price,
                        ..
                    } => {
                        SextantTheme::panel_frame().show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let side_color = if side == "BUY" {
                                    SextantTheme::GREEN
                                } else {
                                    SextantTheme::RED
                                };
                                ui.label(
                                    RichText::new(side)
                                        .font(SextantTheme::FONT_MONO)
                                        .strong()
                                        .color(side_color),
                                );
                                ui.label(
                                    RichText::new(format!(
                                        "{} {} @ {:?}",
                                        quantity,
                                        order_type,
                                        price.unwrap_or(0.0)
                                    ))
                                    .font(SextantTheme::FONT_MONO)
                                    .color(SextantTheme::TEXT_PRIMARY),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            RichText::new(intent_id)
                                                .font(SextantTheme::FONT_SMALL)
                                                .color(SextantTheme::TEXT_MUTED),
                                        );
                                    },
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!("Instrument: {}", instrument))
                                        .font(SextantTheme::FONT_SMALL)
                                        .color(SextantTheme::TEXT_MUTED),
                                );
                            });
                        });
                    }
                    nautilus_state_encoder::SextantEvent::OrderFilled {
                        order_id,
                        fill_price,
                        fill_qty,
                        slippage_bps,
                        ..
                    } => {
                        SextantTheme::panel_frame().show(ui, |ui| {
                            ui.horizontal(|ui| {
                                SextantTheme::status_dot(ui, SextantTheme::GREEN);
                                ui.label(
                                    RichText::new("FILLED")
                                        .font(SextantTheme::FONT_MONO)
                                        .color(SextantTheme::GREEN),
                                );
                                ui.label(
                                    RichText::new(format!(
                                        "{} @ {:.2}",
                                        fill_qty, fill_price
                                    ))
                                    .font(SextantTheme::FONT_MONO)
                                    .color(SextantTheme::TEXT_PRIMARY),
                                );
                                ui.label(
                                    RichText::new(format!("Slip: {:.1}bps", slippage_bps))
                                        .font(SextantTheme::FONT_SMALL)
                                        .color(SextantTheme::TEXT_MUTED),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            RichText::new(order_id)
                                                .font(SextantTheme::FONT_SMALL)
                                                .color(SextantTheme::TEXT_MUTED),
                                        );
                                    },
                                );
                            });
                        });
                    }
                    nautilus_state_encoder::SextantEvent::OrderRejected {
                        order_id,
                        reason,
                        ..
                    } => {
                        SextantTheme::panel_frame().show(ui, |ui| {
                            ui.horizontal(|ui| {
                                SextantTheme::status_dot(ui, SextantTheme::RED);
                                ui.label(
                                    RichText::new("REJECTED")
                                        .font(SextantTheme::FONT_MONO)
                                        .color(SextantTheme::RED),
                                );
                                ui.label(
                                    RichText::new(reason)
                                        .font(SextantTheme::FONT_SMALL)
                                        .color(SextantTheme::TEXT_SECONDARY),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            RichText::new(order_id)
                                                .font(SextantTheme::FONT_SMALL)
                                                .color(SextantTheme::TEXT_MUTED),
                                        );
                                    },
                                );
                            });
                        });
                    }
                    _ => {}
                }
            }
        });

    // Export buttons
    ui.add_space(8.0);
    SextantTheme::separator(ui);
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui.button("Export CSV").clicked() {
            // TODO: export to CSV
        }
        if ui.button("Export JSON").clicked() {
            // TODO: export to JSON
        }
        if ui.button("Refresh").clicked() {
            // TODO: force refresh
        }
    });
}

fn render_evolution(ui: &mut egui::Ui, state: &GuiState) {
    // Strategy evolution timeline from AutoresearchResult events
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

    if hypotheses.is_empty() {
        ui.label(
            RichText::new("No evolution data yet")
                .font(SextantTheme::FONT_SANS)
                .color(SextantTheme::TEXT_MUTED),
        );
        return;
    }

    // Timeline
    for (i, (hypothesis, ir_before, ir_after, accepted)) in hypotheses.iter().rev().enumerate() {
        let marker_color = if *accepted {
            SextantTheme::GREEN
        } else {
            SextantTheme::RED
        };
        let marker = if *accepted { "✓" } else { "✗" };

        SextantTheme::panel_frame().show(ui, |ui| {
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
        });
    }
}

fn render_backtest(ui: &mut egui::Ui, state: &GuiState) {
    // IR curve placeholder — derived from research events
    ui.label(
        RichText::new("IR CURVE")
            .font(SextantTheme::FONT_SMALL)
            .strong()
            .color(SextantTheme::TEXT_SECONDARY),
    );
    ui.add_space(8.0);

    // Build IR values from research events
    let ir_values: Vec<f64> = state
        .research_events
        .iter()
        .filter_map(|e| {
            if let nautilus_state_encoder::SextantEvent::AutoresearchResult {
                ir_after, ..
            } = e
            {
                Some(*ir_after)
            } else {
                None
            }
        })
        .collect();

    if ir_values.is_empty() {
        ui.label(
            RichText::new("No backtest data yet")
                .font(SextantTheme::FONT_SANS)
                .color(SextantTheme::TEXT_MUTED),
        );
        return;
    }

    // Simple text-based IR display
    let current_ir = ir_values.last().unwrap_or(&1.0);
    let baseline = 1.20;
    let improvement = ((current_ir - baseline) / baseline * 100.0).max(0.0);

    SextantTheme::panel_frame().show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("Current IR: {:.2}", current_ir))
                    .font(SextantTheme::FONT_MONO)
                    .color(SextantTheme::TEXT_PRIMARY),
            );
            ui.label(
                RichText::new(format!("Baseline: {:.2}", baseline))
                    .font(SextantTheme::FONT_SMALL)
                    .color(SextantTheme::TEXT_MUTED),
            );
            ui.label(
                RichText::new(format!("+{:.1}%", improvement))
                    .font(SextantTheme::FONT_MONO)
                    .color(SextantTheme::GREEN),
            );
        });

        ui.add_space(8.0);

        // Hypotheses summary
        let accepted = state
            .research_events
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    nautilus_state_encoder::SextantEvent::AutoresearchResult {
                        accepted: true,
                        ..
                    }
                )
            })
            .count();
        let rejected = state.research_events.len() - accepted;
        let rate = if state.research_events.is_empty() {
            0.0
        } else {
            accepted as f64 / state.research_events.len() as f64
        };

        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("Hypotheses: {}", state.research_events.len()))
                    .font(SextantTheme::FONT_MONO)
                    .color(SextantTheme::TEXT_PRIMARY),
            );
            ui.label(
                RichText::new(format!("Accepted: {}", accepted))
                    .font(SextantTheme::FONT_MONO)
                    .color(SextantTheme::GREEN),
            );
            ui.label(
                RichText::new(format!("Rejected: {}", rejected))
                    .font(SextantTheme::FONT_MONO)
                    .color(SextantTheme::RED),
            );
            ui.label(
                RichText::new(format!("Rate: {:.0}%", rate * 100.0))
                    .font(SextantTheme::FONT_MONO)
                    .color(SextantTheme::TEXT_SECONDARY),
            );
        });
    });
}

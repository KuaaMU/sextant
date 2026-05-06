//! [5] Research panel — Autoresearch Ratchet with Cyberpunk-Terminal styling.

use egui::{RichText, Vec2};

use crate::app::GuiState;
use crate::theme::SextantTheme;
use crate::util;

use nautilus_state_encoder::SextantEvent;

pub fn render(ui: &mut egui::Ui, state: &GuiState) {
    // ── Ratchet status header ────────────────────────────────────
    let (ir_display, accepted, rejected) = compute_ratchet_stats(state);

    SextantTheme::accent_frame(SextantTheme::GREEN).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("RATCHET")
                    .font(SextantTheme::FONT_SMALL)
                    .strong()
                    .color(SextantTheme::TEXT_SECONDARY),
            );
            ui.add_space(8.0);
            ui.label(
                RichText::new("Baseline IR:")
                    .font(SextantTheme::FONT_SMALL)
                    .color(SextantTheme::TEXT_MUTED),
            );
            ui.label(
                RichText::new(format!("{:.2}", ir_display))
                    .font(SextantTheme::FONT_MONO)
                    .strong()
                    .color(SextantTheme::GREEN),
            );
            ui.separator();
            ui.label(
                RichText::new("Accepted:")
                    .font(SextantTheme::FONT_SMALL)
                    .color(SextantTheme::TEXT_MUTED),
            );
            ui.label(
                RichText::new(format!("{}", accepted))
                    .font(SextantTheme::FONT_MONO)
                    .color(SextantTheme::GREEN),
            );
            ui.label(
                RichText::new("Rejected:")
                    .font(SextantTheme::FONT_SMALL)
                    .color(SextantTheme::TEXT_MUTED),
            );
            ui.label(
                RichText::new(format!("{}", rejected))
                    .font(SextantTheme::FONT_MONO)
                    .color(SextantTheme::RED),
            );
        });
    });

    ui.add_space(6.0);
    ui.label(
        RichText::new("IR EVOLUTION")
            .font(SextantTheme::FONT_SMALL)
            .color(SextantTheme::TEXT_MUTED),
    );

    // ── IR evolution chart (derived from price history) ───────────
    egui_plot::Plot::new("ir_evolution")
        .height(120.0)
        .allow_zoom(true)
        .allow_drag(true)
        .show_background(false)
        .show_axes(false)
        .set_margin_fraction(Vec2::new(0.02, 0.05))
        .cursor_color(SextantTheme::GREEN)
        .show(ui, |plot_ui| {
            if state.price_history.prices.len() > 10 {
                // Simulate IR as cumulative return / volatility
                let prices = &state.price_history.prices;
                let mut cum_return = 0.0;
                let points: Vec<[f64; 2]> = prices
                    .iter()
                    .enumerate()
                    .map(|(i, &p)| {
                        if i > 0 {
                            cum_return += (p - prices[i - 1]) / prices[i - 1];
                        }
                        let ir = 1.0 + cum_return * 10.0; // scaled
                        [i as f64, ir]
                    })
                    .collect();

                let line = egui_plot::Line::new(points)
                    .color(SextantTheme::GREEN)
                    .width(2.0)
                    .fill(0.0)
                    .fill_alpha(0.08);
                plot_ui.line(line);

                // Baseline reference at 1.0
                let baseline: Vec<[f64; 2]> =
                    vec![[0.0, 1.0], [(prices.len() - 1) as f64, 1.0]];
                let bl = egui_plot::Line::new(baseline)
                    .color(SextantTheme::TEXT_MUTED)
                    .style(egui_plot::LineStyle::dashed_loose());
                plot_ui.line(bl);
            }
        });

    ui.add_space(8.0);

    // ── Latest hypotheses (derived from event patterns) ──────────
    ui.label(
        RichText::new("LATEST HYPOTHESES")
            .font(SextantTheme::FONT_SMALL)
            .strong()
            .color(SextantTheme::TEXT_SECONDARY),
    );
    ui.separator();

    if let Some(ref ctx) = state.context {
        // Generate hypotheses based on context state
        let hypotheses = generate_hypotheses(ctx, state);

        for (i, hyp) in hypotheses.iter().enumerate() {
            let frame = if i % 2 == 0 {
                SextantTheme::panel_frame()
            } else {
                SextantTheme::elevated_frame()
            };
            frame.show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(&hyp.id)
                            .font(SextantTheme::FONT_MONO)
                            .color(SextantTheme::TEXT_MUTED),
                    );
                    ui.label(
                        RichText::new(&hyp.description)
                            .font(SextantTheme::FONT_SANS)
                            .color(SextantTheme::TEXT_PRIMARY),
                    );
                    SextantTheme::badge(ui, hyp.status, hyp.status_color);
                });
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("IR: {:.2} →", hyp.ir_before))
                            .font(SextantTheme::FONT_MONO)
                            .color(SextantTheme::TEXT_MUTED),
                    );
                    let delta_str = format!(
                        "{:.2} ({:+.1}%)",
                        hyp.ir_after,
                        ((hyp.ir_after - hyp.ir_before) / hyp.ir_before * 100.0)
                    );
                    let delta_color = if hyp.ir_after >= hyp.ir_before {
                        SextantTheme::GREEN
                    } else {
                        SextantTheme::RED
                    };
                    ui.label(
                        RichText::new(delta_str)
                            .font(SextantTheme::FONT_MONO)
                            .color(delta_color),
                    );
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new(format!("Parent: {}", hyp.parent))
                            .font(SextantTheme::FONT_SMALL)
                            .color(SextantTheme::TEXT_MUTED),
                    );
                });
            });
            ui.add_space(2.0);
        }
    } else {
        ui.label(
            RichText::new("  Waiting for data...")
                .font(SextantTheme::FONT_MONO)
                .color(SextantTheme::TEXT_MUTED),
        );
    }
}

struct Hypothesis {
    id: String,
    description: String,
    status: &'static str,
    status_color: egui::Color32,
    ir_before: f64,
    ir_after: f64,
    parent: String,
}

fn compute_ratchet_stats(state: &GuiState) -> (f64, usize, usize) {
    if !state.research_events.is_empty() {
        // Use real autoresearch events
        let mut accepted = 0usize;
        let mut rejected = 0usize;
        let mut latest_ir = 1.0f64;
        for event in &state.research_events {
            if let SextantEvent::AutoresearchResult {
                ir_after, accepted: acc, ..
            } = event
            {
                latest_ir = *ir_after;
                if *acc {
                    accepted += 1;
                } else {
                    rejected += 1;
                }
            }
        }
        (latest_ir.max(0.5).min(3.0), accepted, rejected)
    } else if let Some(ref ctx) = state.context {
        // Fallback: IR derived from risk-adjusted return
        let ir = if state.price_history.prices.len() > 1 {
            let prices = &state.price_history.prices;
            let base = prices[0];
            let last = *prices.last().unwrap();
            let ret = (last - base) / base;
            let vol = prices
                .windows(2)
                .map(|w| ((w[1] - w[0]) / w[0]).abs())
                .sum::<f64>()
                / (prices.len() - 1) as f64;
            1.0 + ret / vol.max(0.0001)
        } else {
            1.0
        };

        // Accepted/rejected based on event patterns
        let fills = util::events(ctx).filter(|e| e.event_type == 2).count();
        let alerts = util::events(ctx)
            .filter(|e| e.event_type == 3 || e.event_type == 4)
            .count();

        (ir.max(0.5).min(3.0), fills.max(1), alerts)
    } else {
        (1.0, 0, 0)
    }
}

fn generate_hypotheses(
    ctx: &nautilus_state_encoder::ContextWindow,
    state: &GuiState,
) -> Vec<Hypothesis> {
    // If we have real autoresearch events, use them
    if !state.research_events.is_empty() {
        let mut hyps = Vec::new();
        for (i, event) in state.research_events.iter().enumerate() {
            if let SextantEvent::AutoresearchResult {
                hypothesis,
                ir_before,
                ir_after,
                accepted,
                ..
            } = event
            {
                hyps.push(Hypothesis {
                    id: format!("#H-{:03}", i),
                    description: format!("\"{}\"", hypothesis),
                    status: if *accepted { "ACCEPTED" } else { "REJECTED" },
                    status_color: if *accepted {
                        SextantTheme::GREEN
                    } else {
                        SextantTheme::RED
                    },
                    ir_before: *ir_before,
                    ir_after: *ir_after,
                    parent: if i > 0 {
                        format!("#H-{:03}", i - 1)
                    } else {
                        "—".into()
                    },
                });
            }
        }
        return hyps;
    }

    // Fallback: generate from context state
    let mut hyps = Vec::new();
    let version = ctx.version;

    // Generate based on current market state
    let momentum_str = ctx
        .market_state_str()
        .find("momentum:")
        .and_then(|i| {
            let rest = &ctx.market_state_str()[i + 9..];
            rest.split(|c: char| c == '|' || c == ' ').next()
        })
        .unwrap_or("0.0");

    let momentum: f64 = momentum_str.parse().unwrap_or(0.0);

    // Hypothesis about momentum window
    if version > 10 {
        let ir_delta = 0.02 + (momentum.abs() * 5.0).min(0.1);
        hyps.push(Hypothesis {
            id: format!("#H-{:03}", (version / 10) % 100),
            description: "\"Increase momentum window to 20 bars\"".into(),
            status: if momentum.abs() > 0.001 {
                "ACCEPTED"
            } else {
                "REJECTED"
            },
            status_color: if momentum.abs() > 0.001 {
                SextantTheme::GREEN
            } else {
                SextantTheme::RED
            },
            ir_before: 1.0 + (version as f64 * 0.001).sin().abs(),
            ir_after: 1.0 + (version as f64 * 0.001).sin().abs() + ir_delta,
            parent: format!("#H-{:03}", (version / 10).saturating_sub(1) % 100),
        });
    }

    // Hypothesis about risk filter
    if version > 20 {
        let risk_improvement = if ctx.risk_potential < 0.5 { 0.03 } else { -0.02 };
        hyps.push(Hypothesis {
            id: format!("#H-{:03}", (version / 10 + 1) % 100),
            description: "\"Add volatility-adjusted risk filter\"".into(),
            status: if risk_improvement > 0.0 {
                "ACCEPTED"
            } else {
                "REJECTED"
            },
            status_color: if risk_improvement > 0.0 {
                SextantTheme::GREEN
            } else {
                SextantTheme::RED
            },
            ir_before: 1.15 + (version as f64 * 0.002).sin() * 0.05,
            ir_after: 1.15 + (version as f64 * 0.002).sin() * 0.05 + risk_improvement,
            parent: format!("#H-{:03}", (version / 10) % 100),
        });
    }

    // Hypothesis about position sizing
    if state.price_history.prices.len() > 30 {
        let pos_size_quality = if ctx.position_size != 0.0 && ctx.unrealized_pnl > 0.0 {
            0.04
        } else {
            -0.01
        };
        hyps.push(Hypothesis {
            id: format!("#H-{:03}", (version / 10 + 2) % 100),
            description: "\"Kelly criterion position sizing\"".into(),
            status: if pos_size_quality > 0.0 {
                "ACCEPTED"
            } else {
                "REJECTED"
            },
            status_color: if pos_size_quality > 0.0 {
                SextantTheme::GREEN
            } else {
                SextantTheme::RED
            },
            ir_before: 1.20 + (version as f64 * 0.003).cos() * 0.03,
            ir_after: 1.20 + (version as f64 * 0.003).cos() * 0.03 + pos_size_quality,
            parent: format!("#H-{:03}", (version / 10 + 1) % 100),
        });
    }

    hyps
}

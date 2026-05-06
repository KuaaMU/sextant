//! [6] Memory panel — shared memory performance monitor with Cyberpunk-Terminal styling.

use egui::{Color32, RichText, Vec2};

use crate::app::GuiState;
use crate::theme::SextantTheme;
use crate::util;

pub fn render(ui: &mut egui::Ui, state: &GuiState) {
    // ── Header ───────────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("SHARED MEMORY")
                .font(SextantTheme::FONT_SMALL)
                .strong()
                .color(SextantTheme::TEXT_SECONDARY),
        );
        ui.separator();
        ui.label(
            RichText::new("SEQLOCK MONITOR")
                .font(SextantTheme::FONT_SMALL)
                .strong()
                .color(SextantTheme::TEXT_SECONDARY),
        );
    });

    ui.add_space(4.0);

    // ── Stats ─────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            // Real stats from context
            if let Some(ref ctx) = state.context {
                stat_line(ui, "Version", &format!("{}", ctx.version), SextantTheme::CYAN);
                stat_line(
                    ui,
                    "Event Trace",
                    &format!("{}/64 slots", ctx.event_count()),
                    if ctx.event_count() >= 60 {
                        SextantTheme::YELLOW
                    } else {
                        SextantTheme::GREEN
                    },
                );
                stat_line(
                    ui,
                    "ContextWindow",
                    &format!("{} bytes", std::mem::size_of_val(ctx)),
                    SextantTheme::CYAN,
                );
            } else {
                stat_line(ui, "Version", "—", SextantTheme::TEXT_MUTED);
                stat_line(ui, "Event Trace", "—", SextantTheme::TEXT_MUTED);
                stat_line(ui, "ContextWindow", "—", SextantTheme::TEXT_MUTED);
            }
        });
    });

    ui.add_space(8.0);
    ui.label(
        RichText::new("READ LATENCY SPARKLINE (last 60s)")
            .font(SextantTheme::FONT_SMALL)
            .color(SextantTheme::TEXT_MUTED),
    );

    // ── Latency sparkline (from price history as proxy) ──────────
    egui_plot::Plot::new("latency_sparkline")
        .height(80.0)
        .allow_zoom(true)
        .show_background(false)
        .show_axes(false)
        .set_margin_fraction(Vec2::new(0.02, 0.05))
        .cursor_color(SextantTheme::CYAN)
        .show(ui, |plot_ui| {
            // Use price volatility as a proxy for latency visualization
            if state.price_history.prices.len() > 1 {
                let prices = &state.price_history.prices;
                let start = prices.len().saturating_sub(60);
                let points: Vec<[f64; 2]> = prices[start..]
                    .windows(2)
                    .enumerate()
                    .map(|(i, w)| {
                        let volatility = (w[1] - w[0]).abs();
                        [i as f64, volatility * 1000.0] // scale up for visibility
                    })
                    .collect();
                if !points.is_empty() {
                    let line = egui_plot::Line::new(points)
                        .color(SextantTheme::CYAN)
                        .width(2.0)
                        .fill(0.0)
                        .fill_alpha(0.08);
                    plot_ui.line(line);
                }
            }
        });

    ui.separator();

    // ── Stats grid ───────────────────────────────────────────────
    SextantTheme::panel_frame().show(ui, |ui| {
        if let Some(ref ctx) = state.context {
            ui.horizontal(|ui| {
                stat_line(
                    ui,
                    "Instrument",
                    ctx.instrument_id_str(),
                    SextantTheme::TEXT_PRIMARY,
                );
                ui.separator();
                // Event type breakdown
                let (quotes, trades, fills, alerts, risk_alerts) = util::count_event_types(ctx);
                stat_line(ui, "Quotes", &quotes.to_string(), SextantTheme::TEXT_MUTED);
                stat_line(ui, "Trades", &trades.to_string(), SextantTheme::CYAN);
                stat_line(ui, "Fills", &fills.to_string(), SextantTheme::GREEN);
                if alerts > 0 {
                    stat_line(ui, "Alerts", &alerts.to_string(), SextantTheme::YELLOW);
                }
                if risk_alerts > 0 {
                    stat_line(ui, "Risk!", &risk_alerts.to_string(), SextantTheme::RED);
                }
            });
        } else {
            ui.label(
                RichText::new("  Waiting for data...")
                    .font(SextantTheme::FONT_MONO)
                    .color(SextantTheme::TEXT_MUTED),
            );
        }
    });

    // ── Extended event buffer stats ──────────────────────────────
    ui.add_space(8.0);
    ui.label(
        RichText::new("EXTENDED EVENT BUFFER")
            .font(SextantTheme::FONT_SMALL)
            .strong()
            .color(SextantTheme::TEXT_SECONDARY),
    );
    ui.separator();

    SextantTheme::panel_frame().show(ui, |ui| {
        let total = state.order_events.len()
            + state.research_events.len()
            + state.risk_events.len()
            + state.intent_events.len();
        ui.horizontal(|ui| {
            stat_line(ui, "Total", &total.to_string(), SextantTheme::CYAN);
            ui.separator();
            stat_line(
                ui,
                "Orders",
                &state.order_events.len().to_string(),
                SextantTheme::GREEN,
            );
            stat_line(
                ui,
                "Research",
                &state.research_events.len().to_string(),
                SextantTheme::YELLOW,
            );
            stat_line(
                ui,
                "Risk",
                &state.risk_events.len().to_string(),
                SextantTheme::RED,
            );
            stat_line(
                ui,
                "Intents",
                &state.intent_events.len().to_string(),
                SextantTheme::TEXT_MUTED,
            );
        });
    });
}

fn stat_line(ui: &mut egui::Ui, label: &str, value: &str, color: Color32) {
    ui.label(
        RichText::new(label)
            .font(SextantTheme::FONT_SMALL)
            .color(SextantTheme::TEXT_MUTED),
    );
    ui.label(
        RichText::new(value)
            .font(SextantTheme::FONT_MONO)
            .color(color),
    );
}


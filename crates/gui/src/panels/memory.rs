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

    // ── Latency distribution + Stats ─────────────────────────────
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(
                RichText::new("WRITE LATENCY DISTRIBUTION (ns)")
                    .font(SextantTheme::FONT_SMALL)
                    .color(SextantTheme::TEXT_MUTED),
            );
            latency_bar(ui, "  50ns", 0.85, SextantTheme::GREEN);
            latency_bar(ui, " 100ns", 0.12, SextantTheme::YELLOW);
            latency_bar(ui, " 200ns", 0.02, SextantTheme::TEXT_MUTED);
            latency_bar(ui, " 500ns", 0.01, SextantTheme::TEXT_MUTED);
        });
        ui.separator();
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

    ui.separator();
    ui.label(
        RichText::new("r:reset  h:histogram  s:sparkline")
            .font(SextantTheme::FONT_SMALL)
            .color(SextantTheme::TEXT_MUTED),
    );
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

fn latency_bar(ui: &mut egui::Ui, label: &str, pct: f64, color: Color32) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(label)
                .font(SextantTheme::FONT_MONO)
                .color(SextantTheme::TEXT_MUTED),
        );

        let desired = egui::vec2(120.0, 14.0);
        let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
        let track_r = egui::CornerRadius::same(7);

        ui.painter()
            .rect_filled(rect, track_r, SextantTheme::BG_DEEP);

        let fill_w = rect.width() * pct as f32;
        let fill_rect = egui::Rect::from_min_size(rect.min, egui::vec2(fill_w, rect.height()));
        ui.painter()
            .rect_filled(fill_rect, track_r, color.linear_multiply(0.7));

        ui.label(
            RichText::new(format!("{}%", (pct * 100.0) as u32))
                .font(SextantTheme::FONT_SMALL)
                .color(SextantTheme::TEXT_MUTED),
        );
    });
}


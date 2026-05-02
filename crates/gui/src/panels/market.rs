//! [1] Market panel — live price chart with Cyberpunk-Terminal styling.

use egui::{FontFamily, FontId, RichText, Vec2};

use crate::app::GuiState;
use crate::theme::SextantTheme;

pub fn render(ui: &mut egui::Ui, state: &GuiState) {
    // ── Header: instrument + price hero ──────────────────────────
    if let Some(ref ctx) = state.context {
        let instrument = ctx.instrument_id_str();
        let price = state.current_price;
        let change_pct = ((price - 150.0) / 150.0 * 100.0 * 100.0).round() / 100.0;
        let (accent, arrow) = if change_pct >= 0.0 {
            (SextantTheme::CYAN, "+")
        } else {
            (SextantTheme::RED, "")
        };

        SextantTheme::accent_frame(accent).show(ui, |ui| {
            ui.horizontal(|ui| {
                // Instrument name
                ui.label(
                    RichText::new(instrument)
                        .font(FontId::new(14.0, FontFamily::Proportional))
                        .strong()
                        .color(SextantTheme::TEXT_PRIMARY),
                );
                ui.add_space(8.0);
                // Large price display
                ui.label(
                    RichText::new(format!("{:.2}", price))
                        .font(FontId::new(22.0, FontFamily::Proportional))
                        .strong()
                        .color(accent),
                );
                ui.add_space(4.0);
                // Change %
                ui.label(
                    RichText::new(format!("{}{:.2}%", arrow, change_pct))
                        .font(SextantTheme::FONT_MONO)
                        .color(accent),
                );
                ui.add_space(12.0);
                // SIM badge
                SextantTheme::badge(ui, "SIM", SextantTheme::CYAN);
            });
        });
    } else {
        ui.label(
            RichText::new("  Waiting for data...")
                .font(SextantTheme::FONT_MONO)
                .color(SextantTheme::TEXT_MUTED),
        );
    }

    ui.add_space(4.0);

    // ── Price chart ──────────────────────────────────────────────
    egui_plot::Plot::new("price_chart")
        .height(ui.available_height() - 100.0)
        .allow_zoom(true)
        .allow_drag(true)
        .show_grid(true)
        .show_background(false)
        .show_axes(false)
        .set_margin_fraction(Vec2::new(0.02, 0.05))
        .cursor_color(SextantTheme::CYAN)
        .show(ui, |plot_ui| {
            if !state.price_history.prices.is_empty() {
                let points: Vec<[f64; 2]> = state
                    .price_history
                    .prices
                    .iter()
                    .enumerate()
                    .map(|(i, &p)| [i as f64, p])
                    .collect();

                // Fill area under the price line
                let line = egui_plot::Line::new(points.clone())
                    .color(SextantTheme::CYAN)
                    .width(2.5)
                    .fill(0.0)
                    .fill_alpha(0.08);
                plot_ui.line(line);

                // Current price horizontal reference line
                if let Some(&last_price) = state.price_history.prices.last() {
                    plot_ui.hline(
                        egui_plot::HLine::new(last_price)
                            .color(SextantTheme::CYAN)
                            .width(1.0)
                            .style(egui_plot::LineStyle::Dotted { spacing: 4.0 }),
                    );
                }
            }
        });

    // ── Position bar ─────────────────────────────────────────────
    ui.add_space(4.0);
    if let Some(ref ctx) = state.context {
        SextantTheme::panel_frame().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("POSITION")
                        .font(SextantTheme::FONT_SMALL)
                        .color(SextantTheme::TEXT_MUTED),
                );
                ui.add_space(8.0);
                if ctx.position_size != 0.0 {
                    ui.label(
                        RichText::new(format!("{:.1}", ctx.position_size))
                            .font(SextantTheme::FONT_MONO)
                            .color(SextantTheme::TEXT_PRIMARY),
                    );
                    let (pnl_color, pnl_label) = if ctx.unrealized_pnl >= 0.0 {
                        (SextantTheme::GREEN, "PnL")
                    } else {
                        (SextantTheme::RED, "PnL")
                    };
                    ui.label(
                        RichText::new(format!("{}: {:+.2}", pnl_label, ctx.unrealized_pnl))
                            .font(SextantTheme::FONT_MONO)
                            .color(pnl_color),
                    );
                    ui.label(
                        RichText::new(format!("Entry: {:.2}", ctx.entry_price))
                            .font(SextantTheme::FONT_MONO)
                            .color(SextantTheme::TEXT_MUTED),
                    );
                } else {
                    ui.label(
                        RichText::new("FLAT")
                            .font(SextantTheme::FONT_MONO)
                            .color(SextantTheme::TEXT_MUTED),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("v{}", ctx.version))
                            .font(SextantTheme::FONT_SMALL)
                            .color(SextantTheme::TEXT_MUTED),
                    );
                });
            });
        });
    }
}

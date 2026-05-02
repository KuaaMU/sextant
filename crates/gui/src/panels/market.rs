//! [1] Market panel — live price chart with Cyberpunk-Terminal styling.
//!
//! Renders candlestick charts from OHLCV data using egui_plot primitives,
//! with a fallback line chart for sparse data.

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
                ui.label(
                    RichText::new(instrument)
                        .font(FontId::new(14.0, FontFamily::Proportional))
                        .strong()
                        .color(SextantTheme::TEXT_PRIMARY),
                );
                ui.add_space(8.0);
                ui.label(
                    RichText::new(format!("{:.2}", price))
                        .font(FontId::new(22.0, FontFamily::Proportional))
                        .strong()
                        .color(accent),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!("{}{:.2}%", arrow, change_pct))
                        .font(SextantTheme::FONT_MONO)
                        .color(accent),
                );
                ui.add_space(12.0);
                SextantTheme::badge(ui, "SIM", SextantTheme::CYAN);

                // OHLCV bar count
                let bar_count = state.ohlcv.bars.len();
                if bar_count > 0 {
                    ui.add_space(8.0);
                    SextantTheme::badge(ui, &format!("{} bars", bar_count), SextantTheme::TEXT_MUTED);
                }
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

    // ── Price chart (candlestick or line) ────────────────────────
    let has_candles = state.ohlcv.bars.len() > 5;

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
            if has_candles {
                render_candlesticks(plot_ui, state);
            } else if !state.price_history.prices.is_empty() {
                render_line_chart(plot_ui, state);
            }

            // Current price horizontal reference line
            plot_ui.hline(
                egui_plot::HLine::new(state.current_price)
                    .color(SextantTheme::CYAN)
                    .width(1.0)
                    .style(egui_plot::LineStyle::Dotted { spacing: 4.0 }),
            );
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
                    let pnl_color = if ctx.unrealized_pnl >= 0.0 {
                        SextantTheme::GREEN
                    } else {
                        SextantTheme::RED
                    };
                    ui.label(
                        RichText::new(format!("PnL: {:+.2}", ctx.unrealized_pnl))
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

/// Render candlesticks using egui_plot primitives.
///
/// Wicks: batched into a single `Line` with NaN gaps between segments.
/// Bodies: individual `Polygon` rectangles per bar.
fn render_candlesticks(plot_ui: &mut egui_plot::PlotUi, state: &GuiState) {
    let bars = &state.ohlcv.bars;
    let n = bars.len();
    if n == 0 {
        return;
    }

    // Body width: ~60% of the spacing between bars
    let body_half = 0.4;

    // ── Wick line (all wicks in one polyline with NaN breaks) ────
    let mut wick_points: Vec<[f64; 2]> = Vec::with_capacity(n * 3);
    for (i, bar) in bars.iter().enumerate() {
        let x = i as f64;
        wick_points.push([x, bar.low]);
        wick_points.push([x, bar.high]);
        if i + 1 < n {
            wick_points.push([f64::NAN, f64::NAN]);
        }
    }

    let wick_line = egui_plot::Line::new(wick_points)
        .color(SextantTheme::TEXT_MUTED)
        .width(1.0);
    plot_ui.line(wick_line);

    // ── Candle bodies ────────────────────────────────────────────
    for (i, bar) in bars.iter().enumerate() {
        let x = i as f64;
        let (top, bottom) = if bar.close >= bar.open {
            (bar.close, bar.open)
        } else {
            (bar.open, bar.close)
        };

        // Degenerate body (doji): draw as a thin horizontal line
        if (top - bottom).abs() < 0.001 {
            let color = if bar.close >= bar.open {
                SextantTheme::GREEN
            } else {
                SextantTheme::RED
            };
            plot_ui.line(
                egui_plot::Line::new(vec![[x - body_half, top], [x + body_half, top]])
                    .color(color)
                    .width(1.5),
            );
            continue;
        }

        // Rectangle: 4 corners (closed polygon)
        let rect = vec![
            [x - body_half, bottom],
            [x + body_half, bottom],
            [x + body_half, top],
            [x - body_half, top],
        ];

        let (fill_color, stroke_color) = if bar.close >= bar.open {
            (
                Color32::from_rgba_premultiplied(0x06, 0xff, 0xa5, 180),
                SextantTheme::GREEN,
            )
        } else {
            (
                Color32::from_rgba_premultiplied(0xff, 0x38, 0x64, 180),
                SextantTheme::RED,
            )
        };

        let poly = egui_plot::Polygon::new(rect)
            .fill_color(fill_color)
            .stroke(egui::Stroke::new(1.0, stroke_color));
        plot_ui.polygon(poly);
    }
}

/// Simple line chart fallback (few data points).
fn render_line_chart(plot_ui: &mut egui_plot::PlotUi, state: &GuiState) {
    let points: Vec<[f64; 2]> = state
        .price_history
        .prices
        .iter()
        .enumerate()
        .map(|(i, &p)| [i as f64, p])
        .collect();

    let line = egui_plot::Line::new(points)
        .color(SextantTheme::CYAN)
        .width(2.5)
        .fill(0.0)
        .fill_alpha(0.08);
    plot_ui.line(line);
}

use egui::Color32;

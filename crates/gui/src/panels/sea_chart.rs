//! Sea chart — flowing liquidity/volatility visualization.
//!
//! Not K-lines. The sea chart shows:
//! - Price flow as a smooth line (the "current")
//! - Volatility as color temperature (cool blue → warm red)
//! - Liquidity as line thickness/brightness

use egui::{Color32, RichText, Vec2};

use crate::app::GuiState;
use crate::theme::SextantTheme;

pub fn render(ui: &mut egui::Ui, state: &GuiState) {
    // Background
    ui.painter().rect_filled(
        ui.max_rect(),
        egui::CornerRadius::ZERO,
        SextantTheme::BG_DEEP,
    );

    // Header
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("SEA CHART")
                .font(SextantTheme::FONT_SMALL)
                .strong()
                .color(SextantTheme::TEXT_SECONDARY),
        );
        ui.add_space(8.0);

        // Volatility indicator
        let vol = compute_volatility(state);
        let vol_color = volatility_color(vol);
        let vol_label = if vol < 0.001 {
            "CALM"
        } else if vol < 0.003 {
            "CHOPPY"
        } else {
            "STORM"
        };
        SextantTheme::status_dot(ui, vol_color);
        ui.label(
            RichText::new(vol_label)
                .font(SextantTheme::FONT_SMALL)
                .color(vol_color),
        );

        ui.add_space(12.0);

        // Spread indicator (parsed from market_state)
        if let Some(ref ctx) = state.context {
            let ms = ctx.market_state_str();
            if let Some(spread) = parse_spread(ms) {
                let spread_color = if spread < 1.0 {
                    SextantTheme::GREEN
                } else if spread < 3.0 {
                    SextantTheme::YELLOW
                } else {
                    SextantTheme::RED
                };
                ui.label(
                    RichText::new(format!("spread {:.1}bps", spread))
                        .font(SextantTheme::FONT_SMALL)
                        .color(spread_color),
                );
            }
        }
    });

    ui.add_space(4.0);

    // Main chart area
    let chart_height = ui.available_height() - 8.0;
    if chart_height < 50.0 {
        return;
    }

    egui_plot::Plot::new("sea_chart")
        .height(chart_height)
        .allow_zoom(true)
        .allow_drag(true)
        .show_background(false)
        .show_axes(false)
        .set_margin_fraction(Vec2::new(0.02, 0.05))
        .cursor_color(SextantTheme::SEA_CURRENT)
        .show(ui, |plot_ui| {
            let prices = &state.price_history.prices;
            if prices.len() < 2 {
                return;
            }

            // Compute volatility per segment for coloring
            let mut points: Vec<[f64; 2]> = Vec::with_capacity(prices.len());
            for (i, &p) in prices.iter().enumerate() {
                points.push([i as f64, p]);
            }

            // Main price flow line
            let line = egui_plot::Line::new(points.clone())
                .color(SextantTheme::SEA_CURRENT)
                .width(2.0);
            plot_ui.line(line);

            // Fill under the line (sea floor)
            if prices.len() > 1 {
                let min_price = prices.iter().cloned().fold(f64::INFINITY, f64::min);
                let baseline = min_price * 0.9999;

                let mut fill_points: Vec<[f64; 2]> = Vec::with_capacity(prices.len() + 2);
                fill_points.push([0.0, baseline]);
                for (i, &p) in prices.iter().enumerate() {
                    fill_points.push([i as f64, p]);
                }
                fill_points.push([(prices.len() - 1) as f64, baseline]);

                let fill_line = egui_plot::Line::new(fill_points)
                    .color(Color32::from_rgba_premultiplied(0x3a, 0x7b, 0xd5, 0x15))
                    .width(0.0)
                    .fill(baseline as f32)
                    .fill_alpha(0.0);
                plot_ui.line(fill_line);
            }

            // Volatility heat overlay — color segments based on local vol
            if prices.len() > 10 {
                let window = 5;
                for i in window..prices.len() {
                    let segment_vol = prices[i - window..i]
                        .windows(2)
                        .map(|w| ((w[1] - w[0]) / w[0]).abs())
                        .sum::<f64>()
                        / window as f64;

                    if segment_vol > 0.0005 {
                        let color = volatility_color(segment_vol);
                        let alpha = ((segment_vol * 5000.0).min(1.0) * 40.0) as u8;
                        let dot_color = Color32::from_rgba_premultiplied(
                            color.r(),
                            color.g(),
                            color.b(),
                            alpha,
                        );

                        // Draw a subtle glow point
                        plot_ui.points(
                            egui_plot::Points::new(vec![[i as f64, prices[i]]])
                                .color(dot_color)
                                .radius(4.0),
                        );
                    }
                }
            }
        });
}

fn compute_volatility(state: &GuiState) -> f64 {
    let prices = &state.price_history.prices;
    if prices.len() < 2 {
        return 0.0;
    }
    let n = prices.len().min(20);
    let start = prices.len() - n;
    prices[start..]
        .windows(2)
        .map(|w| ((w[1] - w[0]) / w[0]).abs())
        .sum::<f64>()
        / n as f64
}

fn volatility_color(vol: f64) -> Color32 {
    if vol < 0.0005 {
        SextantTheme::SEA_COOL
    } else if vol < 0.002 {
        SextantTheme::SEA_CURRENT
    } else {
        SextantTheme::SEA_WARM
    }
}

fn parse_spread(state: &str) -> Option<f64> {
    let marker = state.find("spread:")?;
    let rest = &state[marker + 7..];
    let s = rest.split(|c: char| c == '|' || c == ' ').next()?;
    s.parse().ok()
}

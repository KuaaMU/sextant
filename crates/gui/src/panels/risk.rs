//! [3] Risk panel — potential field + Greeks matrix with Cyberpunk-Terminal styling.

use egui::{Color32, RichText};

use crate::app::GuiState;
use crate::theme::SextantTheme;

pub fn render(ui: &mut egui::Ui, state: &GuiState) {
    if let Some(ref ctx) = state.context {
        let risk_pot = ctx.risk_potential;
        let pos_pot = ctx.position_potential;
        let dd_pot = ctx.drawdown_potential;

        let risk_color = if risk_pot > 0.8 {
            SextantTheme::RED
        } else if risk_pot > 0.5 {
            SextantTheme::YELLOW
        } else {
            SextantTheme::GREEN
        };

        let gradient = (pos_pot * pos_pot + dd_pot * dd_pot).sqrt();
        let grad_label = if gradient > 0.5 {
            "HIGH"
        } else if gradient > 0.2 {
            "MODERATE"
        } else {
            "LOW"
        };

        // ── Risk header with accent stripe ───────────────────────
        SextantTheme::accent_frame(risk_color).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(ctx.instrument_id_str())
                        .font(SextantTheme::FONT_SANS)
                        .strong()
                        .color(SextantTheme::TEXT_PRIMARY),
                );
                ui.add_space(12.0);
                ui.label(
                    RichText::new("Risk:")
                        .font(SextantTheme::FONT_SMALL)
                        .color(SextantTheme::TEXT_MUTED),
                );
                ui.label(
                    RichText::new(format!("{:.2}", risk_pot))
                        .font(SextantTheme::FONT_MONO)
                        .strong()
                        .color(risk_color),
                );
                ui.separator();
                ui.label(
                    RichText::new("Gradient:")
                        .font(SextantTheme::FONT_SMALL)
                        .color(SextantTheme::TEXT_MUTED),
                );
                ui.label(
                    RichText::new(format!("{:.2} ({})", gradient, grad_label))
                        .font(SextantTheme::FONT_MONO)
                        .color(risk_color),
                );
            });
        });

        ui.add_space(8.0);

        // ── Potential bars ────────────────────────────────────────
        potential_bar(ui, "Position", pos_pot, 1.0);
        potential_bar(ui, "Drawdown", dd_pot, 1.0);
        potential_bar(ui, "Concentration", risk_pot * 0.6, 1.0);

        ui.add_space(8.0);

        // ── Greeks matrix ────────────────────────────────────────
        ui.label(
            RichText::new("GREEKS MATRIX")
                .font(SextantTheme::FONT_SMALL)
                .strong()
                .color(SextantTheme::TEXT_SECONDARY),
        );

        egui_extras::TableBuilder::new(ui)
            .column(egui_extras::Column::auto())
            .column(egui_extras::Column::auto())
            .column(egui_extras::Column::auto())
            .column(egui_extras::Column::auto())
            .column(egui_extras::Column::auto())
            .header(22.0, |mut header| {
                let hdr = |ui: &mut egui::Ui, label: &str| {
                    ui.label(
                        RichText::new(label)
                            .font(SextantTheme::FONT_SMALL)
                            .strong()
                            .color(SextantTheme::TEXT_SECONDARY),
                    );
                };
                header.col(|ui| { hdr(ui, ""); });
                header.col(|ui| { hdr(ui, "DELTA"); });
                header.col(|ui| { hdr(ui, "GAMMA"); });
                header.col(|ui| { hdr(ui, "THETA"); });
                header.col(|ui| { hdr(ui, "VEGA"); });
            })
            .body(|mut body| {
                body.row(22.0, |mut row| {
                    let cell = |ui: &mut egui::Ui, val: &str, color: Color32| {
                        ui.label(
                            RichText::new(val)
                                .font(SextantTheme::FONT_MONO)
                                .color(color),
                        );
                    };
                    row.col(|ui| {
                        ui.label(
                            RichText::new(ctx.instrument_id_str())
                                .font(SextantTheme::FONT_MONO)
                                .color(SextantTheme::TEXT_PRIMARY),
                        );
                    });
                    let delta_c = if ctx.greeks.delta >= 0.0 { SextantTheme::CYAN } else { SextantTheme::MAGENTA };
                    let theta_c = if ctx.greeks.theta >= 0.0 { SextantTheme::CYAN } else { SextantTheme::MAGENTA };
                    let vega_c = if ctx.greeks.vega >= 0.0 { SextantTheme::CYAN } else { SextantTheme::MAGENTA };
                    row.col(|ui| { cell(ui, &format!("{:+.3}", ctx.greeks.delta), delta_c); });
                    row.col(|ui| { cell(ui, &format!("{:.4}", ctx.greeks.gamma), SextantTheme::TEXT_PRIMARY); });
                    row.col(|ui| { cell(ui, &format!("{:+.1}", ctx.greeks.theta), theta_c); });
                    row.col(|ui| { cell(ui, &format!("{:+.1}", ctx.greeks.vega), vega_c); });
                });
            });
    } else {
        ui.label(
            RichText::new("  Waiting for data...")
                .font(SextantTheme::FONT_MONO)
                .color(SextantTheme::TEXT_MUTED),
        );
    }
}

fn potential_bar(ui: &mut egui::Ui, label: &str, value: f64, limit: f64) {
    let pct = ((value / limit) * 100.0).min(100.0);
    let color = if pct > 80.0 {
        SextantTheme::RED
    } else if pct > 50.0 {
        SextantTheme::YELLOW
    } else {
        SextantTheme::GREEN
    };

    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("{:<14}", label))
                .font(SextantTheme::FONT_MONO)
                .color(SextantTheme::TEXT_MUTED),
        );
        ui.label(
            RichText::new(format!("U = {:.2}", value))
                .font(SextantTheme::FONT_MONO)
                .color(color),
        );
        ui.add_space(4.0);

        // Custom styled bar
        let desired = egui::vec2(ui.available_width().min(200.0), 14.0);
        let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());

        // Track
        let track_r = egui::CornerRadius::same(7);
        ui.painter()
            .rect_filled(rect, track_r, SextantTheme::BG_DEEP);

        // Fill
        let fill_w = rect.width() * (pct as f32 / 100.0);
        let fill_rect = egui::Rect::from_min_size(
            rect.min,
            egui::vec2(fill_w, rect.height()),
        );
        ui.painter()
            .rect_filled(fill_rect, track_r, color.linear_multiply(0.7));

        // Percentage label
        ui.label(
            RichText::new(format!("{:.0}%", pct))
                .font(SextantTheme::FONT_SMALL)
                .color(SextantTheme::TEXT_MUTED),
        );
    });
}

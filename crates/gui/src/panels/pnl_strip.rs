//! PnL strip — top bar with position, PnL, price, and command input.

use egui::RichText;

use crate::app::GuiState;
use crate::theme::SextantTheme;

pub fn render(ui: &mut egui::Ui, state: &GuiState) {
    SextantTheme::panel_frame().show(ui, |ui| {
        ui.horizontal(|ui| {
            // Instrument
            let instrument = state
                .context
                .as_ref()
                .map(|c| c.instrument_id_str().to_string())
                .unwrap_or_else(|| "—".into());

            ui.label(
                RichText::new(&instrument)
                    .font(SextantTheme::FONT_SANS)
                    .strong()
                    .color(SextantTheme::TEXT_PRIMARY),
            );

            ui.add_space(16.0);

            // Price hero
            ui.label(
                RichText::new(format!("{:.2}", state.current_price))
                    .font(SextantTheme::FONT_DISPLAY)
                    .strong()
                    .color(SextantTheme::TEXT_PRIMARY),
            );

            ui.add_space(24.0);

            // Position
            if let Some(ref ctx) = state.context {
                if ctx.position_size != 0.0 {
                    let side = if ctx.position_size > 0.0 { "LONG" } else { "SHORT" };
                    let side_color = if ctx.position_size > 0.0 {
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
                        RichText::new(format!("{:.4}", ctx.position_size.abs()))
                            .font(SextantTheme::FONT_MONO)
                            .color(SextantTheme::TEXT_SECONDARY),
                    );

                    ui.add_space(12.0);

                    // PnL
                    let pnl = ctx.unrealized_pnl;
                    let pnl_color = if pnl >= 0.0 {
                        SextantTheme::GREEN
                    } else {
                        SextantTheme::RED
                    };
                    ui.label(
                        RichText::new(format!("{:+.2}", pnl))
                            .font(SextantTheme::FONT_MONO)
                            .strong()
                            .color(pnl_color),
                    );

                    // Entry price
                    ui.label(
                        RichText::new(format!("entry {:.2}", ctx.entry_price))
                            .font(SextantTheme::FONT_SMALL)
                            .color(SextantTheme::TEXT_MUTED),
                    );
                } else {
                    ui.label(
                        RichText::new("FLAT")
                            .font(SextantTheme::FONT_MONO)
                            .color(SextantTheme::TEXT_MUTED),
                    );
                }
            }

            // Right side — version + command hint
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("v{}", state.last_version))
                        .font(SextantTheme::FONT_SMALL)
                        .color(SextantTheme::TEXT_MUTED),
                );
                ui.label(
                    RichText::new("[1] Orders  [2] Research  [3] Memory  [4] Hull  [Esc] Close")
                        .font(SextantTheme::FONT_SMALL)
                        .color(SextantTheme::TEXT_DISABLED),
                );
            });
        });
    });
}

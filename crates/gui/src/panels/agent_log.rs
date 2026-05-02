//! [2] Agent Log panel — decision flow with Cyberpunk-Terminal styling.

use egui::RichText;

use crate::app::GuiState;
use crate::theme::SextantTheme;

pub fn render(ui: &mut egui::Ui, state: &GuiState) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        if state.log_entries.is_empty() {
            ui.add_space(8.0);
            ui.label(
                RichText::new("  Waiting for data...")
                    .font(SextantTheme::FONT_MONO)
                    .color(SextantTheme::TEXT_MUTED),
            );
        } else {
            for (i, entry) in state.log_entries.iter().enumerate() {
                // Alternating row background for scanability
                let bg = if i % 2 == 0 {
                    SextantTheme::BG_SURFACE
                } else {
                    SextantTheme::BG_ELEVATED
                };
                egui::Frame::new()
                    .fill(bg)
                    .inner_margin(egui::Margin::symmetric(4, 2))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            // Timestamp
                            ui.label(
                                RichText::new(&entry.timestamp)
                                    .font(SextantTheme::FONT_MONO)
                                    .color(SextantTheme::TEXT_MUTED),
                            );
                            ui.add_space(6.0);
                            // Agent type badge
                            SextantTheme::badge(ui, entry.agent_type.label(), entry.agent_type.color());
                            ui.add_space(6.0);
                            // Latency (color-coded)
                            let lat_color = SextantTheme::latency_color(entry.latency_ms);
                            ui.label(
                                RichText::new(format!("{:>3}ms", entry.latency_ms))
                                    .font(SextantTheme::FONT_MONO)
                                    .color(lat_color),
                            );
                            ui.add_space(6.0);
                            // Summary
                            ui.label(
                                RichText::new(&entry.summary)
                                    .font(SextantTheme::FONT_MONO)
                                    .color(SextantTheme::TEXT_PRIMARY),
                            );
                        });
                    });
            }
        }
    });

    // ── Filter bar ───────────────────────────────────────────────
    ui.separator();
    ui.label(
        RichText::new("Filter: [ALL] PERCEPTION STRATEGY RISK EXEC  /:search  f:follow")
            .font(SextantTheme::FONT_SMALL)
            .color(SextantTheme::TEXT_MUTED),
    );
}

//! Crew status bar — bottom bar with agent cells + autonomy slider + E-Stop.

use egui::{Color32, CornerRadius, RichText, Stroke, Vec2};

use crate::app::{AgentStatus, Autonomy, GuiCommand, GuiState};
use crate::theme::SextantTheme;

pub fn render(ui: &mut egui::Ui, state: &mut GuiState) {
    SextantTheme::panel_frame().show(ui, |ui| {
        ui.horizontal(|ui| {
            // Agent cells
            for agent in &state.agents {
                let bg = match agent.status {
                    AgentStatus::Active => Color32::from_rgba_premultiplied(0x10, 0xB9, 0x81, 0x15),
                    AgentStatus::Busy => Color32::from_rgba_premultiplied(0xF5, 0x9E, 0x0B, 0x15),
                    AgentStatus::Alert => Color32::from_rgba_premultiplied(0xEF, 0x44, 0x44, 0x15),
                    AgentStatus::Idle => SextantTheme::BG_ELEVATED,
                };

                egui::Frame::new()
                    .fill(bg)
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(Vec2::new(8.0, 4.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            SextantTheme::status_dot(ui, agent.status.color());
                            ui.add_space(4.0);

                            ui.label(
                                RichText::new(&agent.id)
                                    .font(SextantTheme::FONT_SMALL)
                                    .strong()
                                    .color(SextantTheme::TEXT_SECONDARY),
                            );

                            ui.add_space(4.0);

                            // Task (one line, truncated — UTF-8 safe)
                            let task_text = if agent.task.chars().count() > 30 {
                                let truncated: String = agent.task.chars().take(29).collect();
                                format!("{}…", truncated)
                            } else {
                                agent.task.clone()
                            };
                            ui.label(
                                RichText::new(task_text)
                                    .font(SextantTheme::FONT_SMALL)
                                    .color(SextantTheme::TEXT_MUTED),
                            );
                        });
                    });

                ui.add_space(4.0);
            }

            // Spacer
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // E-Stop button
                let estop_color = if state.e_stopped {
                    SextantTheme::RED
                } else {
                    Color32::from_rgba_premultiplied(0xEF, 0x44, 0x44, 0x30)
                };
                let estop_text = if state.e_stopped { "STOPPED" } else { "E-STOP" };

                let estop_btn = egui::Button::new(
                    RichText::new(estop_text)
                        .font(SextantTheme::FONT_SMALL)
                        .strong()
                        .color(if state.e_stopped {
                            SextantTheme::BG_DEEP
                        } else {
                            SextantTheme::RED
                        }),
                )
                .fill(estop_color)
                .corner_radius(CornerRadius::same(12))
                .stroke(Stroke::new(1.5, SextantTheme::RED));

                if ui.add(estop_btn).clicked() {
                    state.e_stopped = !state.e_stopped;
                    if let Some(tx) = &state.cmd_tx {
                        let _ = tx.send(GuiCommand::SetEStop(state.e_stopped));
                    }
                }

                ui.add_space(12.0);

                // Autonomy slider
                ui.label(
                    RichText::new("AUTONOMY")
                        .font(SextantTheme::FONT_SMALL)
                        .color(SextantTheme::TEXT_MUTED),
                );

                for (mode, label) in [
                    (Autonomy::Manual, "MANUAL"),
                    (Autonomy::Assisted, "ASSISTED"),
                    (Autonomy::Auto, "AUTO"),
                ] {
                    let is_active = state.autonomy == mode;
                    let btn = egui::Button::new(
                        RichText::new(label)
                            .font(SextantTheme::FONT_SMALL)
                            .color(if is_active {
                                SextantTheme::TEXT_PRIMARY
                            } else {
                                SextantTheme::TEXT_MUTED
                            }),
                    )
                    .fill(if is_active {
                        SextantTheme::BG_ELEVATED
                    } else {
                        Color32::TRANSPARENT
                    })
                    .corner_radius(CornerRadius::same(4));

                    if ui.add(btn).clicked() {
                        state.autonomy = mode;
                        if let Some(tx) = &state.cmd_tx {
                            let _ = tx.send(GuiCommand::SetAutonomy(mode));
                        }
                    }
                }
            });
        });
    });
}

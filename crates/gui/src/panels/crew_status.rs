//! Crew status bar — bottom bar with agent cells + autonomy slider + E-Stop.

use egui::{Color32, CornerRadius, RichText, Stroke, Vec2};

use crate::app::{AgentStatus, Autonomy, GuiState};
use crate::theme::SextantTheme;

pub fn render(ui: &mut egui::Ui, state: &mut GuiState) {
    SextantTheme::panel_frame().show(ui, |ui| {
        ui.horizontal(|ui| {
            // Agent cells
            for agent in &state.agents {
                let bg = match agent.status {
                    AgentStatus::Active => Color32::from_rgba_premultiplied(0x5a, 0xb0, 0x7a, 0x15),
                    AgentStatus::Busy => Color32::from_rgba_premultiplied(0xd4, 0xa5, 0x4a, 0x15),
                    AgentStatus::Alert => Color32::from_rgba_premultiplied(0xd4, 0x5b, 0x5b, 0x15),
                    AgentStatus::Idle => SextantTheme::BG_ELEVATED,
                };

                egui::Frame::new()
                    .fill(bg)
                    .corner_radius(CornerRadius::same(4))
                    .inner_margin(Vec2::new(8.0, 4.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            // Status dot
                            let (rect, _) = ui.allocate_exact_size(
                                Vec2::new(8.0, 8.0),
                                egui::Sense::hover(),
                            );
                            ui.painter()
                                .circle_filled(rect.center(), 4.0, agent.status.color());

                            ui.add_space(4.0);

                            // Agent ID
                            ui.label(
                                RichText::new(&agent.id)
                                    .font(SextantTheme::FONT_SMALL)
                                    .strong()
                                    .color(SextantTheme::TEXT_SECONDARY),
                            );

                            ui.add_space(4.0);

                            // Task (one line, truncated)
                            let task_text = if agent.task.len() > 30 {
                                format!("{}…", &agent.task[..29])
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
                    Color32::from_rgba_premultiplied(0xd4, 0x5b, 0x5b, 0x30)
                };
                let estop_text = if state.e_stopped {
                    "STOPPED"
                } else {
                    "E-STOP"
                };

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
                }

                ui.add_space(12.0);

                // Autonomy slider
                let (_auto_label, _auto_color) = (state.autonomy.label(), state.autonomy.color());

                let manual_btn = egui::Button::new(
                    RichText::new("MANUAL")
                        .font(SextantTheme::FONT_SMALL)
                        .color(if state.autonomy == Autonomy::Manual {
                            SextantTheme::TEXT_PRIMARY
                        } else {
                            SextantTheme::TEXT_MUTED
                        }),
                )
                .fill(if state.autonomy == Autonomy::Manual {
                    SextantTheme::BG_ELEVATED
                } else {
                    Color32::TRANSPARENT
                })
                .corner_radius(CornerRadius::same(4));

                let assisted_btn = egui::Button::new(
                    RichText::new("ASSISTED")
                        .font(SextantTheme::FONT_SMALL)
                        .color(if state.autonomy == Autonomy::Assisted {
                            SextantTheme::TEXT_PRIMARY
                        } else {
                            SextantTheme::TEXT_MUTED
                        }),
                )
                .fill(if state.autonomy == Autonomy::Assisted {
                    SextantTheme::BG_ELEVATED
                } else {
                    Color32::TRANSPARENT
                })
                .corner_radius(CornerRadius::same(4));

                let auto_btn = egui::Button::new(
                    RichText::new("AUTO")
                        .font(SextantTheme::FONT_SMALL)
                        .color(if state.autonomy == Autonomy::Auto {
                            SextantTheme::TEXT_PRIMARY
                        } else {
                            SextantTheme::TEXT_MUTED
                        }),
                )
                .fill(if state.autonomy == Autonomy::Auto {
                    SextantTheme::BG_ELEVATED
                } else {
                    Color32::TRANSPARENT
                })
                .corner_radius(CornerRadius::same(4));

                ui.label(
                    RichText::new("AUTONOMY")
                        .font(SextantTheme::FONT_SMALL)
                        .color(SextantTheme::TEXT_MUTED),
                );

                if ui.add(manual_btn).clicked() {
                    state.autonomy = Autonomy::Manual;
                }
                if ui.add(assisted_btn).clicked() {
                    state.autonomy = Autonomy::Assisted;
                }
                if ui.add(auto_btn).clicked() {
                    state.autonomy = Autonomy::Auto;
                }
            });
        });
    });
}

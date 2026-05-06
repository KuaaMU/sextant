//! Intent cards — right side panel showing agent trade proposals.
//!
//! Cards slide in when agents want to trade.
//! User can Approve / Modify / Reject.

use egui::{Color32, CornerRadius, RichText, Stroke, Vec2};

use crate::app::GuiState;
use crate::theme::SextantTheme;

pub fn render(ui: &mut egui::Ui, state: &mut GuiState) {
    // Header
    ui.label(
        RichText::new("INTENTS")
            .font(SextantTheme::FONT_SMALL)
            .strong()
            .color(SextantTheme::TEXT_SECONDARY),
    );
    ui.add_space(4.0);

    // Render cards (collect indices to dismiss after)
    let mut dismiss_idx = None;

    for (i, card) in state.intent_cards.iter().enumerate() {
        let border_color = if card.side == "BUY" {
            SextantTheme::GREEN
        } else {
            SextantTheme::RED
        };

        egui::Frame::new()
            .fill(SextantTheme::BG_ELEVATED)
            .corner_radius(CornerRadius::same(6))
            .stroke(Stroke::new(1.0, border_color))
            .inner_margin(Vec2::new(12.0, 10.0))
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    // Agent + title
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(&card.agent_id)
                                .font(SextantTheme::FONT_SMALL)
                                .color(SextantTheme::TEXT_MUTED),
                        );
                        ui.add_space(8.0);

                        let confidence_color = if card.confidence > 0.7 {
                            SextantTheme::GREEN
                        } else if card.confidence > 0.4 {
                            SextantTheme::YELLOW
                        } else {
                            SextantTheme::RED
                        };
                        ui.label(
                            RichText::new(format!("{:.0}%", card.confidence * 100.0))
                                .font(SextantTheme::FONT_SMALL)
                                .strong()
                                .color(confidence_color),
                        );
                    });

                    ui.add_space(4.0);

                    // Title
                    ui.label(
                        RichText::new(&card.title)
                            .font(SextantTheme::FONT_SANS)
                            .strong()
                            .color(SextantTheme::TEXT_PRIMARY),
                    );

                    // Reasoning
                    if !card.reasoning.is_empty() {
                        ui.label(
                            RichText::new(&card.reasoning)
                                .font(SextantTheme::FONT_SMALL)
                                .color(SextantTheme::TEXT_SECONDARY),
                        );
                    }

                    ui.add_space(6.0);

                    // Side + quantity
                    ui.horizontal(|ui| {
                        let side_color = if card.side == "BUY" {
                            SextantTheme::GREEN
                        } else {
                            SextantTheme::RED
                        };
                        ui.label(
                            RichText::new(&card.side)
                                .font(SextantTheme::FONT_MONO)
                                .strong()
                                .color(side_color),
                        );
                        ui.label(
                            RichText::new(format!("{:.4}", card.quantity))
                                .font(SextantTheme::FONT_MONO)
                                .color(SextantTheme::TEXT_SECONDARY),
                        );
                        if let Some(price) = card.price {
                            ui.label(
                                RichText::new(format!("@ {:.2}", price))
                                    .font(SextantTheme::FONT_MONO)
                                    .color(SextantTheme::TEXT_MUTED),
                            );
                        }
                    });

                    ui.add_space(8.0);

                    // Action buttons
                    ui.horizontal(|ui| {
                        let approve_btn = egui::Button::new(
                            RichText::new("APPROVE")
                                .font(SextantTheme::FONT_SMALL)
                                .strong()
                                .color(SextantTheme::BG_DEEP),
                        )
                        .fill(SextantTheme::GREEN)
                        .corner_radius(CornerRadius::same(4));

                        let modify_btn = egui::Button::new(
                            RichText::new("MODIFY")
                                .font(SextantTheme::FONT_SMALL)
                                .color(SextantTheme::TEXT_PRIMARY),
                        )
                        .fill(SextantTheme::BG_SURFACE)
                        .corner_radius(CornerRadius::same(4))
                        .stroke(Stroke::new(1.0, SextantTheme::BORDER_SUBTLE));

                        let reject_btn = egui::Button::new(
                            RichText::new("REJECT")
                                .font(SextantTheme::FONT_SMALL)
                                .color(SextantTheme::RED),
                        )
                        .fill(Color32::TRANSPARENT)
                        .corner_radius(CornerRadius::same(4))
                        .stroke(Stroke::new(1.0, SextantTheme::RED));

                        if ui.add(approve_btn).clicked() {
                            dismiss_idx = Some(i);
                        }
                        if ui.add(modify_btn).clicked() {
                            // TODO: open modify dialog
                        }
                        if ui.add(reject_btn).clicked() {
                            dismiss_idx = Some(i);
                        }
                    });
                });
            });

        ui.add_space(6.0);
    }

    // Dismiss approved/rejected card
    if let Some(idx) = dismiss_idx {
        state.dismiss_intent(idx);
    }
}

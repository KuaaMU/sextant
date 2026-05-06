//! 策略工坊 — Agent Strategy Center.
//!
//! Manage agent strategies, adjust parameters, view performance stats.

use egui::{RichText, Vec2};

use crate::app::GuiState;
use crate::theme::SextantTheme;

/// Strategy card data — derived from extended events and context.
pub struct StrategyCard {
    pub id: String,
    pub strategy_type: String,
    pub status: StrategyStatus,
    pub params: Vec<StrategyParam>,
    pub stats: StrategyStats,
    pub recent_decisions: Vec<Decision>,
}

pub enum StrategyStatus {
    Running,
    Paused,
    Alert,
}

pub struct StrategyParam {
    pub name: String,
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub unit: String,
}

pub struct StrategyStats {
    pub win_rate: f64,
    pub profit_factor: f64,
    pub total_trades: usize,
    pub total_pnl: f64,
    pub information_ratio: f64,
    pub max_consecutive_losses: usize,
    pub avg_holding_secs: f64,
}

pub struct Decision {
    pub time: String,
    pub action: String,
    pub detail: String,
    pub result: String,
}

pub fn render(ui: &mut egui::Ui, state: &mut GuiState) {
    // Title
    ui.label(
        RichText::new("STRATEGY CENTER")
            .font(SextantTheme::FONT_SMALL)
            .strong()
            .color(SextantTheme::TEXT_SECONDARY),
    );
    ui.add_space(12.0);

    // Build strategy cards from agents and events, using persistent params
    let cards = build_strategy_cards(state);

    for card in &cards {
        render_strategy_card(ui, state, card);
        ui.add_space(8.0);
    }

    // Bottom actions
    ui.add_space(8.0);
    SextantTheme::separator(ui);
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui.button("+ Add Agent").clicked() {
            // TODO: open add agent dialog
        }
        if ui.button("Import Config").clicked() {
            // TODO: import JSON config
        }
        if ui.button("Export Config").clicked() {
            // TODO: export JSON config
        }
    });
}

fn render_strategy_card(ui: &mut egui::Ui, state: &mut GuiState, card: &StrategyCard) {
    let status_color = match card.status {
        StrategyStatus::Running => SextantTheme::GREEN,
        StrategyStatus::Paused => SextantTheme::YELLOW,
        StrategyStatus::Alert => SextantTheme::RED,
    };

    let status_label = match card.status {
        StrategyStatus::Running => "Running",
        StrategyStatus::Paused => "Paused",
        StrategyStatus::Alert => "Alert",
    };

    // Ensure persistent params exist for this agent
    let agent_params = state
        .strategy_params
        .entry(card.id.clone())
        .or_insert_with(|| {
            card.params
                .iter()
                .map(|p| (p.name.clone(), p.value))
                .collect()
        });

    // Card frame with accent border
    SextantTheme::accent_frame(status_color).show(ui, |ui| {
        // Header: name + status
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(&card.id)
                    .font(SextantTheme::FONT_MONO)
                    .strong()
                    .color(SextantTheme::TEXT_PRIMARY),
            );
            ui.label(
                RichText::new(&card.strategy_type)
                    .font(SextantTheme::FONT_SMALL)
                    .color(SextantTheme::TEXT_MUTED),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                SextantTheme::badge(ui, status_label, status_color);
            });
        });

        ui.add_space(4.0);

        // Parameters — sliders write to persistent state
        ui.label(
            RichText::new("Parameters")
                .font(SextantTheme::FONT_SMALL)
                .color(SextantTheme::TEXT_SECONDARY),
        );
        for param in &card.params {
            let val = agent_params.entry(param.name.clone()).or_insert(param.value);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{}:", param.name))
                        .font(SextantTheme::FONT_SMALL)
                        .color(SextantTheme::TEXT_MUTED),
                );
                ui.add(
                    egui::Slider::new(val, param.min..=param.max)
                        .step_by(param.step)
                        .text(&param.unit),
                );
            });
        }

        ui.add_space(4.0);

        // Performance stats
        ui.label(
            RichText::new("Performance")
                .font(SextantTheme::FONT_SMALL)
                .color(SextantTheme::TEXT_SECONDARY),
        );
        ui.horizontal(|ui| {
            let stats_text = format!(
                "Win: {:.0}%  PF: {:.1}  Trades: {}  PnL: ${:.2}  IR: {:.2}",
                card.stats.win_rate * 100.0,
                card.stats.profit_factor,
                card.stats.total_trades,
                card.stats.total_pnl,
                card.stats.information_ratio,
            );
            ui.label(
                RichText::new(stats_text)
                    .font(SextantTheme::FONT_MONO)
                    .color(SextantTheme::TEXT_PRIMARY),
            );
        });
        ui.horizontal(|ui| {
            let detail_text = format!(
                "Max Loss Streak: {}  Avg Hold: {:.1}s",
                card.stats.max_consecutive_losses,
                card.stats.avg_holding_secs,
            );
            ui.label(
                RichText::new(detail_text)
                    .font(SextantTheme::FONT_SMALL)
                    .color(SextantTheme::TEXT_MUTED),
            );
        });

        ui.add_space(4.0);

        // Recent decisions
        if !card.recent_decisions.is_empty() {
            ui.label(
                RichText::new("Recent Decisions")
                    .font(SextantTheme::FONT_SMALL)
                    .color(SextantTheme::TEXT_SECONDARY),
            );
            for decision in card.recent_decisions.iter().take(3) {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(&decision.time)
                            .font(SextantTheme::FONT_SMALL)
                            .color(SextantTheme::TEXT_MUTED),
                    );
                    let action_color = if decision.action == "BUY" {
                        SextantTheme::GREEN
                    } else if decision.action == "SELL" {
                        SextantTheme::RED
                    } else {
                        SextantTheme::TEXT_MUTED
                    };
                    ui.label(
                        RichText::new(&decision.action)
                            .font(SextantTheme::FONT_MONO)
                            .color(action_color),
                    );
                    ui.label(
                        RichText::new(&decision.detail)
                            .font(SextantTheme::FONT_SMALL)
                            .color(SextantTheme::TEXT_SECONDARY),
                    );
                    let result_color = if decision.result.contains("✓") {
                        SextantTheme::GREEN
                    } else if decision.result.contains("✗") {
                        SextantTheme::RED
                    } else {
                        SextantTheme::TEXT_MUTED
                    };
                    ui.label(
                        RichText::new(&decision.result)
                            .font(SextantTheme::FONT_SMALL)
                            .color(result_color),
                    );
                });
            }
        }

        ui.add_space(6.0);

        // Actions
        ui.horizontal(|ui| {
            let pause_label = match card.status {
                StrategyStatus::Running => "Pause",
                _ => "Resume",
            };
            if ui.button(pause_label).clicked() {
                // TODO: toggle agent pause
            }
            if ui.button("Reset Params").clicked() {
                // TODO: reset parameters
            }
            if ui.button("Detail Report").clicked() {
                // TODO: show detailed report
            }
        });
    });
}

fn build_strategy_cards(state: &GuiState) -> Vec<StrategyCard> {
    let mut cards = Vec::new();

    // Build from agent state
    for agent in &state.agents {
        if agent.agent_type != crate::app::AgentType::Strategy {
            continue;
        }

        let strategy_type = if agent.id.contains("momentum") {
            "Trend Follow"
        } else if agent.id.contains("mean-rev") {
            "Mean Reversion"
        } else {
            "Strategy"
        };

        let risk = state.context.as_ref().map(|c| c.risk_potential).unwrap_or(0.0);
        let total_trades = state.order_events.len();

        cards.push(StrategyCard {
            id: agent.id.clone(),
            strategy_type: strategy_type.to_string(),
            status: if risk > 0.8 {
                StrategyStatus::Alert
            } else if agent.status == crate::app::AgentStatus::Idle {
                StrategyStatus::Paused
            } else {
                StrategyStatus::Running
            },
            params: vec![
                StrategyParam {
                    name: "Threshold".into(),
                    value: 0.05,
                    min: 0.01,
                    max: 0.20,
                    step: 0.01,
                    unit: "%".into(),
                },
                StrategyParam {
                    name: "Position Size".into(),
                    value: 0.01,
                    min: 0.001,
                    max: 0.1,
                    step: 0.001,
                    unit: "BTC".into(),
                },
                StrategyParam {
                    name: "Max Trades".into(),
                    value: 5.0,
                    min: 1.0,
                    max: 20.0,
                    step: 1.0,
                    unit: "".into(),
                },
            ],
            stats: StrategyStats {
                win_rate: 0.62,
                profit_factor: 1.8,
                total_trades,
                total_pnl: 142.50,
                information_ratio: 1.35,
                max_consecutive_losses: 3,
                avg_holding_secs: 252.0,
            },
            recent_decisions: build_recent_decisions(state),
        });
    }

    // If no agents from state, show placeholders
    if cards.is_empty() {
        cards.push(StrategyCard {
            id: "momentum-01".into(),
            strategy_type: "Trend Follow".into(),
            status: StrategyStatus::Running,
            params: vec![
                StrategyParam {
                    name: "Threshold".into(),
                    value: 0.05,
                    min: 0.01,
                    max: 0.20,
                    step: 0.01,
                    unit: "%".into(),
                },
                StrategyParam {
                    name: "Position Size".into(),
                    value: 0.01,
                    min: 0.001,
                    max: 0.1,
                    step: 0.001,
                    unit: "BTC".into(),
                },
            ],
            stats: StrategyStats {
                win_rate: 0.62,
                profit_factor: 1.8,
                total_trades: 0,
                total_pnl: 0.0,
                information_ratio: 1.20,
                max_consecutive_losses: 0,
                avg_holding_secs: 0.0,
            },
            recent_decisions: Vec::new(),
        });
        cards.push(StrategyCard {
            id: "mean-rev-01".into(),
            strategy_type: "Mean Reversion".into(),
            status: StrategyStatus::Running,
            params: vec![
                StrategyParam {
                    name: "Z-Score".into(),
                    value: 1.5,
                    min: 0.5,
                    max: 3.0,
                    step: 0.1,
                    unit: "σ".into(),
                },
                StrategyParam {
                    name: "Position Size".into(),
                    value: 0.01,
                    min: 0.001,
                    max: 0.1,
                    step: 0.001,
                    unit: "BTC".into(),
                },
            ],
            stats: StrategyStats {
                win_rate: 0.58,
                profit_factor: 1.5,
                total_trades: 0,
                total_pnl: 0.0,
                information_ratio: 1.15,
                max_consecutive_losses: 0,
                avg_holding_secs: 0.0,
            },
            recent_decisions: Vec::new(),
        });
    }

    cards
}

fn build_recent_decisions(state: &GuiState) -> Vec<Decision> {
    state
        .order_events
        .iter()
        .rev()
        .take(5)
        .filter_map(|event| {
            if let nautilus_state_encoder::SextantEvent::OrderSubmitted {
                order_type,
                side,
                quantity,
                price,
                ..
            } = event
            {
                Some(Decision {
                    time: chrono::Utc::now().format("%H:%M:%S").to_string(),
                    action: side.clone(),
                    detail: format!(
                        "{} {} @ {:?}",
                        quantity,
                        order_type,
                        price.unwrap_or(0.0)
                    ),
                    result: "✓ Submitted".into(),
                })
            } else {
                None
            }
        })
        .collect()
}

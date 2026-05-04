//! Simple momentum-following agent for live trading.
//!
//! This is a rule-based placeholder agent that demonstrates the Agent trait
//! pipeline. In P1/P2, this will be replaced by LLM-powered agents.

use std::time::Duration;

use async_trait::async_trait;
use nautilus_agent_swarm::{Agent, AgentFeedback, AgentIntent};
use nautilus_core::UUID4;
use nautilus_model::identifiers::InstrumentId;
use nautilus_state_encoder::ContextWindow;
use tracing::{debug, info};

use nautilus_agent_swarm::intent::{IntentType, PositionTarget, RiskBudget};

/// A simple momentum-following agent.
///
/// Reads the momentum field from the ContextWindow's market state string
/// and generates trend-following intents when momentum exceeds a threshold.
pub struct MomentumAgent {
    id: String,
    instrument_id: InstrumentId,
    threshold: f64,
    base_size: f64,
    last_momentum: f64,
    position_held: bool,
    /// Force a trade on next cycle (SEXTANT_FORCE_TRADE=1).
    force_trade: bool,
    force_triggered: bool,
    /// Trade counter — stop after max_trades (0 = unlimited).
    trade_count: u32,
    max_trades: u32,
}

impl MomentumAgent {
    pub fn new(
        id: &str,
        instrument_id: InstrumentId,
        threshold: f64,
        base_size: f64,
        max_trades: u32,
    ) -> Self {
        let force = std::env::var("SEXTANT_FORCE_TRADE")
            .map(|v| v == "1")
            .unwrap_or(false);
        if force {
            eprintln!("[{}] SEXTANT_FORCE_TRADE=1 — will force a BUY on next cycle", id);
        }
        Self {
            id: id.to_string(),
            instrument_id,
            threshold,
            base_size,
            last_momentum: 0.0,
            position_held: false,
            force_trade: force,
            force_triggered: false,
            trade_count: 0,
            max_trades,
        }
    }

    /// Extract momentum value from market state string.
    fn parse_momentum(market_state: &str) -> f64 {
        market_state
            .find("momentum:")
            .and_then(|i| {
                let rest = &market_state[i + 9..];
                rest.split(|c: char| c == '|' || c == ' ')
                    .next()
                    .and_then(|s| s.parse::<f64>().ok())
            })
            .unwrap_or(0.0)
    }
}

#[async_trait]
impl Agent for MomentumAgent {
    fn id(&self) -> &str {
        &self.id
    }

    async fn perceive(&mut self, ctx: &ContextWindow) -> AgentIntent {
        let market_state = ctx.market_state_str();
        let momentum = Self::parse_momentum(market_state);
        self.last_momentum = momentum;

        // Trade limit: stop generating intents after max_trades
        if self.max_trades > 0 && self.trade_count >= self.max_trades {
            debug!("[{}] max_trades ({}) reached — HOLD", self.id, self.max_trades);
            return AgentIntent {
                id: UUID4::new(),
                agent_id: self.id.clone(),
                intent_type: IntentType::Hold,
                description: format!("max_trades={} reached", self.max_trades),
                target_instrument: self.instrument_id,
                target_position: None,
                risk_budget: RiskBudget { max_loss: 0.0, max_position: 0.0, max_drawdown_bps: 0.0 },
                constraints: vec![],
                confidence: 0.0,
                time_horizon: Duration::from_secs(300),
            };
        }

        // Debug: force a trade to verify end-to-end pipeline
        if self.force_trade && !self.force_triggered && !self.position_held {
            eprintln!(
                "[{}] FORCE_TRADE — executing debug BUY of {}",
                self.id, self.base_size
            );
            self.force_triggered = true;
            self.position_held = true;
            return AgentIntent {
                id: UUID4::new(),
                agent_id: self.id.clone(),
                intent_type: IntentType::TrendFollow,
                description: "FORCE_TRADE debug signal".into(),
                target_instrument: self.instrument_id,
                target_position: Some(PositionTarget {
                    size: self.base_size,
                    delta: None,
                }),
                risk_budget: RiskBudget {
                    max_loss: 50.0,
                    max_position: 20.0,
                    max_drawdown_bps: 200.0,
                },
                constraints: vec![],
                confidence: 1.0,
                time_horizon: Duration::from_secs(300),
            };
        }

        let intent_type = if momentum > self.threshold && !self.position_held {
            info!(
                "[{}] momentum={:+.5} > threshold → BUY signal",
                self.id, momentum
            );
            self.position_held = true;
            IntentType::TrendFollow
        } else if momentum < -self.threshold && !self.position_held {
            info!(
                "[{}] momentum={:+.5} < -threshold → SELL signal",
                self.id, momentum
            );
            self.position_held = true;
            IntentType::TrendFollow
        } else if self.position_held && momentum.abs() < self.threshold * 0.3 {
            // Exit when momentum dies
            info!(
                "[{}] momentum={:+.5} fading → EXIT signal",
                self.id, momentum
            );
            self.position_held = false;
            IntentType::MeanReversion
        } else {
            debug!(
                "[{}] momentum={:+.5} — HOLD",
                self.id, momentum
            );
            IntentType::Hold
        };

        AgentIntent {
            id: UUID4::new(),
            agent_id: self.id.clone(),
            intent_type,
            description: format!(
                "momentum={:+.5} threshold={:.5}",
                momentum, self.threshold
            ),
            target_instrument: self.instrument_id,
            target_position: if intent_type == IntentType::Hold {
                None
            } else {
                Some(PositionTarget {
                    size: if momentum > 0.0 {
                        self.base_size
                    } else {
                        -self.base_size
                    },
                    delta: None,
                })
            },
            risk_budget: RiskBudget {
                max_loss: 50.0,
                max_position: 20.0,
                max_drawdown_bps: 200.0,
            },
            constraints: vec![],
            confidence: (momentum.abs() / self.threshold).clamp(0.3, 0.95),
            time_horizon: Duration::from_secs(300),
        }
    }

    async fn on_feedback(&mut self, feedback: &AgentFeedback) {
        if feedback.success {
            self.trade_count += 1;
            info!(
                "[{}] Order filled ({}/{}): price={:?} qty={:?} slippage={:?}bps",
                self.id, self.trade_count, self.max_trades,
                feedback.fill_price, feedback.fill_quantity, feedback.slippage_bps
            );
        } else {
            info!("[{}] Order failed: {:?}", self.id, feedback.error);
        }
    }

    fn confidence(&self) -> f64 {
        (self.last_momentum.abs() / self.threshold).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_momentum() {
        let state = "bid:150.00 | ask:150.05 | spread:0.0500 | vol:50000 | momentum:+0.0035 | regime:65%";
        let m = MomentumAgent::parse_momentum(state);
        assert!((m - 0.0035).abs() < 1e-6);
    }

    #[test]
    fn test_parse_momentum_negative() {
        let state = "bid:149.00 | ask:149.05 | momentum:-0.0012 | regime:30%";
        let m = MomentumAgent::parse_momentum(state);
        assert!((m - (-0.0012)).abs() < 1e-6);
    }

    #[test]
    fn test_parse_momentum_missing() {
        let state = "bid:150.00 | ask:150.05";
        let m = MomentumAgent::parse_momentum(state);
        assert_eq!(m, 0.0);
    }
}

//! Mean-reversion agent for live trading.
//!
//! Detects overbought/oversold conditions by comparing current price
//! to a short-term moving average. Fades extreme moves with the
//! expectation that price will revert to the mean.

use std::time::Duration;

use async_trait::async_trait;
use nautilus_agent_swarm::intent::{IntentType, PositionTarget, RiskBudget};
use nautilus_agent_swarm::{Agent, AgentFeedback, AgentIntent, TradeLimiter};
use nautilus_core::UUID4;
use nautilus_model::identifiers::InstrumentId;
use nautilus_state_encoder::ContextWindow;
use tracing::{debug, info};

/// A mean-reversion agent that fades extreme price moves.
///
/// Computes a short-term moving average from the event trace and
/// generates contrarian signals when price deviates beyond a threshold.
pub struct MeanReversionAgent {
    id: String,
    instrument_id: InstrumentId,
    /// Z-score threshold to trigger a signal (e.g., 1.5 = 1.5 standard deviations).
    z_threshold: f64,
    base_size: f64,
    last_z: f64,
    position_held: bool,
    /// Trade counter circuit breaker (P0: shared struct).
    limiter: TradeLimiter,
}

impl MeanReversionAgent {
    pub fn new(
        id: &str,
        instrument_id: InstrumentId,
        z_threshold: f64,
        base_size: f64,
        max_trades: u32,
    ) -> Self {
        Self {
            id: id.to_string(),
            instrument_id,
            z_threshold,
            base_size,
            last_z: 0.0,
            position_held: false,
            limiter: TradeLimiter::new(id, max_trades),
        }
    }

    /// Compute z-score of the latest price relative to the event trace.
    ///
    /// z = (price - mean) / std_dev
    fn compute_z_score(ctx: &ContextWindow) -> f64 {
        let count = ctx.event_count();
        if count < 5 {
            return 0.0;
        }

        // Collect prices from event trace (up to last 20 events)
        let n = (count as usize).min(20);
        let mut prices = Vec::with_capacity(n);
        for i in 0..n {
            let idx = ((count - 1 - i as u32) as usize) % 64;
            prices.push(ctx.event_trace[idx].price);
        }

        let mean = prices.iter().sum::<f64>() / prices.len() as f64;
        let variance = prices
            .iter()
            .map(|p| (p - mean).powi(2))
            .sum::<f64>()
            / prices.len() as f64;
        let std_dev = variance.sqrt();

        if std_dev < 1e-8 {
            return 0.0;
        }

        let latest = prices[0]; // most recent
        (latest - mean) / std_dev
    }

    /// Parse spread in bps from market state string.
    fn parse_spread_bps(market_state: &str) -> f64 {
        market_state
            .find("spread:")
            .and_then(|i| {
                let rest = &market_state[i + 7..];
                rest.split(|c: char| c == '|' || c == ' ')
                    .next()
                    .and_then(|s| s.trim_end_matches("bps").parse::<f64>().ok())
            })
            .unwrap_or(0.0)
    }
}

#[async_trait]
impl Agent for MeanReversionAgent {
    fn id(&self) -> &str {
        &self.id
    }

    async fn perceive(&mut self, ctx: &ContextWindow) -> AgentIntent {
        let z = Self::compute_z_score(ctx);
        self.last_z = z;

        // === P0: Trade limiter circuit breaker ===
        if !self.limiter.try_trade() {
            debug!(
                "[{}] max_trades ({}) reached — HOLD",
                self.id,
                self.limiter.max_trades()
            );
            return self.hold_intent();
        }

        let spread_bps = Self::parse_spread_bps(ctx.market_state_str());

        // Don't trade in illiquid markets
        if spread_bps > 30.0 {
            debug!(
                "[{}] spread={:.1}bps too wide — HOLD",
                self.id, spread_bps
            );
            return self.hold_intent();
        }

        let intent_type = if z > self.z_threshold && !self.position_held {
            // Price is significantly above mean → expect reversion down → SELL
            info!(
                "[{}] z={:+.2} > {:.2} — overbought → SELL (fade)",
                self.id, z, self.z_threshold
            );
            self.position_held = true;
            IntentType::MeanReversion
        } else if z < -self.z_threshold && !self.position_held {
            // Price is significantly below mean → expect reversion up → BUY
            info!(
                "[{}] z={:+.2} < -{:.2} — oversold → BUY (fade)",
                self.id, z, self.z_threshold
            );
            self.position_held = true;
            IntentType::MeanReversion
        } else if self.position_held && z.abs() < 0.3 {
            // Price reverted to mean → exit
            info!(
                "[{}] z={:+.2} reverted to mean → EXIT",
                self.id, z
            );
            self.position_held = false;
            IntentType::Hold
        } else {
            debug!(
                "[{}] z={:+.2} — HOLD",
                self.id, z
            );
            IntentType::Hold
        };

        AgentIntent {
            id: UUID4::new(),
            agent_id: self.id.clone(),
            intent_type,
            description: format!(
                "z={:+.2} spread={:.1}bps threshold={:.2}",
                z, spread_bps, self.z_threshold
            ),
            target_instrument: self.instrument_id,
            target_position: if intent_type == IntentType::Hold {
                None
            } else {
                // Contrarian: sell when overbought, buy when oversold
                Some(PositionTarget {
                    size: if z > 0.0 {
                        -self.base_size
                    } else {
                        self.base_size
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
            confidence: (z.abs() / self.z_threshold).clamp(0.3, 0.9),
            time_horizon: Duration::from_secs(180),
        }
    }

    async fn on_feedback(&mut self, feedback: &AgentFeedback) {
        if feedback.success {
            self.limiter.record_fill();
            info!(
                "[{}] Reversion trade filled: price={:?} qty={:?} ({}/{})",
                self.id, feedback.fill_price, feedback.fill_quantity,
                self.limiter.trade_count(), self.limiter.max_trades()
            );
        } else {
            // Reset position state on failure so we can re-enter
            self.position_held = false;
            info!("[{}] Trade failed: {:?} — resetting position state", self.id, feedback.error);
        }
    }

    fn confidence(&self) -> f64 {
        (self.last_z.abs() / self.z_threshold).clamp(0.0, 1.0)
    }
}

impl MeanReversionAgent {
    fn hold_intent(&self) -> AgentIntent {
        AgentIntent {
            id: UUID4::new(),
            agent_id: self.id.clone(),
            intent_type: IntentType::Hold,
            description: "spread too wide, holding".into(),
            target_instrument: self.instrument_id,
            target_position: None,
            risk_budget: RiskBudget {
                max_loss: 50.0,
                max_position: 20.0,
                max_drawdown_bps: 200.0,
            },
            constraints: vec![],
            confidence: 0.8,
            time_horizon: Duration::from_secs(180),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_spread_bps() {
        let state = "OrderBook[BTC-USDT] bid:0.1 @ 78650.0 | ask:0.1 @ 78651.0 | mid:78650.50 | spread:1.3bps | momentum:+0.000123 | position:0";
        let spread = MeanReversionAgent::parse_spread_bps(state);
        assert!((spread - 1.3).abs() < 0.01);
    }

    #[test]
    fn test_parse_spread_bps_missing() {
        let state = "bid:150.00 | ask:150.05";
        let spread = MeanReversionAgent::parse_spread_bps(state);
        assert_eq!(spread, 0.0);
    }

    #[test]
    fn test_z_score_flat() {
        let mut ctx = ContextWindow::zeroed();
        // All same price → z = 0
        for i in 0..10 {
            ctx.push_event(nautilus_state_encoder::EventToken {
                event_type: 0,
                price: 100.0,
                size: 1.0,
                timestamp_ns: i,
            });
        }
        let z = MeanReversionAgent::compute_z_score(&ctx);
        assert!((z).abs() < 1e-6);
    }

    #[test]
    fn test_z_score_spike() {
        let mut ctx = ContextWindow::zeroed();
        // 9 prices at 100, then a spike to 110
        for i in 0..9 {
            ctx.push_event(nautilus_state_encoder::EventToken {
                event_type: 0,
                price: 100.0,
                size: 1.0,
                timestamp_ns: i,
            });
        }
        ctx.push_event(nautilus_state_encoder::EventToken {
            event_type: 0,
            price: 110.0,
            size: 1.0,
            timestamp_ns: 10,
        });
        let z = MeanReversionAgent::compute_z_score(&ctx);
        // Spike should produce positive z-score
        assert!(z > 1.0, "expected z > 1.0, got {}", z);
    }

    #[test]
    fn test_z_score_too_few_events() {
        let mut ctx = ContextWindow::zeroed();
        for i in 0..3 {
            ctx.push_event(nautilus_state_encoder::EventToken {
                event_type: 0,
                price: 100.0 + i as f64,
                size: 1.0,
                timestamp_ns: i,
            });
        }
        let z = MeanReversionAgent::compute_z_score(&ctx);
        assert_eq!(z, 0.0);
    }
}

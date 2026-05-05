//! RiskAgent: risk-aware agent that can veto trades when risk is too high.

use async_trait::async_trait;
use nautilus_model::identifiers::InstrumentId;
use nautilus_state_encoder::ContextWindow;
use tracing::{info, warn};

use crate::agent::{Agent, AgentFeedback};
use crate::intent::{AgentIntent, ConfidenceLabel, IntentType, RiskBudget, RiskSnapshot};
use std::time::Duration;

/// Risk agent that monitors risk potential and can veto trades.
///
/// Acts as the "immune system" of the trading organism:
/// - Low risk: produces Hold (lets other agents trade)
/// - High risk: produces Veto (overrides all other intents)
pub struct RiskAgent {
    id: String,
    /// Risk potential threshold for veto (default: 2.0).
    veto_threshold: f64,
    /// Risk potential threshold for warning (default: 1.0).
    warn_threshold: f64,
    /// Instrument to monitor.
    instrument: InstrumentId,
}

impl RiskAgent {
    pub fn new(id: &str, instrument: InstrumentId) -> Self {
        let veto_threshold: f64 = std::env::var("SEXTANT_RISK_VETO_THRESHOLD")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(2.0);
        let warn_threshold: f64 = std::env::var("SEXTANT_RISK_WARN_THRESHOLD")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1.0);

        info!(
            "RiskAgent '{}' created: veto_threshold={:.1}, warn_threshold={:.1}",
            id, veto_threshold, warn_threshold
        );

        Self {
            id: id.to_string(),
            veto_threshold,
            warn_threshold,
            instrument,
        }
    }
}

#[async_trait]
impl Agent for RiskAgent {
    fn id(&self) -> &str {
        &self.id
    }

    async fn perceive(&mut self, ctx: &ContextWindow) -> AgentIntent {
        let risk_potential = ctx.risk_potential;
        let position = ctx.position_size;

        if risk_potential >= self.veto_threshold {
            // High risk — VETO
            warn!(
                "RiskAgent '{}': VETO — risk_potential={:.2} >= {:.1} (position={:.6})",
                self.id, risk_potential, self.veto_threshold, position
            );
            AgentIntent {
                id: nautilus_core::UUID4::new(),
                agent_id: self.id.clone(),
                intent_type: IntentType::Veto,
                description: format!(
                    "RISK VETO: potential={:.2} >= {:.1}",
                    risk_potential, self.veto_threshold
                ),
                target_instrument: self.instrument,
                target_position: None,
                risk_budget: RiskBudget {
                    max_loss: 0.0,
                    max_position: 0.0,
                    max_drawdown_bps: 0.0,
                },
                constraints: vec![],
                confidence: 1.0,
                reputation_score: 1.0, // Risk agent has highest reputation
                time_horizon: Duration::from_secs(60),
                title: format!("VETO: risk={:.2}", risk_potential),
                reasoning: format!("Risk potential {:.2} exceeds veto threshold {:.1}", risk_potential, self.veto_threshold),
                confidence_label: ConfidenceLabel::High,
                risk_snapshot: RiskSnapshot::default(),
                expires_at: None,
                tags: vec!["risk".to_string(), "veto".to_string()],
            }
        } else if risk_potential >= self.warn_threshold {
            // Elevated risk — Hold with reduced confidence
            info!(
                "RiskAgent '{}': WARN — risk_potential={:.2} >= {:.1}",
                self.id, risk_potential, self.warn_threshold
            );
            AgentIntent {
                id: nautilus_core::UUID4::new(),
                agent_id: self.id.clone(),
                intent_type: IntentType::Hold,
                description: format!(
                    "RISK WARNING: potential={:.2} (elevated)",
                    risk_potential
                ),
                target_instrument: self.instrument,
                target_position: None,
                risk_budget: RiskBudget {
                    max_loss: 0.0,
                    max_position: 0.0,
                    max_drawdown_bps: 0.0,
                },
                constraints: vec![],
                confidence: 0.3, // Low confidence = less likely to override
                reputation_score: 1.0,
                time_horizon: Duration::from_secs(300),
                title: format!("WARN: risk={:.2}", risk_potential),
                reasoning: format!("Risk potential {:.2} is elevated", risk_potential),
                confidence_label: ConfidenceLabel::Low,
                risk_snapshot: RiskSnapshot::default(),
                expires_at: None,
                tags: vec!["risk".to_string(), "warning".to_string()],
            }
        } else {
            // Normal risk — Hold (let other agents decide)
            AgentIntent::hold(&self.id, self.instrument)
        }
    }

    async fn on_feedback(&mut self, _feedback: &AgentFeedback) {
        // Risk agent doesn't learn from feedback — it's a pure sensor
    }

    fn confidence(&self) -> f64 {
        1.0 // Risk agent is always confident in its assessment
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ctx(risk_potential: f64, position: f64) -> ContextWindow {
        let mut ctx = ContextWindow::zeroed();
        ctx.risk_potential = risk_potential;
        ctx.position_size = position;
        ctx.timestamp_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        ctx
    }

    #[tokio::test]
    async fn test_risk_agent_low_risk() {
        let mut agent = RiskAgent::new("risk-01", InstrumentId::from("BTC-USDT-SWAP.OKX"));
        let ctx = make_ctx(0.5, 0.0);
        let intent = agent.perceive(&ctx).await;
        assert_eq!(intent.intent_type, IntentType::Hold);
    }

    #[tokio::test]
    async fn test_risk_agent_warn() {
        let mut agent = RiskAgent::new("risk-01", InstrumentId::from("BTC-USDT-SWAP.OKX"));
        let ctx = make_ctx(1.5, 100.0);
        let intent = agent.perceive(&ctx).await;
        assert_eq!(intent.intent_type, IntentType::Hold);
        assert!(intent.description.contains("WARNING"));
    }

    #[tokio::test]
    async fn test_risk_agent_veto() {
        let mut agent = RiskAgent::new("risk-01", InstrumentId::from("BTC-USDT-SWAP.OKX"));
        let ctx = make_ctx(3.0, 500.0);
        let intent = agent.perceive(&ctx).await;
        assert_eq!(intent.intent_type, IntentType::Veto);
        assert!(intent.description.contains("VETO"));
    }
}

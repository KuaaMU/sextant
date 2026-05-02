//! SwarmStrategy: Nautilus Strategy wrapper for the AgentSwarm.
//!
//! This is the primary integration point between Sextant's agent-based trading
//! and NautilusTrader's execution engine. It implements the `Strategy` trait,
//! subscribes to market data via MessageBus, and routes quotes through the
//! SwarmCoordinator to produce execution directives.

use std::fmt::Debug;

use nautilus_common::actor::DataActor;
use nautilus_model::{
    data::QuoteTick,
    enums::{OrderSide as NautilusOrderSide, TimeInForce as NautilusTif},
    identifiers::InstrumentId,
    types::Quantity,
};
use nautilus_state_encoder::StateEncoder;
use nautilus_trading::{
    nautilus_strategy,
    strategy::{Strategy, StrategyConfig, StrategyCore},
};
use tracing::{debug, info, warn};

use crate::intent::{ExecutionDirective, OrderSide, TimeInForce};
use crate::swarm::SwarmCoordinator;

/// Nautilus Strategy wrapper for the Sextant AgentSwarm.
///
/// Subscribes to QuoteTick via MessageBus, feeds quotes into the StateEncoder
/// (which writes to shared memory), runs the SwarmCoordinator cycle, and
/// submits resulting orders to the ExecutionEngine.
pub struct SwarmStrategy {
    core: StrategyCore,
    instrument_id: InstrumentId,
    encoder: StateEncoder,
    swarm: SwarmCoordinator,
}

impl SwarmStrategy {
    /// Creates a new [`SwarmStrategy`] instance.
    pub fn new(
        strategy_id: &str,
        instrument_id: InstrumentId,
        swarm: SwarmCoordinator,
    ) -> Self {
        let config = StrategyConfig {
            strategy_id: Some(nautilus_model::identifiers::StrategyId::from(strategy_id)),
            ..Default::default()
        };

        info!(
            "Creating SwarmStrategy '{}' for {} with {} agents",
            strategy_id,
            instrument_id,
            swarm.agent_count()
        );

        Self {
            core: StrategyCore::new(config),
            instrument_id,
            encoder: StateEncoder::new(&instrument_id.to_string()),
            swarm,
        }
    }

    /// Convert a Sextant ExecutionDirective into Nautilus orders and submit them.
    fn execute_directives(&mut self, directives: Vec<ExecutionDirective>) {
        for directive in directives {
            debug!(
                "Executing directive for intent {:?}: {} orders, style={:?}",
                directive.intent_id,
                directive.orders.len(),
                directive.execution_style
            );

            for order_spec in &directive.orders {
                let side = match order_spec.side {
                    OrderSide::Buy => NautilusOrderSide::Buy,
                    OrderSide::Sell => NautilusOrderSide::Sell,
                };

                let tif = match order_spec.time_in_force {
                    TimeInForce::Gtc => NautilusTif::Gtc,
                    TimeInForce::Ioc => NautilusTif::Ioc,
                    TimeInForce::Fok => NautilusTif::Fok,
                    TimeInForce::Gtd => NautilusTif::Gtd,
                };

                let quantity = Quantity::new(order_spec.quantity, 8);

                let order = self.core.order_factory().market(
                    order_spec.instrument_id,
                    side,
                    quantity,
                    Some(tif),
                    None, // reduce_only
                    None, // quote_quantity
                    None, // display_qty
                    None, // expire_time
                    None, // emulation_trigger
                    None, // tags
                );

                if let Err(e) = self.submit_order(order, None, None) {
                    warn!("Failed to submit order: {}", e);
                }
            }
        }
    }
}

nautilus_strategy!(SwarmStrategy);

impl Debug for SwarmStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SwarmStrategy")
            .field("instrument_id", &self.instrument_id)
            .field("agent_count", &self.swarm.agent_count())
            .finish()
    }
}

impl DataActor for SwarmStrategy {
    fn on_start(&mut self) -> anyhow::Result<()> {
        info!(
            "SwarmStrategy started, subscribing to quotes for {}",
            self.instrument_id
        );
        self.subscribe_quotes(self.instrument_id, None, None);
        Ok(())
    }

    fn on_stop(&mut self) -> anyhow::Result<()> {
        info!("SwarmStrategy stopped");
        self.unsubscribe_quotes(self.instrument_id, None, None);
        Ok(())
    }

    fn on_quote(&mut self, quote: &QuoteTick) -> anyhow::Result<()> {
        // 1. Feed quote into StateEncoder → ContextWindow → SharedStateBuffer
        self.encoder.on_quote(quote);

        // 2. Read current context and run swarm cycle
        let ctx = self.encoder.current_context();
        // Note: run_cycle is async, but on_quote is sync.
        // We use block_on here since the swarm cycle should be fast (<10ms for Layer 1).
        let directives = tokio::runtime::Handle::current().block_on(async {
            self.swarm.run_cycle(ctx).await
        });

        // 3. Execute resulting directives
        if !directives.is_empty() {
            debug!("Swarm produced {} directives", directives.len());
            self.execute_directives(directives);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, AgentFeedback};
    use crate::intent::{AgentIntent, IntentType, PositionTarget, RiskBudget};
    use async_trait::async_trait;
    use nautilus_core::UUID4;
    use std::time::Duration;

    struct TestAgent {
        id: String,
        intent_type: IntentType,
    }

    #[async_trait]
    impl Agent for TestAgent {
        fn id(&self) -> &str {
            &self.id
        }

        async fn perceive(&mut self, _ctx: &nautilus_state_encoder::ContextWindow) -> AgentIntent {
            AgentIntent {
                id: UUID4::new(),
                agent_id: self.id.clone(),
                intent_type: self.intent_type,
                description: format!("{:?} from {}", self.intent_type, self.id),
                target_instrument: InstrumentId::from("SOL-USDC.OKX"),
                target_position: Some(PositionTarget {
                    size: 10.0,
                    delta: None,
                }),
                risk_budget: RiskBudget {
                    max_loss: 100.0,
                    max_position: 100.0,
                    max_drawdown_bps: 500.0,
                },
                constraints: vec![],
                confidence: 0.8,
                time_horizon: Duration::from_secs(300),
            }
        }

        async fn on_feedback(&mut self, _feedback: &AgentFeedback) {}
    }

    #[test]
    fn test_swarm_strategy_creation() {
        let mut swarm = SwarmCoordinator::new(crate::swarm::ConsensusStrategy::Pipeline);
        swarm.add_agent(Box::new(TestAgent {
            id: "test-agent".to_string(),
            intent_type: IntentType::TrendFollow,
        }));

        let strategy = SwarmStrategy::new(
            "SWARM-001",
            InstrumentId::from("SOL-USDC.OKX"),
            swarm,
        );

        assert_eq!(strategy.swarm.agent_count(), 1);
        assert_eq!(strategy.instrument_id, InstrumentId::from("SOL-USDC.OKX"));
    }

    #[test]
    fn test_swarm_strategy_debug() {
        let mut swarm = SwarmCoordinator::new(crate::swarm::ConsensusStrategy::Pipeline);
        swarm.add_agent(Box::new(TestAgent {
            id: "debug-agent".to_string(),
            intent_type: IntentType::Hold,
        }));

        let strategy = SwarmStrategy::new(
            "SWARM-DBG",
            InstrumentId::from("ETH-USDC.OKX"),
            swarm,
        );

        let debug_str = format!("{:?}", strategy);
        assert!(debug_str.contains("SwarmStrategy"));
        assert!(debug_str.contains("ETH-USDC.OKX"));
        assert!(debug_str.contains("agent_count: 1"));
    }
}

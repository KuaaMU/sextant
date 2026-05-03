//! End-to-end pipeline integration test.
//!
//! Verifies: ContextWindow → PerceptionRouter → SwarmCoordinator → IntentCompiler → ExecutionDirective
//! Uses MockLlm to avoid network access.

use nautilus_agent_swarm::perception::{
    LlmAction, MockLlm, PerceptionRouter, PromptTemplate,
};
use nautilus_agent_swarm::{ConsensusStrategy, SwarmCoordinator};
use nautilus_state_encoder::ContextWindow;

fn make_market_ctx(market: &str) -> ContextWindow {
    let mut ctx = ContextWindow::zeroed();
    ctx.set_instrument_id("BTC-USDT");
    ctx.set_market_state(market);
    ctx
}

#[tokio::test]
async fn test_full_pipeline_with_router() {
    // 1. Build a router with a mock LLM that always buys
    let mock = MockLlm::new(LlmAction::Buy, 0.85, "Strong breakout above resistance");
    let router = PerceptionRouter::new(Default::default()).with_small_llm(Box::new(mock));

    // 2. Wire router into SwarmCoordinator (no additional agents)
    let mut swarm = SwarmCoordinator::new(ConsensusStrategy::Pipeline).with_router(router);

    // 3. Build a context with no clear rule signal → escalates to LLM
    let ctx = make_market_ctx("OrderBook bid:50000.0 | ask:50001.0");

    // 4. Run the full cycle
    let directives = swarm.run_cycle(&ctx).await;

    // 5. Verify: router produced a BUY intent → compiled to directive with orders
    assert!(!directives.is_empty(), "Should produce at least one directive");
    let directive = &directives[0];
    assert!(
        !directive.orders.is_empty(),
        "Router BUY intent should compile to orders"
    );
    assert_eq!(directive.orders[0].side, nautilus_agent_swarm::OrderSide::Buy);
}

#[tokio::test]
async fn test_full_pipeline_router_hold_escalates_to_agent() {
    // Router produces low-confidence hold, agent overrides with a strong sell
    let mock = MockLlm::new(LlmAction::Hold, 0.3, "Uncertain");
    let router = PerceptionRouter::new(Default::default()).with_small_llm(Box::new(mock));

    // Add a mock agent that always sells
    struct SellAgent;
    #[async_trait::async_trait]
    impl nautilus_agent_swarm::Agent for SellAgent {
        fn id(&self) -> &str { "sell-agent" }
        async fn perceive(&mut self, _ctx: &ContextWindow) -> nautilus_agent_swarm::AgentIntent {
            nautilus_agent_swarm::AgentIntent {
                id: nautilus_core::UUID4::new(),
                agent_id: "sell-agent".to_string(),
                intent_type: nautilus_agent_swarm::IntentType::TrendFollow,
                description: "Bearish signal".to_string(),
                target_instrument: nautilus_model::identifiers::InstrumentId::from("BTC-USDT.OKX"),
                target_position: Some(nautilus_agent_swarm::PositionTarget {
                    size: -10.0,
                    delta: None,
                }),
                risk_budget: nautilus_agent_swarm::RiskBudget {
                    max_loss: 100.0,
                    max_position: 100.0,
                    max_drawdown_bps: 500.0,
                },
                constraints: vec![],
                confidence: 0.9,
                time_horizon: std::time::Duration::from_secs(300),
            }
        }
        async fn on_feedback(&mut self, _feedback: &nautilus_agent_swarm::AgentFeedback) {}
    }

    let mut swarm = SwarmCoordinator::new(ConsensusStrategy::Pipeline)
        .with_router(router);
    swarm.add_agent(Box::new(SellAgent));

    let ctx = make_market_ctx("OrderBook bid:50000.0 | ask:50001.0");
    let directives = swarm.run_cycle(&ctx).await;

    // Both router (hold) and agent (sell) produce intents.
    // Pipeline consensus: non-Hold intents win. Agent's sell should dominate.
    let has_sell = directives.iter().any(|d| {
        d.orders
            .iter()
            .any(|o| o.side == nautilus_agent_swarm::OrderSide::Sell)
    });
    assert!(has_sell, "Agent's SELL intent should produce a sell order");
}

#[test]
fn test_prompt_template_from_context() {
    let mut ctx = ContextWindow::zeroed();
    ctx.set_instrument_id("ETH-USDC");
    ctx.set_market_state("OrderBook bid:3200.0 @ 5.0 | ask:3200.1 @ 3.0");
    ctx.position_size = 2.5;
    ctx.entry_price = 3180.0;
    ctx.unrealized_pnl = 50.0;
    ctx.risk_potential = 0.2;

    for i in 0..5 {
        ctx.push_event(nautilus_state_encoder::EventToken {
            event_type: 0,
            price: 3200.0 + i as f64 * 0.05,
            size: 4.0,
            timestamp_ns: 1_700_000_000 + i * 500_000_000,
        });
    }

    let prompt = PromptTemplate::build_perception_prompt(&ctx);
    assert!(prompt.contains("ETH-USDC"));
    assert!(prompt.contains("3200.0"));
    assert!(prompt.contains("2.5000"));
    assert!(prompt.contains("JSON"));
    assert!(prompt.contains("QUOTE"));
}

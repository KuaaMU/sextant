//! Sextant Live — OKX demo trading via NautilusTrader LiveNode.
//!
//! Connects the SwarmStrategy to OKX demo environment for live paper trading.
//!
//! ## Setup
//!
//! 1. Create a `.env` file at the workspace root with:
//!    ```text
//!    OKX_API_KEY=your-api-key
//!    OKX_API_SECRET=your-api-secret
//!    OKX_API_PASSPHRASE=your-passphrase
//!    ```
//!
//! 2. Run:
//!    ```bash
//!    cargo run -p sextant-live
//!    ```

mod momentum_agent;

use nautilus_common::enums::Environment;
use nautilus_live::node::LiveNode;
use nautilus_model::identifiers::{AccountId, InstrumentId, TraderId};
use nautilus_okx::{
    common::enums::{OKXEnvironment, OKXInstrumentType},
    config::{OKXDataClientConfig, OKXExecClientConfig},
    factories::{OKXDataClientFactory, OKXExecutionClientFactory},
};

use nautilus_agent_swarm::{ConsensusStrategy, SwarmCoordinator, SwarmStrategy};

use momentum_agent::MomentumAgent;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env file (ignore if missing)
    dotenvy::dotenv().ok();

    // NOTE: Do NOT init tracing_subscriber here — NautilusTrader's LiveNode
    // registers its own logger on startup and will panic if one already exists.

    // Use eprintln for pre-node messages since the logger isn't available yet.
    eprintln!("Sextant Live — OKX Demo Trading");

    // ── Configuration ───────────────────────────────────────────
    let trader_id = TraderId::from("SEXTANT-001");
    let account_id = AccountId::from("OKX-DEMO-001");
    let instrument_id = InstrumentId::from("ETH-USDT-SWAP.OKX");
    let environment = Environment::Live; // LiveNode requires Live; OKX env is Demo

    let data_config = OKXDataClientConfig {
        api_key: None,        // Uses OKX_API_KEY env var
        api_secret: None,     // Uses OKX_API_SECRET env var
        api_passphrase: None, // Uses OKX_API_PASSPHRASE env var
        instrument_types: vec![OKXInstrumentType::Swap],
        environment: OKXEnvironment::Demo,
        ..Default::default()
    };

    let exec_config = OKXExecClientConfig {
        trader_id,
        account_id,
        api_key: None,
        api_secret: None,
        api_passphrase: None,
        instrument_types: vec![OKXInstrumentType::Swap],
        environment: OKXEnvironment::Demo,
        ..Default::default()
    };

    // ── Node ────────────────────────────────────────────────────
    let data_factory = OKXDataClientFactory::new();
    let exec_factory = OKXExecutionClientFactory::new();

    let mut node = LiveNode::builder(trader_id, environment)?
        .with_name("Sextant-Live".to_string())
        .add_data_client(
            Some("OKX".to_string()),
            Box::new(data_factory),
            Box::new(data_config),
        )?
        .add_exec_client(
            Some("OKX".to_string()),
            Box::new(exec_factory),
            Box::new(exec_config),
        )?
        .with_reconciliation(true)
        .with_delay_post_stop_secs(5)
        .build()?;

    // ── Strategy ────────────────────────────────────────────────
    let mut swarm = SwarmCoordinator::new(ConsensusStrategy::Pipeline);
    swarm.add_agent(Box::new(MomentumAgent::new(
        "momentum-01",
        instrument_id,
        0.002,  // momentum threshold
        5.0,    // base position size
    )));

    let strategy = SwarmStrategy::new("SWARM-001", instrument_id, swarm);
    node.add_strategy(strategy)?;

    eprintln!("Starting live node for {} (OKX Demo)...", instrument_id);

    // ── Run ─────────────────────────────────────────────────────
    node.run().await?;

    Ok(())
}

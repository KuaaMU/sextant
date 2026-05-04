//! Sextant Live — OKX demo trading via NautilusTrader LiveNode.
//!
//! Connects the SwarmStrategy to OKX demo environment for live paper trading.
//! Loads LLM config from `llm.toml` at workspace root.
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
//! 2. Set `OPENROUTER_API_KEY` in env or `.env` for LLM inference.
//!
//! 3. Run:
//!    ```bash
//!    cargo run -p sextant-live
//!    ```

mod mean_reversion_agent;
mod momentum_agent;

use log::LevelFilter;
use nautilus_common::{
    enums::Environment,
    logging::logger::LoggerConfig,
};
use nautilus_live::node::LiveNode;
use nautilus_model::identifiers::{AccountId, InstrumentId, TraderId};
use nautilus_okx::{
    common::enums::{OKXEnvironment, OKXInstrumentType},
    config::{OKXDataClientConfig, OKXExecClientConfig},
    factories::{OKXDataClientFactory, OKXExecutionClientFactory},
};

use nautilus_agent_swarm::{
    ConsensusStrategy, PerceptionRouter, RouterConfig, SwarmCoordinator, SwarmStrategy,
};

use mean_reversion_agent::MeanReversionAgent;
use momentum_agent::MomentumAgent;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "debug");
    }

    eprintln!("Sextant Live — OKX Demo Trading");

    // ── Configuration ───────────────────────────────────────────
    let trader_id = TraderId::from("SEXTANT-001");
    let instrument_id = InstrumentId::from("BTC-USDT-SWAP.OKX");

    // OKX environment: OKX_ENVIRONMENT=live|demo (default: demo)
    let okx_env = std::env::var("OKX_ENVIRONMENT").unwrap_or_default();
    let (environment, okx_environment, account_label) = if okx_env == "live" {
        (Environment::Live, OKXEnvironment::Live, "OKX-LIVE")
    } else {
        (Environment::Live, OKXEnvironment::Demo, "OKX-DEMO")
    };
    let account_id = AccountId::from(account_label);

    // Order size: SEXTANT_BASE_SIZE (default: 1 contract = 0.01 BTC for swap)
    let base_size: f64 = std::env::var("SEXTANT_BASE_SIZE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1.0);

    // Max trades: SEXTANT_MAX_TRADES (default: 1 for live, unlimited for demo)
    let max_trades: u32 = std::env::var("SEXTANT_MAX_TRADES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(if okx_environment == OKXEnvironment::Live { 1 } else { 0 });

    eprintln!("Environment: {} | base_size: {} contracts | max_trades: {}",
        if okx_environment == OKXEnvironment::Live { "LIVE" } else { "DEMO" },
        base_size,
        if max_trades == 0 { "unlimited".to_string() } else { max_trades.to_string() },
    );

    // Load OKX credentials from environment (.env file)
    let okx_api_key = std::env::var("OKX_API_KEY").ok();
    let okx_api_secret = std::env::var("OKX_API_SECRET").ok();
    let okx_passphrase = std::env::var("OKX_API_PASSPHRASE").ok();

    let data_config = OKXDataClientConfig {
        api_key: okx_api_key.clone(),
        api_secret: okx_api_secret.clone(),
        api_passphrase: okx_passphrase.clone(),
        instrument_types: vec![OKXInstrumentType::Swap],
        environment: okx_environment,
        ..Default::default()
    };

    let exec_config = OKXExecClientConfig {
        trader_id,
        account_id,
        api_key: okx_api_key,
        api_secret: okx_api_secret,
        api_passphrase: okx_passphrase,
        instrument_types: vec![OKXInstrumentType::Swap],
        environment: okx_environment,
        ..Default::default()
    };

    // ── Node ────────────────────────────────────────────────────
    let data_factory = OKXDataClientFactory::new();
    let exec_factory = OKXExecutionClientFactory::new();

    let logging_config = LoggerConfig {
        stdout_level: LevelFilter::Debug,
        use_tracing: true,
        print_config: true,
        ..Default::default()
    };

    let mut node = LiveNode::builder(trader_id, environment)?
        .with_name("Sextant-Live".to_string())
        .with_logging(logging_config)
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

    // ── LLM Config ──────────────────────────────────────────────
    let config_path = std::path::Path::new("llm.toml");
    let router = if config_path.exists() {
        eprintln!("Loading LLM config from llm.toml...");
        match nautilus_agent_swarm::LlmConfigFile::load(config_path) {
            Ok(config) => {
                eprintln!(
                    "  Models: {:?}, Roles: perception={}, strategy={}",
                    config.models.keys().collect::<Vec<_>>(),
                    config.roles.perception,
                    config.roles.strategy,
                );
                match config.build_role_llm("perception") {
                    Ok(llm) => {
                        eprintln!("  LLM ready: {}", llm.chat_url());
                        Some(PerceptionRouter::new(RouterConfig::default()).with_small_llm(Box::new(llm)))
                    }
                    Err(e) => {
                        eprintln!("  Warning: failed to build LLM: {}. Using rules-only.", e);
                        Some(PerceptionRouter::new(RouterConfig::default()))
                    }
                }
            }
            Err(e) => {
                eprintln!("  Warning: failed to load llm.toml: {}. Using rules-only.", e);
                Some(PerceptionRouter::new(RouterConfig::default()))
            }
        }
    } else {
        eprintln!("No llm.toml found. Using rules-only perception.");
        Some(PerceptionRouter::new(RouterConfig::default()))
    };

    // ── Strategy ────────────────────────────────────────────────
    let mut swarm = SwarmCoordinator::new(ConsensusStrategy::Pipeline);
    if let Some(router) = router {
        swarm = swarm.with_router(router);
    }
    swarm.add_agent(Box::new(MomentumAgent::new(
        "momentum-01",
        instrument_id,
        0.0005, // 0.05% — BTC moves ~0.1% per 10 ticks on demo
        base_size,
        max_trades,
    )));
    swarm.add_agent(Box::new(MeanReversionAgent::new(
        "mean-rev-01",
        instrument_id,
        1.5,    // 1.5 sigma z-score threshold
        base_size,
    )));

    let strategy = SwarmStrategy::new("SWARM-001", instrument_id, swarm);
    node.add_strategy(strategy)?;

    eprintln!("Starting live node for {} (OKX Demo)...", instrument_id);

    // ── Run ─────────────────────────────────────────────────────
    node.run().await?;

    Ok(())
}

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
    common::enums::{OKXEnvironment, OKXInstrumentType, OKXMarginMode},
    config::{OKXDataClientConfig, OKXExecClientConfig},
    factories::{OKXDataClientFactory, OKXExecutionClientFactory},
    OKXHttpClient,
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

    // SEXTANT_INSTRUMENT env var (default: BTC-USDT-SWAP.OKX)
    // Use DOGE-USDT-SWAP.OKX for cheap architecture testing (~$0.03 margin per order)
    let instrument_id = InstrumentId::from(
        std::env::var("SEXTANT_INSTRUMENT")
            .unwrap_or_else(|_| "BTC-USDT-SWAP.OKX".to_string())
            .as_str(),
    );

    // OKX environment: OKX_ENVIRONMENT=live|demo (default: demo)
    let okx_env = std::env::var("OKX_ENVIRONMENT").unwrap_or_default();
    let (environment, okx_environment, account_label) = if okx_env == "live" {
        (Environment::Live, OKXEnvironment::Live, "OKX-LIVE")
    } else {
        (Environment::Live, OKXEnvironment::Demo, "OKX-DEMO")
    };
    let account_id = AccountId::from(account_label);

    // Leverage: SEXTANT_LEVERAGE (default: 2 — keep low during testing to limit losses)
    let leverage: u32 = std::env::var("SEXTANT_LEVERAGE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2);

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

    let cooldown_secs: u64 = std::env::var("SEXTANT_COOLDOWN_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30);

    eprintln!("Environment: {} | base_size: {} contracts | max_trades: {} | cooldown: {}s | leverage: {}x",
        if okx_environment == OKXEnvironment::Live { "LIVE" } else { "DEMO" },
        base_size,
        if max_trades == 0 { "unlimited".to_string() } else { max_trades.to_string() },
        cooldown_secs,
        leverage,
    );

    // Load OKX credentials from environment (.env file)
    let okx_api_key = std::env::var("OKX_API_KEY").ok();
    let okx_api_secret = std::env::var("OKX_API_SECRET").ok();
    let okx_passphrase = std::env::var("OKX_API_PASSPHRASE").ok();

    // ── Set Leverage ───────────────────────────────────────────
    // Create a temporary HTTP client to set leverage before the node starts.
    let inst_str = instrument_id.symbol.as_str().to_string();
    match OKXHttpClient::with_credentials(
        okx_api_key.clone(),
        okx_api_secret.clone(),
        okx_passphrase.clone(),
        None, // default base URL
        10,   // timeout_secs
        3,    // max_retries
        500,  // retry_delay_ms
        5000, // retry_delay_max_ms
        okx_environment,
        None, // proxy_url
    ) {
        Ok(http_client) => {
            match http_client
                .set_leverage(&inst_str, leverage, OKXMarginMode::Cross)
                .await
            {
                Ok(()) => eprintln!("Leverage set: {}x for {} (cross margin)", leverage, inst_str),
                Err(e) => eprintln!("Warning: failed to set leverage: {}. Continuing.", e),
            }
        }
        Err(e) => eprintln!("Warning: failed to create HTTP client for leverage: {}. Continuing.", e),
    }

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
        max_trades,
    )));

    let strategy = SwarmStrategy::new("SWARM-001", instrument_id, swarm);
    node.add_strategy(strategy)?;

    eprintln!("Starting live node for {} (OKX Demo)...", instrument_id);

    // ── Run ─────────────────────────────────────────────────────
    node.run().await?;

    Ok(())
}

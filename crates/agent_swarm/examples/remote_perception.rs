//! Example: Remote LLM perception pipeline with TOML config.
//!
//! Demonstrates: ContextWindow → PromptTemplate → RemoteLlm (from config) → LlmDecision
//!
//! Usage:
//!   OPENROUTER_API_KEY=sk-xxx cargo run -p nautilus-agent-swarm --example remote_perception
//!
//! Or with a config file:
//!   OPENROUTER_API_KEY=sk-xxx cargo run -p nautilus-agent-swarm --example remote_perception -- --config llm.toml
//!
//! Or with a local vLLM/Ollama:
//!   cargo run -p nautilus-agent-swarm --example remote_perception -- --config llm.toml

use nautilus_agent_swarm::perception::{
    LlmBackend, LlmConfigFile, LlmDecision, PromptTemplate, RemoteLlm, RemoteLlmConfig,
};
use nautilus_state_encoder::ContextWindow;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let args: Vec<String> = std::env::args().collect();
    let config_path = args
        .iter()
        .position(|a| a == "--config")
        .and_then(|i| args.get(i + 1));

    // Build a realistic ContextWindow
    let mut ctx = ContextWindow::zeroed();
    ctx.set_instrument_id("BTC-USDT");
    ctx.set_market_state("OrderBook[BTC-USDT] bid:50000.0 @ 1.5 | ask:50001.0 @ 2.0");
    ctx.position_size = 0.5;
    ctx.entry_price = 49800.0;
    ctx.unrealized_pnl = 100.0;
    ctx.risk_potential = 0.3;

    for i in 0..10 {
        ctx.push_event(nautilus_state_encoder::EventToken {
            event_type: 0,
            price: 50000.0 + i as f64 * 0.5,
            size: 1.0 + i as f64 * 0.1,
            timestamp_ns: 1_700_000_000 + i * 500_000_000,
        });
    }

    let prompt = PromptTemplate::build_perception_prompt(&ctx);
    println!("=== Prompt ===\n{}\n", prompt);

    // Build LLM from config or fallback
    let llm = if let Some(path) = config_path {
        println!("Loading config from: {}", path);
        let config = LlmConfigFile::load(path)?;
        println!(
            "Models: {:?}, Roles: perception={}, strategy={}",
            config.models.keys().collect::<Vec<_>>(),
            config.roles.perception,
            config.roles.strategy,
        );
        config.build_role_llm("perception")?
    } else {
        let api_key = std::env::var("OPENROUTER_API_KEY")
            .or_else(|_| std::env::var("OPENAI_API_KEY"))
            .unwrap_or_default();

        if api_key.is_empty() {
            eprintln!("No API key. Set OPENROUTER_API_KEY or use --config llm.toml");
            RemoteLlm::new(RemoteLlmConfig::local(8000, "qwen3-8b"))
        } else {
            RemoteLlm::new(RemoteLlmConfig::openrouter(&api_key, "qwen/qwen3-8b"))
        }
    };

    println!("Calling LLM ({})...", llm.chat_url());
    let response = llm.infer(&prompt).await?;
    println!("=== Raw Response ===\n{}\n", response);

    match LlmDecision::from_json(&response) {
        Some(decision) => {
            println!("=== Parsed Decision ===");
            println!("  Action:     {:?}", decision.action);
            println!("  Confidence: {:.2}", decision.confidence);
            println!("  Reasoning:  {}", decision.reasoning);
        }
        None => {
            println!("Failed to parse JSON response.");
        }
    }

    Ok(())
}

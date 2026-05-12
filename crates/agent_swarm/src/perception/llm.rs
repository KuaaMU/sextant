//! LLM backend trait and prompt construction for ContextWindow inference.

use async_trait::async_trait;
use nautilus_state_encoder::ContextWindow;
use tracing::debug;

/// Configuration for an LLM backend.
#[derive(Clone, Debug)]
pub struct LlmConfig {
    /// Path to GGUF model file.
    pub model_path: String,
    /// Context window size (tokens).
    pub ctx_size: u32,
    /// Temperature for sampling.
    pub temperature: f32,
    /// Top-p for nucleus sampling.
    pub top_p: f32,
    /// Maximum tokens to generate.
    pub max_tokens: u32,
    /// Number of threads for inference.
    pub n_threads: u32,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            model_path: String::new(),
            ctx_size: 4096,
            temperature: 0.7,
            top_p: 0.9,
            max_tokens: 512,
            n_threads: 4,
        }
    }
}

/// Construct a prompt from a ContextWindow for LLM consumption.
pub struct PromptTemplate;

impl PromptTemplate {
    /// Build a trading analysis prompt from the current market state.
    pub fn build_perception_prompt(ctx: &ContextWindow) -> String {
        let market = ctx.market_state_str();
        let position = ctx.position_size;
        let entry = ctx.entry_price;
        let pnl = ctx.unrealized_pnl;
        let risk = ctx.risk_potential;

        let events = Self::format_event_trace(ctx);

        format!(
            r#"You are a quantitative trading agent analyzing real-time market data.

## Current State
Instrument: {instrument}
Market: {market}
Position: {position:.4} @ {entry:.2} | uPnL: {pnl:.2}
Risk Potential: {risk:.4}

## Recent Events
{events}

## Task
Analyze the market state and decide:
1. Action: BUY / SELL / HOLD
2. Confidence: 0.0-1.0
3. Reasoning: one sentence

Respond in JSON:
{{"action": "BUY|SELL|HOLD", "confidence": 0.0-1.0, "reasoning": "..."}}"#,
            instrument = ctx.instrument_id_str(),
            market = if market.is_empty() { "No data" } else { market },
            position = position,
            entry = entry,
            pnl = pnl,
            risk = risk,
            events = events,
        )
    }

    fn format_event_trace(ctx: &ContextWindow) -> String {
        let count = ctx.event_count();
        if count == 0 {
            return "No recent events.".to_string();
        }

        let mut lines = Vec::new();
        let start = count.saturating_sub(64);
        for i in start..count {
            let idx = (i as usize) % 64;
            let ev = &ctx.event_trace[idx];
            let kind = match ev.event_type {
                0 => "QUOTE",
                1 => "TRADE",
                2 => "FILL",
                3 => "ALERT",
                4 => "RISK",
                _ => "UNKNOWN",
            };
            lines.push(format!(
                "  [{kind}] price={:.2} size={:.2} ts={}",
                ev.price, ev.size, ev.timestamp_ns
            ));
        }
        lines.join("\n")
    }
}

/// Parsed LLM response for a trading decision.
#[derive(Clone, Debug, PartialEq)]
pub struct LlmDecision {
    pub action: LlmAction,
    pub confidence: f64,
    pub reasoning: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LlmAction {
    Buy,
    Sell,
    Hold,
}

impl LlmDecision {
    /// Parse a JSON response from the LLM.
    pub fn from_json(json_str: &str) -> Option<Self> {
        // Find the JSON object in the response (LLMs may wrap in markdown)
        let start = json_str.find('{')?;
        let end = json_str.rfind('}')? + 1;
        let json = &json_str[start..end];

        let parsed: serde_json::Value = serde_json::from_str(json).ok()?;

        let action = match parsed.get("action")?.as_str()? {
            "BUY" => LlmAction::Buy,
            "SELL" => LlmAction::Sell,
            "HOLD" => LlmAction::Hold,
            _ => return None,
        };

        let confidence = parsed.get("confidence")?.as_f64()?;
        let reasoning = parsed
            .get("reasoning")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        Some(LlmDecision {
            action,
            confidence,
            reasoning,
        })
    }
}

/// Trait for LLM inference backends.
#[async_trait]
pub trait LlmBackend: Send + Sync {
    /// Run inference on a prompt and return the raw response text.
    async fn infer(&self, prompt: &str) -> anyhow::Result<String>;

    /// Run inference on a ContextWindow and return a parsed decision.
    async fn perceive(&self, ctx: &ContextWindow) -> anyhow::Result<LlmDecision> {
        let prompt = PromptTemplate::build_perception_prompt(ctx);
        debug!("LLM prompt ({} chars)", prompt.len());
        let response = self.infer(&prompt).await?;
        debug!("LLM response: {}", response);
        LlmDecision::from_json(&response)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse LLM response: {}", response))
    }
}

/// Mock LLM backend for testing — returns configurable decisions.
pub struct MockLlm {
    pub decision: LlmDecision,
}

impl MockLlm {
    pub fn new(action: LlmAction, confidence: f64, reasoning: &str) -> Self {
        Self {
            decision: LlmDecision {
                action,
                confidence,
                reasoning: reasoning.to_string(),
            },
        }
    }

    /// Create a mock that always holds.
    pub fn hold() -> Self {
        Self::new(LlmAction::Hold, 1.0, "Mock hold")
    }
}

#[async_trait]
impl LlmBackend for MockLlm {
    async fn infer(&self, _prompt: &str) -> anyhow::Result<String> {
        let json = match self.decision.action {
            LlmAction::Buy => format!(
                r#"{{"action": "BUY", "confidence": {}, "reasoning": "{}"}}"#,
                self.decision.confidence, self.decision.reasoning
            ),
            LlmAction::Sell => format!(
                r#"{{"action": "SELL", "confidence": {}, "reasoning": "{}"}}"#,
                self.decision.confidence, self.decision.reasoning
            ),
            LlmAction::Hold => format!(
                r#"{{"action": "HOLD", "confidence": {}, "reasoning": "{}"}}"#,
                self.decision.confidence, self.decision.reasoning
            ),
        };
        Ok(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_template() {
        let mut ctx = ContextWindow::zeroed();
        ctx.set_instrument_id("BTC-USDT");
        ctx.set_market_state("OrderBook[BTC-USDT] bid:50000.0 @ 1.5 | ask:50001.0 @ 2.0");
        ctx.position_size = 0.5;
        ctx.entry_price = 49800.0;
        ctx.unrealized_pnl = 100.0;

        let prompt = PromptTemplate::build_perception_prompt(&ctx);
        assert!(prompt.contains("BTC-USDT"));
        assert!(prompt.contains("50000"));
        assert!(prompt.contains("0.5000"));
        assert!(prompt.contains("JSON"));
    }

    #[test]
    fn test_parse_llm_response() {
        let json = r#"{"action": "BUY", "confidence": 0.85, "reasoning": "Strong momentum"}"#;
        let decision = LlmDecision::from_json(json).unwrap();
        assert_eq!(decision.action, LlmAction::Buy);
        assert!((decision.confidence - 0.85).abs() < 1e-6);
        assert_eq!(decision.reasoning, "Strong momentum");
    }

    #[test]
    fn test_parse_llm_response_with_markdown() {
        let json = "```json\n{\"action\": \"SELL\", \"confidence\": 0.6, \"reasoning\": \"Reversal\"}\n```";
        let decision = LlmDecision::from_json(json).unwrap();
        assert_eq!(decision.action, LlmAction::Sell);
    }

    #[test]
    fn test_parse_invalid_json() {
        assert!(LlmDecision::from_json("not json").is_none());
        assert!(LlmDecision::from_json("{}").is_none());
    }

    #[tokio::test]
    async fn test_mock_llm() {
        let mock = MockLlm::new(LlmAction::Buy, 0.9, "test");
        let ctx = ContextWindow::zeroed();
        let decision = mock.perceive(&ctx).await.unwrap();
        assert_eq!(decision.action, LlmAction::Buy);
        assert_eq!(decision.confidence, 0.9);
    }
}

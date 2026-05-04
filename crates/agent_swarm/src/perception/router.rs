//! Three-layer perception router.
//!
//! Layer 1 (Rule-based): Instant pattern matching on ContextWindow fields.
//! Layer 2 (Small LLM): Fast inference for tactical decisions.
//! Layer 3 (Large LLM): Deep inference for strategic analysis.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use nautilus_state_encoder::ContextWindow;
use tracing::{debug, info, warn};

use crate::intent::{AgentIntent, IntentType, PositionTarget, RiskBudget};
use super::llm::{LlmAction, LlmBackend, LlmDecision};

/// Which layer produced the decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoutingLayer {
    Rule,
    SmallLlm,
    LargeLlm,
}

/// A perception decision with routing metadata.
#[derive(Clone, Debug)]
pub struct PerceptionDecision {
    pub layer: RoutingLayer,
    pub intent_type: IntentType,
    pub confidence: f64,
    pub reasoning: String,
    pub target_size: f64,
}

impl PerceptionDecision {
    /// Convert to an AgentIntent for the swarm pipeline.
    pub fn to_intent(&self, agent_id: &str, instrument: nautilus_model::identifiers::InstrumentId) -> AgentIntent {
        AgentIntent {
            id: nautilus_core::UUID4::new(),
            agent_id: agent_id.to_string(),
            intent_type: self.intent_type,
            description: format!("[{:?}] {}", self.layer, self.reasoning),
            target_instrument: instrument,
            target_position: if self.target_size.abs() > 1e-8 {
                Some(PositionTarget {
                    size: self.target_size,
                    delta: None,
                })
            } else {
                None
            },
            risk_budget: RiskBudget {
                max_loss: 100.0,
                max_position: 1000.0,
                max_drawdown_bps: 500.0,
            },
            constraints: vec![],
            confidence: self.confidence,
            time_horizon: std::time::Duration::from_secs(300),
        }
    }
}

/// Configuration for routing thresholds.
#[derive(Clone, Debug)]
pub struct RouterConfig {
    /// Layer 1 confidence threshold — below this, escalate to Layer 2.
    pub l1_threshold: f64,
    /// Layer 2 confidence threshold — below this, escalate to Layer 3.
    pub l2_threshold: f64,
    /// Base position size for signals.
    pub base_size: f64,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            l1_threshold: 0.5,
            l2_threshold: 0.6,
            base_size: 0.001, // 0.001 BTC (~$78) for demo
        }
    }
}

/// Simple TTL cache for LLM responses.
// === P0: Hash of market_state[:200], P1: MarketStateKey with semantic features ===
struct LlmCache {
    entries: std::collections::HashMap<u64, (Instant, PerceptionDecision)>,
    ttl: Duration,
}

impl LlmCache {
    fn new(ttl_secs: u64) -> Self {
        Self {
            entries: std::collections::HashMap::new(),
            ttl: Duration::from_secs(ttl_secs),
        }
    }

    fn get(&self, key: u64) -> Option<PerceptionDecision> {
        self.entries.get(&key)
            .filter(|(t, _)| t.elapsed() < self.ttl)
            .map(|(_, d)| d.clone())
    }

    fn insert(&mut self, key: u64, decision: PerceptionDecision) {
        // Evict expired entries periodically
        if self.entries.len() > 100 {
            self.entries.retain(|_, (t, _)| t.elapsed() < self.ttl);
        }
        self.entries.insert(key, (Instant::now(), decision));
    }
}

/// Three-layer perception router.
pub struct PerceptionRouter {
    config: RouterConfig,
    small_llm: Option<Box<dyn LlmBackend>>,
    large_llm: Option<Box<dyn LlmBackend>>,
    cache: Mutex<LlmCache>,
}

impl PerceptionRouter {
    pub fn new(config: RouterConfig) -> Self {
        Self {
            config,
            small_llm: None,
            large_llm: None,
            cache: Mutex::new(LlmCache::new(60)),
        }
    }

    /// Set the small LLM backend (Layer 2).
    pub fn with_small_llm(mut self, llm: Box<dyn LlmBackend>) -> Self {
        self.small_llm = Some(llm);
        self
    }

    /// Set the large LLM backend (Layer 3).
    pub fn with_large_llm(mut self, llm: Box<dyn LlmBackend>) -> Self {
        self.large_llm = Some(llm);
        self
    }

    /// Route a ContextWindow through the three layers and produce a decision.
    pub async fn route(&self, ctx: &ContextWindow) -> PerceptionDecision {
        // Layer 1: Rule-based
        let l1 = self.layer1_rules(ctx);
        debug!(
            "L1 decision: {:?} confidence={:.2} — {}",
            l1.intent_type, l1.confidence, l1.reasoning
        );

        if l1.confidence >= self.config.l1_threshold {
            info!("L1 resolved: {:?} (confidence {:.2})", l1.intent_type, l1.confidence);
            return l1;
        }

        // === P0: Check LLM cache before calling L2/L3 ===
        let cache_key = Self::cache_key(ctx);
        if let Ok(cache) = self.cache.lock() {
            if let Some(cached) = cache.get(cache_key) {
                debug!("LLM cache hit — returning cached {:?} (confidence {:.2})", cached.intent_type, cached.confidence);
                return cached;
            }
        }

        // Layer 2: Small LLM (5s timeout)
        if let Some(ref llm) = self.small_llm {
            match tokio::time::timeout(Duration::from_secs(5), llm.perceive(ctx)).await {
                Ok(Ok(decision)) => {
                    let l2 = self.llm_to_perception(decision, RoutingLayer::SmallLlm);
                    debug!(
                        "L2 decision: {:?} confidence={:.2} — {}",
                        l2.intent_type, l2.confidence, l2.reasoning
                    );
                    // Cache the result
                    if let Ok(mut cache) = self.cache.lock() {
                        cache.insert(cache_key, l2.clone());
                    }
                    if l2.confidence >= self.config.l2_threshold {
                        info!("L2 resolved: {:?} (confidence {:.2})", l2.intent_type, l2.confidence);
                        return l2;
                    }
                    // Fall through to L3
                }
                Ok(Err(e)) => {
                    debug!("L2 inference failed: {}, falling through to L3", e);
                }
                Err(_) => {
                    warn!("L2 LLM timeout (5s) — falling back to L3");
                }
            }
        }

        // Layer 3: Large LLM (15s timeout)
        if let Some(ref llm) = self.large_llm {
            match tokio::time::timeout(Duration::from_secs(15), llm.perceive(ctx)).await {
                Ok(Ok(decision)) => {
                    let l3 = self.llm_to_perception(decision, RoutingLayer::LargeLlm);
                    info!("L3 resolved: {:?} (confidence {:.2})", l3.intent_type, l3.confidence);
                    // Cache the result
                    if let Ok(mut cache) = self.cache.lock() {
                        cache.insert(cache_key, l3.clone());
                    }
                    return l3;
                }
                Ok(Err(e)) => {
                    debug!("L3 inference failed: {}, using L1 fallback", e);
                }
                Err(_) => {
                    warn!("L3 LLM timeout (15s) — using L1 fallback");
                }
            }
        }

        // All LLM layers failed or unavailable — return L1 decision regardless
        info!("No LLM available, using L1 fallback: {:?}", l1.intent_type);
        l1
    }

    /// Compute cache key from market state (first 200 bytes, with bounds check).
    fn cache_key(ctx: &ContextWindow) -> u64 {
        let state = ctx.market_state;
        let len = (ctx.market_state_len as usize).min(state.len()).min(200);
        let mut hasher = DefaultHasher::new();
        state[..len].hash(&mut hasher);
        hasher.finish()
    }

    /// Layer 1: Rule-based pattern matching on ContextWindow fields.
    fn layer1_rules(&self, ctx: &ContextWindow) -> PerceptionDecision {
        let market = ctx.market_state_str();
        let position = ctx.position_size;
        let risk = ctx.risk_potential;

        // Rule 1: Risk gradient pressure → proportional position reduction
        // Smooth transition: risk 0.5 → 25% reduction, 0.8 → 80%, 1.0 → full exit
        if risk > 0.5 {
            let reduction = ((risk - 0.5) * 2.0).clamp(0.0, 1.0); // 0.5→0.0, 1.0→1.0
            let target = position * (1.0 - reduction);
            let confidence = 0.7 + risk * 0.25; // 0.7..0.95
            return PerceptionDecision {
                layer: RoutingLayer::Rule,
                intent_type: IntentType::MeanReversion,
                confidence,
                reasoning: format!(
                    "Risk gradient {:.2} → reducing position {:.4} → {:.4} ({:.0}% reduction)",
                    risk, position, target, reduction * 100.0
                ),
                target_size: target,
            };
        }

        // Rule 2: Parse bid/ask spread from market state
        let (bid, ask) = Self::parse_bid_ask(market);
        if let (Some(bid), Some(ask)) = (bid, ask) {
            let mid = (bid + ask) / 2.0;
            let spread_bps = ((ask - bid) / mid) * 10000.0;

            // Wide spread → hold (illiquid)
            if spread_bps > 50.0 {
                return PerceptionDecision {
                    layer: RoutingLayer::Rule,
                    intent_type: IntentType::Hold,
                    confidence: 0.9,
                    reasoning: format!("Wide spread ({:.1} bps), illiquid market", spread_bps),
                    target_size: position,
                };
            }

            // Check momentum from event trace
            let momentum = Self::calculate_momentum(ctx);
            if momentum > 0.0005 && position < self.config.base_size {
                return PerceptionDecision {
                    layer: RoutingLayer::Rule,
                    intent_type: IntentType::TrendFollow,
                    confidence: 0.75,
                    reasoning: format!("Positive momentum ({:.4}), following trend", momentum),
                    target_size: self.config.base_size,
                };
            }
            if momentum < -0.0005 && position > -self.config.base_size {
                return PerceptionDecision {
                    layer: RoutingLayer::Rule,
                    intent_type: IntentType::TrendFollow,
                    confidence: 0.75,
                    reasoning: format!("Negative momentum ({:.4}), following trend", momentum),
                    target_size: -self.config.base_size,
                };
            }
        }

        // Default: low confidence hold — escalate to LLM
        PerceptionDecision {
            layer: RoutingLayer::Rule,
            intent_type: IntentType::Hold,
            confidence: 0.3,
            reasoning: "No clear signal from rules".to_string(),
            target_size: position,
        }
    }

    /// Calculate price momentum from the event trace.
    fn calculate_momentum(ctx: &ContextWindow) -> f64 {
        let count = ctx.event_count();
        if count < 2 {
            return 0.0;
        }

        // Compare recent price to older price
        let recent_idx = ((count - 1) as usize) % 64;
        let old_idx = if count > 10 {
            ((count - 10) as usize) % 64
        } else {
            0
        };

        let recent = ctx.event_trace[recent_idx].price;
        let old = ctx.event_trace[old_idx].price;

        if old > 0.0 {
            (recent - old) / old
        } else {
            0.0
        }
    }

    /// Parse bid and ask prices from market state text.
    fn parse_bid_ask(market: &str) -> (Option<f64>, Option<f64>) {
        let bid = market
            .find("bid:")
            .and_then(|i| market[i + 4..].split(' ').next())
            .and_then(|s| s.parse::<f64>().ok());
        let ask = market
            .find("ask:")
            .and_then(|i| market[i + 4..].split(' ').next())
            .and_then(|s| s.parse::<f64>().ok());
        (bid, ask)
    }

    /// Convert an LlmDecision into a PerceptionDecision.
    fn llm_to_perception(&self, decision: LlmDecision, layer: RoutingLayer) -> PerceptionDecision {
        let (intent_type, size) = match decision.action {
            LlmAction::Buy => (IntentType::TrendFollow, self.config.base_size),
            LlmAction::Sell => (IntentType::TrendFollow, -self.config.base_size),
            LlmAction::Hold => (IntentType::Hold, 0.0),
        };

        PerceptionDecision {
            layer,
            intent_type,
            confidence: decision.confidence,
            reasoning: decision.reasoning,
            target_size: size,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::llm::{LlmAction, MockLlm};

    fn make_ctx_with_market(market: &str) -> ContextWindow {
        let mut ctx = ContextWindow::zeroed();
        ctx.set_instrument_id("BTC-USDT");
        ctx.set_market_state(market);
        ctx
    }

    #[tokio::test]
    async fn test_layer1_high_risk() {
        let router = PerceptionRouter::new(RouterConfig::default());
        let mut ctx = make_ctx_with_market("OrderBook bid:50000.0 | ask:50001.0");
        ctx.risk_potential = 0.9;

        let decision = router.route(&ctx).await;
        assert_eq!(decision.layer, RoutingLayer::Rule);
        assert_eq!(decision.intent_type, IntentType::MeanReversion);
        assert!(decision.confidence > 0.9);
    }

    #[tokio::test]
    async fn test_layer1_wide_spread() {
        let router = PerceptionRouter::new(RouterConfig::default());
        let ctx = make_ctx_with_market("OrderBook bid:49000.0 | ask:51000.0");

        let decision = router.route(&ctx).await;
        assert_eq!(decision.layer, RoutingLayer::Rule);
        assert_eq!(decision.intent_type, IntentType::Hold);
    }

    #[tokio::test]
    async fn test_layer1_no_signal_escalates() {
        let mock = MockLlm::new(LlmAction::Buy, 0.85, "Strong breakout");
        let router = PerceptionRouter::new(RouterConfig::default())
            .with_small_llm(Box::new(mock));

        let ctx = make_ctx_with_market("OrderBook bid:50000.0 | ask:50001.0");
        let decision = router.route(&ctx).await;

        // L1 gives low confidence, LLM should resolve
        assert_eq!(decision.layer, RoutingLayer::SmallLlm);
        assert_eq!(decision.intent_type, IntentType::TrendFollow);
    }

    #[tokio::test]
    async fn test_layer2_low_confidence_escalates_to_l3() {
        let small = MockLlm::new(LlmAction::Hold, 0.4, "Uncertain");
        let large = MockLlm::new(LlmAction::Sell, 0.9, "Bearish divergence");
        let router = PerceptionRouter::new(RouterConfig::default())
            .with_small_llm(Box::new(small))
            .with_large_llm(Box::new(large));

        let ctx = make_ctx_with_market("OrderBook bid:50000.0 | ask:50001.0");
        let decision = router.route(&ctx).await;

        assert_eq!(decision.layer, RoutingLayer::LargeLlm);
        assert_eq!(decision.intent_type, IntentType::TrendFollow);
        assert!((decision.target_size + 0.001).abs() < 1e-8); // Sell = negative base_size
    }

    #[test]
    fn test_momentum_calculation() {
        let mut ctx = ContextWindow::zeroed();
        // Push 10 events with rising prices
        for i in 0..10 {
            ctx.push_event(nautilus_state_encoder::EventToken {
                event_type: 0,
                price: 100.0 + i as f64,
                size: 1.0,
                timestamp_ns: i,
            });
        }
        let momentum = PerceptionRouter::calculate_momentum(&ctx);
        assert!(momentum > 0.0); // Rising prices = positive momentum
    }
}

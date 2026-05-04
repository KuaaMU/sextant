//! SwarmCoordinator: multi-agent coordination and conflict resolution.

use nautilus_state_encoder::ContextWindow;
use tracing::{debug, info};

use crate::agent::{Agent, AgentFeedback};
use crate::compiler::IntentCompiler;
use crate::intent::{AgentIntent, ExecutionDirective, IntentType};
use crate::perception::router::PerceptionRouter;

/// Strategy for resolving conflicts between multiple agents.
#[derive(Clone, Debug)]
pub enum ConsensusStrategy {
    /// Risk agent has veto power. Priority-ordered.
    Hierarchical { priority: Vec<String> },
    /// Weighted vote by reputation score.
    WeightedVote,
    /// Serial pipeline: Perception → Strategy → Risk → Execution.
    Pipeline,
}

/// Coordinates multiple agents and compiles their intents into directives.
pub struct SwarmCoordinator {
    agents: Vec<Box<dyn Agent>>,
    consensus: ConsensusStrategy,
    router: Option<PerceptionRouter>,
    /// Default venue for instrument IDs (e.g., "OKX").
    venue: String,
}

impl SwarmCoordinator {
    pub fn new(consensus: ConsensusStrategy) -> Self {
        Self {
            agents: Vec::new(),
            consensus,
            router: None,
            venue: "OKX".to_string(),
        }
    }

    /// Set the default venue for instrument IDs (default: "OKX").
    pub fn with_venue(mut self, venue: &str) -> Self {
        self.venue = venue.to_string();
        self
    }

    /// Attach a three-layer perception router to the swarm.
    pub fn with_router(mut self, router: PerceptionRouter) -> Self {
        self.router = Some(router);
        self
    }

    /// Register an agent in the swarm.
    pub fn add_agent(&mut self, agent: Box<dyn Agent>) {
        info!("Registered agent: {}", agent.id());
        self.agents.push(agent);
    }

    /// Run one perception-decision cycle across all agents.
    ///
    /// If a router is attached, it runs first as a baseline perception layer.
    /// Agents then refine or override the router's decision.
    pub async fn run_cycle(&mut self, ctx: &ContextWindow) -> Vec<ExecutionDirective> {
        // === P0: Staleness guard — skip cycle if data is too old ===
        // 5 seconds = 5_000_000_000 nanoseconds
        if ctx.is_stale(5_000_000_000) {
            debug!("ContextWindow stale (>5s) — skipping swarm cycle");
            return vec![];
        }

        // 1. Router baseline (if attached)
        let mut intents = Vec::new();
        if let Some(ref router) = self.router {
            let decision = router.route(ctx).await;
            info!(
                "Router baseline: {:?} from {:?} (confidence {:.2})",
                decision.intent_type, decision.layer, decision.confidence
            );
            let symbol = ctx.instrument_id_str();
            let instrument = nautilus_model::identifiers::InstrumentId::from(
                format!("{}.{}", symbol, self.venue).as_str()
            );
            intents.push(decision.to_intent("router", instrument));
        }

        // 2. Collect intents from all agents
        for agent in &mut self.agents {
            let intent = agent.perceive(ctx).await;
            debug!(
                "Agent '{}' produced intent: {:?} (confidence: {:.2})",
                agent.id(),
                intent.intent_type,
                intent.confidence
            );
            intents.push(intent);
        }

        // 2. Resolve conflicts
        let resolved = self.resolve_conflicts(intents);

        // 3. Compile to execution directives
        resolved
            .into_iter()
            .filter_map(|intent| {
                match IntentCompiler::compile(&intent, ctx) {
                    Ok(directive) => Some(directive),
                    Err(e) => {
                        debug!("IntentCompiler rejected intent from '{}': {}", intent.agent_id, e);
                        None
                    }
                }
            })
            .collect()
    }

    /// Resolve conflicts between intents on the same instrument.
    fn resolve_conflicts(&self, intents: Vec<AgentIntent>) -> Vec<AgentIntent> {
        match &self.consensus {
            ConsensusStrategy::Pipeline => {
                // === P0: Pick highest-confidence non-Hold intent ===
                // TODO(P1): Use ConsensusEngine trait with normalized confidence
                let mut first_intent: Option<AgentIntent> = None;
                let non_hold: Vec<AgentIntent> = intents
                    .into_iter()
                    .filter_map(|intent| {
                        if first_intent.is_none() {
                            first_intent = Some(intent.clone());
                        }
                        if intent.intent_type != IntentType::Hold {
                            Some(intent)
                        } else {
                            None
                        }
                    })
                    .collect();

                match non_hold.len() {
                    0 => {
                        // All agents said hold — return first intent as-is
                        first_intent.into_iter().collect()
                    }
                    1 => non_hold,
                    _ => {
                        // Multiple non-Hold — pick highest confidence (NaN-safe)
                        let best = non_hold
                            .into_iter()
                            .max_by(|a, b| {
                                // NaN treated as lowest confidence
                                let ca = if a.confidence.is_nan() { -1.0 } else { a.confidence };
                                let cb = if b.confidence.is_nan() { -1.0 } else { b.confidence };
                                ca.partial_cmp(&cb).unwrap_or(std::cmp::Ordering::Equal)
                            })
                            .unwrap();
                        vec![best]
                    }
                }
            }
            ConsensusStrategy::Hierarchical { priority } => {
                // Group by instrument, pick highest-priority non-Hold intent
                let mut by_instrument: std::collections::HashMap<String, Vec<AgentIntent>> =
                    std::collections::HashMap::new();
                for intent in intents {
                    by_instrument
                        .entry(intent.target_instrument.to_string())
                        .or_default()
                        .push(intent);
                }

                let mut result = Vec::new();
                for (_, group) in by_instrument {
                    let best = group
                        .into_iter()
                        .filter(|i| i.intent_type != IntentType::Hold)
                        .max_by(|a, b| {
                            let pa = priority
                                .iter()
                                .position(|id| id == &a.agent_id)
                                .unwrap_or(usize::MAX);
                            let pb = priority
                                .iter()
                                .position(|id| id == &b.agent_id)
                                .unwrap_or(usize::MAX);
                            pb.cmp(&pa) // lower index = higher priority
                        });

                    match best {
                        Some(intent) => result.push(intent),
                        None => {
                            // All hold for this instrument
                        }
                    }
                }
                result
            }
            ConsensusStrategy::WeightedVote => {
                // Simple: take the intent with highest confidence
                let mut by_instrument: std::collections::HashMap<String, Vec<AgentIntent>> =
                    std::collections::HashMap::new();
                for intent in intents {
                    by_instrument
                        .entry(intent.target_instrument.to_string())
                        .or_default()
                        .push(intent);
                }

                by_instrument
                    .into_values()
                    .filter_map(|group| {
                        group
                            .into_iter()
                            .filter(|i| i.intent_type != IntentType::Hold)
                            .max_by(|a, b| {
                                a.confidence
                                    .partial_cmp(&b.confidence)
                                    .unwrap_or(std::cmp::Ordering::Equal)
                            })
                    })
                    .collect()
            }
        }
    }

    /// Forward order fill feedback to all agents.
    pub async fn on_order_filled(&mut self, feedback: &AgentFeedback) {
        for agent in &mut self.agents {
            agent.on_feedback(feedback).await;
        }
    }

    /// Get the number of registered agents.
    pub fn agent_count(&self) -> usize {
        self.agents.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::AgentFeedback;
    use async_trait::async_trait;
    use nautilus_model::identifiers::InstrumentId;
    use std::time::Duration;

    struct MockAgent {
        id: String,
        intent_type: IntentType,
        confidence: f64,
    }

    #[async_trait]
    impl Agent for MockAgent {
        fn id(&self) -> &str {
            &self.id
        }

        async fn perceive(&mut self, _ctx: &ContextWindow) -> AgentIntent {
            AgentIntent {
                id: nautilus_core::UUID4::new(),
                agent_id: self.id.clone(),
                intent_type: self.intent_type,
                description: format!("{:?} from {}", self.intent_type, self.id),
                target_instrument: InstrumentId::from("SOL-USDC.OKX"),
                target_position: Some(crate::intent::PositionTarget {
                    size: 10.0,
                    delta: None,
                }),
                risk_budget: crate::intent::RiskBudget {
                    max_loss: 100.0,
                    max_position: 100.0,
                    max_drawdown_bps: 500.0,
                },
                constraints: vec![],
                confidence: self.confidence,
                time_horizon: Duration::from_secs(300),
            }
        }

        async fn on_feedback(&mut self, _feedback: &AgentFeedback) {}

        fn confidence(&self) -> f64 {
            self.confidence
        }
    }

    /// Create a non-stale ContextWindow for tests.
    fn fresh_ctx() -> ContextWindow {
        let mut ctx = ContextWindow::zeroed();
        ctx.timestamp_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        ctx
    }

    #[tokio::test]
    async fn test_swarm_pipeline() {
        let mut swarm = SwarmCoordinator::new(ConsensusStrategy::Pipeline);
        swarm.add_agent(Box::new(MockAgent {
            id: "perception".to_string(),
            intent_type: IntentType::TrendFollow,
            confidence: 0.7,
        }));
        swarm.add_agent(Box::new(MockAgent {
            id: "strategy".to_string(),
            intent_type: IntentType::DeltaHedge,
            confidence: 0.9,
        }));

        let ctx = fresh_ctx();
        let directives = swarm.run_cycle(&ctx).await;
        assert!(!directives.is_empty());
    }

    #[tokio::test]
    async fn test_swarm_weighted_vote() {
        let mut swarm = SwarmCoordinator::new(ConsensusStrategy::WeightedVote);
        swarm.add_agent(Box::new(MockAgent {
            id: "low-confidence".to_string(),
            intent_type: IntentType::MeanReversion,
            confidence: 0.3,
        }));
        swarm.add_agent(Box::new(MockAgent {
            id: "high-confidence".to_string(),
            intent_type: IntentType::TrendFollow,
            confidence: 0.9,
        }));

        let ctx = fresh_ctx();
        let directives = swarm.run_cycle(&ctx).await;
        // High confidence agent should win
        assert_eq!(directives.len(), 1);
    }
}

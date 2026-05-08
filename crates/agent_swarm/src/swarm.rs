//! SwarmCoordinator: multi-agent coordination and conflict resolution.

use nautilus_state_encoder::ContextWindow;
use tracing::{debug, info};

use crate::agent::{Agent, AgentFeedback};
use crate::compiler::IntentCompiler;
use crate::consensus::{ConsensusEngine, HierarchicalConsensus, PipelineConsensus, WeightedVoteConsensus, normalize_confidences};
use crate::intent::{AgentIntent, ExecutionDirective};
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

impl ConsensusStrategy {
    fn to_engine(&self) -> Box<dyn ConsensusEngine> {
        match self {
            Self::Pipeline => Box::new(PipelineConsensus),
            Self::WeightedVote => Box::new(WeightedVoteConsensus),
            Self::Hierarchical { priority } => Box::new(HierarchicalConsensus {
                priority: priority.clone(),
            }),
        }
    }
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
            let mut intent = agent.perceive(ctx).await;
            intent.reputation_score = agent.reputation_score();
            debug!(
                "Agent '{}' produced intent: {:?} (confidence: {:.2}, reputation: {:.2})",
                agent.id(),
                intent.intent_type,
                intent.confidence,
                intent.reputation_score
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
    ///
    /// Normalizes confidence scores across agents before passing to the
    /// consensus engine, so agents with different confidence scales compete fairly.
    fn resolve_conflicts(&self, mut intents: Vec<AgentIntent>) -> Vec<AgentIntent> {
        // Normalize confidences across agents before consensus
        normalize_confidences(&mut intents);

        let engine = self.consensus.to_engine();
        engine.resolve(intents)
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
    use crate::intent::IntentType;
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
                reputation_score: 0.5,
                time_horizon: Duration::from_secs(300),
                title: format!("{:?} from {}", self.intent_type, self.id),
                reasoning: String::new(),
                confidence_label: crate::intent::ConfidenceLabel::Low,
                risk_snapshot: crate::intent::RiskSnapshot::default(),
                expires_at: None,
                tags: vec![],
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

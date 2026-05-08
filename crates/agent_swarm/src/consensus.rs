//! ConsensusEngine — pluggable conflict resolution for multi-agent swarms.
//!
//! Normalizes confidence scores across agents before comparing, so agents
//! with different confidence scales compete fairly.

use crate::intent::{AgentIntent, IntentType};
use tracing::info;

/// Trait for pluggable consensus strategies.
pub trait ConsensusEngine: Send + Sync {
    /// Resolve a group of intents into a single winning intent.
    /// Intents are already confidence-normalized when passed here.
    fn resolve(&self, intents: Vec<AgentIntent>) -> Vec<AgentIntent>;

    /// Human-readable name for logging.
    fn name(&self) -> &str;
}

/// Pipeline consensus: Veto → pick highest normalized confidence.
pub struct PipelineConsensus;

impl ConsensusEngine for PipelineConsensus {
    fn resolve(&self, intents: Vec<AgentIntent>) -> Vec<AgentIntent> {
        // Veto overrides everything
        let veto = intents.iter().find(|i| i.intent_type == IntentType::Veto);
        if let Some(veto_intent) = veto {
            info!(
                "RISK VETO from '{}': {} — all intents overridden",
                veto_intent.agent_id, veto_intent.description
            );
            return vec![AgentIntent::hold(
                &veto_intent.agent_id,
                veto_intent.target_instrument,
            )];
        }

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
            0 => first_intent.into_iter().collect(),
            1 => non_hold,
            _ => {
                let best = non_hold
                    .into_iter()
                    .max_by(|a, b| {
                        let ca = if a.confidence.is_nan() { -1.0 } else { a.confidence };
                        let cb = if b.confidence.is_nan() { -1.0 } else { b.confidence };
                        ca.partial_cmp(&cb).unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .unwrap();
                vec![best]
            }
        }
    }

    fn name(&self) -> &str {
        "Pipeline"
    }
}

/// Weighted vote: score = normalized_confidence * reputation_score.
pub struct WeightedVoteConsensus;

impl ConsensusEngine for WeightedVoteConsensus {
    fn resolve(&self, intents: Vec<AgentIntent>) -> Vec<AgentIntent> {
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
                        let score_a = a.confidence * a.reputation_score;
                        let score_b = b.confidence * b.reputation_score;
                        score_a
                            .partial_cmp(&score_b)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
            })
            .collect()
    }

    fn name(&self) -> &str {
        "WeightedVote"
    }
}

/// Hierarchical: priority-ordered, highest-priority non-Hold wins per instrument.
pub struct HierarchicalConsensus {
    pub priority: Vec<String>,
}

impl ConsensusEngine for HierarchicalConsensus {
    fn resolve(&self, intents: Vec<AgentIntent>) -> Vec<AgentIntent> {
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
                    let pa = self
                        .priority
                        .iter()
                        .position(|id| id == &a.agent_id)
                        .unwrap_or(usize::MAX);
                    let pb = self
                        .priority
                        .iter()
                        .position(|id| id == &b.agent_id)
                        .unwrap_or(usize::MAX);
                    pb.cmp(&pa) // lower index = higher priority
                });

            if let Some(intent) = best {
                result.push(intent);
            }
        }
        result
    }

    fn name(&self) -> &str {
        "Hierarchical"
    }
}

/// Normalize confidence scores across agents using min-max scaling.
///
/// Each agent's confidence is mapped to [0, 1] based on the observed range
/// in this batch. This prevents agents with naturally wider confidence ranges
/// from dominating the consensus.
///
/// If an agent has only one intent (or all identical), confidence stays as-is.
pub fn normalize_confidences(intents: &mut [AgentIntent]) {
    // Group by agent_id
    let mut agent_ranges: std::collections::HashMap<String, (f64, f64)> =
        std::collections::HashMap::new();

    for intent in intents.iter() {
        let c = intent.confidence;
        if c.is_nan() {
            continue;
        }
        let entry = agent_ranges
            .entry(intent.agent_id.clone())
            .or_insert((c, c));
        entry.0 = entry.0.min(c);
        entry.1 = entry.1.max(c);
    }

    // Apply normalization
    for intent in intents.iter_mut() {
        if let Some(&(min, max)) = agent_ranges.get(&intent.agent_id) {
            let range = max - min;
            if range > 1e-10 {
                intent.confidence = (intent.confidence - min) / range;
            }
            // else: single value or all identical — keep as-is
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nautilus_core::UUID4;
    use nautilus_model::identifiers::InstrumentId;
    use std::time::Duration;

    fn make_intent(agent_id: &str, intent_type: IntentType, confidence: f64) -> AgentIntent {
        AgentIntent {
            id: UUID4::new(),
            agent_id: agent_id.to_string(),
            intent_type,
            description: String::new(),
            target_instrument: InstrumentId::from("BTC-USDT-SWAP.OKX"),
            target_position: None,
            risk_budget: crate::intent::RiskBudget {
                max_loss: 100.0,
                max_position: 100.0,
                max_drawdown_bps: 500.0,
            },
            constraints: vec![],
            confidence,
            reputation_score: 0.5,
            time_horizon: Duration::from_secs(300),
            title: String::new(),
            reasoning: String::new(),
            confidence_label: crate::intent::ConfidenceLabel::Low,
            risk_snapshot: crate::intent::RiskSnapshot::default(),
            expires_at: None,
            tags: vec![],
        }
    }

    #[test]
    fn test_pipeline_picks_highest_confidence() {
        let engine = PipelineConsensus;
        let intents = vec![
            make_intent("a", IntentType::TrendFollow, 0.3),
            make_intent("b", IntentType::MeanReversion, 0.9),
            make_intent("c", IntentType::TrendFollow, 0.6),
        ];
        let result = engine.resolve(intents);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].agent_id, "b");
    }

    #[test]
    fn test_pipeline_veto_overrides() {
        let engine = PipelineConsensus;
        let intents = vec![
            make_intent("a", IntentType::TrendFollow, 0.9),
            make_intent("risk", IntentType::Veto, 1.0),
        ];
        let result = engine.resolve(intents);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].intent_type, IntentType::Hold);
    }

    #[test]
    fn test_pipeline_all_hold() {
        let engine = PipelineConsensus;
        let intents = vec![
            make_intent("a", IntentType::Hold, 0.0),
            make_intent("b", IntentType::Hold, 0.0),
        ];
        let result = engine.resolve(intents);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].intent_type, IntentType::Hold);
    }

    #[test]
    fn test_weighted_vote() {
        let engine = WeightedVoteConsensus;
        let mut a = make_intent("a", IntentType::TrendFollow, 0.8);
        a.reputation_score = 0.3;
        let mut b = make_intent("b", IntentType::MeanReversion, 0.6);
        b.reputation_score = 0.9;
        // a: 0.8*0.3=0.24, b: 0.6*0.9=0.54 → b wins
        let result = engine.resolve(vec![a, b]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].agent_id, "b");
    }

    #[test]
    fn test_hierarchical_priority() {
        let engine = HierarchicalConsensus {
            priority: vec!["risk".to_string(), "strategy".to_string()],
        };
        let intents = vec![
            make_intent("strategy", IntentType::TrendFollow, 0.9),
            make_intent("risk", IntentType::MeanReversion, 0.5),
        ];
        let result = engine.resolve(intents);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].agent_id, "risk"); // higher priority
    }

    #[test]
    fn test_normalize_same_agent() {
        let mut intents = vec![
            make_intent("a", IntentType::TrendFollow, 0.2),
            make_intent("a", IntentType::TrendFollow, 0.8),
        ];
        normalize_confidences(&mut intents);
        // 0.2 → 0.0, 0.8 → 1.0
        assert!((intents[0].confidence - 0.0).abs() < 1e-10);
        assert!((intents[1].confidence - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_normalize_cross_agent() {
        // Agent A: [0.4, 0.8] → [0.0, 1.0]
        // Agent B: [0.1, 0.9] → [0.0, 1.0]
        let mut intents = vec![
            make_intent("a", IntentType::TrendFollow, 0.6),  // mid → 0.5
            make_intent("b", IntentType::MeanReversion, 0.5), // mid → 0.5
        ];
        normalize_confidences(&mut intents);
        // Both should be at 0.5 after normalization
        // But with single values per agent, range=0 → keep as-is
        assert!((intents[0].confidence - 0.6).abs() < 1e-10);
        assert!((intents[1].confidence - 0.5).abs() < 1e-10);
    }

    #[test]
    fn test_normalize_different_scales() {
        // Agent A outputs [0.4, 0.6] (tight range)
        // Agent B outputs [0.1, 0.9] (wide range)
        // Raw comparison: A's 0.6 > B's 0.5 → A wins
        // After normalization: A's 0.6→1.0, B's 0.5→0.5 → A still wins
        // But B's 0.9→1.0 ties A's max
        let mut intents = vec![
            make_intent("a", IntentType::TrendFollow, 0.4),
            make_intent("a", IntentType::TrendFollow, 0.6),
            make_intent("b", IntentType::MeanReversion, 0.1),
            make_intent("b", IntentType::MeanReversion, 0.9),
        ];
        normalize_confidences(&mut intents);
        // A: (0.4-0.4)/0.2=0.0, (0.6-0.4)/0.2=1.0
        // B: (0.1-0.1)/0.8=0.0, (0.9-0.1)/0.8=1.0
        assert!((intents[0].confidence - 0.0).abs() < 1e-10);
        assert!((intents[1].confidence - 1.0).abs() < 1e-10);
        assert!((intents[2].confidence - 0.0).abs() < 1e-10);
        assert!((intents[3].confidence - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_normalize_nan_safe() {
        let mut intents = vec![
            make_intent("a", IntentType::TrendFollow, f64::NAN),
            make_intent("a", IntentType::TrendFollow, 0.5),
        ];
        normalize_confidences(&mut intents);
        // NaN skipped in range calculation, 0.5 stays as single value
        assert!(intents[0].confidence.is_nan());
        assert!((intents[1].confidence - 0.5).abs() < 1e-10);
    }
}

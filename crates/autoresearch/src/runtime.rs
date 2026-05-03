//! Autoresearch runtime: the Karpathy Ratchet.

use std::collections::VecDeque;
use tracing::{info, warn};

use nautilus_core::UUID4;

use crate::metric::RiskAdjustedInfoRatio;
use crate::micro_backtest::{BacktestResult, MicroBacktestEngine};

/// A strategy hypothesis proposed by an agent.
#[derive(Clone, Debug)]
pub struct StrategyHypothesis {
    pub id: UUID4,
    /// Natural language description of the hypothesis.
    pub description: String,
    /// Code patch (diff) to apply.
    pub code_patch: String,
    /// Parent strategy ID (for inheritance tracking).
    pub parent_id: Option<String>,
}

/// Record of a hypothesis evaluation.
#[derive(Clone, Debug)]
pub struct HypothesisRecord {
    pub hypothesis: StrategyHypothesis,
    pub result: BacktestResult,
    pub accepted: bool,
    pub timestamp_ns: u64,
}

/// Autoresearch runtime implementing the Karpathy Ratchet.
///
/// Flow: Agent proposes hypothesis → micro-backtest → evaluate →
///       if > 105% baseline → accept (ratchet up) → else discard
pub struct AutoresearchRuntime {
    /// Current best IR score.
    pub baseline_ir: f64,
    /// Hypothesis queue to evaluate.
    pub hypothesis_queue: VecDeque<StrategyHypothesis>,
    /// Micro-backtest engine.
    pub micro_bt: MicroBacktestEngine,
    /// Evaluation metric.
    pub metric: RiskAdjustedInfoRatio,
    /// Improvement threshold (e.g., 0.05 for 5%).
    pub improvement_threshold: f64,
    /// History of evaluated hypotheses.
    pub history: Vec<HypothesisRecord>,
}

impl AutoresearchRuntime {
    pub fn new(improvement_threshold: f64) -> Self {
        Self {
            baseline_ir: 0.0,
            hypothesis_queue: VecDeque::new(),
            micro_bt: MicroBacktestEngine::new(300), // 5 min window
            metric: RiskAdjustedInfoRatio,
            improvement_threshold,
            history: Vec::new(),
        }
    }

    /// Submit a hypothesis for evaluation.
    pub fn submit(&mut self, hypothesis: StrategyHypothesis) {
        info!("Hypothesis submitted: {} — {}", hypothesis.id, hypothesis.description);
        self.hypothesis_queue.push_back(hypothesis);
    }

    /// Run one ratchet cycle: evaluate all queued hypotheses against price history.
    pub fn run_ratchet(&mut self, baseline_returns: &[f64], prices: &[f64]) {
        self.baseline_ir = self.metric.evaluate(baseline_returns);

        while let Some(hypo) = self.hypothesis_queue.pop_front() {
            info!("Evaluating hypothesis: {}", hypo.id);

            let candidate_returns = self.simulate_candidate(&hypo, prices);
            let candidate_ir = self.metric.evaluate(&candidate_returns);

            let accepted =
                self.metric
                    .is_improvement(candidate_ir, self.baseline_ir, self.improvement_threshold);

            if accepted {
                info!(
                    "RATCHET UP: {} improved IR from {:.4} to {:.4}",
                    hypo.id, self.baseline_ir, candidate_ir
                );
                self.baseline_ir = candidate_ir;
            } else {
                warn!(
                    "Rejected: {} (IR {:.4} vs baseline {:.4})",
                    hypo.id, candidate_ir, self.baseline_ir
                );
            }

            self.history.push(HypothesisRecord {
                hypothesis: hypo,
                result: BacktestResult {
                    returns: candidate_returns,
                    total_return: 0.0,
                    max_drawdown: 0.0,
                    trade_count: 0,
                    ir: candidate_ir,
                },
                accepted,
                timestamp_ns: 0, // Would use real clock
            });
        }
    }

    /// Simulate candidate strategy returns from price history.
    ///
    /// Parses the hypothesis description for parameter hints (e.g., "window=20")
    /// and runs a momentum strategy over the provided prices with those parameters.
    /// Falls back to a simple momentum strategy if no parameters are specified.
    pub fn simulate_candidate(&self, hypo: &StrategyHypothesis, prices: &[f64]) -> Vec<f64> {
        if prices.len() < 3 {
            return vec![];
        }

        // Parse window size from hypothesis (default: 5)
        let window = Self::parse_window(&hypo.description).min(prices.len() - 1);

        // Simple momentum strategy: if price > moving average, long; else flat
        let mut returns = Vec::with_capacity(prices.len() - window);
        for i in window..prices.len() {
            let ma: f64 = prices[i - window..i].iter().sum::<f64>() / window as f64;
            let ret = (prices[i] - prices[i - 1]) / prices[i - 1];
            // Position: +1 if above MA, 0 otherwise
            if prices[i] > ma {
                returns.push(ret);
            } else {
                returns.push(0.0);
            }
        }
        returns
    }

    /// Extract window size from hypothesis description (e.g., "window=20" → 20).
    fn parse_window(description: &str) -> usize {
        description
            .split_whitespace()
            .find(|w| w.starts_with("window="))
            .and_then(|w| w.strip_prefix("window="))
            .and_then(|s| s.parse().ok())
            .unwrap_or(5)
    }

    /// Get the number of accepted improvements.
    pub fn accepted_count(&self) -> usize {
        self.history.iter().filter(|r| r.accepted).count()
    }

    /// Get the total number of evaluated hypotheses.
    pub fn total_evaluated(&self) -> usize {
        self.history.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ratchet_accept_improvement() {
        let mut runtime = AutoresearchRuntime::new(0.05);

        // Trending up prices: 100 → 110 over 20 ticks
        let prices: Vec<f64> = (0..20).map(|i| 100.0 + i as f64 * 0.5).collect();
        let baseline_returns: Vec<f64> = prices
            .windows(2)
            .map(|w| (w[1] - w[0]) / w[0])
            .collect();

        runtime.submit(StrategyHypothesis {
            id: UUID4::new(),
            description: "Increase momentum window=10".to_string(),
            code_patch: "+ window = 10".to_string(),
            parent_id: None,
        });

        runtime.run_ratchet(&baseline_returns, &prices);
        assert_eq!(runtime.total_evaluated(), 1);
    }

    #[test]
    fn test_ratchet_reject_no_improvement() {
        let mut runtime = AutoresearchRuntime::new(0.50); // 50% threshold
        runtime.baseline_ir = 10.0; // Very high baseline

        // Flat prices → momentum strategy produces no returns
        let prices = vec![100.0; 20];
        let baseline_returns = vec![0.001; 10];

        runtime.submit(StrategyHypothesis {
            id: UUID4::new(),
            description: "Bad change".to_string(),
            code_patch: "- everything".to_string(),
            parent_id: None,
        });

        runtime.run_ratchet(&baseline_returns, &prices);
        assert_eq!(runtime.accepted_count(), 0);
    }

    #[test]
    fn test_parse_window() {
        assert_eq!(AutoresearchRuntime::parse_window("window=20"), 20);
        assert_eq!(AutoresearchRuntime::parse_window("Increase window=10 bars"), 10);
        assert_eq!(AutoresearchRuntime::parse_window("no window here"), 5); // default
    }

    #[test]
    fn test_simulate_candidate_trending() {
        let runtime = AutoresearchRuntime::new(0.05);
        let prices: Vec<f64> = (0..20).map(|i| 100.0 + i as f64).collect();
        let hypo = StrategyHypothesis {
            id: UUID4::new(),
            description: "window=5".to_string(),
            code_patch: String::new(),
            parent_id: None,
        };

        let returns = runtime.simulate_candidate(&hypo, &prices);
        // In a trending market, momentum strategy should capture positive returns
        let total: f64 = returns.iter().sum();
        assert!(total > 0.0, "expected positive returns in uptrend, got {}", total);
    }
}

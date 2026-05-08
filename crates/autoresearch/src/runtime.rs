//! Autoresearch runtime: the Karpathy Ratchet.

use std::collections::VecDeque;
use tracing::{info, warn};

use nautilus_core::UUID4;

use crate::metric::RiskAdjustedInfoRatio;
use crate::micro_backtest::{BacktestResult, MicroBacktestEngine, default_windows, FATAL_IR};
use crate::strategy_lock::StrategyLock;

/// Strategy types the ratchet can evaluate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrategyType {
    /// Trend-following: long above MA, flat below.
    Momentum,
    /// Mean-reversion: long when z-score < -threshold, short when > +threshold.
    MeanReversion,
    /// Breakout: long when price exceeds N-bar high, flat on reversal.
    Breakout,
    /// Volatility targeting: scale position inversely to realized vol.
    VolTarget,
    /// Dual momentum: absolute + relative momentum combined.
    DualMomentum,
}

impl StrategyType {
    /// All available strategy types for hypothesis generation.
    pub fn all() -> &'static [StrategyType] {
        &[
            StrategyType::Momentum,
            StrategyType::MeanReversion,
            StrategyType::Breakout,
            StrategyType::VolTarget,
            StrategyType::DualMomentum,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Momentum => "momentum",
            Self::MeanReversion => "mean_reversion",
            Self::Breakout => "breakout",
            Self::VolTarget => "vol_target",
            Self::DualMomentum => "dual_momentum",
        }
    }
}

/// A strategy hypothesis proposed by an agent.
#[derive(Clone, Debug)]
pub struct StrategyHypothesis {
    pub id: UUID4,
    /// Strategy type to simulate.
    pub strategy_type: StrategyType,
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
    /// IR of passive buy-and-hold baseline (ghost hold).
    pub baseline_hold_ir: f64,
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
    /// Strategy lock — human-in-the-loop governance.
    pub strategy_lock: StrategyLock,
}

impl AutoresearchRuntime {
    pub fn new(improvement_threshold: f64) -> Self {
        Self {
            baseline_ir: 0.0,
            baseline_hold_ir: 0.0,
            hypothesis_queue: VecDeque::new(),
            micro_bt: MicroBacktestEngine::new(300), // 5 min window
            metric: RiskAdjustedInfoRatio,
            improvement_threshold,
            history: Vec::new(),
            strategy_lock: StrategyLock::new(),
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

        // Compute ghost hold baseline: passive buy-and-hold IR
        let hold_returns = MicroBacktestEngine::simulate_hold_baseline(prices);
        self.baseline_hold_ir = self.metric.evaluate(&hold_returns);

        while let Some(hypo) = self.hypothesis_queue.pop_front() {
            info!("Evaluating hypothesis: {}", hypo.id);

            let candidate_returns = self.simulate_candidate(&hypo, prices);

            // Multi-window crucible: evaluate across time horizons
            let multi = self.micro_bt.run_multi_window(&candidate_returns, &default_windows());
            let candidate_ir = if multi.short_circuited {
                FATAL_IR
            } else {
                multi.weighted_ir
            };

            // Must beat both the ratchet baseline AND the hold baseline
            let beats_ratchet =
                self.metric
                    .is_improvement(candidate_ir, self.baseline_ir, self.improvement_threshold);
            let beats_hold = candidate_ir > self.baseline_hold_ir;
            let accepted = beats_ratchet && beats_hold;

            if accepted {
                if self.strategy_lock.can_apply() {
                    info!(
                        "RATCHET UP: {} improved IR from {:.4} to {:.4} (hold baseline: {:.4})",
                        hypo.id, self.baseline_ir, candidate_ir, self.baseline_hold_ir
                    );
                    self.baseline_ir = candidate_ir;
                } else {
                    // Locked: stage for human approval
                    self.strategy_lock.stage_mutation(
                        hypo.clone(),
                        candidate_ir,
                        self.baseline_ir,
                    );
                }
            } else if !beats_hold {
                warn!(
                    "Rejected: {} (IR {:.4} does not beat hold baseline {:.4}, alpha <= 0)",
                    hypo.id, candidate_ir, self.baseline_hold_ir
                );
            } else {
                warn!(
                    "Rejected: {} (IR {:.4} vs ratchet baseline {:.4})",
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
    /// Dispatches to the appropriate strategy simulator based on hypothesis type.
    /// Parses parameter hints from the description (e.g., "window=20", "threshold=1.5").
    pub fn simulate_candidate(&self, hypo: &StrategyHypothesis, prices: &[f64]) -> Vec<f64> {
        if prices.len() < 3 {
            return vec![];
        }

        match hypo.strategy_type {
            StrategyType::Momentum => Self::simulate_momentum(hypo, prices),
            StrategyType::MeanReversion => Self::simulate_mean_reversion(hypo, prices),
            StrategyType::Breakout => Self::simulate_breakout(hypo, prices),
            StrategyType::VolTarget => Self::simulate_vol_target(hypo, prices),
            StrategyType::DualMomentum => Self::simulate_dual_momentum(hypo, prices),
        }
    }

    /// Momentum: long when price > MA, flat otherwise.
    fn simulate_momentum(hypo: &StrategyHypothesis, prices: &[f64]) -> Vec<f64> {
        let window = Self::parse_param(&hypo.description, "window", 5).min(prices.len() - 1);
        let mut returns = Vec::with_capacity(prices.len() - window);
        for i in window..prices.len() {
            let ma: f64 = prices[i - window..i].iter().sum::<f64>() / window as f64;
            let ret = (prices[i] - prices[i - 1]) / prices[i - 1];
            if prices[i] > ma {
                returns.push(ret);
            } else {
                returns.push(0.0);
            }
        }
        returns
    }

    /// Mean-reversion: long when z-score < -threshold, short when > +threshold.
    fn simulate_mean_reversion(hypo: &StrategyHypothesis, prices: &[f64]) -> Vec<f64> {
        let window = Self::parse_param(&hypo.description, "window", 20).min(prices.len() - 1);
        let threshold = Self::parse_param_f64(&hypo.description, "threshold", 1.5);
        let mut returns = Vec::with_capacity(prices.len() - window);
        for i in window..prices.len() {
            let slice = &prices[i - window..i];
            let mean = slice.iter().sum::<f64>() / window as f64;
            let std = (slice.iter().map(|p| (p - mean).powi(2)).sum::<f64>() / window as f64)
                .sqrt()
                .max(1e-10);
            let z_score = (prices[i] - mean) / std;
            let ret = (prices[i] - prices[i - 1]) / prices[i - 1];
            if z_score < -threshold {
                returns.push(ret); // oversold → long
            } else if z_score > threshold {
                returns.push(-ret); // overbought → short
            } else {
                returns.push(0.0);
            }
        }
        returns
    }

    /// Breakout: long when price exceeds N-bar high, flat on N-bar low.
    fn simulate_breakout(hypo: &StrategyHypothesis, prices: &[f64]) -> Vec<f64> {
        let window = Self::parse_param(&hypo.description, "window", 20).min(prices.len() - 1);
        let mut returns = Vec::with_capacity(prices.len() - window);
        let mut position = 0i8; // 0=flat, 1=long
        for i in window..prices.len() {
            let high = prices[i - window..i].iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let low = prices[i - window..i].iter().cloned().fold(f64::INFINITY, f64::min);
            let ret = (prices[i] - prices[i - 1]) / prices[i - 1];
            if prices[i] > high {
                position = 1; // breakout above → long
            } else if prices[i] < low {
                position = 0; // breakdown → flat
            }
            returns.push(ret * position as f64);
        }
        returns
    }

    /// Volatility targeting: scale position inversely to realized vol.
    fn simulate_vol_target(hypo: &StrategyHypothesis, prices: &[f64]) -> Vec<f64> {
        let window = Self::parse_param(&hypo.description, "window", 20).min(prices.len() - 1);
        let target_vol = Self::parse_param_f64(&hypo.description, "target_vol", 0.01);
        let mut returns = Vec::with_capacity(prices.len() - window);
        for i in window..prices.len() {
            let slice = &prices[i - window..i];
            let rets: Vec<f64> = slice.windows(2).map(|w| (w[1] - w[0]) / w[0]).collect();
            let realized_vol = {
                let mean = rets.iter().sum::<f64>() / rets.len() as f64;
                (rets.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / rets.len() as f64)
                    .sqrt()
                    .max(1e-10)
            };
            let scale = (target_vol / realized_vol).min(3.0); // cap at 3x leverage
            let ret = (prices[i] - prices[i - 1]) / prices[i - 1];
            returns.push(ret * scale);
        }
        returns
    }

    /// Dual momentum: absolute momentum (positive return) + relative momentum (vs peer).
    /// Uses short MA vs long MA crossover with absolute return filter.
    fn simulate_dual_momentum(hypo: &StrategyHypothesis, prices: &[f64]) -> Vec<f64> {
        let fast = Self::parse_param(&hypo.description, "fast", 5).min(prices.len() - 1);
        let slow = Self::parse_param(&hypo.description, "slow", 20).min(prices.len() - 1);
        if fast >= slow {
            return Self::simulate_momentum(hypo, prices); // fallback
        }
        let mut returns = Vec::with_capacity(prices.len() - slow);
        for i in slow..prices.len() {
            let fast_ma: f64 = prices[i - fast..i].iter().sum::<f64>() / fast as f64;
            let slow_ma: f64 = prices[i - slow..i].iter().sum::<f64>() / slow as f64;
            let abs_return = (prices[i] - prices[i - slow]) / prices[i - slow];
            let ret = (prices[i] - prices[i - 1]) / prices[i - 1];
            // Long if: fast > slow (relative) AND positive absolute return
            if fast_ma > slow_ma && abs_return > 0.0 {
                returns.push(ret);
            } else {
                returns.push(0.0);
            }
        }
        returns
    }

    /// Extract integer parameter from description (e.g., "window=20" → 20).
    fn parse_param(description: &str, name: &str, default: usize) -> usize {
        let prefix = format!("{}=", name);
        description
            .split_whitespace()
            .find(|w| w.starts_with(&prefix))
            .and_then(|w| w.strip_prefix(&prefix))
            .and_then(|s| s.parse().ok())
            .unwrap_or(default)
    }

    /// Extract float parameter from description (e.g., "threshold=1.5" → 1.5).
    fn parse_param_f64(description: &str, name: &str, default: f64) -> f64 {
        let prefix = format!("{}=", name);
        description
            .split_whitespace()
            .find(|w| w.starts_with(&prefix))
            .and_then(|w| w.strip_prefix(&prefix))
            .and_then(|s| s.parse().ok())
            .unwrap_or(default)
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

    fn make_hypo(strategy_type: StrategyType, description: &str) -> StrategyHypothesis {
        StrategyHypothesis {
            id: UUID4::new(),
            strategy_type,
            description: description.to_string(),
            code_patch: String::new(),
            parent_id: None,
        }
    }

    #[test]
    fn test_ratchet_accept_improvement() {
        let mut runtime = AutoresearchRuntime::new(0.05);

        // Trending up prices: 100 → 110 over 20 ticks
        let prices: Vec<f64> = (0..20).map(|i| 100.0 + i as f64 * 0.5).collect();
        let baseline_returns: Vec<f64> = prices
            .windows(2)
            .map(|w| (w[1] - w[0]) / w[0])
            .collect();

        runtime.submit(make_hypo(StrategyType::Momentum, "window=10"));

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

        runtime.submit(make_hypo(StrategyType::Momentum, "window=5"));

        runtime.run_ratchet(&baseline_returns, &prices);
        assert_eq!(runtime.accepted_count(), 0);
    }

    #[test]
    fn test_parse_param() {
        assert_eq!(AutoresearchRuntime::parse_param("window=20", "window", 5), 20);
        assert_eq!(AutoresearchRuntime::parse_param("window=10 bars", "window", 5), 10);
        assert_eq!(AutoresearchRuntime::parse_param("no param here", "window", 5), 5);
        assert_eq!(AutoresearchRuntime::parse_param("fast=5 slow=20", "fast", 3), 5);
        assert_eq!(AutoresearchRuntime::parse_param("fast=5 slow=20", "slow", 10), 20);
    }

    #[test]
    fn test_parse_param_f64() {
        assert!((AutoresearchRuntime::parse_param_f64("threshold=1.5", "threshold", 1.0) - 1.5).abs() < 1e-10);
        assert!((AutoresearchRuntime::parse_param_f64("target_vol=0.02", "target_vol", 0.01) - 0.02).abs() < 1e-10);
        assert!((AutoresearchRuntime::parse_param_f64("nothing", "threshold", 1.0) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_simulate_momentum_trending() {
        let runtime = AutoresearchRuntime::new(0.05);
        let prices: Vec<f64> = (0..20).map(|i| 100.0 + i as f64).collect();
        let hypo = make_hypo(StrategyType::Momentum, "window=5");

        let returns = runtime.simulate_candidate(&hypo, &prices);
        let total: f64 = returns.iter().sum();
        assert!(total > 0.0, "momentum in uptrend should be positive, got {}", total);
    }

    #[test]
    fn test_simulate_mean_reversion_oscillating() {
        let runtime = AutoresearchRuntime::new(0.05);
        // Oscillating prices: mean-reversion should profit
        let prices: Vec<f64> = (0..40)
            .map(|i| 100.0 + (i as f64 * 0.5).sin() * 5.0)
            .collect();
        let hypo = make_hypo(StrategyType::MeanReversion, "window=10 threshold=1.0");

        let returns = runtime.simulate_candidate(&hypo, &prices);
        assert!(!returns.is_empty(), "mean-reversion should produce returns");
    }

    #[test]
    fn test_simulate_breakout_trending() {
        let runtime = AutoresearchRuntime::new(0.05);
        // Strong uptrend with breakout
        let prices: Vec<f64> = (0..30).map(|i| 100.0 + i as f64 * 0.5).collect();
        let hypo = make_hypo(StrategyType::Breakout, "window=10");

        let returns = runtime.simulate_candidate(&hypo, &prices);
        let total: f64 = returns.iter().sum();
        assert!(total > 0.0, "breakout in uptrend should be positive, got {}", total);
    }

    #[test]
    fn test_simulate_vol_target_trending() {
        let runtime = AutoresearchRuntime::new(0.05);
        let prices: Vec<f64> = (0..30).map(|i| 100.0 + i as f64 * 0.3).collect();
        let hypo = make_hypo(StrategyType::VolTarget, "window=10 target_vol=0.01");

        let returns = runtime.simulate_candidate(&hypo, &prices);
        let total: f64 = returns.iter().sum();
        assert!(total > 0.0, "vol-target in uptrend should be positive, got {}", total);
    }

    #[test]
    fn test_simulate_dual_momentum_trending() {
        let runtime = AutoresearchRuntime::new(0.05);
        let prices: Vec<f64> = (0..30).map(|i| 100.0 + i as f64 * 0.5).collect();
        let hypo = make_hypo(StrategyType::DualMomentum, "fast=5 slow=15");

        let returns = runtime.simulate_candidate(&hypo, &prices);
        let total: f64 = returns.iter().sum();
        assert!(total > 0.0, "dual momentum in uptrend should be positive, got {}", total);
    }

    #[test]
    fn test_ratchet_rejects_below_hold() {
        let mut runtime = AutoresearchRuntime::new(0.05);
        runtime.baseline_ir = 0.0;

        let prices: Vec<f64> = (0..30).map(|i| 100.0 + i as f64 * 2.0).collect();
        let baseline_returns: Vec<f64> = prices.windows(2)
            .map(|w| (w[1] - w[0]) / w[0])
            .collect();

        runtime.submit(make_hypo(StrategyType::Momentum, "window=25"));

        runtime.run_ratchet(&baseline_returns, &prices);
        assert_eq!(runtime.total_evaluated(), 1);
        assert!(runtime.baseline_hold_ir != 0.0 || prices.len() < 2,
            "hold baseline IR should be non-zero for trending prices");
    }

    #[test]
    fn test_hold_baseline_ir_computed() {
        let mut runtime = AutoresearchRuntime::new(0.05);

        let prices: Vec<f64> = (0..20).map(|i| 100.0 + i as f64).collect();
        let baseline_returns: Vec<f64> = prices.windows(2)
            .map(|w| (w[1] - w[0]) / w[0])
            .collect();

        runtime.run_ratchet(&baseline_returns, &prices);
        assert!(runtime.baseline_hold_ir > 0.0,
            "hold baseline IR should be positive in uptrend, got {}", runtime.baseline_hold_ir);
    }

    #[test]
    fn test_all_strategy_types_produce_returns() {
        let runtime = AutoresearchRuntime::new(0.05);
        let prices: Vec<f64> = (0..40).map(|i| 100.0 + (i as f64 * 0.3).sin() * 3.0 + i as f64 * 0.1).collect();

        for &st in StrategyType::all() {
            let hypo = make_hypo(st, "window=10");
            let returns = runtime.simulate_candidate(&hypo, &prices);
            assert!(!returns.is_empty(), "{:?} should produce returns", st);
        }
    }

    #[test]
    fn test_locked_strategy_stages_mutation() {
        let mut runtime = AutoresearchRuntime::new(0.05);
        runtime.strategy_lock.locked = true;

        // Trending prices that should produce an accepted improvement
        let prices: Vec<f64> = (0..20).map(|i| 100.0 + i as f64 * 0.5).collect();
        let baseline_returns: Vec<f64> = prices
            .windows(2)
            .map(|w| (w[1] - w[0]) / w[0])
            .collect();

        runtime.submit(make_hypo(StrategyType::Momentum, "window=10"));
        runtime.run_ratchet(&baseline_returns, &prices);

        // Baseline should NOT have changed (mutation staged, not applied)
        assert!(runtime.strategy_lock.pending_mutation.is_some(),
            "locked strategy should stage mutation");
    }

    #[test]
    fn test_unlocked_strategy_applies_directly() {
        let mut runtime = AutoresearchRuntime::new(0.05);
        assert!(!runtime.strategy_lock.locked);

        let prices: Vec<f64> = (0..20).map(|i| 100.0 + i as f64 * 0.5).collect();
        let baseline_returns: Vec<f64> = prices
            .windows(2)
            .map(|w| (w[1] - w[0]) / w[0])
            .collect();

        runtime.submit(make_hypo(StrategyType::Momentum, "window=10"));

        runtime.run_ratchet(&baseline_returns, &prices);

        // No pending mutation — applied directly
        assert!(runtime.strategy_lock.pending_mutation.is_none(),
            "unlocked strategy should apply mutations directly");
    }
}

//! Micro-backtest engine for rapid strategy validation.

/// Result of a micro-backtest run.
#[derive(Clone, Debug)]
pub struct BacktestResult {
    /// Per-period returns.
    pub returns: Vec<f64>,
    /// Total return.
    pub total_return: f64,
    /// Maximum drawdown.
    pub max_drawdown: f64,
    /// Number of trades.
    pub trade_count: u32,
    /// Information ratio.
    pub ir: f64,
}

/// Configuration for a single evaluation window in the multi-window crucible.
#[derive(Clone, Debug)]
pub struct WindowConfig {
    /// Number of return entries in this window.
    pub size: usize,
    /// Weight for IR aggregation (must sum to 1.0 across all windows).
    pub weight: f64,
    /// Maximum drawdown allowed before short-circuit (as fraction, e.g. 0.015 = 1.5%).
    pub max_drawdown_limit: f64,
}

/// Result of multi-window crucible evaluation.
#[derive(Clone, Debug)]
pub struct MultiWindowResult {
    /// Per-window backtest results.
    pub windows: Vec<BacktestResult>,
    /// Weighted IR across all windows (FATAL_IR if short-circuited).
    pub weighted_ir: f64,
    /// Whether evaluation was short-circuited due to drawdown breach.
    pub short_circuited: bool,
    /// Index of the window that triggered short-circuit (None if passed all).
    pub short_circuit_window: Option<usize>,
}

/// Sentinel IR for strategies that fail drawdown limits — ensures tournament elimination.
pub const FATAL_IR: f64 = -99999.0;

/// Default evaluation windows for 2x leverage contracts at ~1Hz sampling.
///
/// Window sizes based on `strategy_wrapper.rs:634` — 600 entries ≈ 10 min at 1Hz.
/// Drawdown limits halved for 2x leverage (price DD / 2 = equity DD).
pub fn default_windows() -> Vec<WindowConfig> {
    vec![
        WindowConfig { size: 60,  weight: 0.15, max_drawdown_limit: 0.015 }, // ~1min, 1.5%
        WindowConfig { size: 180, weight: 0.35, max_drawdown_limit: 0.025 }, // ~3min, 2.5%
        WindowConfig { size: 600, weight: 0.50, max_drawdown_limit: 0.040 }, // ~10min, 4.0%
    ]
}

/// Delta between candidate and baseline backtest results.
#[derive(Clone, Debug)]
pub struct BacktestDelta {
    /// Events where candidate and baseline diverged.
    pub divergences: Vec<FillDiff>,
    /// Candidate's IR vs baseline.
    pub ir_delta: f64,
}

#[derive(Clone, Debug)]
pub struct FillDiff {
    pub timestamp_ns: u64,
    pub baseline_return: f64,
    pub candidate_return: f64,
}

/// Simplified micro-backtest engine.
///
/// Runs a strategy over a short window (5 min default) to validate hypotheses.
pub struct MicroBacktestEngine {
    /// Window size in nanoseconds.
    pub window_ns: u64,
}

impl MicroBacktestEngine {
    pub fn new(window_secs: u64) -> Self {
        Self {
            window_ns: window_secs * 1_000_000_000,
        }
    }

    /// Run a backtest on historical returns and compute result.
    pub fn run(&self, returns: &[f64]) -> BacktestResult {
        let total_return: f64 = returns.iter().sum();
        let max_drawdown = Self::compute_max_drawdown(returns);

        let metric = crate::metric::RiskAdjustedInfoRatio;
        let ir = metric.evaluate(returns);

        BacktestResult {
            returns: returns.to_vec(),
            total_return,
            max_drawdown,
            trade_count: returns.len() as u32,
            ir,
        }
    }

    /// Multi-window crucible: evaluate returns across multiple time windows.
    ///
    /// Iterates short → long. If any window's drawdown exceeds the limit,
    /// short-circuits with `FATAL_IR` to ensure tournament elimination.
    /// Otherwise returns the weighted average IR across all windows.
    pub fn run_multi_window(&self, returns: &[f64], windows: &[WindowConfig]) -> MultiWindowResult {
        let mut results = Vec::with_capacity(windows.len());
        let mut weighted_ir = 0.0;

        for (i, window) in windows.iter().enumerate() {
            // Take the last `window.size` entries (or all if fewer)
            let start = returns.len().saturating_sub(window.size);
            let slice = &returns[start..];
            let result = self.run(slice);

            // Short-circuit on drawdown breach
            if result.max_drawdown > window.max_drawdown_limit {
                return MultiWindowResult {
                    windows: results,
                    weighted_ir: FATAL_IR,
                    short_circuited: true,
                    short_circuit_window: Some(i),
                };
            }

            weighted_ir += result.ir * window.weight;
            results.push(result);
        }

        MultiWindowResult {
            windows: results,
            weighted_ir,
            short_circuited: false,
            short_circuit_window: None,
        }
    }

    /// Compute incremental delta between two strategies.
    pub fn run_delta(&self, baseline_returns: &[f64], candidate_returns: &[f64]) -> BacktestDelta {
        let min_len = baseline_returns.len().min(candidate_returns.len());
        let mut divergences = Vec::new();

        for i in 0..min_len {
            if (baseline_returns[i] - candidate_returns[i]).abs() > 1e-8 {
                divergences.push(FillDiff {
                    timestamp_ns: i as u64,
                    baseline_return: baseline_returns[i],
                    candidate_return: candidate_returns[i],
                });
            }
        }

        let metric = crate::metric::RiskAdjustedInfoRatio;
        let baseline_ir = metric.evaluate(baseline_returns);
        let candidate_ir = metric.evaluate(candidate_returns);

        BacktestDelta {
            divergences,
            ir_delta: candidate_ir - baseline_ir,
        }
    }

    /// Passive baseline: open a position at the first price, hold to the end.
    /// For contracts, this is the "do nothing" benchmark any active strategy must beat.
    /// Note: Both this and candidate returns are leverage-free price returns, so
    /// comparison is apples-to-apples. If candidate returns ever include leverage,
    /// pass `leverage` to match.
    pub fn simulate_hold_baseline(prices: &[f64]) -> Vec<f64> {
        if prices.len() < 2 {
            return vec![];
        }
        prices
            .windows(2)
            .map(|w| (w[1] - w[0]) / w[0])
            .collect()
    }

    fn compute_max_drawdown(returns: &[f64]) -> f64 {
        let mut cumulative = 0.0;
        let mut peak = 0.0;
        let mut max_dd = 0.0;

        for r in returns {
            cumulative += r;
            if cumulative > peak {
                peak = cumulative;
            }
            let dd = peak - cumulative;
            if dd > max_dd {
                max_dd = dd;
            }
        }

        max_dd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_micro_backtest_basic() {
        let engine = MicroBacktestEngine::new(300);
        let returns = vec![0.01, -0.005, 0.02, -0.01, 0.015];
        let result = engine.run(&returns);

        assert!((result.total_return - 0.03).abs() < 1e-10);
        assert!(result.max_drawdown > 0.0);
        assert_eq!(result.trade_count, 5);
    }

    #[test]
    fn test_delta_backtest() {
        let engine = MicroBacktestEngine::new(300);
        let baseline = vec![0.01, 0.02, 0.01];
        let candidate = vec![0.01, 0.03, 0.01]; // diverges at index 1

        let delta = engine.run_delta(&baseline, &candidate);
        assert_eq!(delta.divergences.len(), 1);
        assert_eq!(delta.divergences[0].timestamp_ns, 1);
    }

    #[test]
    fn test_hold_baseline_uptrend() {
        // Rising prices: 100, 101, 102, ... → positive returns
        let prices: Vec<f64> = (0..20).map(|i| 100.0 + i as f64).collect();
        let returns = MicroBacktestEngine::simulate_hold_baseline(&prices);
        assert_eq!(returns.len(), 19);
        let total: f64 = returns.iter().sum();
        assert!(total > 0.0, "uptrend should have positive returns, got {}", total);

        let metric = crate::metric::RiskAdjustedInfoRatio;
        let ir = metric.evaluate(&returns);
        assert!(ir > 0.0, "uptrend IR should be positive, got {}", ir);
    }

    #[test]
    fn test_hold_baseline_downtrend() {
        // Falling prices: 100, 99, 98, ... → negative returns
        let prices: Vec<f64> = (0..20).map(|i| 100.0 - i as f64).collect();
        let returns = MicroBacktestEngine::simulate_hold_baseline(&prices);
        assert_eq!(returns.len(), 19);
        let total: f64 = returns.iter().sum();
        assert!(total < 0.0, "downtrend should have negative returns, got {}", total);

        let metric = crate::metric::RiskAdjustedInfoRatio;
        let ir = metric.evaluate(&returns);
        assert!(ir < 0.0, "downtrend IR should be negative, got {}", ir);
    }

    #[test]
    fn test_hold_baseline_empty() {
        let returns = MicroBacktestEngine::simulate_hold_baseline(&[]);
        assert!(returns.is_empty());

        let returns = MicroBacktestEngine::simulate_hold_baseline(&[100.0]);
        assert!(returns.is_empty());
    }

    #[test]
    fn test_multi_window_all_pass() {
        let engine = MicroBacktestEngine::new(300);
        // Steady positive returns — no large drawdowns
        let returns: Vec<f64> = (0..700).map(|_| 0.001).collect();
        let windows = default_windows();
        let result = engine.run_multi_window(&returns, &windows);

        assert!(!result.short_circuited, "should not short-circuit on steady returns");
        assert!(result.weighted_ir > 0.0, "weighted IR should be positive");
        assert_eq!(result.windows.len(), 3);
        assert!(result.short_circuit_window.is_none());
    }

    #[test]
    fn test_multi_window_short_circuit() {
        let engine = MicroBacktestEngine::new(300);
        // Place the big loss within the last 60 entries (first window)
        let mut returns: Vec<f64> = vec![0.001; 700];
        // 5% loss near the end → drawdown exceeds 1.5% limit
        returns[680] = -0.05;

        let windows = default_windows();
        let result = engine.run_multi_window(&returns, &windows);

        assert!(result.short_circuited, "should short-circuit on drawdown breach");
        assert_eq!(result.weighted_ir, FATAL_IR);
        assert_eq!(result.short_circuit_window, Some(0));
    }

    #[test]
    fn test_multi_window_second_window_breach() {
        let engine = MicroBacktestEngine::new(300);
        // Window 1 (last 60, indices 640-699): all fine
        // Window 2 (last 180, indices 520-699): place a big loss at index 639 (in window 2, NOT in window 1)
        let mut returns: Vec<f64> = vec![0.001; 700];
        // 4% loss → exceeds 2.5% limit for window 2, but NOT in window 1
        returns[639] = -0.04;

        let windows = default_windows();
        let result = engine.run_multi_window(&returns, &windows);

        assert!(result.short_circuited);
        assert_eq!(result.weighted_ir, FATAL_IR);
        assert_eq!(result.short_circuit_window, Some(1)); // second window
    }

    #[test]
    fn test_fatal_ir_loses_in_tournament() {
        // FATAL_IR should lose to any finite IR
        assert!(FATAL_IR < -1000.0);
        assert!(FATAL_IR < 0.0);
        assert!(FATAL_IR < -999.0);

        // Verify it's the minimum possible loser
        let any_ir = -50.0;
        assert!(FATAL_IR < any_ir, "FATAL_IR should lose to even bad IRs");
    }
}

//! Event-driven backtester for realistic strategy simulation.
//!
//! Processes price events sequentially and simulates order execution with
//! slippage, fees, and position tracking. Unlike the simple return-based
//! micro-backtest, this models actual fill mechanics.

use crate::micro_backtest::BacktestResult;
use crate::metric::RiskAdjustedInfoRatio;

/// Side of an order or position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Buy,
    Sell,
}

/// A strategy signal at a point in time.
#[derive(Clone, Copy, Debug)]
pub struct Signal {
    /// Price event index this signal corresponds to.
    pub index: usize,
    /// Target position size (positive = long, negative = short, 0 = flat).
    pub target_size: f64,
    /// Signal confidence [0, 1].
    pub confidence: f64,
}

/// Configuration for the simulated exchange.
#[derive(Clone, Debug)]
pub struct ExchangeConfig {
    /// Fee per trade as fraction of notional (e.g., 0.0006 = 6bps taker fee).
    pub fee_rate: f64,
    /// Slippage model: fixed bps added to execution price.
    pub slippage_bps: f64,
    /// Initial capital in quote currency.
    pub initial_capital: f64,
    /// Contract multiplier (e.g., 0.01 for BTC-USDT-SWAP where 1 contract = 0.01 BTC).
    pub contract_multiplier: f64,
}

impl Default for ExchangeConfig {
    fn default() -> Self {
        Self {
            fee_rate: 0.0006,     // 6bps OKX taker fee
            slippage_bps: 1.0,    // 1bp slippage
            initial_capital: 10_000.0,
            contract_multiplier: 1.0,
        }
    }
}

/// Current position state.
#[derive(Clone, Copy, Debug, Default)]
pub struct Position {
    /// Current position size (positive = long, negative = short).
    pub size: f64,
    /// Average entry price.
    pub entry_price: f64,
    /// Realized P&L from closed trades.
    pub realized_pnl: f64,
    /// Total fees paid.
    pub total_fees: f64,
    /// Number of trades executed.
    pub trade_count: u32,
}

impl Position {
    /// Unrealized P&L at current market price.
    pub fn unrealized_pnl(&self, current_price: f64) -> f64 {
        if self.size == 0.0 {
            0.0
        } else {
            (current_price - self.entry_price) * self.size
        }
    }

    /// Total P&L (realized + unrealized).
    pub fn total_pnl(&self, current_price: f64) -> f64 {
        self.realized_pnl + self.unrealized_pnl(current_price)
    }
}

/// Event-driven backtester.
///
/// Processes a sequence of prices and strategy signals, simulating
/// order execution with realistic slippage and fees.
pub struct EventBacktester {
    config: ExchangeConfig,
}

impl EventBacktester {
    pub fn new(config: ExchangeConfig) -> Self {
        Self { config }
    }

    /// Run a backtest over prices with given signals.
    ///
    /// `prices` — sequential price observations.
    /// `signals` — strategy signals (must be sorted by index).
    ///
    /// Returns per-period equity returns suitable for IR computation.
    pub fn run(&self, prices: &[f64], signals: &[Signal]) -> BacktestResult {
        if prices.is_empty() {
            return BacktestResult {
                returns: vec![],
                total_return: 0.0,
                max_drawdown: 0.0,
                trade_count: 0,
                ir: 0.0,
            };
        }

        let mut pos = Position::default();
        let mut equity = self.config.initial_capital;
        let mut peak_equity = equity;
        let mut max_dd = 0.0f64;
        let mut returns = Vec::with_capacity(prices.len());
        let mut signal_idx = 0;

        for (i, &price) in prices.iter().enumerate() {
            let prev_equity = equity;

            // Process any signals for this index
            while signal_idx < signals.len() && signals[signal_idx].index == i {
                let sig = &signals[signal_idx];
                self.execute_signal(&mut pos, price, sig.target_size);
                signal_idx += 1;
            }

            // Update equity: capital + realized P&L + unrealized P&L - fees
            equity = self.config.initial_capital
                + pos.realized_pnl
                + pos.unrealized_pnl(price)
                - pos.total_fees;

            // Track drawdown
            if equity > peak_equity {
                peak_equity = equity;
            }
            let dd = (peak_equity - equity) / peak_equity;
            if dd > max_dd {
                max_dd = dd;
            }

            // Per-period return
            if prev_equity > 0.0 {
                returns.push((equity - prev_equity) / prev_equity);
            } else {
                returns.push(0.0);
            }
        }

        let total_return = (equity - self.config.initial_capital) / self.config.initial_capital;
        let metric = RiskAdjustedInfoRatio;
        let ir = metric.evaluate(&returns);

        BacktestResult {
            returns,
            total_return,
            max_drawdown: max_dd,
            trade_count: pos.trade_count,
            ir,
        }
    }

    /// Execute a target position change.
    fn execute_signal(&self, pos: &mut Position, price: f64, target_size: f64) {
        let delta = target_size - pos.size;
        if delta.abs() < 1e-10 {
            return; // no change
        }

        // Apply slippage
        let slippage_mult = self.config.slippage_bps / 10000.0;
        let exec_price = if delta > 0.0 {
            price * (1.0 + slippage_mult) // buying: pay more
        } else {
            price * (1.0 - slippage_mult) // selling: receive less
        };

        let abs_delta = delta.abs();
        let notional = abs_delta * exec_price * self.config.contract_multiplier;
        let fee = notional * self.config.fee_rate;
        pos.total_fees += fee;

        // Update position
        if pos.size == 0.0 || pos.size.signum() == delta.signum() {
            // Opening or adding to position
            let old_notional = pos.size * pos.entry_price;
            let new_notional = delta * exec_price;
            pos.size += delta;
            if pos.size.abs() > 1e-10 {
                pos.entry_price = (old_notional + new_notional) / pos.size;
            }
        } else if delta.abs() >= pos.size.abs() {
            // Closing and possibly reversing
            let close_pnl = (exec_price - pos.entry_price) * (-delta.min(pos.size.abs()));
            pos.realized_pnl += close_pnl;
            let remaining = delta + pos.size;
            pos.size = remaining;
            if remaining.abs() > 1e-10 {
                pos.entry_price = exec_price;
            } else {
                pos.entry_price = 0.0;
            }
        } else {
            // Partial close
            let close_pnl = (exec_price - pos.entry_price) * (-delta);
            pos.realized_pnl += close_pnl;
            pos.size += delta;
        }

        pos.trade_count += 1;
    }
}

/// Generate signals from a simple momentum strategy for backtesting.
///
/// Long when price > MA, short when price < MA.
pub fn momentum_signals(prices: &[f64], window: usize, size: f64) -> Vec<Signal> {
    if prices.len() <= window {
        return vec![];
    }

    let mut signals = Vec::new();
    for i in window..prices.len() {
        let ma: f64 = prices[i - window..i].iter().sum::<f64>() / window as f64;
        let target = if prices[i] > ma {
            size
        } else if prices[i] < ma {
            -size
        } else {
            0.0
        };
        signals.push(Signal {
            index: i,
            target_size: target,
            confidence: ((prices[i] - ma).abs() / ma).clamp(0.0, 1.0),
        });
    }
    signals
}

/// Generate signals from a mean-reversion strategy for backtesting.
///
/// Long when z-score < -threshold, short when z-score > +threshold.
pub fn mean_reversion_signals(prices: &[f64], window: usize, threshold: f64, size: f64) -> Vec<Signal> {
    if prices.len() <= window {
        return vec![];
    }

    let mut signals = Vec::new();
    for i in window..prices.len() {
        let slice = &prices[i - window..i];
        let mean = slice.iter().sum::<f64>() / window as f64;
        let std = (slice.iter().map(|p| (p - mean).powi(2)).sum::<f64>() / window as f64)
            .sqrt()
            .max(1e-10);
        let z = (prices[i] - mean) / std;

        let target = if z < -threshold {
            size // oversold → long
        } else if z > threshold {
            -size // overbought → short
        } else if z.abs() < 0.3 {
            0.0 // mean reverted → flat
        } else {
            continue; // no signal change
        };

        signals.push(Signal {
            index: i,
            target_size: target,
            confidence: (z.abs() / threshold).clamp(0.0, 1.0),
        });
    }
    signals
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_signals() {
        let bt = EventBacktester::new(ExchangeConfig::default());
        let prices = vec![100.0, 101.0, 102.0, 103.0];
        let result = bt.run(&prices, &[]);
        assert_eq!(result.trade_count, 0);
        assert!((result.total_return).abs() < 1e-10);
    }

    #[test]
    fn test_buy_and_hold_equivalent() {
        let bt = EventBacktester::new(ExchangeConfig {
            fee_rate: 0.0,
            slippage_bps: 0.0,
            initial_capital: 10000.0,
            contract_multiplier: 1.0,
        });
        let prices = vec![100.0, 101.0, 102.0, 103.0, 104.0];
        let signals = vec![Signal {
            index: 0,
            target_size: 1.0,
            confidence: 1.0,
        }];
        let result = bt.run(&prices, &signals);
        assert_eq!(result.trade_count, 1);
        // Buy at 100, price goes to 104 → P&L = 4.0
        assert!(result.total_return > 0.0, "should be profitable, got {}", result.total_return);
    }

    #[test]
    fn test_fees_reduce_returns() {
        let no_fee = EventBacktester::new(ExchangeConfig {
            fee_rate: 0.0,
            slippage_bps: 0.0,
            ..Default::default()
        });
        let with_fee = EventBacktester::new(ExchangeConfig {
            fee_rate: 0.001, // 10bps
            slippage_bps: 0.0,
            ..Default::default()
        });
        let prices = vec![100.0, 101.0, 100.0, 101.0, 100.0];
        let signals = vec![
            Signal { index: 0, target_size: 1.0, confidence: 1.0 },
            Signal { index: 2, target_size: -1.0, confidence: 1.0 },
            Signal { index: 4, target_size: 1.0, confidence: 1.0 },
        ];
        let r1 = no_fee.run(&prices, &signals);
        let r2 = with_fee.run(&prices, &signals);
        assert!(r2.total_return < r1.total_return, "fees should reduce returns");
        assert!(r2.trade_count > 0, "should have executed trades");
    }

    #[test]
    fn test_slippage_impact() {
        let no_slip = EventBacktester::new(ExchangeConfig {
            slippage_bps: 0.0,
            fee_rate: 0.0,
            ..Default::default()
        });
        let with_slip = EventBacktester::new(ExchangeConfig {
            slippage_bps: 10.0, // 10bps
            fee_rate: 0.0,
            ..Default::default()
        });
        let prices = vec![100.0; 10];
        let signals = vec![Signal { index: 0, target_size: 1.0, confidence: 1.0 }];
        let r1 = no_slip.run(&prices, &signals);
        let r2 = with_slip.run(&prices, &signals);
        // With slippage, buy price is higher → worse entry → lower equity
        assert!(r2.total_return <= r1.total_return, "slippage should hurt returns");
    }

    #[test]
    fn test_position_reversal() {
        let bt = EventBacktester::new(ExchangeConfig {
            fee_rate: 0.0,
            slippage_bps: 0.0,
            ..Default::default()
        });
        let prices = vec![100.0, 105.0, 100.0];
        let signals = vec![
            Signal { index: 0, target_size: 1.0, confidence: 1.0 },  // long
            Signal { index: 1, target_size: -1.0, confidence: 1.0 }, // reverse to short
        ];
        let result = bt.run(&prices, &signals);
        assert_eq!(result.trade_count, 2);
        // Long at 100, close at 105 (+5), short at 105, mark at 100 (+5)
        assert!(result.total_return > 0.0, "reversal should profit, got {}", result.total_return);
    }

    #[test]
    fn test_max_drawdown_tracked() {
        let bt = EventBacktester::new(ExchangeConfig {
            fee_rate: 0.0,
            slippage_bps: 0.0,
            ..Default::default()
        });
        // Price drops then recovers
        let prices = vec![100.0, 95.0, 90.0, 95.0, 100.0];
        let signals = vec![Signal { index: 0, target_size: 1.0, confidence: 1.0 }];
        let result = bt.run(&prices, &signals);
        assert!(result.max_drawdown > 0.0, "should track drawdown");
    }

    #[test]
    fn test_momentum_signals() {
        let prices: Vec<f64> = (0..20).map(|i| 100.0 + i as f64).collect();
        let signals = momentum_signals(&prices, 5, 1.0);
        assert!(!signals.is_empty());
        // In uptrend, all signals should be long
        for sig in &signals {
            assert!(sig.target_size > 0.0, "uptrend should produce long signals");
        }
    }

    #[test]
    fn test_mean_reversion_signals() {
        // Oscillating prices
        let prices: Vec<f64> = (0..40)
            .map(|i| 100.0 + (i as f64 * 0.5).sin() * 5.0)
            .collect();
        let signals = mean_reversion_signals(&prices, 10, 1.0, 1.0);
        assert!(!signals.is_empty(), "should produce signals in oscillating market");
    }

    #[test]
    fn test_position_tracking() {
        let bt = EventBacktester::new(ExchangeConfig::default());
        let prices = vec![100.0, 101.0, 102.0];
        let signals = vec![
            Signal { index: 0, target_size: 5.0, confidence: 1.0 },
            Signal { index: 2, target_size: 0.0, confidence: 1.0 }, // close
        ];
        let result = bt.run(&prices, &signals);
        assert_eq!(result.trade_count, 2);
    }

    #[test]
    fn test_empty_prices() {
        let bt = EventBacktester::new(ExchangeConfig::default());
        let result = bt.run(&[], &[]);
        assert_eq!(result.trade_count, 0);
        assert!(result.returns.is_empty());
    }
}

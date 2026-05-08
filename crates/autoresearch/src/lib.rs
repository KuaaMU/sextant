//! Autoresearch runtime for Sextant.
//!
//! Implements the Karpathy Ratchet: Agent proposes strategy hypotheses,
//! micro-backtests validate them, and only improvements are retained.

pub mod event_backtest;
pub mod metric;
pub mod micro_backtest;
pub mod runtime;
pub mod strategy_lock;

pub use event_backtest::{EventBacktester, ExchangeConfig, Signal, Side, momentum_signals, mean_reversion_signals};
pub use metric::RiskAdjustedInfoRatio;
pub use runtime::{AutoresearchRuntime, StrategyHypothesis, StrategyType};
pub use strategy_lock::{PendingMutation, StrategyLock};

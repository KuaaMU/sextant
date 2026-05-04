//! Trade limiter — shared circuit breaker for agents.
//!
// === P0: Simple struct, P1: May become trait for dynamic limits ===

use tracing::info;

/// Per-agent trade counter with a hard cap.
///
/// Once `max_trades` is reached, `try_trade()` returns `false` and
/// the agent should produce a Hold intent. `record_fill()` increments
/// the counter after a successful execution.
pub struct TradeLimiter {
    agent_id: String,
    trade_count: u32,
    max_trades: u32,
}

impl TradeLimiter {
    pub fn new(agent_id: &str, max_trades: u32) -> Self {
        Self {
            agent_id: agent_id.to_string(),
            trade_count: 0,
            max_trades,
        }
    }

    /// Returns `true` if the agent is still allowed to trade.
    pub fn try_trade(&self) -> bool {
        if self.max_trades == 0 {
            return true; // 0 = unlimited
        }
        self.trade_count < self.max_trades
    }

    /// Record a successful fill. Call this from `on_feedback`.
    pub fn record_fill(&mut self) {
        self.trade_count += 1;
        if self.max_trades > 0 {
            info!(
                "[{}] Trade {}/{} recorded",
                self.agent_id, self.trade_count, self.max_trades
            );
        }
    }

    /// Current trade count.
    pub fn trade_count(&self) -> u32 {
        self.trade_count
    }

    /// Max trades (0 = unlimited).
    pub fn max_trades(&self) -> u32 {
        self.max_trades
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unlimited() {
        let mut limiter = TradeLimiter::new("test", 0);
        assert!(limiter.try_trade());
        limiter.record_fill();
        assert!(limiter.try_trade()); // still unlimited
    }

    #[test]
    fn test_limited() {
        let mut limiter = TradeLimiter::new("test", 2);
        assert!(limiter.try_trade());
        limiter.record_fill();
        assert!(limiter.try_trade()); // 1/2
        limiter.record_fill();
        assert!(!limiter.try_trade()); // 2/2 — blocked
    }
}

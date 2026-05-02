//! Simulated market data generator with dynamic Greeks, risk, and event traces.

use nautilus_state_encoder::{ContextWindow, EventToken, SharedStateBuffer};

use super::data_source::{DataSource, LogAgentType, LogEntry};

pub struct Simulator {
    base_price: f64,
    price: f64,
    tick_count: u64,
    position_size: f64,
    entry_price: f64,
    buffer: SharedStateBuffer,

    // Dynamic state
    volatility: f64,
    trend: f64,
    momentum: f64,
    volume_accum: f64,

    // Agent log simulation
    pending_logs: Vec<LogEntry>,
}

impl Simulator {
    pub fn new(base_price: f64) -> Self {
        Self {
            base_price,
            price: base_price,
            tick_count: 0,
            position_size: 0.0,
            entry_price: 0.0,
            buffer: SharedStateBuffer::new(),
            volatility: 0.015,
            trend: 0.0,
            momentum: 0.0,
            volume_accum: 0.0,
            pending_logs: Vec::new(),
        }
    }

    fn tick(&mut self) {
        self.tick_count += 1;
        let t = self.tick_count as f64;

        // ── Price dynamics: regime switching + volatility clustering ──
        let regime = ((t * 0.003).sin() * 0.5 + 0.5).powf(0.3); // 0..1 smooth regime
        let vol_shock = (t * 0.17).sin() * 0.3 + ((t * 0.43).cos()) * 0.2;
        self.volatility = 0.008 + regime * 0.012 + vol_shock.abs() * 0.005;

        // Trend component with mean-reversion
        self.trend = self.trend * 0.98 + ((t * 0.02).sin() * 0.001);
        let noise = ((self.tick_count * 7919) % 1000) as f64 / 1000.0 - 0.5;
        let noise = noise * self.base_price * self.volatility;

        self.price = self.price * (1.0 + self.trend) + noise;
        self.price = self.price.max(self.base_price * 0.85).min(self.base_price * 1.15);

        // Momentum (rate of change over last ~20 ticks)
        self.momentum = self.momentum * 0.95 + (noise / self.base_price) * 5.0;

        // Volume accumulation
        self.volume_accum += 1000.0 + (noise.abs() / self.base_price * 50000.0);

        // ── Spread & market state ─────────────────────────────────
        let spread = self.base_price * (0.0001 + self.volatility * 0.01);
        let bid = self.price - spread / 2.0;
        let ask = self.price + spread / 2.0;
        let vol_display = (self.volume_accum as u64).min(9_999_999);

        let market_state = format!(
            "bid:{:.2} | ask:{:.2} | spread:{:.4} | vol:{} | momentum:{:+.4} | regime:{:.0}%",
            bid, ask, spread, vol_display, self.momentum, regime * 100.0
        );

        // ── Position management: momentum-following strategy ──────
        if self.tick_count % 30 == 0 {
            if self.position_size == 0.0 && self.momentum > 0.002 {
                // Enter long on positive momentum
                self.position_size = 5.0 + (self.momentum * 500.0).min(15.0);
                self.entry_price = self.price;
            } else if self.position_size > 0.0 && self.momentum < -0.001 {
                // Exit on momentum reversal
                self.position_size = 0.0;
                self.entry_price = 0.0;
            } else if self.position_size == 0.0 && self.momentum < -0.003 {
                // Enter short on strong negative momentum
                self.position_size = -(5.0 + (self.momentum.abs() * 500.0).min(15.0));
                self.entry_price = self.price;
            } else if self.position_size < 0.0 && self.momentum > -0.001 {
                // Exit short
                self.position_size = 0.0;
                self.entry_price = 0.0;
            }
        }

        let unrealized = if self.position_size != 0.0 {
            (self.price - self.entry_price) * self.position_size
        } else {
            0.0
        };

        // ── Greeks: dynamic based on position and volatility ──────
        let abs_pos = self.position_size.abs();
        let delta = if abs_pos > 0.0 {
            (self.position_size / 20.0).clamp(-1.0, 1.0)
        } else {
            0.0
        };
        let gamma = 0.005 + self.volatility * 0.8 + (self.momentum.abs() * 0.1);
        let theta = if abs_pos > 0.0 {
            -2.0 - abs_pos * 0.5 - self.volatility * 100.0
        } else {
            -0.5
        };
        let vega = abs_pos * 0.8 + self.volatility * 50.0;

        // ── Risk potentials: dynamic based on exposure ────────────
        let position_risk = (abs_pos / 20.0).min(1.0);
        let vol_risk = (self.volatility / 0.03).min(1.0);
        let drawdown_risk = if unrealized < 0.0 {
            (unrealized.abs() / (self.base_price * abs_pos).max(1.0)).min(1.0)
        } else {
            0.02
        };

        let risk_pot = (position_risk * 0.4 + vol_risk * 0.3 + drawdown_risk * 0.3).min(1.0);
        let pos_pot = position_risk;
        let dd_pot = drawdown_risk;

        // ── Build ContextWindow ───────────────────────────────────
        let mut ctx = ContextWindow::zeroed();
        ctx.version = self.tick_count;
        ctx.timestamp_ns = 1700000000_000_000_000 + self.tick_count * 100_000_000;
        ctx.set_instrument_id("SOL-USDC.OKX");
        ctx.set_market_state(&market_state);
        ctx.position_size = self.position_size;
        ctx.entry_price = self.entry_price;
        ctx.unrealized_pnl = unrealized;
        ctx.greeks = nautilus_state_encoder::Greeks {
            delta,
            gamma,
            theta,
            vega,
        };
        ctx.risk_potential = risk_pot as f64;
        ctx.position_potential = pos_pot as f64;
        ctx.drawdown_potential = dd_pot as f64;

        // ── Event trace: varied event types ───────────────────────
        // Always push a Trade event
        ctx.push_event(EventToken {
            event_type: 1, // Trade
            price: self.price,
            size: 1.0 + (noise.abs() / self.base_price * 10.0),
            timestamp_ns: ctx.timestamp_ns,
        });

        // Push a Quote event every 3 ticks
        if self.tick_count % 3 == 0 {
            ctx.push_event(EventToken {
                event_type: 0, // Quote
                price: bid,
                size: ask - bid,
                timestamp_ns: ctx.timestamp_ns,
            });
        }

        // Push a Fill event on position changes
        if self.tick_count % 30 == 0 && self.position_size != 0.0 {
            ctx.push_event(EventToken {
                event_type: 2, // Fill
                price: self.price,
                size: self.position_size.abs(),
                timestamp_ns: ctx.timestamp_ns,
            });
        }

        // Push Alert on high volatility
        if self.volatility > 0.025 {
            ctx.push_event(EventToken {
                event_type: 3, // Alert
                price: self.volatility,
                size: 0.0,
                timestamp_ns: ctx.timestamp_ns,
            });
        }

        // Push RiskAlert on high risk
        if risk_pot > 0.7 {
            ctx.push_event(EventToken {
                event_type: 4, // RiskAlert
                price: risk_pot,
                size: drawdown_risk,
                timestamp_ns: ctx.timestamp_ns,
            });
        }

        self.buffer.write(&ctx);

        // Generate log entries
        self.pending_logs.extend(self.generate_log_entries());
    }

    /// Generate synthetic agent log entries based on current state.
    pub fn generate_log_entries(&self) -> Vec<LogEntry> {
        let mut entries = Vec::new();
        let t = self.tick_count;
        let base_ts = 1700000000 + t / 10; // ~10 ticks per second

        // Perception agent: processes market data every tick
        if t > 0 {
            entries.push(LogEntry {
                timestamp: format_ts(base_ts),
                agent_type: LogAgentType::Perception,
                latency_ms: 2 + (t % 7) as u64,
                summary: format!(
                    "tick {} price={:.2} spread={:.4} vol={:.0}",
                    t,
                    self.price,
                    self.base_price * self.volatility * 0.01,
                    self.volume_accum
                ),
            });
        }

        // Strategy agent: evaluates momentum every 10 ticks
        if t % 10 == 0 && t > 0 {
            let action = if self.momentum > 0.002 {
                "BUY signal"
            } else if self.momentum < -0.003 {
                "SELL signal"
            } else {
                "HOLD"
            };
            entries.push(LogEntry {
                timestamp: format_ts(base_ts),
                agent_type: LogAgentType::Strategy,
                latency_ms: 8 + (t % 15) as u64,
                summary: format!(
                    "momentum={:+.4} regime={:.0}% → {}",
                    self.momentum,
                    ((t as f64 * 0.003).sin() * 50.0 + 50.0),
                    action
                ),
            });
        }

        // Risk agent: monitors on position changes or high vol
        if t % 30 == 0 || self.volatility > 0.025 {
            let risk_level = if self.volatility > 0.025 { "HIGH" } else { "NORMAL" };
            entries.push(LogEntry {
                timestamp: format_ts(base_ts),
                agent_type: LogAgentType::Risk,
                latency_ms: 3 + (t % 5) as u64,
                summary: format!(
                    "risk={:.2} pos_pot={:.2} dd={:.2} vol={} | {}",
                    (self.position_size.abs() / 20.0).min(1.0) * 0.4
                        + (self.volatility / 0.03).min(1.0) * 0.3,
                    self.position_size.abs() / 20.0,
                    if self.unrealized_pnl() < 0.0 {
                        self.unrealized_pnl().abs() / (self.base_price * self.position_size.abs().max(1.0))
                    } else {
                        0.02
                    },
                    risk_level,
                    if risk_level == "HIGH" { "ALERT" } else { "OK" }
                ),
            });
        }

        // Execution agent: on fills
        if t % 30 == 0 && self.position_size != 0.0 {
            entries.push(LogEntry {
                timestamp: format_ts(base_ts),
                agent_type: LogAgentType::Execution,
                latency_ms: 1 + (t % 3) as u64,
                summary: format!(
                    "FILL {} {:.1} @ {:.2} (pos: {:.1})",
                    if self.position_size > 0.0 { "BUY" } else { "SELL" },
                    self.position_size.abs(),
                    self.price,
                    self.position_size
                ),
            });
        }

        entries
    }

    fn unrealized_pnl(&self) -> f64 {
        if self.position_size != 0.0 {
            (self.price - self.entry_price) * self.position_size
        } else {
            0.0
        }
    }
}

impl DataSource for Simulator {
    fn try_read(&mut self) -> Option<ContextWindow> {
        self.tick();
        self.buffer.read()
    }

    fn drain_logs(&mut self) -> Vec<LogEntry> {
        std::mem::take(&mut self.pending_logs)
    }
}

fn format_ts(epoch_secs: u64) -> String {
    let h = (epoch_secs % 86400) / 3600;
    let m = (epoch_secs % 3600) / 60;
    let s = epoch_secs % 60;
    format!("{:02}:{:02}:{:02}", h, m, s)
}

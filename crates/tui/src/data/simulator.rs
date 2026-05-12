//! Simulated market data generator for demo/testing.
//!
//! Generates realistic price movements and writes them to a SharedStateBuffer,
//! which the TUI reads for display.

use nautilus_state_encoder::{ContextWindow, EventToken, SharedStateBuffer};

/// Simulated market data state.
pub struct Simulator {
    base_price: f64,
    price: f64,
    tick_count: u64,
    position_size: f64,
    entry_price: f64,
    buffer: SharedStateBuffer,
    /// Path to mmap file for cross-process reading.
    mmap_path: Option<String>,
    mmap_writer: Option<nautilus_state_encoder::MmapWriter>,
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
            mmap_path: None,
            mmap_writer: None,
        }
    }

    /// Enable file-backed mmap output for cross-process TUI reading.
    pub fn with_mmap(mut self, path: impl Into<String>) -> Self {
        let path = path.into();
        match nautilus_state_encoder::MmapWriter::open(&path) {
            Ok(writer) => {
                self.mmap_path = Some(path);
                self.mmap_writer = Some(writer);
            }
            Err(e) => {
                tracing::warn!("Failed to create mmap file: {}", e);
            }
        }
        self
    }

    /// Generate next tick and write to buffer.
    pub fn tick(&mut self) {
        self.tick_count += 1;

        // Simulate price: sine wave + random walk
        let trend = (self.tick_count as f64 * 0.05).sin() * self.base_price * 0.02;
        let noise = ((self.tick_count * 7919) % 1000) as f64 / 1000.0 - 0.5;
        let noise = noise * self.base_price * 0.003;
        self.price = self.base_price + trend + noise;

        let spread = self.base_price * 0.0002; // 2bps spread
        let bid = self.price - spread / 2.0;
        let ask = self.price + spread / 2.0;

        // Build market state text
        let market_state = format!(
            "bid:{:.2} | ask:{:.2} | spread:{:.4} | vol:1250000",
            bid, ask, spread
        );

        // Simulate position changes
        if self.tick_count.is_multiple_of(50) {
            if self.position_size == 0.0 {
                self.position_size = 10.0;
                self.entry_price = self.price;
            } else {
                self.position_size = 0.0;
                self.entry_price = 0.0;
            }
        }

        let unrealized = if self.position_size != 0.0 {
            (self.price - self.entry_price) * self.position_size
        } else {
            0.0
        };

        // Build ContextWindow
        let mut ctx = ContextWindow::zeroed();
        ctx.version = self.tick_count;
        ctx.timestamp_ns = 1_700_000_000_000_000_000 + self.tick_count * 100_000_000; // 100ms ticks
        ctx.set_instrument_id("SOL-USDC.OKX");
        ctx.set_market_state(&market_state);
        ctx.position_size = self.position_size;
        ctx.entry_price = self.entry_price;
        ctx.unrealized_pnl = unrealized;
        ctx.greeks = nautilus_state_encoder::Greeks {
            delta: 0.45,
            gamma: 0.012,
            theta: -23.5,
            vega: 12.3,
        };
        ctx.risk_potential = 0.52;
        ctx.position_potential = 0.45;
        ctx.drawdown_potential = 0.08;

        // Push event
        ctx.push_event(EventToken {
            event_type: 1, // QUOTE
            price: self.price,
            size: 1.0,
            timestamp_ns: ctx.timestamp_ns,
        });

        // Write to in-process buffer
        self.buffer.write(&ctx);

        // Write to mmap file if configured
        if let Some(ref mut writer) = self.mmap_writer {
            writer.write(&ctx);
        }
    }

    /// Read latest context from in-process buffer.
    pub fn read(&self) -> Option<ContextWindow> {
        self.buffer.read()
    }

    pub fn tick_count(&self) -> u64 {
        self.tick_count
    }

    pub fn price(&self) -> f64 {
        self.price
    }
}

//! StateEncoder: bridges Nautilus events to shared-memory ContextWindow.

use nautilus_model::data::QuoteTick;

use crate::context_window::{ContextWindow, EventToken};
use crate::extended_events::{ExtendedEventWriter, SextantEvent};
use crate::mmap_shm::MmapWriter;
use crate::shared_buffer::SharedStateBuffer;

/// Encodes Nautilus internal events into shared-memory ContextWindows.
///
/// Hooks into MessageBus publish path as a sidecar — does not modify
/// the original event flow.
/// Shared memory path: `/dev/shm/sextant_state_{instrument_id}`
pub struct StateEncoder {
    buffer: SharedStateBuffer,
    current: ContextWindow,
    mmap_writer: Option<MmapWriter>,
    event_writer: Option<ExtendedEventWriter>,
}

impl StateEncoder {
    pub fn new(instrument_id: &str) -> Self {
        let mut current = ContextWindow::zeroed();
        current.set_instrument_id(instrument_id);

        Self {
            buffer: SharedStateBuffer::new(),
            current,
            mmap_writer: None,
            event_writer: None,
        }
    }

    /// Enable file-backed mmap output for cross-process reading (e.g., TUI).
    pub fn with_mmap(mut self, path: impl AsRef<std::path::Path>) -> std::io::Result<Self> {
        let writer = MmapWriter::open(path)?;
        self.mmap_writer = Some(writer);
        Ok(self)
    }

    /// Enable extended event output for order/research/agent events.
    pub fn with_extended_events(mut self, path: impl AsRef<std::path::Path>) -> std::io::Result<Self> {
        let writer = ExtendedEventWriter::open(path)?;
        self.event_writer = Some(writer);
        Ok(self)
    }

    /// Push an event into the extended event buffer (if enabled).
    pub fn push_event(&mut self, event: &SextantEvent) {
        if let Some(ref mut writer) = self.event_writer {
            writer.push(event);
        }
    }

    /// Handle a quote tick update.
    pub fn on_quote(&mut self, quote: &QuoteTick) {
        self.current.version += 1;
        self.current.timestamp_ns = quote.ts_event.as_u64();

        // Push to event trace first (needed for momentum calculation)
        let mid = (quote.bid_price.as_f64() + quote.ask_price.as_f64()) / 2.0;
        self.current.push_event(EventToken {
            event_type: 0, // Quote
            price: mid,
            size: quote.bid_size.as_f64() + quote.ask_size.as_f64(),
            timestamp_ns: quote.ts_event.as_u64(),
        });

        // Calculate momentum from event trace
        let momentum = self.calculate_momentum();

        // Format market state as LLM-readable text
        let spread_bps = if mid > 0.0 {
            ((quote.ask_price.as_f64() - quote.bid_price.as_f64()) / mid) * 10000.0
        } else {
            0.0
        };
        let state = format!(
            "OrderBook[{}] bid:{} @ {} | ask:{} @ {} | mid:{:.2} | spread:{:.1}bps | momentum:{:+.6} | position:{}",
            self.current.instrument_id_str(),
            quote.bid_size,
            quote.bid_price,
            quote.ask_size,
            quote.ask_price,
            mid,
            spread_bps,
            momentum,
            self.current.position_size,
        );
        self.current.set_market_state(&state);

        // Write to shared memory
        self.buffer.write(&self.current);
        if let Some(ref mut writer) = self.mmap_writer {
            writer.write(&self.current);
        }
    }

    /// Calculate price momentum from the event trace (percentage change over ~10 ticks).
    fn calculate_momentum(&self) -> f64 {
        let count = self.current.event_count();
        if count < 2 {
            return 0.0;
        }
        let recent_idx = ((count - 1) as usize) % 64;
        let old_idx = if count > 10 {
            ((count - 10) as usize) % 64
        } else {
            0
        };
        let recent = self.current.event_trace[recent_idx].price;
        let old = self.current.event_trace[old_idx].price;
        if old > 0.0 {
            (recent - old) / old
        } else {
            0.0
        }
    }

    /// Update position state.
    pub fn update_position(&mut self, size: f64, entry_price: f64, unrealized_pnl: f64) {
        self.current.position_size = size;
        self.current.entry_price = entry_price;
        self.current.unrealized_pnl = unrealized_pnl;
        self.current.version += 1;
        self.buffer.write(&self.current);
        if let Some(ref mut writer) = self.mmap_writer {
            writer.write(&self.current);
        }
    }

    /// Update risk potential values.
    pub fn update_risk(&mut self, total: f64, position: f64, drawdown: f64) {
        self.current.risk_potential = total;
        self.current.position_potential = position;
        self.current.drawdown_potential = drawdown;
        self.current.version += 1;
        self.buffer.write(&self.current);
        if let Some(ref mut writer) = self.mmap_writer {
            writer.write(&self.current);
        }
    }

    /// Get read access to the shared buffer (for agent processes).
    pub fn buffer(&self) -> &SharedStateBuffer {
        &self.buffer
    }

    /// Get the current local context (before write).
    pub fn current_context(&self) -> &ContextWindow {
        &self.current
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nautilus_core::UnixNanos;
    use nautilus_model::data::QuoteTick;
    use nautilus_model::identifiers::InstrumentId;
    use nautilus_model::types::{Price, Quantity};
    use tempfile::NamedTempFile;

    #[test]
    fn test_encoder_quote_update() {
        let id = InstrumentId::from("SOL-USDC.OKX");
        let mut encoder = StateEncoder::new("SOL-USDC");

        let quote = QuoteTick::new(
            id,
            Price::from("150.00"),
            Price::from("150.10"),
            Quantity::from("10.0"),
            Quantity::from("5.0"),
            UnixNanos::from(1_000_000_000),
            UnixNanos::from(1_000_000_001),
        );

        encoder.on_quote(&quote);

        let ctx = encoder.buffer.read().unwrap();
        assert_eq!(ctx.version, 1);
        assert!(ctx.market_state_str().contains("150.00"));
        assert_eq!(ctx.event_count(), 1);
    }

    #[test]
    fn test_encoder_mmap_output() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path();

        let id = InstrumentId::from("ETH-USDC.OKX");
        let mut encoder = StateEncoder::new("ETH-USDC").with_mmap(path).unwrap();

        let quote = QuoteTick::new(
            id,
            Price::from("3200.00"),
            Price::from("3200.10"),
            Quantity::from("5.0"),
            Quantity::from("3.0"),
            UnixNanos::from(2_000_000_000),
            UnixNanos::from(2_000_000_001),
        );

        encoder.on_quote(&quote);

        // Read back via MmapReader (simulates cross-process TUI)
        let reader = crate::mmap_shm::MmapReader::open(path).unwrap();
        let ctx = reader.read().unwrap();
        assert_eq!(ctx.version, 1);
        assert!(ctx.market_state_str().contains("3200.00"));
    }

    #[test]
    fn test_encoder_mmap_position_update() {
        let tmp = NamedTempFile::new().unwrap();
        let mut encoder = StateEncoder::new("SOL-USDC").with_mmap(tmp.path()).unwrap();

        encoder.update_position(10.0, 150.25, 5.0);

        let reader = crate::mmap_shm::MmapReader::open(tmp.path()).unwrap();
        let ctx = reader.read().unwrap();
        assert_eq!(ctx.position_size, 10.0);
        assert_eq!(ctx.entry_price, 150.25);
        assert_eq!(ctx.unrealized_pnl, 5.0);
    }
}

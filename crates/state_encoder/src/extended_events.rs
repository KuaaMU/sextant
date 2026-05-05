//! Extended event buffer for order, research, and agent events.
//!
//! Separate mmap file alongside ContextWindow. Uses the same seqlock
//! double-buffer protocol. Events are JSON-serialized into a ring buffer.

use serde::{Deserialize, Serialize};

/// Event types that flow through the extended event buffer.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SextantEvent {
    /// Agent produced an intent (full unified contract).
    IntentGenerated {
        agent_id: String,
        intent_type: String,
        title: String,
        reasoning: String,
        confidence: f64,
        confidence_label: String,
        instrument: String,
        timestamp_ns: u64,
    },
    /// Intent was approved for execution.
    IntentApproved {
        intent_id: String,
        by: String,
        timestamp_ns: u64,
    },
    /// Intent was rejected.
    IntentRejected {
        intent_id: String,
        reason: String,
        timestamp_ns: u64,
    },
    /// An order was submitted to the exchange.
    OrderSubmitted {
        intent_id: String,
        order_type: String,
        instrument: String,
        side: String,
        quantity: f64,
        price: Option<f64>,
        timestamp_ns: u64,
    },
    /// An order was filled.
    OrderFilled {
        order_id: String,
        fill_price: f64,
        fill_qty: f64,
        slippage_bps: f64,
        timestamp_ns: u64,
    },
    /// An order was rejected by the exchange.
    OrderRejected {
        order_id: String,
        reason: String,
        timestamp_ns: u64,
    },
    /// Risk alert from the risk potential field.
    RiskAlert {
        dimension: String,
        value: f64,
        threshold: f64,
        timestamp_ns: u64,
    },
    /// Autoresearch hypothesis result.
    AutoresearchResult {
        hypothesis: String,
        ir_before: f64,
        ir_after: f64,
        accepted: bool,
        timestamp_ns: u64,
    },
}

/// Ring buffer header for the extended event mmap file.
///
/// Layout in file:
/// - bytes 0..8:   seq (u64, same seqlock protocol as ContextWindow)
/// - bytes 8..12:  write_index (u32, next slot to write)
/// - bytes 12..16: event_count (u32, total events written, wraps)
/// - bytes 16..N:  events[EVENT_BUFFER_SIZE] (JSON-serialized SextantEvent)
///
/// Each event slot is MAX_EVENT_BYTES bytes. Events smaller than this
/// are zero-padded. The write_index advances by 1 per event (circular).
#[repr(C)]
pub struct ExtendedEventHeader {
    pub seq: u64,
    pub write_index: u32,
    pub event_count: u32,
}

pub const EVENT_BUFFER_SIZE: usize = 256;
pub const MAX_EVENT_BYTES: usize = 512;
pub const HEADER_SIZE: usize = 16;
pub const TOTAL_FILE_SIZE: usize = HEADER_SIZE + EVENT_BUFFER_SIZE * MAX_EVENT_BYTES;

/// Engine-side writer for extended events.
pub struct ExtendedEventWriter {
    _file: std::fs::File,
    mmap: memmap2::MmapMut,
}

impl ExtendedEventWriter {
    pub fn open(path: impl AsRef<std::path::Path>) -> std::io::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path.as_ref())?;
        file.set_len(TOTAL_FILE_SIZE as u64)?;
        let mmap = unsafe { memmap2::MmapMut::map_mut(&file)? };
        Ok(Self {
            _file: file,
            mmap,
        })
    }

    /// Push an event into the ring buffer.
    pub fn push(&mut self, event: &SextantEvent) {
        let bytes = match serde_json::to_vec(event) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("Failed to serialize event: {}", e);
                return;
            }
        };
        if bytes.len() > MAX_EVENT_BYTES {
            tracing::warn!(
                "Event too large ({} bytes > {} max), skipping",
                bytes.len(),
                MAX_EVENT_BYTES
            );
            return;
        }

        // Seqlock write protocol
        let header = unsafe { &mut *(self.mmap.as_mut_ptr() as *mut ExtendedEventHeader) };
        let current = header.seq;
        header.seq = ((current >> 1) + 1) << 1 | 1; // set writing bit
        std::sync::atomic::fence(std::sync::atomic::Ordering::Release);

        let idx = header.write_index as usize % EVENT_BUFFER_SIZE;
        let offset = HEADER_SIZE + idx * MAX_EVENT_BYTES;
        let slot = &mut self.mmap[offset..offset + MAX_EVENT_BYTES];
        slot.fill(0);
        slot[..bytes.len()].copy_from_slice(&bytes);

        header.write_index = (header.write_index + 1) % EVENT_BUFFER_SIZE as u32;
        header.event_count = header.event_count.wrapping_add(1);
        std::sync::atomic::fence(std::sync::atomic::Ordering::Release);
        header.seq &= !1; // clear writing bit
    }
}

/// TUI/GUI-side reader for extended events.
pub struct ExtendedEventReader {
    _file: std::fs::File,
    mmap: memmap2::Mmap,
    last_count: u32,
}

impl ExtendedEventReader {
    pub fn open(path: impl AsRef<std::path::Path>) -> std::io::Result<Self> {
        let file = std::fs::OpenOptions::new().read(true).open(path.as_ref())?;
        let mmap = unsafe { memmap2::Mmap::map(&file)? };
        Ok(Self {
            _file: file,
            mmap,
            last_count: 0,
        })
    }

    /// Read all new events since last call.
    pub fn drain_new(&mut self) -> Vec<SextantEvent> {
        let header = unsafe { &*(self.mmap.as_ptr() as *const ExtendedEventHeader) };

        // Seqlock read with retry
        for _ in 0..100 {
            let s = header.seq;
            if s & 1 != 0 {
                std::hint::spin_loop();
                continue;
            }

            let count = header.event_count;
            let new_events = count.wrapping_sub(self.last_count) as usize;
            if new_events == 0 {
                return vec![];
            }

            let to_read = new_events.min(EVENT_BUFFER_SIZE);
            let mut events = Vec::with_capacity(to_read);
            let start_idx = count.wrapping_sub(to_read as u32) as usize % EVENT_BUFFER_SIZE;

            for i in 0..to_read {
                let idx = (start_idx + i) % EVENT_BUFFER_SIZE;
                let offset = HEADER_SIZE + idx * MAX_EVENT_BYTES;
                let slot = &self.mmap[offset..offset + MAX_EVENT_BYTES];
                // Find the end of the JSON data (first null byte)
                let end = slot.iter().position(|&b| b == 0).unwrap_or(slot.len());
                if end > 0 {
                    if let Ok(event) = serde_json::from_slice::<SextantEvent>(&slot[..end]) {
                        events.push(event);
                    }
                }
            }

            // Verify seq hasn't changed (consistent read)
            let s2 = header.seq;
            if s == s2 {
                self.last_count = count;
                return events;
            }
        }
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.mmap");

        let mut writer = ExtendedEventWriter::open(&path).unwrap();
        let mut reader = ExtendedEventReader::open(&path).unwrap();

        // Push events
        for i in 0..5 {
            writer.push(&SextantEvent::OrderSubmitted {
                intent_id: format!("intent-{}", i),
                order_type: "Market".to_string(),
                instrument: "BTC-USDT-SWAP.OKX".to_string(),
                side: "Buy".to_string(),
                quantity: 0.01,
                price: None,
                timestamp_ns: 1_700_000_000 + i,
            });
        }

        // Read back
        let events = reader.drain_new();
        assert_eq!(events.len(), 5);
        match &events[0] {
            SextantEvent::OrderSubmitted { intent_id, side, .. } => {
                assert_eq!(intent_id, "intent-0");
                assert_eq!(side, "Buy");
            }
            _ => panic!("Expected OrderSubmitted"),
        }

        // Second read should be empty
        let events2 = reader.drain_new();
        assert!(events2.is_empty());
    }

    #[test]
    fn test_ring_buffer_wrap() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events_wrap.mmap");

        let mut writer = ExtendedEventWriter::open(&path).unwrap();
        let mut reader = ExtendedEventReader::open(&path).unwrap();

        // Push more than EVENT_BUFFER_SIZE events
        for i in 0..300u64 {
            writer.push(&SextantEvent::RiskAlert {
                dimension: "position".to_string(),
                value: i as f64,
                threshold: 100.0,
                timestamp_ns: i,
            });
        }

        // Should read at most EVENT_BUFFER_SIZE
        let events = reader.drain_new();
        assert!(events.len() <= EVENT_BUFFER_SIZE);
        assert!(events.len() > 0);
    }

    #[test]
    fn test_event_types() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events_types.mmap");

        let mut writer = ExtendedEventWriter::open(&path).unwrap();
        let mut reader = ExtendedEventReader::open(&path).unwrap();

        let now = 1_700_000_000u64;

        writer.push(&SextantEvent::IntentGenerated {
            agent_id: "momentum-01".to_string(),
            intent_type: "TrendFollow".to_string(),
            title: "Buy BTC".to_string(),
            reasoning: "Strong momentum".to_string(),
            confidence: 0.85,
            confidence_label: "High".to_string(),
            instrument: "BTC-USDT-SWAP.OKX".to_string(),
            timestamp_ns: now,
        });
        writer.push(&SextantEvent::OrderFilled {
            order_id: "ord-001".to_string(),
            fill_price: 65000.0,
            fill_qty: 0.01,
            slippage_bps: 2.5,
            timestamp_ns: now + 1,
        });
        writer.push(&SextantEvent::AutoresearchResult {
            hypothesis: "Increase momentum window".to_string(),
            ir_before: 1.2,
            ir_after: 1.35,
            accepted: true,
            timestamp_ns: now + 2,
        });

        let events = reader.drain_new();
        assert_eq!(events.len(), 3);
    }
}

//! Shared utilities for GUI panels.

use nautilus_state_encoder::{ContextWindow, EventToken};

/// Access event from the circular buffer in chronological order.
/// The write counter `event_trace_len` wraps via modulo 64.
pub fn event_at(ctx: &ContextWindow, index: usize) -> Option<EventToken> {
    let count = ctx.event_count() as usize;
    if index >= count {
        return None;
    }
    let start = if ctx.event_trace_len >= 64 {
        (ctx.event_trace_len as usize) % 64
    } else {
        0
    };
    let idx = (start + index) % 64;
    Some(ctx.event_trace[idx])
}

/// Iterate all events in chronological order.
pub fn events(ctx: &ContextWindow) -> impl Iterator<Item = EventToken> + '_ {
    let count = ctx.event_count() as usize;
    (0..count).filter_map(move |i| event_at(ctx, i))
}

/// Count events by type. Returns (quotes, trades, fills, alerts, risk_alerts).
pub fn count_event_types(ctx: &ContextWindow) -> (usize, usize, usize, usize, usize) {
    let mut quotes = 0;
    let mut trades = 0;
    let mut fills = 0;
    let mut alerts = 0;
    let mut risk_alerts = 0;
    for evt in events(ctx) {
        match evt.event_type {
            0 => quotes += 1,
            1 => trades += 1,
            2 => fills += 1,
            3 => alerts += 1,
            4 => risk_alerts += 1,
            _ => {}
        }
    }
    (quotes, trades, fills, alerts, risk_alerts)
}

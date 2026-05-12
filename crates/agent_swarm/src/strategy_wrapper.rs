//! SwarmStrategy: Nautilus Strategy wrapper for the AgentSwarm.
//!
//! This is the primary integration point between Sextant's agent-based trading
//! and NautilusTrader's execution engine. It implements the `Strategy` trait,
//! subscribes to market data via MessageBus, and routes quotes through the
//! SwarmCoordinator to produce execution directives.

use std::fmt::Debug;
use std::time::Instant;

use nautilus_common::actor::DataActor;
use nautilus_model::{
    data::QuoteTick,
    enums::{OrderSide as NautilusOrderSide, TimeInForce as NautilusTif, TrailingOffsetType, TriggerType},
    events::order::filled::OrderFilled,
    identifiers::{ClientOrderId, InstrumentId},
    types::{Price, Quantity},
};
use rust_decimal::Decimal;
use nautilus_state_encoder::{StateEncoder, SextantEvent};
use nautilus_trading::{
    nautilus_strategy,
    strategy::{Strategy, StrategyConfig, StrategyCore},
};
use tracing::{debug, info, warn};

use crate::agent::AgentFeedback;
use crate::intent::{ExecutionDirective, ExecutionStyle, OrderSide, TimeInForce};
use crate::swarm::SwarmCoordinator;
use nautilus_reputation::AutonomySlider;
use nautilus_risk_potential::RiskPotentialField;
use nautilus_autoresearch::{AutoresearchRuntime, StrategyHypothesis, StrategyType};

/// Nautilus Strategy wrapper for the Sextant AgentSwarm.
///
/// Subscribes to QuoteTick via MessageBus, feeds quotes into the StateEncoder
/// (which writes to shared memory), runs the SwarmCoordinator cycle, and
/// submits resulting orders to the ExecutionEngine.
pub struct SwarmStrategy {
    core: StrategyCore,
    instrument_id: InstrumentId,
    encoder: StateEncoder,
    swarm: SwarmCoordinator,
    position_size: f64,
    entry_price: f64,
    /// Reputation-based autonomy slider — gates position size.
    autonomy: AutonomySlider,
    /// Running trade count for reputation scoring.
    trade_count: u32,
    /// Running win count for reputation scoring.
    win_count: u32,
    /// Stop-loss percentage (e.g. 0.02 = 2%). 0 disables SL.
    sl_pct: f64,
    /// Client order ID of the active stop-loss order, if any.
    active_sl_order_id: Option<ClientOrderId>,
    /// Minimum seconds between swarm cycles (cooldown).
    cooldown_secs: u64,
    /// Timestamp of last swarm cycle.
    last_cycle_time: Option<Instant>,
    /// Dry-run mode: log orders without submitting (SEXTANT_DRY_RUN=1).
    dry_run: bool,
    /// Risk potential field for gradient-based position regulation.
    risk_field: RiskPotentialField,
    /// Autoresearch runtime (Karpathy Ratchet).
    autoresearch: AutoresearchRuntime,
    /// Price history for autoresearch.
    price_history: Vec<f64>,
    /// Timestamp of last autoresearch run.
    last_autoresearch: Option<Instant>,
    /// Optional callback for broadcasting events to external consumers (WebSocket, GUI).
    event_callback: Option<Box<dyn Fn(SextantEvent) + Send + Sync>>,
    /// Last observed mid price from quote feed (for slippage computation).
    last_mid_price: f64,
}

impl SwarmStrategy {
    /// Creates a new [`SwarmStrategy`] instance.
    pub fn new(
        strategy_id: &str,
        instrument_id: InstrumentId,
        swarm: SwarmCoordinator,
    ) -> Self {
        let config = StrategyConfig {
            strategy_id: Some(nautilus_model::identifiers::StrategyId::from(strategy_id)),
            ..Default::default()
        };

        info!(
            "Creating SwarmStrategy '{}' for {} with {} agents",
            strategy_id,
            instrument_id,
            swarm.agent_count()
        );

        // Stop-loss: SEXTANT_SL_PCT env var (default 0.02 = 2%). 0 disables.
        let sl_pct: f64 = std::env::var("SEXTANT_SL_PCT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.02);
        if sl_pct > 0.0 {
            eprintln!("Stop-loss enabled: {:.1}%", sl_pct * 100.0);
        }

        // Cooldown: SEXTANT_COOLDOWN_SECS env var (default 30s).
        let cooldown_secs: u64 = std::env::var("SEXTANT_COOLDOWN_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(30);
        eprintln!("Cooldown: {}s between cycles", cooldown_secs);

        // Dry-run: SEXTANT_DRY_RUN=1 logs orders without submitting.
        let dry_run = std::env::var("SEXTANT_DRY_RUN")
            .map(|v| v == "1" || v == "true")
            .unwrap_or(false);
        if dry_run {
            eprintln!("DRY RUN MODE — orders will be logged but NOT submitted");
        }

        // Risk potential field: position_limit, drawdown_limit, concentration_limit
        // SEXTANT_RISK_POSITION_LIMIT (default: 10000 USDT notional)
        // SEXTANT_RISK_DRAWDOWN_LIMIT (default: -0.05 = -5%)
        // SEXTANT_RISK_CONCENTRATION_LIMIT (default: 0.3 = 30%)
        let position_limit: f64 = std::env::var("SEXTANT_RISK_POSITION_LIMIT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10000.0);
        let drawdown_limit: f64 = std::env::var("SEXTANT_RISK_DRAWDOWN_LIMIT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(-0.05);
        let concentration_limit: f64 = std::env::var("SEXTANT_RISK_CONCENTRATION_LIMIT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.3);
        let risk_field = RiskPotentialField::new(position_limit, drawdown_limit, concentration_limit);
        eprintln!("Risk field: position_limit={} drawdown_limit={:.1}% concentration_limit={:.0}%",
            position_limit, drawdown_limit * 100.0, concentration_limit * 100.0);

        // Autoresearch: Karpathy Ratchet for strategy evolution
        let improvement_threshold: f64 = std::env::var("SEXTANT_RATCHET_THRESHOLD")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.05); // 5% improvement required
        let autoresearch = AutoresearchRuntime::new(improvement_threshold);
        eprintln!("Autoresearch: improvement_threshold={:.1}%", improvement_threshold * 100.0);

        // ContextWindow stores symbol only (e.g. "BTC-USDT"), venue is added by SwarmCoordinator
        let symbol = instrument_id.symbol.as_str().to_string();
        Self {
            core: StrategyCore::new(config),
            instrument_id,
            encoder: StateEncoder::new(&symbol),
            swarm,
            position_size: 0.0,
            entry_price: 0.0,
            autonomy: AutonomySlider::new(5), // 5 trades before level changes
            trade_count: 0,
            win_count: 0,
            sl_pct,
            active_sl_order_id: None,
            cooldown_secs,
            last_cycle_time: None,
            dry_run,
            risk_field,
            autoresearch,
            price_history: Vec::with_capacity(600), // ~10 min of 1Hz data
            last_autoresearch: None,
            event_callback: None,
            last_mid_price: 0.0,
        }
    }

    /// Set an event callback for broadcasting events to external consumers.
    pub fn with_event_callback(mut self, callback: Box<dyn Fn(SextantEvent) + Send + Sync>) -> Self {
        self.event_callback = Some(callback);
        self
    }

    /// Enable extended event mmap output for GUI/TUI consumption.
    pub fn with_extended_events(&mut self, path: impl AsRef<std::path::Path>) -> std::io::Result<()> {
        self.encoder.with_extended_events(path)?;
        Ok(())
    }

    /// Convert a Sextant ExecutionDirective into Nautilus orders and submit them.
    fn execute_directives(&mut self, directives: Vec<ExecutionDirective>) {
        for directive in &directives {
            // === DRY RUN: Log orders without submitting ===
            if self.dry_run {
                for order_spec in &directive.orders {
                    info!(
                        "[DRY RUN] Would execute: {:?} qty={:.6} instrument={} tif={:?} style={:?}",
                        order_spec.side,
                        order_spec.quantity,
                        order_spec.instrument_id,
                        order_spec.time_in_force,
                        directive.execution_style,
                    );
                }
                continue;
            }

            debug!(
                "Executing directive for intent {:?}: {} orders, style={:?}",
                directive.intent_id,
                directive.orders.len(),
                directive.execution_style
            );

            match &directive.execution_style {
                ExecutionStyle::Twap { slices, interval } => {
                    self.execute_twap(directive, *slices, *interval);
                }
                ExecutionStyle::Limit { price, post_only } => {
                    self.execute_limit(directive, *price, *post_only);
                }
                ExecutionStyle::TrailingStop { offset_bps } => {
                    self.execute_trailing_stop(directive, *offset_bps);
                }
                ExecutionStyle::Ioc | ExecutionStyle::Fok => {
                    self.execute_market(directive);
                }
                ExecutionStyle::Vwap { .. } => {
                    warn!("VWAP execution not yet implemented, using market order");
                    self.execute_market(directive);
                }
            }
        }
    }

    /// Execute directive as market orders (existing behavior).
    fn execute_market(&mut self, directive: &ExecutionDirective) {
        for order_spec in &directive.orders {
            let side = match order_spec.side {
                OrderSide::Buy => NautilusOrderSide::Buy,
                OrderSide::Sell => NautilusOrderSide::Sell,
            };
            let tif = match order_spec.time_in_force {
                TimeInForce::Gtc => NautilusTif::Gtc,
                TimeInForce::Ioc => NautilusTif::Ioc,
                TimeInForce::Fok => NautilusTif::Fok,
                TimeInForce::Gtd => NautilusTif::Gtd,
            };
            let quantity = Quantity::new(order_spec.quantity, 8);
            let order = self.core.order_factory().market(
                order_spec.instrument_id,
                side,
                quantity,
                Some(tif),
                None, None, None, None, None, None,
            );
            if let Err(e) = self.submit_order(order, None, None) {
                warn!("Failed to submit market order: {}", e);
            } else {
                self.emit_order_submitted(directive, order_spec, "Market");
            }
        }
    }

    /// Execute directive as limit orders (for MeanReversion).
    fn execute_limit(&mut self, directive: &ExecutionDirective, price: f64, post_only: bool) {
        for order_spec in &directive.orders {
            let side = match order_spec.side {
                OrderSide::Buy => NautilusOrderSide::Buy,
                OrderSide::Sell => NautilusOrderSide::Sell,
            };
            let quantity = Quantity::new(order_spec.quantity, 8);
            let limit_price = Price::new(price, 8);
            let order = self.core.order_factory().limit(
                order_spec.instrument_id,
                side,
                quantity,
                limit_price,
                Some(NautilusTif::Gtc), // time_in_force
                None,                   // expire_time
                Some(post_only),        // post_only
                None,                   // reduce_only
                None,                   // quote_quantity
                None,                   // display_qty
                None,                   // emulation_trigger
                None,                   // trigger_instrument_id
                None,                   // exec_algorithm_id
                None,                   // exec_algorithm_params
                None,                   // tags
                None,                   // client_order_id
            );
            if let Err(e) = self.submit_order(order, None, None) {
                warn!("Failed to submit limit order: {}", e);
            } else {
                self.emit_order_submitted(directive, order_spec, "Limit");
            }
        }
    }

    /// Execute directive as trailing stop orders (for TrendFollow).
    fn execute_trailing_stop(&mut self, directive: &ExecutionDirective, offset_bps: f64) {
        for order_spec in &directive.orders {
            let side = match order_spec.side {
                OrderSide::Buy => NautilusOrderSide::Buy,
                OrderSide::Sell => NautilusOrderSide::Sell,
            };
            let quantity = Quantity::new(order_spec.quantity, 8);

            // Compute trailing offset from bps: e.g. 50bps on $60000 BTC = $30
            let mid = Self::parse_mid_from_encoder(&self.encoder);
            let trailing_offset_price = mid * offset_bps / 10000.0;
            let trailing_offset = Decimal::try_from(trailing_offset_price)
                .unwrap_or(Decimal::from(1));

            // Activation price: current mid (trailing starts here)
            let activation_price = Price::new(mid, 8);

            let order = self.core.order_factory().trailing_stop_market(
                order_spec.instrument_id,
                side,
                quantity,
                trailing_offset,                   // trailing_offset (Decimal)
                Some(TrailingOffsetType::Price),   // trailing_offset_type
                Some(activation_price),            // activation_price
                None,                              // trigger_price
                Some(TriggerType::LastPrice),      // trigger_type
                Some(NautilusTif::Gtc),            // time_in_force
                None,                              // expire_time
                None,                              // reduce_only
                None,                              // quote_quantity
                None,                              // display_qty
                None,                              // emulation_trigger
                None,                              // trigger_instrument_id
                None,                              // exec_algorithm_id
                None,                              // exec_algorithm_params
                None,                              // tags
                None,                              // client_order_id
            );
            if let Err(e) = self.submit_order(order, None, None) {
                warn!("Failed to submit trailing stop order: {}", e);
            } else {
                self.emit_order_submitted(directive, order_spec, "TrailingStop");
            }
        }
    }

    /// Execute directive as TWAP (time-weighted average price).
    /// MVP: submits all slices immediately. Timer-based spacing is P1 refinement.
    fn execute_twap(&mut self, directive: &ExecutionDirective, slices: u32, _interval: std::time::Duration) {
        for order_spec in &directive.orders {
            let side = match order_spec.side {
                OrderSide::Buy => NautilusOrderSide::Buy,
                OrderSide::Sell => NautilusOrderSide::Sell,
            };
            let tif = match order_spec.time_in_force {
                TimeInForce::Gtc => NautilusTif::Gtc,
                TimeInForce::Ioc => NautilusTif::Ioc,
                TimeInForce::Fok => NautilusTif::Fok,
                TimeInForce::Gtd => NautilusTif::Gtd,
            };
            let slice_qty = order_spec.quantity / slices as f64;

            for i in 0..slices {
                let quantity = Quantity::new(slice_qty, 8);
                let order = self.core.order_factory().market(
                    order_spec.instrument_id,
                    side,
                    quantity,
                    Some(tif),
                    None, None, None, None, None, None,
                );
                if let Err(e) = self.submit_order(order, None, None) {
                    warn!("Failed to submit TWAP slice {}/{}: {}", i + 1, slices, e);
                }
            }
            self.emit_order_submitted(directive, order_spec, "Twap");
            info!(
                "TWAP: submitted {} slices of {:.6} each (timer-based spacing is P1)",
                slices, slice_qty
            );
        }
    }

    /// Emit an OrderSubmitted event to the extended event buffer and callback.
    fn emit_order_submitted(&mut self, directive: &ExecutionDirective, order_spec: &crate::intent::OrderSpecification, order_type: &str) {
        let event = SextantEvent::OrderSubmitted {
            intent_id: format!("{:?}", directive.intent_id),
            order_type: order_type.to_string(),
            instrument: order_spec.instrument_id.to_string(),
            side: format!("{:?}", order_spec.side),
            quantity: order_spec.quantity,
            price: order_spec.price,
            timestamp_ns: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64,
        };
        self.encoder.push_event(&event);
        if let Some(ref cb) = self.event_callback {
            cb(event);
        }
    }

    /// Parse mid price from the encoder's current ContextWindow.
    fn parse_mid_from_encoder(encoder: &StateEncoder) -> f64 {
        let ctx = encoder.current_context();
        let state = ctx.market_state_str();
        let bid = state
            .find("bid:")
            .and_then(|i| state[i + 4..].split(' ').next())
            .and_then(|s| s.parse::<f64>().ok());
        let ask = state
            .find("ask:")
            .and_then(|i| state[i + 4..].split(' ').next())
            .and_then(|s| s.parse::<f64>().ok());
        match (bid, ask) {
            (Some(b), Some(a)) if b > 0.0 && a > 0.0 => (b + a) / 2.0,
            _ => 0.0,
        }
    }
}

nautilus_strategy!(SwarmStrategy);

impl Debug for SwarmStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SwarmStrategy")
            .field("instrument_id", &self.instrument_id)
            .field("agent_count", &self.swarm.agent_count())
            .field("autonomy", &self.autonomy.current())
            .field("trades", &self.trade_count)
            .finish()
    }
}

impl DataActor for SwarmStrategy {
    fn on_start(&mut self) -> anyhow::Result<()> {
        info!(
            "SwarmStrategy started, subscribing to quotes for {}",
            self.instrument_id
        );
        self.subscribe_quotes(self.instrument_id, None, None);
        Ok(())
    }

    fn on_stop(&mut self) -> anyhow::Result<()> {
        info!("SwarmStrategy stopped");
        self.unsubscribe_quotes(self.instrument_id, None, None);
        Ok(())
    }

    fn on_order_filled(&mut self, event: &OrderFilled) -> anyhow::Result<()> {
        let qty = event.last_qty.as_f64();
        let price = event.last_px.as_f64();
        let side_sign = if event.order_side == NautilusOrderSide::Buy {
            1.0
        } else {
            -1.0
        };

        // Update position with weighted average entry price
        let delta = side_sign * qty;
        let old_size = self.position_size;
        let new_size = old_size + delta;

        if new_size.abs() < 1e-12 {
            // Position closed
            self.position_size = 0.0;
            self.entry_price = 0.0;
        } else if old_size.abs() < 1e-12 {
            // New position
            self.position_size = new_size;
            self.entry_price = price;
        } else if old_size.signum() == delta.signum() {
            // Adding to position — weighted average entry
            self.entry_price =
                (self.entry_price * old_size.abs() + price * qty) / (old_size.abs() + qty);
            self.position_size = new_size;
        } else {
            // Reducing position
            self.position_size = new_size;
        }

        let unrealized_pnl = if self.position_size.abs() > 1e-12 {
            (price - self.entry_price) * self.position_size
        } else {
            0.0
        };

        info!(
            "Order filled: {} {} @ {:.2} → position={:.6} entry={:.2} pnl={:.2}",
            event.order_side, qty, price, self.position_size, self.entry_price, unrealized_pnl
        );

        // Emit OrderFilled event to extended event buffer and callback
        // Slippage: deviation of fill price from last mid, in bps
        let slippage_bps = if self.last_mid_price > 0.0 {
            let side_sign = if event.order_side == NautilusOrderSide::Buy { 1.0 } else { -1.0 };
            ((price - self.last_mid_price) / self.last_mid_price * side_sign * 10000.0).abs()
        } else {
            0.0
        };
        let fill_event = SextantEvent::OrderFilled {
            order_id: event.client_order_id.to_string(),
            fill_price: price,
            fill_qty: qty,
            slippage_bps,
            timestamp_ns: event.ts_event.as_u64(),
        };
        self.encoder.push_event(&fill_event);
        if let Some(ref cb) = self.event_callback {
            cb(fill_event);
        }

        // ── Stop-loss management ──────────────────────────────────
        if new_size.abs() > 1e-12 && old_size.abs() < 1e-12 && self.sl_pct > 0.0 {
            // New position opened — submit stop-loss order
            let sl_side = if new_size > 0.0 {
                NautilusOrderSide::Sell
            } else {
                NautilusOrderSide::Buy
            };
            let sl_trigger_price = if new_size > 0.0 {
                self.entry_price * (1.0 - self.sl_pct)
            } else {
                self.entry_price * (1.0 + self.sl_pct)
            };
            let sl_qty = Quantity::new(new_size.abs(), 8);
            let sl_price = Price::new(sl_trigger_price, 8);
            let sl_cl_ord_id = ClientOrderId::new(
                format!("SL-{}", event.client_order_id).as_str(),
            );

            info!(
                "Submitting SL: side={:?} trigger={:.8} qty={:.8} ({}% from entry {:.8})",
                sl_side, sl_trigger_price, new_size.abs(), self.sl_pct * 100.0, self.entry_price
            );

            let sl_order = self.core.order_factory().stop_market(
                self.instrument_id,
                sl_side,
                sl_qty,
                sl_price,
                Some(TriggerType::LastPrice), // trigger on last traded price
                Some(NautilusTif::Gtc),
                None,    // expire_time
                Some(true), // reduce_only
                None,    // quote_quantity
                None,    // display_qty
                None,    // emulation_trigger
                None,    // trigger_instrument_id
                None,    // exec_algorithm_id
                None,    // exec_algorithm_params
                None,    // tags
                Some(sl_cl_ord_id),
            );

            if let Err(e) = self.submit_order(sl_order, None, None) {
                warn!("Failed to submit SL order: {}", e);
            } else {
                self.active_sl_order_id = Some(sl_cl_ord_id);
            }
        } else if new_size.abs() < 1e-12 && old_size.abs() > 1e-12 {
            // Position closed — clear SL tracking
            // The SL order should auto-cancel (reduce_only + no position),
            // but clear our tracking regardless.
            self.active_sl_order_id = None;
        }

        // Forward feedback to swarm agents (trade_count, etc.)
        let feedback = AgentFeedback {
            intent_id: nautilus_core::UUID4::new(), // not tied to a specific intent
            success: true,
            fill_price: Some(price),
            fill_quantity: Some(qty),
            slippage_bps: None,
            error: None,
        };
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                self.swarm.on_order_filled(&feedback).await;
            })
        });

        // Update StateEncoder with new position
        self.encoder
            .update_position(self.position_size, self.entry_price, unrealized_pnl);

        // Update reputation on position close
        if new_size.abs() < 1e-12 && old_size.abs() > 1e-12 {
            let realized_pnl = (price - self.entry_price) * old_size;
            self.trade_count += 1;
            if realized_pnl > 0.0 {
                self.win_count += 1;
            }
            let win_rate = self.win_count as f64 / self.trade_count as f64;
            // Score: 50 base + 50 * win_rate, clamped to 0-100
            let score = (50.0 + 50.0 * win_rate).clamp(0.0, 100.0);
            let level = self.autonomy.update(score);
            info!(
                "Reputation: trade#{} win_rate={:.1}% score={:.1} autonomy={:?}",
                self.trade_count,
                win_rate * 100.0,
                score,
                level
            );
        }

        Ok(())
    }

    fn on_quote(&mut self, quote: &QuoteTick) -> anyhow::Result<()> {
        info!(
            "Quote received: {} bid={} ask={}",
            quote.instrument_id, quote.bid_price, quote.ask_price
        );

        // 1. Feed quote into StateEncoder → ContextWindow → SharedStateBuffer
        self.encoder.on_quote(quote);

        // 2. Compute risk potential gradient and update encoder
        let position = self.position_size;
        let drawdown = if self.entry_price > 0.0 && position.abs() > 0.0 {
            let mid = (quote.bid_price.as_f64() + quote.ask_price.as_f64()) / 2.0;
            (mid - self.entry_price) / self.entry_price * position.signum()
        } else {
            0.0
        };
        let gradient = self.risk_field.gradient(position.abs(), drawdown, 0.0);
        let total_potential = self.risk_field.total(position.abs(), drawdown, 0.0);
        self.encoder.update_risk(total_potential, gradient.position, gradient.drawdown);

        if total_potential > 1.0 {
            warn!(
                "Risk potential HIGH: {:.2} (position={:.6}, drawdown={:.4}, gradient_mag={:.4})",
                total_potential, position, drawdown, gradient.magnitude()
            );
        } else {
            debug!(
                "Risk potential: {:.4} (position={:.6}, drawdown={:.4})",
                total_potential, position, drawdown
            );
        }

        // 3. Collect price history for autoresearch
        let mid = (quote.bid_price.as_f64() + quote.ask_price.as_f64()) / 2.0;
        self.last_mid_price = mid;
        self.price_history.push(mid);
        if self.price_history.len() > 600 {
            self.price_history.remove(0); // Keep last 600 prices (~10 min at 1Hz)
        }

        // Run autoresearch ratchet every 5 minutes (if enough data)
        let should_run_ratchet = self.price_history.len() >= 60
            && self.last_autoresearch.is_none_or(|t| t.elapsed().as_secs() > 300);

        if should_run_ratchet {
            self.last_autoresearch = Some(Instant::now());
            let prices = self.price_history.clone();
            let returns: Vec<f64> = prices.windows(2)
                .map(|w| (w[1] - w[0]) / w[0])
                .collect();

            // Generate hypotheses across multiple strategy types
            let recent_momentum = if returns.len() >= 10 {
                returns[returns.len()-10..].iter().sum::<f64>()
            } else {
                0.0
            };
            let recent_vol = if returns.len() >= 10 {
                let mean = recent_momentum / 10.0;
                (returns[returns.len()-10..].iter().map(|r| (r - mean).powi(2)).sum::<f64>() / 10.0).sqrt()
            } else {
                0.001
            };

            // Submit diverse hypotheses — one per strategy type
            let window = if recent_momentum > 0.0 { 10 } else { 20 };
            self.autoresearch.submit(StrategyHypothesis {
                id: nautilus_core::UUID4::new(),
                strategy_type: StrategyType::Momentum,
                description: format!("momentum window={}", window),
                code_patch: String::new(),
                parent_id: None,
            });

            self.autoresearch.submit(StrategyHypothesis {
                id: nautilus_core::UUID4::new(),
                strategy_type: StrategyType::MeanReversion,
                description: format!("mean_reversion window={} threshold=1.5", window),
                code_patch: String::new(),
                parent_id: None,
            });

            self.autoresearch.submit(StrategyHypothesis {
                id: nautilus_core::UUID4::new(),
                strategy_type: StrategyType::Breakout,
                description: format!("breakout window={}", window),
                code_patch: String::new(),
                parent_id: None,
            });

            self.autoresearch.submit(StrategyHypothesis {
                id: nautilus_core::UUID4::new(),
                strategy_type: StrategyType::VolTarget,
                description: format!("vol_target window={} target_vol={:.4}", window, recent_vol * 1.5),
                code_patch: String::new(),
                parent_id: None,
            });

            self.autoresearch.submit(StrategyHypothesis {
                id: nautilus_core::UUID4::new(),
                strategy_type: StrategyType::DualMomentum,
                description: format!("dual_momentum fast=5 slow={}", window),
                code_patch: String::new(),
                parent_id: None,
            });

            // Run ratchet
            self.autoresearch.run_ratchet(&returns, &prices);
            info!(
                "Autoresearch: {} hypotheses evaluated, {} accepted, baseline_ir={:.4}",
                self.autoresearch.total_evaluated(),
                self.autoresearch.accepted_count(),
                self.autoresearch.baseline_ir
            );
        }

        // 4. Cooldown check — skip swarm cycle if too soon
        let now = Instant::now();
        if let Some(last) = self.last_cycle_time {
            let elapsed = now.duration_since(last).as_secs();
            if elapsed < self.cooldown_secs {
                debug!(
                    "Cooldown: {}s elapsed, need {}s — skipping cycle",
                    elapsed, self.cooldown_secs
                );
                return Ok(());
            }
        }
        self.last_cycle_time = Some(now);

        // 4. Read current context and run swarm cycle
        // === P0: Cycle timeout to prevent hangs ===
        let ctx = self.encoder.current_context();
        let directives = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                match tokio::time::timeout(
                    std::time::Duration::from_secs(30),
                    self.swarm.run_cycle(ctx),
                ).await {
                    Ok(directives) => directives,
                    Err(_) => {
                        tracing::error!("Swarm cycle timeout (30s) — skipping cycle");
                        vec![]
                    }
                }
            })
        });

        debug!("Swarm cycle produced {} directives", directives.len());

        // 5. Execute resulting directives
        if !directives.is_empty() {
            debug!("Swarm produced {} directives", directives.len());
            self.execute_directives(directives);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, AgentFeedback};
    use crate::intent::{AgentIntent, IntentType, PositionTarget, RiskBudget};
    use async_trait::async_trait;
    use nautilus_core::UUID4;
    use std::time::Duration;

    struct TestAgent {
        id: String,
        intent_type: IntentType,
    }

    #[async_trait]
    impl Agent for TestAgent {
        fn id(&self) -> &str {
            &self.id
        }

        async fn perceive(&mut self, _ctx: &nautilus_state_encoder::ContextWindow) -> AgentIntent {
            AgentIntent {
                id: UUID4::new(),
                agent_id: self.id.clone(),
                intent_type: self.intent_type,
                description: format!("{:?} from {}", self.intent_type, self.id),
                target_instrument: InstrumentId::from("SOL-USDC.OKX"),
                target_position: Some(PositionTarget {
                    size: 10.0,
                    delta: None,
                }),
                risk_budget: RiskBudget {
                    max_loss: 100.0,
                    max_position: 100.0,
                    max_drawdown_bps: 500.0,
                },
                constraints: vec![],
                confidence: 0.8,
                reputation_score: 0.5,
                time_horizon: Duration::from_secs(300),
                title: format!("{:?} from {}", self.intent_type, self.id),
                reasoning: String::new(),
                confidence_label: crate::intent::ConfidenceLabel::Low,
                risk_snapshot: crate::intent::RiskSnapshot::default(),
                expires_at: None,
                tags: vec![],
            }
        }

        async fn on_feedback(&mut self, _feedback: &AgentFeedback) {}
    }

    #[test]
    fn test_swarm_strategy_creation() {
        let mut swarm = SwarmCoordinator::new(crate::swarm::ConsensusStrategy::Pipeline);
        swarm.add_agent(Box::new(TestAgent {
            id: "test-agent".to_string(),
            intent_type: IntentType::TrendFollow,
        }));

        let strategy = SwarmStrategy::new(
            "SWARM-001",
            InstrumentId::from("SOL-USDC.OKX"),
            swarm,
        );

        assert_eq!(strategy.swarm.agent_count(), 1);
        assert_eq!(strategy.instrument_id, InstrumentId::from("SOL-USDC.OKX"));
    }

    #[test]
    fn test_swarm_strategy_debug() {
        let mut swarm = SwarmCoordinator::new(crate::swarm::ConsensusStrategy::Pipeline);
        swarm.add_agent(Box::new(TestAgent {
            id: "debug-agent".to_string(),
            intent_type: IntentType::Hold,
        }));

        let strategy = SwarmStrategy::new(
            "SWARM-DBG",
            InstrumentId::from("ETH-USDC.OKX"),
            swarm,
        );

        let debug_str = format!("{:?}", strategy);
        assert!(debug_str.contains("SwarmStrategy"));
        assert!(debug_str.contains("ETH-USDC.OKX"));
        assert!(debug_str.contains("agent_count: 1"));
    }
}

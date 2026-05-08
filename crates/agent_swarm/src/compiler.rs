//! IntentCompiler: two-stage compilation from AgentIntent to ExecutionDirective.
//!
//! Stage 1: Template matching (deterministic, <1μs)
//! Stage 2: Parameter optimization (Almgren-Chriss, ~1ms)

use tracing::debug;

use nautilus_state_encoder::ContextWindow;

use crate::intent::{
    AgentIntent, ExecutionDirective, ExecutionStyle, IntentType, OrderSide, OrderSpecification,
    TimeInForce,
};

/// Errors during intent compilation.
// === P0: Basic error enum, P1: ValidationRule chain ===
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompileError {
    /// Instrument ID missing venue suffix (e.g., ".OKX").
    InvalidInstrument,
    /// Quantity is non-finite, negative, or zero.
    InvalidQuantity,
    /// Could not parse a valid mid price from market state.
    InvalidPrice,
    /// Intent violates risk budget constraints.
    RiskBudgetViolation(String),
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInstrument => write!(f, "invalid instrument ID (missing venue suffix)"),
            Self::InvalidQuantity => write!(f, "invalid quantity (non-finite, negative, or zero)"),
            Self::InvalidPrice => write!(f, "invalid price (could not parse mid from market state)"),
            Self::RiskBudgetViolation(msg) => write!(f, "risk budget violation: {}", msg),
        }
    }
}

impl std::error::Error for CompileError {}

/// Pre-defined execution templates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionTemplate {
    DeltaHedgeTwap,
    GammaScalpIoc,
    TrendFollowTrailing,
    MeanRevertLimit,
    LiquidationCaptureIoc,
    Hold,
}

impl ExecutionTemplate {
    /// Stage 1: Deterministic mapping from intent type to template.
    pub fn from_intent(intent: &AgentIntent) -> Self {
        match intent.intent_type {
            IntentType::DeltaHedge => Self::DeltaHedgeTwap,
            IntentType::GammaScalp => Self::GammaScalpIoc,
            IntentType::TrendFollow => Self::TrendFollowTrailing,
            IntentType::MeanReversion => Self::MeanRevertLimit,
            IntentType::LiquidationCapture => Self::LiquidationCaptureIoc,
            IntentType::Hold | IntentType::Veto => Self::Hold,
        }
    }
}

/// Two-stage intent compiler.
pub struct IntentCompiler;

impl IntentCompiler {
    /// Compile an agent intent into an execution directive.
    pub fn compile(intent: &AgentIntent, ctx: &ContextWindow) -> Result<ExecutionDirective, CompileError> {
        // === P0: Basic validation, P1: ValidationRule chain ===
        let template = ExecutionTemplate::from_intent(intent);

        // Hold intents skip validation (no orders to place)
        if template == ExecutionTemplate::Hold {
            return Ok(Self::compile_hold(intent));
        }

        // Validate instrument has venue suffix
        let inst_str = intent.target_instrument.to_string();
        if !inst_str.contains('.') {
            return Err(CompileError::InvalidInstrument);
        }

        // Validate target position quantity (if present)
        if let Some(ref target) = intent.target_position {
            if !target.size.is_finite() || target.size.abs() < 1e-12 {
                return Err(CompileError::InvalidQuantity);
            }
        }

        // Validate risk budget constraints
        Self::validate_risk_budget(intent, ctx)?;

        // Wedge zone filter: skip dust orders below minimum notional
        if let Some(ref target) = intent.target_position {
            let mid = Self::parse_mid_price(ctx).unwrap_or(0.0);
            let notional = target.size.abs() * mid;
            let min_notional: f64 = std::env::var("SEXTANT_MIN_NOTIONAL_USD")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(20.0);

            // Volatility bypass: if spread > 20bps, allow sub-threshold orders
            let spread_bps = Self::parse_spread_bps(ctx);
            let volatility_bypass = spread_bps > 20.0;

            if notional < min_notional && !volatility_bypass {
                debug!(
                    "Wedge filter: notional ${:.2} < ${:.2} min, returning hold",
                    notional, min_notional
                );
                return Ok(Self::compile_hold(intent));
            }
        }

        let mut directive = match template {
            ExecutionTemplate::Hold => unreachable!(),
            ExecutionTemplate::DeltaHedgeTwap => Self::compile_delta_hedge(intent, ctx)?,
            ExecutionTemplate::GammaScalpIoc => Self::compile_gamma_scalp(intent),
            ExecutionTemplate::TrendFollowTrailing => Self::compile_trend_follow(intent),
            ExecutionTemplate::MeanRevertLimit => Self::compile_mean_revert(intent, ctx)?,
            ExecutionTemplate::LiquidationCaptureIoc => Self::compile_liquidation(intent),
        };

        // Apply sigmoid-based exposure sizing (adjusts quantity, never reverses direction)
        Self::apply_sigmoid_sizing(&mut directive, intent, ctx);

        Ok(directive)
    }

    fn compile_hold(intent: &AgentIntent) -> ExecutionDirective {
        ExecutionDirective {
            intent_id: intent.id,
            orders: vec![],
            execution_style: ExecutionStyle::Ioc,
            time_horizon: intent.time_horizon,
            max_slippage_bps: 0.0,
        }
    }

    fn compile_delta_hedge(intent: &AgentIntent, ctx: &ContextWindow) -> Result<ExecutionDirective, CompileError> {
        let target = intent.target_position.unwrap_or(crate::intent::PositionTarget {
            size: 0.0,
            delta: None,
        });
        let delta = target.size - ctx.position_size;

        if delta.abs() < 1e-8 {
            return Ok(Self::compile_hold(intent));
        }

        let side = if delta > 0.0 {
            OrderSide::Buy
        } else {
            OrderSide::Sell
        };

        // Almgren-Chriss simplified: optimal slices based on urgency
        let slices = Self::optimal_slices(delta.abs(), intent.time_horizon);
        let interval = intent.time_horizon / slices;

        Ok(ExecutionDirective {
            intent_id: intent.id,
            orders: vec![OrderSpecification {
                instrument_id: intent.target_instrument,
                side,
                quantity: delta.abs(),
                price: None,
                time_in_force: TimeInForce::Gtc,
            }],
            execution_style: ExecutionStyle::Twap { slices, interval },
            time_horizon: intent.time_horizon,
            max_slippage_bps: intent
                .constraints
                .iter()
                .find_map(|c| {
                    if let crate::intent::ConstraintValue::SlippageBps(s) = c.value {
                        Some(s)
                    } else {
                        None
                    }
                })
                .unwrap_or(50.0),
        })
    }

    fn compile_gamma_scalp(intent: &AgentIntent) -> ExecutionDirective {
        let target = intent.target_position.unwrap_or(crate::intent::PositionTarget {
            size: 0.0,
            delta: None,
        });
        let side = if target.size > 0.0 {
            OrderSide::Buy
        } else {
            OrderSide::Sell
        };

        ExecutionDirective {
            intent_id: intent.id,
            orders: vec![OrderSpecification {
                instrument_id: intent.target_instrument,
                side,
                quantity: target.size.abs(),
                price: None,
                time_in_force: TimeInForce::Ioc,
            }],
            execution_style: ExecutionStyle::Ioc,
            time_horizon: intent.time_horizon,
            max_slippage_bps: 100.0,
        }
    }

    fn compile_trend_follow(intent: &AgentIntent) -> ExecutionDirective {
        let target = intent.target_position.unwrap_or(crate::intent::PositionTarget {
            size: 0.0,
            delta: None,
        });
        let side = if target.size > 0.0 {
            OrderSide::Buy
        } else {
            OrderSide::Sell
        };

        ExecutionDirective {
            intent_id: intent.id,
            orders: vec![OrderSpecification {
                instrument_id: intent.target_instrument,
                side,
                quantity: target.size.abs(),
                price: None,
                time_in_force: TimeInForce::Gtc,
            }],
            execution_style: ExecutionStyle::TrailingStop { offset_bps: 50.0 },
            time_horizon: intent.time_horizon,
            max_slippage_bps: 200.0,
        }
    }

    fn compile_mean_revert(intent: &AgentIntent, ctx: &ContextWindow) -> Result<ExecutionDirective, CompileError> {
        // Parse mid price from market state — fail if unparseable
        let mid = Self::parse_mid_price(ctx)?;
        let target = intent.target_position.unwrap_or(crate::intent::PositionTarget {
            size: 0.0,
            delta: None,
        });
        let side = if target.size > 0.0 {
            OrderSide::Buy
        } else {
            OrderSide::Sell
        };

        Ok(ExecutionDirective {
            intent_id: intent.id,
            orders: vec![OrderSpecification {
                instrument_id: intent.target_instrument,
                side,
                quantity: target.size.abs(),
                price: Some(mid),
                time_in_force: TimeInForce::Gtc,
            }],
            execution_style: ExecutionStyle::Limit {
                price: mid,
                post_only: true,
            },
            time_horizon: intent.time_horizon,
            max_slippage_bps: 10.0,
        })
    }

    fn compile_liquidation(intent: &AgentIntent) -> ExecutionDirective {
        let target = intent.target_position.unwrap_or(crate::intent::PositionTarget {
            size: 0.0,
            delta: None,
        });
        let side = if target.size > 0.0 {
            OrderSide::Buy
        } else {
            OrderSide::Sell
        };

        ExecutionDirective {
            intent_id: intent.id,
            orders: vec![OrderSpecification {
                instrument_id: intent.target_instrument,
                side,
                quantity: target.size.abs(),
                price: None,
                time_in_force: TimeInForce::Ioc,
            }],
            execution_style: ExecutionStyle::Ioc,
            time_horizon: intent.time_horizon,
            max_slippage_bps: 500.0,
        }
    }

    /// Validate intent against risk budget constraints.
    /// Checks: position cap, single-trade max loss, drawdown limit.
    fn validate_risk_budget(intent: &AgentIntent, ctx: &ContextWindow) -> Result<(), CompileError> {
        let budget = &intent.risk_budget;
        let target = match intent.target_position {
            Some(ref t) => t,
            None => return Ok(()),
        };

        // Position cap: target.size.abs() must not exceed max_position
        if target.size.abs() > budget.max_position {
            return Err(CompileError::RiskBudgetViolation(format!(
                "target size {:.4} exceeds max_position {:.4}",
                target.size.abs(), budget.max_position
            )));
        }

        // Single-trade max loss: estimated loss from slippage must not exceed max_loss
        // Use 50bps default slippage estimate (matches max_slippage_bps in most templates)
        let mid = Self::parse_mid_price(ctx).unwrap_or(0.0);
        if mid > 0.0 {
            let slippage_estimate = mid * 0.005; // 50bps
            let estimated_loss = target.size.abs() * slippage_estimate;
            if estimated_loss > budget.max_loss {
                return Err(CompileError::RiskBudgetViolation(format!(
                    "estimated trade loss {:.2} exceeds max_loss {:.2} (size={} * slippage={:.4})",
                    estimated_loss, budget.max_loss, target.size.abs(), slippage_estimate
                )));
            }
        }

        // Drawdown check: current drawdown vs max_drawdown_bps
        // Uses entry_price from ContextWindow (position_cost field)
        if ctx.entry_price > 0.0 && mid > 0.0 {
            let drawdown = (mid - ctx.entry_price).abs() / ctx.entry_price * 10000.0; // bps
            if drawdown > budget.max_drawdown_bps {
                return Err(CompileError::RiskBudgetViolation(format!(
                    "current drawdown {:.0}bps exceeds max_drawdown_bps {:.0}bps",
                    drawdown, budget.max_drawdown_bps
                )));
            }
        }

        Ok(())
    }

    /// Almgren-Chriss simplified: more slices for larger orders relative to time.
    fn optimal_slices(size: f64, horizon: std::time::Duration) -> u32 {
        let urgency = size / (horizon.as_secs_f64() + 1e-8);
        ((urgency * 10.0).ceil() as u32).clamp(1, 100)
    }

    /// Parse mid price from market state text.
    fn parse_mid_price(ctx: &ContextWindow) -> Result<f64, CompileError> {
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
            (Some(b), Some(a)) if b > 0.0 && a > 0.0 => Ok((b + a) / 2.0),
            _ => Err(CompileError::InvalidPrice),
        }
    }

    /// Parse bid-ask spread in basis points from market state text.
    fn parse_spread_bps(ctx: &ContextWindow) -> f64 {
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
            (Some(b), Some(a)) if b > 0.0 && a > 0.0 => ((a - b) / ((a + b) / 2.0)) * 10000.0,
            _ => 0.0,
        }
    }

    /// Apply sigmoid-based exposure sizing to a compiled directive.
    ///
    /// Uses `sigmoid_exposure` to compute target exposure from agent confidence
    /// and current inventory, then scales order quantities accordingly.
    /// Only adjusts magnitude — never reverses direction.
    ///
    /// TODO(P2): When Agent learning is implemented, move Sigmoid into Agent-internal
    /// `perceive()` so intent size matches execution size (avoids feedback distortion).
    fn apply_sigmoid_sizing(
        directive: &mut ExecutionDirective,
        intent: &AgentIntent,
        ctx: &ContextWindow,
    ) {
        let max_position = intent.risk_budget.max_position;
        if max_position <= 0.0 || directive.orders.is_empty() {
            return;
        }

        let beta: f64 = std::env::var("SEXTANT_SIZING_BETA")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(2.0);
        let gamma: f64 = std::env::var("SEXTANT_SIZING_GAMMA")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.5);
        let max_leverage: f64 = std::env::var("SEXTANT_MAX_LEVERAGE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(2.0);

        // Direction from order side: Buy = +1, Sell = -1
        let direction = match directive.orders[0].side {
            OrderSide::Buy => 1.0f64,
            OrderSide::Sell => -1.0f64,
        };

        // Signed signal: direction * confidence
        let signal = direction * intent.confidence;

        // Current exposure: position / max_position, clamped to [-1, 1]
        let current_exposure = (ctx.position_size / max_position).clamp(-1.0, 1.0);

        // Compute target exposure via sigmoid
        let target_exposure = sigmoid_exposure(signal, current_exposure, max_leverage, beta, gamma);

        // Convert to target quantity (magnitude only — direction already encoded)
        let target_qty = target_exposure.abs() * max_position;

        // Scale: use the smaller of sigmoid target and original quantity
        // Never increase beyond what the agent requested
        let original_qty = directive.orders[0].quantity;
        let scaled_qty = target_qty.min(original_qty);

        if scaled_qty < original_qty {
            debug!(
                "Sigmoid sizing: {:.4} → {:.4} (signal={:.2}, exposure={:.2}, β={:.1}, γ={:.1})",
                original_qty, scaled_qty, signal, current_exposure, beta, gamma
            );
            for order in &mut directive.orders {
                order.quantity = scaled_qty;
            }
        }
    }
}

/// Sigmoid exposure controller for contract trading.
///
/// Maps signal + inventory bias to target exposure in [-max_leverage, +max_leverage].
/// Positive = long, negative = short, magnitude = leveraged position.
#[inline(always)]
pub fn sigmoid_exposure(
    signal: f64,           // signed confidence: +1 strong long, -1 strong short
    current_exposure: f64, // current position / max_position, in [-1, 1]
    max_leverage: f64,     // e.g. 2.0 for 2x
    beta: f64,             // signal aggressiveness
    gamma: f64,            // inventory mean-reversion force
) -> f64 {
    let inventory_bias = -current_exposure; // [-1, 1]
    let x = beta * signal + gamma * inventory_bias;
    let normalized = 2.0 / (1.0 + (-x).exp()) - 1.0; // [-1, 1]
    normalized * max_leverage // [-L, +L]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent::{PositionTarget, RiskBudget};
    use nautilus_core::UUID4;
    use nautilus_model::identifiers::InstrumentId;
    use std::time::Duration;

    fn make_intent(intent_type: IntentType) -> AgentIntent {
        AgentIntent {
            id: UUID4::new(),
            agent_id: "test-agent".to_string(),
            intent_type,
            description: "test".to_string(),
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
            title: "test".to_string(),
            reasoning: String::new(),
            confidence_label: crate::intent::ConfidenceLabel::Low,
            risk_snapshot: crate::intent::RiskSnapshot::default(),
            expires_at: None,
            tags: vec![],
        }
    }

    #[test]
    fn test_hold_compilation() {
        let intent = make_intent(IntentType::Hold);
        let ctx = ContextWindow::zeroed();
        let directive = IntentCompiler::compile(&intent, &ctx).unwrap();
        assert!(directive.orders.is_empty());
    }

    #[test]
    fn test_delta_hedge_compilation() {
        let intent = make_intent(IntentType::DeltaHedge);
        let mut ctx = ContextWindow::zeroed();
        ctx.position_size = 5.0;
        ctx.set_market_state("OrderBook bid:150.0 | ask:150.1");

        let directive = IntentCompiler::compile(&intent, &ctx).unwrap();
        assert_eq!(directive.orders.len(), 1);
        assert_eq!(directive.orders[0].quantity, 5.0); // 10 - 5 = 5
        assert_eq!(directive.orders[0].side, OrderSide::Buy);
        assert!(matches!(
            directive.execution_style,
            ExecutionStyle::Twap { .. }
        ));
    }

    #[test]
    fn test_template_mapping() {
        let cases = vec![
            (IntentType::DeltaHedge, "DeltaHedgeTwap"),
            (IntentType::GammaScalp, "GammaScalpIoc"),
            (IntentType::TrendFollow, "TrendFollowTrailing"),
            (IntentType::MeanReversion, "MeanRevertLimit"),
            (IntentType::LiquidationCapture, "LiquidationCaptureIoc"),
            (IntentType::Hold, "Hold"),
        ];

        for (intent_type, _expected) in cases {
            let intent = make_intent(intent_type);
            let template = ExecutionTemplate::from_intent(&intent);
            // Just verify it doesn't panic
            let _ = format!("{:?}", template);
        }
    }

    #[test]
    fn test_risk_budget_max_position_reject() {
        let mut intent = make_intent(IntentType::DeltaHedge);
        intent.target_position = Some(PositionTarget {
            size: 200.0, // exceeds max_position=100
            delta: None,
        });
        let mut ctx = ContextWindow::zeroed();
        ctx.set_market_state("OrderBook bid:150.0 | ask:150.1");

        let result = IntentCompiler::compile(&intent, &ctx);
        assert!(result.is_err());
        match result.unwrap_err() {
            CompileError::RiskBudgetViolation(msg) => {
                assert!(msg.contains("max_position"), "unexpected msg: {}", msg);
            }
            other => panic!("expected RiskBudgetViolation, got: {:?}", other),
        }
    }

    #[test]
    fn test_risk_budget_within_limits() {
        let mut intent = make_intent(IntentType::DeltaHedge);
        intent.target_position = Some(PositionTarget {
            size: 5.0, // within max_position=100
            delta: None,
        });
        intent.risk_budget.max_loss = 1000.0; // generous loss limit
        let mut ctx = ContextWindow::zeroed();
        ctx.set_market_state("OrderBook bid:150.0 | ask:150.1");

        let result = IntentCompiler::compile(&intent, &ctx);
        assert!(result.is_ok(), "expected Ok, got {:?}", result.err());
    }

    #[test]
    fn test_risk_budget_max_loss_reject() {
        let mut intent = make_intent(IntentType::GammaScalp);
        intent.target_position = Some(PositionTarget {
            size: 100.0,
            delta: None,
        });
        intent.risk_budget.max_loss = 1.0; // very tight loss limit
        let mut ctx = ContextWindow::zeroed();
        ctx.set_market_state("OrderBook bid:150.0 | ask:150.1");
        // At mid=150.05, slippage_est = 150.05 * 0.005 = 0.75025
        // estimated_loss = 100 * 0.75025 = 75.025 > 1.0 → reject

        let result = IntentCompiler::compile(&intent, &ctx);
        assert!(result.is_err());
        match result.unwrap_err() {
            CompileError::RiskBudgetViolation(msg) => {
                assert!(msg.contains("max_loss"), "unexpected msg: {}", msg);
            }
            other => panic!("expected RiskBudgetViolation, got: {:?}", other),
        }
    }

    #[test]
    fn test_risk_budget_drawdown_reject() {
        let mut intent = make_intent(IntentType::TrendFollow);
        intent.target_position = Some(PositionTarget {
            size: 5.0,
            delta: None,
        });
        intent.risk_budget.max_drawdown_bps = 100.0; // 1% max drawdown
        let mut ctx = ContextWindow::zeroed();
        ctx.entry_price = 100.0;
        ctx.set_market_state("OrderBook bid:80.0 | ask:80.1");
        // drawdown = |80.05 - 100| / 100 * 10000 = 19950 bps > 100 → reject

        let result = IntentCompiler::compile(&intent, &ctx);
        assert!(result.is_err());
        match result.unwrap_err() {
            CompileError::RiskBudgetViolation(msg) => {
                assert!(msg.contains("drawdown"), "unexpected msg: {}", msg);
            }
            other => panic!("expected RiskBudgetViolation, got: {:?}", other),
        }
    }

    #[test]
    fn test_wedge_filter_dust_order() {
        // Notional = 0.1 * 50.0 = $5.0 < $20 min → should return Hold
        let mut intent = make_intent(IntentType::GammaScalp);
        intent.target_position = Some(PositionTarget {
            size: 0.1,
            delta: None,
        });
        intent.risk_budget.max_loss = 10000.0; // generous
        let mut ctx = ContextWindow::zeroed();
        ctx.set_market_state("OrderBook bid:50.0 | ask:50.01");

        let directive = IntentCompiler::compile(&intent, &ctx).unwrap();
        assert!(directive.orders.is_empty(), "dust order should be filtered to Hold");
    }

    #[test]
    fn test_wedge_filter_passes_normal() {
        // Notional = 1.0 * 150.0 = $150.0 > $20 min → should proceed
        let mut intent = make_intent(IntentType::DeltaHedge);
        intent.target_position = Some(PositionTarget {
            size: 1.0,
            delta: None,
        });
        intent.risk_budget.max_loss = 10000.0; // generous
        let mut ctx = ContextWindow::zeroed();
        ctx.position_size = 0.0; // delta = 1.0 - 0.0 = 1.0
        ctx.set_market_state("OrderBook bid:150.0 | ask:150.1");

        let directive = IntentCompiler::compile(&intent, &ctx).unwrap();
        assert!(!directive.orders.is_empty(), "normal order should pass wedge filter");
    }

    #[test]
    fn test_wedge_filter_high_vol_bypass() {
        // Notional = 0.1 * 50.0 = $5.0 < $20 min
        // But spread = (60.0 - 40.0) / 50.0 * 10000 = 4000 bps > 20 bps → bypass
        let mut intent = make_intent(IntentType::GammaScalp);
        intent.target_position = Some(PositionTarget {
            size: 0.1,
            delta: None,
        });
        intent.risk_budget.max_loss = 10000.0; // generous
        let mut ctx = ContextWindow::zeroed();
        ctx.set_market_state("OrderBook bid:40.0 | ask:60.0");

        let directive = IntentCompiler::compile(&intent, &ctx).unwrap();
        assert!(!directive.orders.is_empty(), "high-vol dust order should bypass wedge filter");
    }

    #[test]
    fn test_sigmoid_neutral() {
        // signal=0, exposure=0 → output ≈ 0
        let result = sigmoid_exposure(0.0, 0.0, 2.0, 2.0, 0.5);
        assert!(result.abs() < 0.01, "neutral signal should produce ~0, got {}", result);
    }

    #[test]
    fn test_sigmoid_strong_long() {
        // signal=0.9, exposure=0 → large positive output
        let result = sigmoid_exposure(0.9, 0.0, 2.0, 2.0, 0.5);
        assert!(result > 1.0, "strong long should produce large positive, got {}", result);
    }

    #[test]
    fn test_sigmoid_short() {
        // signal=-0.9, exposure=0 → large negative output
        let result = sigmoid_exposure(-0.9, 0.0, 2.0, 2.0, 0.5);
        assert!(result < -1.0, "strong short should produce large negative, got {}", result);
    }

    #[test]
    fn test_sigmoid_inventory_reversion() {
        // signal=0.9 but already heavily long (exposure=0.9) → dampened output
        let no_inventory = sigmoid_exposure(0.9, 0.0, 2.0, 2.0, 0.5);
        let with_inventory = sigmoid_exposure(0.9, 0.9, 2.0, 2.0, 0.5);
        assert!(
            with_inventory < no_inventory,
            "inventory should dampen long signal: no_inv={}, with_inv={}",
            no_inventory, with_inventory
        );
    }

    #[test]
    fn test_sigmoid_leverage_2x() {
        // signal=1.0, exposure=0, max_leverage=2.0 → output ≈ 2.0
        let result = sigmoid_exposure(1.0, 0.0, 2.0, 10.0, 0.0); // high beta, no gamma
        assert!(
            (result - 2.0).abs() < 0.01,
            "full signal at 2x leverage should ≈ 2.0, got {}",
            result
        );
    }

    #[test]
    fn test_sigmoid_symmetry() {
        // Positive and negative signals should produce symmetric outputs
        let long = sigmoid_exposure(0.7, 0.0, 2.0, 2.0, 0.5);
        let short = sigmoid_exposure(-0.7, 0.0, 2.0, 2.0, 0.5);
        assert!(
            (long + short).abs() < 0.01,
            "sigmoid should be symmetric: long={}, short={}",
            long,
            short
        );
    }

    #[test]
    fn test_compile_applies_sigmoid() {
        // Verify that compile() applies sigmoid sizing
        // With low confidence (0.1) and large original size, sigmoid should reduce
        let mut intent = make_intent(IntentType::GammaScalp);
        intent.confidence = 0.1; // low confidence
        intent.target_position = Some(PositionTarget {
            size: 200.0, // large original request
            delta: None,
        });
        intent.risk_budget.max_position = 500.0; // must be >= target.size for risk budget check
        intent.risk_budget.max_loss = 100000.0;
        let mut ctx = ContextWindow::zeroed();
        ctx.set_market_state("OrderBook bid:150.0 | ask:150.1");

        let directive = IntentCompiler::compile(&intent, &ctx).unwrap();
        // Sigmoid with signal=0.1 → normalized ≈ 0.1 → target_exposure ≈ 0.2
        // target_qty = 0.2 * 500 = 100, min(200, 100) = 100
        assert!(
            directive.orders[0].quantity < 200.0,
            "sigmoid should reduce quantity from 200, got {}",
            directive.orders[0].quantity
        );
        assert!(
            directive.orders[0].quantity > 0.0,
            "quantity should be positive, got {}",
            directive.orders[0].quantity
        );
    }
}

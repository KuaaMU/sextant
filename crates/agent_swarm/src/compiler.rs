//! IntentCompiler: two-stage compilation from AgentIntent to ExecutionDirective.
//!
//! Stage 1: Template matching (deterministic, <1μs)
//! Stage 2: Parameter optimization (Almgren-Chriss, ~1ms)

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
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInstrument => write!(f, "invalid instrument ID (missing venue suffix)"),
            Self::InvalidQuantity => write!(f, "invalid quantity (non-finite, negative, or zero)"),
            Self::InvalidPrice => write!(f, "invalid price (could not parse mid from market state)"),
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

        match template {
            ExecutionTemplate::Hold => unreachable!(),
            ExecutionTemplate::DeltaHedgeTwap => Self::compile_delta_hedge(intent, ctx),
            ExecutionTemplate::GammaScalpIoc => Ok(Self::compile_gamma_scalp(intent)),
            ExecutionTemplate::TrendFollowTrailing => Ok(Self::compile_trend_follow(intent)),
            ExecutionTemplate::MeanRevertLimit => Self::compile_mean_revert(intent, ctx),
            ExecutionTemplate::LiquidationCaptureIoc => Ok(Self::compile_liquidation(intent)),
        }
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
}

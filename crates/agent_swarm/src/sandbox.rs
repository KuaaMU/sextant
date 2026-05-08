//! AgentSandbox — per-agent isolation via catch_unwind.
//!
//! Wraps an Agent so that panics in `perceive()` or `on_feedback()`
//! are caught and logged instead of crashing the entire swarm.

use std::panic::AssertUnwindSafe;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

use async_trait::async_trait;
use nautilus_model::identifiers::InstrumentId;
use nautilus_state_encoder::ContextWindow;
use tracing::error;

use crate::agent::{Agent, AgentFeedback};
use crate::intent::AgentIntent;

/// Create a noop waker for polling futures in catch_unwind.
fn noop_waker() -> Waker {
    fn clone(_: *const ()) -> RawWaker { RawWaker::new(std::ptr::null(), &VTABLE) }
    fn noop(_: *const ()) {}
    static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, noop, noop, noop);
    unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) }
}

/// Poll an async function to completion inside catch_unwind.
/// Works within an existing tokio runtime (no block_on).
fn block_on_sync<F: std::future::Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);
    loop {
        match f.as_mut().poll(&mut cx) {
            Poll::Ready(output) => return output,
            Poll::Pending => continue, // our agents never truly yield
        }
    }
}

/// Wrapper that isolates an Agent from panics.
///
/// If the inner agent panics during `perceive()`, a Hold intent is returned.
/// If it panics during `on_feedback()`, the panic is logged and swallowed.
pub struct AgentSandbox {
    inner: Box<dyn Agent>,
    /// Fallback instrument for Hold intents when panic occurs.
    fallback_instrument: InstrumentId,
}

impl AgentSandbox {
    pub fn new(agent: Box<dyn Agent>) -> Self {
        Self {
            inner: agent,
            fallback_instrument: InstrumentId::from("BTC-USDT-SWAP.OKX"),
        }
    }

    /// Set a custom fallback instrument for panic-recovery Hold intents.
    pub fn with_fallback_instrument(mut self, instrument: InstrumentId) -> Self {
        self.fallback_instrument = instrument;
        self
    }
}

#[async_trait]
impl Agent for AgentSandbox {
    fn id(&self) -> &str {
        self.inner.id()
    }

    async fn perceive(&mut self, ctx: &ContextWindow) -> AgentIntent {
        let agent_id = self.inner.id().to_string();
        let instrument = self.fallback_instrument;

        // catch_unwind + block_on_sync: works within an existing tokio runtime
        match std::panic::catch_unwind(AssertUnwindSafe(|| {
            block_on_sync(self.inner.perceive(ctx))
        })) {
            Ok(intent) => intent,
            Err(panic_payload) => {
                let msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "unknown panic".to_string()
                };
                error!(
                    "Agent '{}' panicked in perceive(): {} — returning Hold",
                    agent_id, msg
                );
                AgentIntent::hold(&agent_id, instrument)
            }
        }
    }

    async fn on_feedback(&mut self, feedback: &AgentFeedback) {
        let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
            block_on_sync(self.inner.on_feedback(feedback))
        }));
        // If on_feedback panics, we just log and continue — the agent's state
        // may be inconsistent but the swarm survives.
        // The catch_unwind already captures the panic; tracing happens inside
        // the agent or via the default panic hook.
    }

    fn confidence(&self) -> f64 {
        self.inner.confidence()
    }

    fn reputation_score(&self) -> f64 {
        self.inner.reputation_score()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent::{IntentType, PositionTarget, RiskBudget};
    use async_trait::async_trait;
    use nautilus_core::UUID4;
    use nautilus_model::identifiers::InstrumentId;
    use std::time::Duration;

    /// Agent that always panics in perceive().
    struct PanicAgent;

    #[async_trait]
    impl Agent for PanicAgent {
        fn id(&self) -> &str {
            "panic-agent"
        }

        async fn perceive(&mut self, _ctx: &ContextWindow) -> AgentIntent {
            panic!("intentional test panic");
        }

        async fn on_feedback(&mut self, _feedback: &AgentFeedback) {}
    }

    /// Agent that always panics in on_feedback().
    struct FeedbackPanicAgent {
        id: String,
    }

    #[async_trait]
    impl Agent for FeedbackPanicAgent {
        fn id(&self) -> &str {
            &self.id
        }

        async fn perceive(&mut self, _ctx: &ContextWindow) -> AgentIntent {
            AgentIntent::hold(&self.id, InstrumentId::from("BTC-USDT-SWAP.OKX"))
        }

        async fn on_feedback(&mut self, _feedback: &AgentFeedback) {
            panic!("feedback panic");
        }
    }

    /// Normal agent for comparison.
    struct NormalAgent;

    #[async_trait]
    impl Agent for NormalAgent {
        fn id(&self) -> &str {
            "normal-agent"
        }

        async fn perceive(&mut self, _ctx: &ContextWindow) -> AgentIntent {
            AgentIntent {
                id: UUID4::new(),
                agent_id: "normal-agent".to_string(),
                intent_type: IntentType::TrendFollow,
                description: "test".into(),
                target_instrument: InstrumentId::from("BTC-USDT-SWAP.OKX"),
                target_position: Some(PositionTarget {
                    size: 1.0,
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
                title: "test".into(),
                reasoning: String::new(),
                confidence_label: crate::intent::ConfidenceLabel::High,
                risk_snapshot: crate::intent::RiskSnapshot::default(),
                expires_at: None,
                tags: vec![],
            }
        }

        async fn on_feedback(&mut self, _feedback: &AgentFeedback) {}
    }

    fn fresh_ctx() -> ContextWindow {
        let mut ctx = ContextWindow::zeroed();
        ctx.timestamp_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        ctx
    }

    #[tokio::test]
    async fn test_sandbox_catches_panic() {
        let mut sandbox = AgentSandbox::new(Box::new(PanicAgent));
        let ctx = fresh_ctx();
        let intent = sandbox.perceive(&ctx).await;
        // Should return Hold instead of panicking
        assert_eq!(intent.intent_type, IntentType::Hold);
        assert_eq!(intent.agent_id, "panic-agent");
    }

    #[tokio::test]
    async fn test_sandbox_passes_normal() {
        let mut sandbox = AgentSandbox::new(Box::new(NormalAgent));
        let ctx = fresh_ctx();
        let intent = sandbox.perceive(&ctx).await;
        assert_eq!(intent.intent_type, IntentType::TrendFollow);
        assert!((intent.confidence - 0.8).abs() < 1e-10);
    }

    #[tokio::test]
    async fn test_sandbox_feedback_panic_survives() {
        let mut sandbox = AgentSandbox::new(Box::new(FeedbackPanicAgent {
            id: "fb-panic".to_string(),
        }));
        let feedback = AgentFeedback {
            intent_id: UUID4::new(),
            success: true,
            fill_price: Some(100.0),
            fill_quantity: Some(1.0),
            slippage_bps: Some(0.0),
            error: None,
        };
        // Should not panic
        sandbox.on_feedback(&feedback).await;
    }

    #[tokio::test]
    async fn test_sandbox_preserves_id() {
        let sandbox = AgentSandbox::new(Box::new(NormalAgent));
        assert_eq!(sandbox.id(), "normal-agent");
    }
}

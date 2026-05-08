//! Strategy Lock — human-in-the-loop governance for autoresearch.
//!
//! When a strategy is locked, the Karpathy Ratchet still evaluates hypotheses
//! but stages them for human approval instead of applying them directly.
//! Urgent improvements (>200% IR gain) trigger alerts.

use tracing::{info, warn};

use crate::runtime::StrategyHypothesis;

/// A pending mutation that awaits human approval.
#[derive(Clone, Debug)]
pub struct PendingMutation {
    /// The hypothesis that was evaluated.
    pub hypothesis: StrategyHypothesis,
    /// Candidate IR achieved by the mutation.
    pub candidate_ir: f64,
    /// Baseline IR at time of evaluation.
    pub baseline_ir: f64,
    /// Improvement ratio: (candidate_ir - baseline_ir) / (|baseline_ir| + epsilon).
    pub improvement_ratio: f64,
    /// Whether this triggers an urgent alert (improvement > 200%).
    pub is_urgent: bool,
    /// Timestamp when staged (Unix nanos).
    pub staged_at_ns: u64,
}

/// Strategy lock — gates mutation application behind human approval.
///
/// When locked:
/// - Hypotheses are still evaluated (ratchet keeps scoring)
/// - Accepted improvements are staged as `pending_mutation` instead of applied
/// - Urgent improvements (>200% gain) trigger alerts via `urgent_alert`
///
/// When unlocked:
/// - Normal ratchet behavior: accepted improvements are applied immediately
#[derive(Clone, Debug)]
pub struct StrategyLock {
    /// Whether the strategy is locked against autonomous mutations.
    pub locked: bool,
    /// Currently pending mutation (at most one at a time).
    pub pending_mutation: Option<PendingMutation>,
    /// Whether an urgent alert has been triggered and not yet acknowledged.
    pub urgent_alert: bool,
    /// Improvement ratio threshold for urgent alerts (default: 2.0 = 200%).
    pub urgent_threshold: f64,
}

impl StrategyLock {
    pub fn new() -> Self {
        Self {
            locked: false,
            pending_mutation: None,
            urgent_alert: false,
            urgent_threshold: 2.0,
        }
    }

    /// Create a locked strategy lock.
    pub fn locked() -> Self {
        Self {
            locked: true,
            ..Self::new()
        }
    }

    /// Check if a mutation can be applied directly (i.e., not locked).
    pub fn can_apply(&self) -> bool {
        !self.locked
    }

    /// Stage a mutation for human approval instead of applying it.
    ///
    /// Returns `true` if an urgent alert was triggered.
    pub fn stage_mutation(
        &mut self,
        hypothesis: StrategyHypothesis,
        candidate_ir: f64,
        baseline_ir: f64,
    ) -> bool {
        let epsilon = 1e-10;
        let improvement_ratio = if baseline_ir.abs() < epsilon {
            if candidate_ir > 0.0 { f64::INFINITY } else { 0.0 }
        } else {
            (candidate_ir - baseline_ir) / (baseline_ir.abs() + epsilon)
        };

        let is_urgent = improvement_ratio > self.urgent_threshold;

        if is_urgent {
            self.urgent_alert = true;
            warn!(
                "URGENT ALERT: {} improvement ratio {:.1}% exceeds {:.0}% threshold — \
                 requires human approval",
                hypothesis.id,
                improvement_ratio * 100.0,
                self.urgent_threshold * 100.0,
            );
        } else {
            info!(
                "Staged mutation {} for approval (IR {:.4} → {:.4}, +{:.1}%)",
                hypothesis.id,
                baseline_ir,
                candidate_ir,
                improvement_ratio * 100.0,
            );
        }

        self.pending_mutation = Some(PendingMutation {
            hypothesis,
            candidate_ir,
            baseline_ir,
            improvement_ratio,
            is_urgent,
            staged_at_ns: 0, // Would use real clock
        });

        is_urgent
    }

    /// Approve the pending mutation — caller should apply it to the ratchet baseline.
    /// Returns the pending mutation if one exists.
    pub fn approve(&mut self) -> Option<PendingMutation> {
        self.urgent_alert = false;
        let mutation = self.pending_mutation.take();
        if let Some(ref m) = mutation {
            info!("Mutation approved: {}", m.hypothesis.id);
        }
        mutation
    }

    /// Reject the pending mutation — discard it.
    pub fn reject(&mut self) {
        if let Some(m) = self.pending_mutation.take() {
            info!("Mutation rejected: {}", m.hypothesis.id);
        }
        self.urgent_alert = false;
    }

    /// Acknowledge the urgent alert without approving/rejecting the mutation.
    pub fn acknowledge_alert(&mut self) {
        self.urgent_alert = false;
        info!("Urgent alert acknowledged");
    }

    /// Toggle lock state. Returns the new state.
    pub fn toggle(&mut self) -> bool {
        self.locked = !self.locked;
        info!("Strategy lock: {}", if self.locked { "LOCKED" } else { "UNLOCKED" });
        self.locked
    }
}

impl Default for StrategyLock {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nautilus_core::UUID4;
    use crate::runtime::{StrategyHypothesis, StrategyType};

    fn make_hypo(description: &str) -> StrategyHypothesis {
        StrategyHypothesis {
            id: UUID4::new(),
            strategy_type: StrategyType::Momentum,
            description: description.to_string(),
            code_patch: String::new(),
            parent_id: None,
        }
    }

    #[test]
    fn test_new_lock_is_unlocked() {
        let lock = StrategyLock::new();
        assert!(!lock.locked);
        assert!(lock.can_apply());
        assert!(lock.pending_mutation.is_none());
        assert!(!lock.urgent_alert);
    }

    #[test]
    fn test_locked_factory() {
        let lock = StrategyLock::locked();
        assert!(lock.locked);
        assert!(!lock.can_apply());
    }

    #[test]
    fn test_toggle() {
        let mut lock = StrategyLock::new();
        assert!(lock.toggle()); // now locked
        assert!(!lock.can_apply());
        assert!(!lock.toggle()); // now unlocked
        assert!(lock.can_apply());
    }

    #[test]
    fn test_stage_normal_mutation() {
        let mut lock = StrategyLock::locked();
        let urgent = lock.stage_mutation(make_hypo("window=10"), 1.5, 1.0);
        assert!(!urgent); // 50% improvement, below 200%
        assert!(lock.pending_mutation.is_some());
        assert!(!lock.urgent_alert);

        let m = lock.pending_mutation.as_ref().unwrap();
        assert!((m.improvement_ratio - 0.5).abs() < 0.01);
        assert!(!m.is_urgent);
    }

    #[test]
    fn test_stage_urgent_mutation() {
        let mut lock = StrategyLock::locked();
        // IR goes from 1.0 to 4.0 → 300% improvement > 200% threshold
        let urgent = lock.stage_mutation(make_hypo("breakout"), 4.0, 1.0);
        assert!(urgent);
        assert!(lock.urgent_alert);

        let m = lock.pending_mutation.as_ref().unwrap();
        assert!(m.is_urgent);
        assert!((m.improvement_ratio - 3.0).abs() < 0.01);
    }

    #[test]
    fn test_stage_urgent_zero_baseline() {
        let mut lock = StrategyLock::locked();
        // Baseline=0, candidate>0 → infinite improvement → urgent
        let urgent = lock.stage_mutation(make_hypo("vol_target"), 0.5, 0.0);
        assert!(urgent);
        assert!(lock.urgent_alert);
    }

    #[test]
    fn test_approve() {
        let mut lock = StrategyLock::locked();
        lock.stage_mutation(make_hypo("window=10"), 1.5, 1.0);

        let m = lock.approve();
        assert!(m.is_some());
        assert_eq!(m.unwrap().hypothesis.description, "window=10");
        assert!(lock.pending_mutation.is_none());
        assert!(!lock.urgent_alert);
    }

    #[test]
    fn test_reject() {
        let mut lock = StrategyLock::locked();
        lock.stage_mutation(make_hypo("window=10"), 1.5, 1.0);

        lock.reject();
        assert!(lock.pending_mutation.is_none());
        assert!(!lock.urgent_alert);
    }

    #[test]
    fn test_acknowledge_alert() {
        let mut lock = StrategyLock::locked();
        lock.stage_mutation(make_hypo("breakout"), 4.0, 1.0);
        assert!(lock.urgent_alert);

        lock.acknowledge_alert();
        assert!(!lock.urgent_alert);
        // Mutation still pending
        assert!(lock.pending_mutation.is_some());
    }

    #[test]
    fn test_stage_replaces_previous() {
        let mut lock = StrategyLock::locked();
        lock.stage_mutation(make_hypo("first"), 1.5, 1.0);
        lock.stage_mutation(make_hypo("second"), 2.0, 1.0);

        let m = lock.pending_mutation.as_ref().unwrap();
        assert_eq!(m.hypothesis.description, "second");
    }

    #[test]
    fn test_default_is_unlocked() {
        let lock = StrategyLock::default();
        assert!(!lock.locked);
    }
}

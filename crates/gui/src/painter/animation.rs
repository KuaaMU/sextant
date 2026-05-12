#![allow(dead_code)]

//! Animation system — tween interpolation, opacity pulses, slide animations.
//!
//! All animations use wall-clock time. `value()` returns the interpolated
//! result at the current moment. `is_finished()` checks if the animation
//! is complete.

use std::time::{Duration, Instant};

/// Easing functions for animation curves.
#[derive(Clone, Copy, Debug)]
pub enum Easing {
    Linear,
    /// Cubic ease-out: fast start, slow end
    EaseOut,
    /// Spring overshoot: slight bounce at the end
    Spring,
    /// Sine ease-in-out: smooth acceleration/deceleration
    EaseInOut,
}

impl Easing {
    pub fn apply(self, t: f32) -> f32 {
        match self {
            Self::Linear => t,
            Self::EaseOut => {
                // cubic-bezier(0.16, 1, 0.3, 1)
                let t1 = 1.0 - t;
                1.0 - t1 * t1 * t1
            }
            Self::Spring => {
                // cubic-bezier(0.34, 1.56, 0.64, 1)
                let t1 = t - 1.0;
                1.0 + t1 * t1 * (2.5 * t1 + 1.5)
            }
            Self::EaseInOut => {
                // sine ease-in-out
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    let t1 = t - 1.0;
                    1.0 - 2.0 * t1 * t1
                }
            }
        }
    }
}

/// A one-shot animation from `from` to `to` over `duration`.
#[derive(Clone, Debug)]
pub struct Animation {
    pub start_time: Instant,
    pub duration: Duration,
    pub easing: Easing,
    pub from: f32,
    pub to: f32,
}

impl Animation {
    pub fn new(duration: Duration, easing: Easing, from: f32, to: f32) -> Self {
        Self {
            start_time: Instant::now(),
            duration,
            easing,
            from,
            to,
        }
    }

    /// Current interpolated value.
    pub fn value(&self, now: Instant) -> f32 {
        let elapsed = now.duration_since(self.start_time).as_secs_f32();
        let t = (elapsed / self.duration.as_secs_f32()).clamp(0.0, 1.0);
        self.from + (self.to - self.from) * self.easing.apply(t)
    }

    /// Progress 0.0 → 1.0.
    pub fn progress(&self, now: Instant) -> f32 {
        let elapsed = now.duration_since(self.start_time).as_secs_f32();
        (elapsed / self.duration.as_secs_f32()).clamp(0.0, 1.0)
    }

    pub fn is_finished(&self, now: Instant) -> bool {
        now.duration_since(self.start_time) >= self.duration
    }
}

/// Continuous pulse animation (sine wave between min and max).
#[derive(Clone, Debug)]
pub struct Pulse {
    pub period: Duration,
    pub min: f32,
    pub max: f32,
    pub start_time: Instant,
}

impl Pulse {
    pub fn new(period: Duration, min: f32, max: f32) -> Self {
        Self {
            period,
            min,
            max,
            start_time: Instant::now(),
        }
    }

    /// Current value, oscillates between min and max.
    pub fn value(&self, now: Instant) -> f32 {
        let elapsed = now.duration_since(self.start_time).as_secs_f32();
        let period = self.period.as_secs_f32();
        let t = (elapsed % period) / period;
        let sine = (t * std::f32::consts::TAU).sin(); // -1 to 1
        let normalized = (sine + 1.0) / 2.0; // 0 to 1
        self.min + (self.max - self.min) * normalized
    }
}

/// Slide-in animation state for intent cards.
#[derive(Clone, Debug)]
pub struct SlideIn {
    pub animation: Animation,
    pub width: f32,
}

impl SlideIn {
    pub fn new(width: f32) -> Self {
        Self {
            animation: Animation::new(
                Duration::from_millis(300),
                Easing::EaseOut,
                width,  // from: off-screen right
                0.0,    // to: final position
            ),
            width,
        }
    }

    /// Current X offset (positive = still sliding in).
    pub fn offset_x(&self, now: Instant) -> f32 {
        self.animation.value(now)
    }

    pub fn is_finished(&self, now: Instant) -> bool {
        self.animation.is_finished(now)
    }
}

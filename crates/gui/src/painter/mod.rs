//! Custom Painter primitives for the nautical dashboard.
//!
//! egui's built-in widgets can't express gauges, heat maps, or flowing lines.
//! These modules draw directly via `egui::Painter`.

pub mod animation;
pub mod shapes;

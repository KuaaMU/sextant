//! Perception layer: LLM inference and three-layer routing.
//!
//! Layer 1: Rule-based (instant, <1μs) — pattern matching on ContextWindow
//! Layer 2: Small LLM (fast, ~50ms) — Qwen3-8B for tactical decisions
//! Layer 3: Large LLM (deep, ~500ms) — Qwen3-27B for strategic analysis

pub mod config;
pub mod llm;
pub mod remote;
pub mod router;

pub use config::{LlmConfigFile, ModelConfig, ProviderConfig, RoleMapping};
pub use llm::{LlmAction, LlmBackend, LlmConfig, LlmDecision, MockLlm, PromptTemplate};
pub use remote::{RemoteLlm, RemoteLlmConfig};
pub use router::{PerceptionDecision, PerceptionRouter, RouterConfig, RoutingLayer};

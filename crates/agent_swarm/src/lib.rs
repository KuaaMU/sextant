//! Agent swarm for Sextant.
//!
//! Defines the Agent trait, Intent types, IntentCompiler, and SwarmCoordinator
//! for multi-agent trading with AI-native decision making.

pub mod agent;
pub mod compiler;
pub mod intent;
pub mod perception;
pub mod strategy_wrapper;
pub mod swarm;

// Re-export core types
pub use agent::{Agent, AgentFeedback};
pub use compiler::IntentCompiler;
pub use intent::{AgentIntent, ExecutionDirective, ExecutionStyle, IntentType, OrderSide, OrderSpecification, PositionTarget, RiskBudget};
pub use perception::{LlmAction, LlmBackend, LlmConfig, LlmConfigFile, LlmDecision, ModelConfig, MockLlm, PerceptionDecision, PerceptionRouter, PromptTemplate, ProviderConfig, RemoteLlm, RemoteLlmConfig, RoleMapping, RouterConfig, RoutingLayer};
pub use strategy_wrapper::SwarmStrategy;
pub use swarm::{ConsensusStrategy, SwarmCoordinator};

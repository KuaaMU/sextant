//! LLM configuration system.
//!
//! TOML-based config with environment variable interpolation for secrets.
//! Supports role-based model assignment and fallback chains.
//!
//! # Design (inspired by LiteLLM, Continue.dev, Aider)
//!
//! - Provider prefix in model string: `openai/gpt-4o`, `qwen/qwen3-8b`
//! - `${ENV_VAR}` interpolation in api_key and api_base fields
//! - Role-based routing: different models for perception vs strategy vs risk
//! - Fallback chains: if primary fails, try next in list

use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

use super::remote::{RemoteLlm, RemoteLlmConfig};

// ── Config types ────────────────────────────────────────────────────

/// Top-level LLM configuration file.
#[derive(Debug, Clone, Deserialize)]
pub struct LlmConfigFile {
    /// Named provider credentials (reusable across models).
    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,
    /// Model definitions.
    #[serde(default)]
    pub models: HashMap<String, ModelConfig>,
    /// Role → model name mapping.
    #[serde(default)]
    pub roles: RoleMapping,
}

/// A provider's connection details.
#[derive(Debug, Clone, Deserialize)]
pub struct ProviderConfig {
    /// API base URL. Supports `${ENV_VAR}` interpolation.
    pub api_base: String,
    /// API key. Supports `${ENV_VAR}` interpolation.
    pub api_key: String,
    /// Optional default model prefix for this provider.
    #[serde(default)]
    pub default_model: Option<String>,
    /// Optional request timeout in seconds.
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

/// A specific model configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct ModelConfig {
    /// Provider name (references a key in `providers`).
    pub provider: String,
    /// Full model ID (e.g., "qwen/qwen3-8b", "gpt-4o").
    pub model: String,
    /// Sampling temperature.
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    /// Max tokens to generate.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    /// Optional system prompt override.
    #[serde(default)]
    pub system_prompt: Option<String>,
}

/// Role-based model assignment.
///
/// Maps agent roles to model names defined in `models`.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct RoleMapping {
    /// Model for Layer 2 perception (fast tactical decisions).
    #[serde(default = "default_perception_model")]
    pub perception: String,
    /// Model for Layer 3 strategic analysis.
    #[serde(default = "default_strategy_model")]
    pub strategy: String,
    /// Model for risk assessment.
    #[serde(default)]
    pub risk: Option<String>,
    /// Fallback model if primary fails.
    #[serde(default)]
    pub fallback: Option<String>,
}

fn default_timeout() -> u64 {
    30
}
fn default_temperature() -> f32 {
    0.7
}
fn default_max_tokens() -> u32 {
    512
}
fn default_perception_model() -> String {
    "small".to_string()
}
fn default_strategy_model() -> String {
    "large".to_string()
}

// ── Config loading ──────────────────────────────────────────────────

impl LlmConfigFile {
    /// Load from a TOML file.
    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())?;
        Self::from_str(&content)
    }

    /// Parse from a TOML string.
    pub fn from_str(content: &str) -> anyhow::Result<Self> {
        let config: Self = toml::from_str(content)?;
        config.validate()?;
        Ok(config)
    }

    /// Validate that all model→provider references resolve.
    fn validate(&self) -> anyhow::Result<()> {
        for (name, model) in &self.models {
            if !self.providers.contains_key(&model.provider) {
                return Err(anyhow::anyhow!(
                    "Model '{}' references unknown provider '{}'",
                    name,
                    model.provider
                ));
            }
        }
        // Validate role references
        let check_role = |field: &str, value: &str| -> anyhow::Result<()> {
            if !value.is_empty() && !self.models.contains_key(value) {
                return Err(anyhow::anyhow!(
                    "Role '{}' references unknown model '{}'",
                    field,
                    value
                ));
            }
            Ok(())
        };
        check_role("perception", &self.roles.perception)?;
        check_role("strategy", &self.roles.strategy)?;
        if let Some(ref risk) = self.roles.risk {
            check_role("risk", risk)?;
        }
        if let Some(ref fb) = self.roles.fallback {
            check_role("fallback", fb)?;
        }
        Ok(())
    }

    /// Resolve a model name into a `RemoteLlm` instance.
    pub fn build_llm(&self, model_name: &str) -> anyhow::Result<RemoteLlm> {
        let model = self
            .models
            .get(model_name)
            .ok_or_else(|| anyhow::anyhow!("Unknown model: '{}'", model_name))?;
        let provider = self
            .providers
            .get(&model.provider)
            .ok_or_else(|| anyhow::anyhow!("Unknown provider: '{}'", model.provider))?;

        let config = RemoteLlmConfig {
            api_base: interpolate_env(&provider.api_base),
            api_key: interpolate_env(&provider.api_key),
            model: model.model.clone(),
            temperature: model.temperature,
            max_tokens: model.max_tokens,
            timeout_secs: provider.timeout_secs,
        };

        Ok(RemoteLlm::new(config))
    }

    /// Build the LLM for a specific role.
    pub fn build_role_llm(&self, role: &str) -> anyhow::Result<RemoteLlm> {
        let model_name = match role {
            "perception" => &self.roles.perception,
            "strategy" => &self.roles.strategy,
            "risk" => self.roles.risk.as_deref().unwrap_or(&self.roles.strategy),
            "fallback" => self.roles.fallback.as_deref().unwrap_or(&self.roles.perception),
            _ => return Err(anyhow::anyhow!("Unknown role: '{}'", role)),
        };
        self.build_llm(model_name)
    }
}

/// Resolve `${VAR}` references in a string with environment variable values.
fn interpolate_env(s: &str) -> String {
    let mut result = s.to_string();
    while let Some(start) = result.find("${") {
        if let Some(end) = result[start..].find('}') {
            let var_name = &result[start + 2..start + end];
            let value = std::env::var(var_name).unwrap_or_default();
            result = format!("{}{}{}", &result[..start], value, &result[start + end + 1..]);
        } else {
            break;
        }
    }
    result
}

// ── Preset configs ──────────────────────────────────────────────────

impl LlmConfigFile {
    /// OpenRouter preset with Qwen3 models.
    pub fn openrouter_preset(api_key: &str) -> Self {
        let mut providers = HashMap::new();
        providers.insert(
            "openrouter".to_string(),
            ProviderConfig {
                api_base: "https://openrouter.ai/api/v1".to_string(),
                api_key: api_key.to_string(),
                default_model: Some("qwen/qwen3-8b".to_string()),
                timeout_secs: 30,
            },
        );

        let mut models = HashMap::new();
        models.insert(
            "small".to_string(),
            ModelConfig {
                provider: "openrouter".to_string(),
                model: "qwen/qwen3-8b".to_string(),
                temperature: 0.7,
                max_tokens: 512,
                system_prompt: None,
            },
        );
        models.insert(
            "large".to_string(),
            ModelConfig {
                provider: "openrouter".to_string(),
                model: "qwen/qwen3-27b".to_string(),
                temperature: 0.5,
                max_tokens: 1024,
                system_prompt: None,
            },
        );

        Self {
            providers,
            models,
            roles: RoleMapping {
                perception: "small".to_string(),
                strategy: "large".to_string(),
                risk: None,
                fallback: Some("small".to_string()),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_toml_config() {
        let toml = r#"
[providers.openrouter]
api_base = "https://openrouter.ai/api/v1"
api_key = "sk-test"

[models.small]
provider = "openrouter"
model = "qwen/qwen3-8b"
temperature = 0.7
max_tokens = 512

[models.large]
provider = "openrouter"
model = "qwen/qwen3-27b"
temperature = 0.5
max_tokens = 1024

[roles]
perception = "small"
strategy = "large"
"#;
        let config = LlmConfigFile::from_str(toml).unwrap();
        assert_eq!(config.models.len(), 2);
        assert_eq!(config.roles.perception, "small");
        assert_eq!(config.roles.strategy, "large");
    }

    #[test]
    fn test_env_interpolation() {
        std::env::set_var("TEST_API_KEY", "sk-interpolated");
        let result = interpolate_env("Bearer ${TEST_API_KEY}");
        assert_eq!(result, "Bearer sk-interpolated");
        std::env::remove_var("TEST_API_KEY");
    }

    #[test]
    fn test_validate_unknown_provider() {
        let toml = r#"
[models.bad]
provider = "nonexistent"
model = "test"
"#;
        let result = LlmConfigFile::from_str(toml);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("unknown provider"));
    }

    #[test]
    fn test_build_llm() {
        let config = LlmConfigFile::openrouter_preset("sk-test");
        let llm = config.build_llm("small").unwrap();
        assert_eq!(llm.chat_url(), "https://openrouter.ai/api/v1/chat/completions");
    }

    #[test]
    fn test_build_role_llm() {
        let config = LlmConfigFile::openrouter_preset("sk-test");
        let _perception = config.build_role_llm("perception").unwrap();
        let _strategy = config.build_role_llm("strategy").unwrap();
        let _fallback = config.build_role_llm("fallback").unwrap();
    }

    #[test]
    fn test_openrouter_preset() {
        let config = LlmConfigFile::openrouter_preset("sk-test");
        assert_eq!(config.providers.len(), 1);
        assert_eq!(config.models.len(), 2);
        assert_eq!(config.roles.perception, "small");
    }
}

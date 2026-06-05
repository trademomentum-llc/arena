use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ffi::NulError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AgentError {
    #[error("API error: {0}")]
    Api(String),
    #[error("Timeout after {0}ms")]
    Timeout(u64),
    #[error("Rate limited, retry after {0}s")]
    RateLimited(u64),
    #[error("Authentication error: {0}")]
    Auth(String),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Nul error: {0}")]
    Nul(#[from] NulError),
}

/// Configuration for an agent adapter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterConfig {
    /// Backend type identifier
    pub backend: String,
    /// Model identifier
    pub model: String,
    /// API key (if applicable)
    pub api_key: Option<String>,
    /// API base URL (override default)
    pub base_url: Option<String>,
    /// Request timeout in milliseconds
    pub timeout_ms: u64,
    /// Maximum retries on failure
    pub max_retries: u32,
    /// Extra backend-specific config
    pub extra: HashMap<String, String>,
}

impl Default for AdapterConfig {
    fn default() -> Self {
        AdapterConfig {
            backend: "openai".to_string(),
            model: "gpt-4-turbo".to_string(),
            api_key: None,
            base_url: None,
            timeout_ms: 30000,
            max_retries: 3,
            extra: HashMap::new(),
        }
    }
}

/// Request sent to an agent
#[derive(Debug, Clone)]
pub struct AgentRequest {
    /// System prompt
    pub system: String,
    /// User message / task
    pub prompt: String,
    /// Optional context (previous messages, code context, etc.)
    pub context: Option<Vec<Message>>,
    /// Expected output format
    pub output_format: OutputFormat,
    /// Temperature (0.0 for deterministic, higher for creative)
    pub temperature: f64,
    /// Maximum tokens in response
    pub max_tokens: u32,
}

#[derive(Debug, Clone)]
pub enum OutputFormat {
    Text,
    Json,
    Code(String), // Language identifier
}

#[derive(Debug, Clone)]
pub struct Message {
    pub role: MessageRole,
    pub content: String,
}

#[derive(Debug, Clone)]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

/// Response from an agent
#[derive(Debug, Clone)]
pub struct AgentOutput {
    pub content: String,
    pub structured: Option<serde_json::Value>,
    pub usage: UsageStats,
}

#[derive(Debug, Clone)]
pub struct UsageStats {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    pub cost_cents: f64,
    pub latency_ms: u64,
}

/// Unified agent adapter interface
#[async_trait]
pub trait AgentAdapter: Send + Sync + 'static {
    /// Backend identifier
    fn backend_name(&self) -> &str;

    /// Model being used
    fn model_name(&self) -> &str;

    /// Clone self into a boxed trait object
    fn box_clone(&self) -> Box<dyn AgentAdapter>;

    /// Execute a request to the agent
    async fn request(&self, req: &AgentRequest) -> Result<AgentOutput, AgentError>;

    /// Check if the adapter is healthy
    async fn health_check(&self) -> bool;

    /// Get estimated cost for a request (in cents)
    fn estimate_cost(&self, prompt_tokens: u32, completion_tokens: u32) -> f64;
}

/// Registry of available agent adapters
pub struct AgentRegistry {
    adapters: HashMap<String, Box<dyn AgentAdapter>>,
}

impl Clone for AgentRegistry {
    fn clone(&self) -> Self {
        let mut adapters = HashMap::new();
        for (name, adapter) in &self.adapters {
            adapters.insert(name.clone(), adapter.box_clone());
        }
        AgentRegistry { adapters }
    }
}

impl AgentRegistry {
    pub fn new() -> Self {
        AgentRegistry {
            adapters: HashMap::new(),
        }
    }

    pub fn register(&mut self, name: &str, adapter: Box<dyn AgentAdapter>) {
        self.adapters.insert(name.to_string(), adapter);
    }

    pub fn get(&self, name: &str) -> Option<&dyn AgentAdapter> {
        self.adapters.get(name).map(|a| a.as_ref())
    }

    pub fn list(&self) -> Vec<&str> {
        self.adapters.keys().map(|k| k.as_str()).collect()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.adapters.contains_key(name)
    }
}

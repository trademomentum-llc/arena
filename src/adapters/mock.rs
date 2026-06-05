use super::adapter::*;
use async_trait::async_trait;
use std::time::Instant;

/// Mock adapter for demonstration and testing
pub struct MockAdapter {
    name: String,
    response: String,
}

impl MockAdapter {
    pub fn new(name: String, response: Option<String>) -> Self {
        MockAdapter {
            name: name.clone(),
            response: response.unwrap_or_else(|| {
                format!("This is a mock response from the {} agent.", name)
            }),
        }
    }

    pub fn with_config(name: String, response: String) -> Self {
        MockAdapter { name, response }
    }
}

#[async_trait]
impl AgentAdapter for MockAdapter {
    fn backend_name(&self) -> &str {
        "mock"
    }

    fn model_name(&self) -> &str {
        &self.name
    }

    fn box_clone(&self) -> Box<dyn AgentAdapter> {
        Box::new(MockAdapter {
            name: self.name.clone(),
            response: self.response.clone(),
        })
    }

    async fn request(&self, req: &AgentRequest) -> Result<AgentOutput, AgentError> {
        let start = Instant::now();

        Ok(AgentOutput {
            content: self.response.clone(),
            structured: if matches!(req.output_format, OutputFormat::Json) {
                serde_json::from_str(&self.response).ok()
            } else {
                None
            },
            usage: UsageStats {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
                cost_cents: 0.0,
                latency_ms: start.elapsed().as_millis() as u64,
            },
        })
    }

    async fn health_check(&self) -> bool {
        true
    }

    fn estimate_cost(&self, _prompt_tokens: u32, _completion_tokens: u32) -> f64 {
        0.0
    }
}
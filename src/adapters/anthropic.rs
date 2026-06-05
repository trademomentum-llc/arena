use super::adapter::*;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use std::time::Instant;

/// Anthropic adapter (Claude 3 Opus, Sonnet, Haiku, etc.)
pub struct AnthropicAdapter {
    client: Client,
    api_key: String,
    model: String,
    base_url: String,
    timeout_ms: u64,
    max_retries: u32,
    /// Pricing per 1K tokens
    prompt_cost_per_1k: f64,
    completion_cost_per_1k: f64,
    /// Anthropic uses a different header for API version
    api_version: String,
}

impl AnthropicAdapter {
    pub fn new(api_key: String, model: String) -> Self {
        let (prompt_cost_per_1k, completion_cost_per_1k) = match model.as_str() {
            "claude-3-opus-20240229" => (0.015, 0.075),
            "claude-3-sonnet-20240229" => (0.003, 0.015),
            "claude-3-haiku-20240307" => (0.00025, 0.00125),
            "claude-3-5-sonnet-20241022" => (0.003, 0.015),
            "claude-3-5-haiku-20241022" => (0.001, 0.005),
            _ => (0.003, 0.015), // Default to Sonnet pricing
        };

        AnthropicAdapter {
            client: Client::new(),
            api_key,
            model,
            base_url: "https://api.anthropic.com".to_string(),
            timeout_ms: 30000,
            max_retries: 3,
            prompt_cost_per_1k,
            completion_cost_per_1k,
            api_version: "2023-06-01".to_string(),
        }
    }

    pub fn with_config(
        api_key: String,
        model: String,
        base_url: Option<String>,
        timeout_ms: u64,
        max_retries: u32,
    ) -> Self {
        let (prompt_cost_per_1k, completion_cost_per_1k) = match model.as_str() {
            "claude-3-opus-20240229" => (0.015, 0.075),
            "claude-3-sonnet-20240229" => (0.003, 0.015),
            "claude-3-haiku-20240307" => (0.00025, 0.00125),
            "claude-3-5-sonnet-20241022" => (0.003, 0.015),
            "claude-3-5-haiku-20241022" => (0.001, 0.005),
            _ => (0.003, 0.015),
        };

        AnthropicAdapter {
            client: Client::new(),
            api_key,
            model,
            base_url: base_url.unwrap_or_else(|| "https://api.anthropic.com".to_string()),
            timeout_ms,
            max_retries,
            prompt_cost_per_1k,
            completion_cost_per_1k,
            api_version: "2023-06-01".to_string(),
        }
    }

    async fn make_request(
        &self,
        req: &AgentRequest,
    ) -> Result<AgentOutput, AgentError> {
        // Anthropic uses a different message format
        let mut messages = Vec::new();

        // Context messages
        if let Some(ref ctx) = req.context {
            for msg in ctx {
                let role = match msg.role {
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                    MessageRole::System => {
                        // Anthropic doesn't support system messages in the messages array
                        // They go in the top-level system field
                        continue;
                    }
                };
                messages.push(json!({
                    "role": role,
                    "content": msg.content
                }));
            }
        }

        // User prompt
        messages.push(json!({
            "role": "user",
            "content": req.prompt
        }));

        let body = json!({
            "model": self.model,
            "messages": messages,
            "system": req.system,
            "temperature": req.temperature,
            "max_tokens": req.max_tokens,
        });

        let response = self
            .client
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", &self.api_version)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| AgentError::Api(format!("HTTP request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AgentError::Api(format!(
                "API error ({}): {}",
                status, body
            )));
        }

        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| AgentError::Api(format!("JSON parse failed: {}", e)))?;

        // Parse response
        let content = json["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let usage = &json["usage"];
        let prompt_tokens = usage["input_tokens"].as_u64().unwrap_or(0) as u32;
        let completion_tokens = usage["output_tokens"].as_u64().unwrap_or(0) as u32;

        let structured = if matches!(req.output_format, OutputFormat::Json) {
            serde_json::from_str(&content).ok()
        } else {
            None
        };

        Ok(AgentOutput {
            content,
            structured,
            usage: UsageStats {
                prompt_tokens,
                completion_tokens,
                total_tokens: prompt_tokens + completion_tokens,
                cost_cents: self.estimate_cost(prompt_tokens, completion_tokens),
                latency_ms: 0,
            },
        })
    }
}

#[async_trait]
impl AgentAdapter for AnthropicAdapter {
    fn backend_name(&self) -> &str {
        "anthropic"
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn box_clone(&self) -> Box<dyn AgentAdapter> {
        Box::new(AnthropicAdapter {
            client: Client::new(),
            api_key: self.api_key.clone(),
            model: self.model.clone(),
            base_url: self.base_url.clone(),
            timeout_ms: self.timeout_ms,
            max_retries: self.max_retries,
            prompt_cost_per_1k: self.prompt_cost_per_1k,
            completion_cost_per_1k: self.completion_cost_per_1k,
            api_version: self.api_version.clone(),
        })
    }

    async fn request(&self, req: &AgentRequest) -> Result<AgentOutput, AgentError> {
        let start = Instant::now();
        let mut output = self.make_request(req).await?;
        output.usage.latency_ms = start.elapsed().as_millis() as u64;
        Ok(output)
    }

    async fn health_check(&self) -> bool {
        // Anthropic doesn't have a dedicated health endpoint
        // Just check if we can reach the API
        self.client
            .get(format!("{}/v1/models", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", &self.api_version)
            .send()
            .await
            .map(|r| r.status().is_success() || r.status() == 404) // 404 is OK, means API is reachable
            .unwrap_or(false)
    }

    fn estimate_cost(&self, prompt_tokens: u32, completion_tokens: u32) -> f64 {
        let prompt_cost = (prompt_tokens as f64 / 1000.0) * self.prompt_cost_per_1k;
        let completion_cost = (completion_tokens as f64 / 1000.0) * self.completion_cost_per_1k;
        prompt_cost + completion_cost
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anthropic_opus_pricing() {
        let adapter = AnthropicAdapter::new(
            "test-key".to_string(),
            "claude-3-opus-20240229".to_string(),
        );
        assert_eq!(adapter.prompt_cost_per_1k, 0.015);
        assert_eq!(adapter.completion_cost_per_1k, 0.075);
    }

    #[test]
    fn test_anthropic_sonnet_pricing() {
        let adapter = AnthropicAdapter::new(
            "test-key".to_string(),
            "claude-3-sonnet-20240229".to_string(),
        );
        assert_eq!(adapter.prompt_cost_per_1k, 0.003);
        assert_eq!(adapter.completion_cost_per_1k, 0.015);
    }

    #[test]
    fn test_anthropic_haiku_pricing() {
        let adapter = AnthropicAdapter::new(
            "test-key".to_string(),
            "claude-3-haiku-20240307".to_string(),
        );
        assert_eq!(adapter.prompt_cost_per_1k, 0.00025);
        assert_eq!(adapter.completion_cost_per_1k, 0.00125);
    }

    #[test]
    fn test_anthropic_cost_estimate() {
        let adapter =
            AnthropicAdapter::new("test-key".to_string(), "claude-3-sonnet-20240229".to_string());
        // 1000 prompt tokens + 500 completion tokens
        let cost = adapter.estimate_cost(1000, 500);
        // 0.003 + (0.5 * 0.015) = 0.0105
        assert!((cost - 0.0105).abs() < 0.001);
    }

    #[test]
    fn test_anthropic_backend_name() {
        let adapter = AnthropicAdapter::new("test-key".to_string(), "claude-3-opus".to_string());
        assert_eq!(adapter.backend_name(), "anthropic");
    }
}

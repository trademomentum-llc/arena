use super::adapter::*;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use std::time::Instant;

/// OpenAI adapter (GPT-4, GPT-4 Turbo, o1, etc.)
pub struct OpenAIAdapter {
    client: Client,
    api_key: String,
    model: String,
    base_url: String,
    timeout_ms: u64,
    max_retries: u32,
    /// Pricing per 1K tokens (varies by model)
    prompt_cost_per_1k: f64,
    completion_cost_per_1k: f64,
}

impl OpenAIAdapter {
    pub fn new(api_key: String, model: String) -> Self {
        let (prompt_cost_per_1k, completion_cost_per_1k) = match model.as_str() {
            "gpt-4-turbo" | "gpt-4-turbo-preview" => (0.01, 0.03),
            "gpt-4o" | "gpt-4o-2024-05-13" | "gpt-4o-2024-08-06" => (0.005, 0.015),
            "gpt-4o-mini" | "gpt-4o-mini-2024-07-18" => (0.00015, 0.0006),
            "o1" | "o1-preview" => (0.015, 0.06),
            "o1-mini" => (0.003, 0.012),
            m if m.contains("grok-3") => (0.003, 0.015),
            m if m.contains("grok-2") => (0.002, 0.010),
            _ => (0.01, 0.03),
        };

        OpenAIAdapter {
            client: Client::new(),
            api_key,
            model,
            base_url: "https://api.openai.com/v1".to_string(),
            timeout_ms: 30000,
            max_retries: 3,
            prompt_cost_per_1k,
            completion_cost_per_1k,
        }
    }

    pub fn with_config(
        api_key: String,
        model: String,
        base_url: Option<String>,
        timeout_ms: u64,
        max_retries: u32,
    ) -> Self {
        // Set pricing based on model
        let (prompt_cost_per_1k, completion_cost_per_1k) = match model.as_str() {
            "gpt-4-turbo" | "gpt-4-turbo-preview" => (0.01, 0.03),
            "gpt-4o" | "gpt-4o-2024-05-13" | "gpt-4o-2024-08-06" => (0.005, 0.015),
            "gpt-4o-mini" | "gpt-4o-mini-2024-07-18" => (0.00015, 0.0006),
            "o1" | "o1-preview" => (0.015, 0.06),
            "o1-mini" => (0.003, 0.012),
            m if m.contains("grok-3") => (0.003, 0.015),
            m if m.contains("grok-2") => (0.002, 0.010),
            _ => (0.01, 0.03), // Default to GPT-4 pricing
        };

        OpenAIAdapter {
            client: Client::new(),
            api_key,
            model,
            base_url: base_url.unwrap_or_else(|| "https://api.openai.com/v1".to_string()),
            timeout_ms,
            max_retries,
            prompt_cost_per_1k,
            completion_cost_per_1k,
        }
    }

    async fn make_request(
        &self,
        req: &AgentRequest,
    ) -> Result<AgentOutput, AgentError> {
        let mut messages = Vec::new();

        // System message
        messages.push(json!({
            "role": "system",
            "content": req.system
        }));

        // Context messages
        if let Some(ref ctx) = req.context {
            for msg in ctx {
                let role = match msg.role {
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                    MessageRole::System => "system",
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
            "temperature": req.temperature,
            "max_tokens": req.max_tokens,
        });

        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
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
        let choice = &json["choices"][0];
        let content = choice["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let usage = &json["usage"];
        let prompt_tokens = usage["prompt_tokens"].as_u64().unwrap_or(0) as u32;
        let completion_tokens = usage["completion_tokens"].as_u64().unwrap_or(0) as u32;

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
                latency_ms: 0, // Will be set by caller
            },
        })
    }
}

#[async_trait]
impl AgentAdapter for OpenAIAdapter {
    fn backend_name(&self) -> &str {
        "openai"
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn box_clone(&self) -> Box<dyn AgentAdapter> {
        Box::new(OpenAIAdapter {
            client: Client::new(),
            api_key: self.api_key.clone(),
            model: self.model.clone(),
            base_url: self.base_url.clone(),
            timeout_ms: self.timeout_ms,
            max_retries: self.max_retries,
            prompt_cost_per_1k: self.prompt_cost_per_1k,
            completion_cost_per_1k: self.completion_cost_per_1k,
        })
    }

    async fn request(&self, req: &AgentRequest) -> Result<AgentOutput, AgentError> {
        let start = Instant::now();
        let mut output = self.make_request(req).await?;
        output.usage.latency_ms = start.elapsed().as_millis() as u64;
        Ok(output)
    }

    async fn health_check(&self) -> bool {
        self.client
            .get(format!("{}/models", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
            .map(|r| r.status().is_success())
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
    fn test_openai_gpt4_pricing() {
        let adapter = OpenAIAdapter::new("test-key".to_string(), "gpt-4-turbo".to_string());
        assert_eq!(adapter.prompt_cost_per_1k, 0.01);
        assert_eq!(adapter.completion_cost_per_1k, 0.03);
    }

    #[test]
    fn test_openai_gpt4o_pricing() {
        let adapter = OpenAIAdapter::new("test-key".to_string(), "gpt-4o".to_string());
        assert_eq!(adapter.prompt_cost_per_1k, 0.005);
        assert_eq!(adapter.completion_cost_per_1k, 0.015);
    }

    #[test]
    fn test_openai_cost_estimate() {
        let adapter = OpenAIAdapter::new("test-key".to_string(), "gpt-4-turbo".to_string());
        // 1000 prompt tokens + 500 completion tokens
        let cost = adapter.estimate_cost(1000, 500);
        // 0.01 + (0.5 * 0.03) = 0.025
        assert!((cost - 0.025).abs() < 0.001);
    }

    #[test]
    fn test_openai_backend_name() {
        let adapter = OpenAIAdapter::new("test-key".to_string(), "gpt-4".to_string());
        assert_eq!(adapter.backend_name(), "openai");
    }
}

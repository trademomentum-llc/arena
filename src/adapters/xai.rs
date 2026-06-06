use super::openai::OpenAIAdapter;

/// Thin X.AI (Grok) adapter.
/// Since the xAI API is OpenAI-compatible, this is a convenience wrapper
/// around OpenAIAdapter configured for https://api.x.ai/v1 .
/// Use via `XaiAdapter::with_config(key, model, ...)` or the existing
/// OpenAIAdapter + base_url for full flexibility (synthesized from best-of-n candidates).
pub type XaiAdapter = OpenAIAdapter;

pub const XAI_BASE_URL: &str = "https://api.x.ai/v1";

/// Helper to create an XaiAdapter (pure construction).
pub fn new_xai_adapter(api_key: String, model: String) -> XaiAdapter {
    OpenAIAdapter::with_config(api_key, model, Some(XAI_BASE_URL.to_string()), 30_000, 3)
}
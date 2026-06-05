/// Agent adapter interface and implementations for different model backends.
pub mod adapter;
pub mod anthropic;
pub mod mock;
pub mod morphlex;
pub mod openai;
pub mod endpoint;

pub use adapter::*;
pub use anthropic::AnthropicAdapter;
pub use mock::MockAdapter;
pub use morphlex::MorphlexAdapter;
pub use openai::OpenAIAdapter;
pub use endpoint::validate_local_endpoint;

// Full HTTP implementation comes in step 10.
use async_trait::async_trait;

use crate::{cli::Cli, ChronicleError};

use super::LlmBackend;

#[allow(dead_code)] // fields used in step 10
pub struct ClaudeBackend {
    pub(super) model: String,
    pub(super) endpoint: String,
}

impl ClaudeBackend {
    pub fn new(config: &Cli) -> Self {
        let model = config
            .model
            .clone()
            .unwrap_or_else(|| "claude-sonnet-4-6".to_string());
        let endpoint = std::env::var("CHRONICLE_ENDPOINT")
            .unwrap_or_else(|_| "https://api.anthropic.com".to_string());
        Self { model, endpoint }
    }
}

#[async_trait]
impl LlmBackend for ClaudeBackend {
    async fn complete(&self, _prompt: &str) -> Result<String, ChronicleError> {
        Err(ChronicleError::LlmFailure(
            "ClaudeBackend not yet implemented".to_string(),
        ))
    }
}

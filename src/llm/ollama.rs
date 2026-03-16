// Full HTTP implementation comes in step 9.
use async_trait::async_trait;

use crate::{cli::Cli, ChronicleError};

use super::LlmBackend;

#[allow(dead_code)] // fields used in step 9
pub struct OllamaBackend {
    pub(super) model: String,
    pub(super) endpoint: String,
}

impl OllamaBackend {
    pub fn new(config: &Cli) -> Self {
        let model = config
            .model
            .clone()
            .unwrap_or_else(|| "qwen3.5:9b".to_string());
        let endpoint = std::env::var("CHRONICLE_ENDPOINT")
            .unwrap_or_else(|_| "http://localhost:11434".to_string());
        Self { model, endpoint }
    }
}

#[async_trait]
impl LlmBackend for OllamaBackend {
    async fn complete(&self, _prompt: &str) -> Result<String, ChronicleError> {
        Err(ChronicleError::LlmFailure(
            "OllamaBackend not yet implemented".to_string(),
        ))
    }
}

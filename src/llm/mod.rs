pub mod claude;
pub mod ollama;

use async_trait::async_trait;

use crate::{
    cli::{Backend, Cli},
    ChronicleError,
};

use self::{claude::ClaudeBackend, ollama::OllamaBackend};

#[async_trait]
pub trait LlmBackend: Send + Sync {
    async fn complete(&self, prompt: &str) -> Result<String, ChronicleError>;
}

/// Construct the appropriate backend from the parsed CLI config.
pub fn build(config: &Cli) -> Box<dyn LlmBackend> {
    match config.backend {
        Backend::Ollama => Box::new(OllamaBackend::new(config)),
        Backend::Claude => Box::new(ClaudeBackend::new(config)),
    }
}

// ── test-only mock ─────────────────────────────────────────────────────────

#[cfg(test)]
pub mod mock {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use async_trait::async_trait;

    use crate::ChronicleError;

    use super::LlmBackend;

    struct Inner {
        responses: VecDeque<Result<String, ChronicleError>>,
        call_count: usize,
    }

    /// A test double for `LlmBackend` that replays canned responses in order.
    /// Panics with a clear message if `complete()` is called more times than
    /// responses were provided.
    pub struct MockBackend {
        inner: Mutex<Inner>,
    }

    impl MockBackend {
        pub fn new(responses: Vec<Result<String, ChronicleError>>) -> Self {
            Self {
                inner: Mutex::new(Inner {
                    responses: responses.into_iter().collect(),
                    call_count: 0,
                }),
            }
        }

        pub fn call_count(&self) -> usize {
            self.inner.lock().expect("lock").call_count
        }
    }

    #[async_trait]
    impl LlmBackend for MockBackend {
        async fn complete(&self, _prompt: &str) -> Result<String, ChronicleError> {
            let mut inner = self.inner.lock().expect("lock");
            inner.call_count += 1;
            inner.responses.pop_front().unwrap_or_else(|| {
                panic!(
                    "MockBackend exhausted: complete() called {} time(s) but no more responses remain",
                    inner.call_count
                )
            })
        }
    }
}

// ── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::cli::{Backend, Cli};

    use super::mock::MockBackend;
    use super::*;

    fn cli_with_backend(backend: Backend) -> Cli {
        Cli {
            path: Some(PathBuf::from(".")),
            backend,
            model: None,
            group_size: 20,
            no_diffs: false,
            template: None,
            output: None,
            author: vec![],
            since: None,
            branch: None,
            from: None,
            to: None,
        }
    }

    // ── MockBackend ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn mock_returns_responses_in_order() {
        let mock = MockBackend::new(vec![
            Ok("first".to_string()),
            Ok("second".to_string()),
            Ok("third".to_string()),
        ]);

        assert_eq!(mock.complete("p").await.expect("1"), "first");
        assert_eq!(mock.complete("p").await.expect("2"), "second");
        assert_eq!(mock.complete("p").await.expect("3"), "third");
    }

    #[tokio::test]
    async fn mock_tracks_call_count() {
        let mock = MockBackend::new(vec![Ok("a".to_string()), Ok("b".to_string())]);
        assert_eq!(mock.call_count(), 0);
        mock.complete("p").await.ok();
        assert_eq!(mock.call_count(), 1);
        mock.complete("p").await.ok();
        assert_eq!(mock.call_count(), 2);
    }

    #[tokio::test]
    async fn mock_returns_err_responses() {
        use crate::ChronicleError;
        let mock = MockBackend::new(vec![
            Err(ChronicleError::LlmFailure("backend down".to_string())),
            Ok("recovered".to_string()),
        ]);

        assert!(mock.complete("p").await.is_err());
        assert_eq!(mock.complete("p").await.expect("ok"), "recovered");
        assert_eq!(mock.call_count(), 2);
    }

    #[tokio::test]
    #[should_panic(expected = "MockBackend exhausted")]
    async fn mock_panics_when_exhausted() {
        let mock = MockBackend::new(vec![Ok("only one".to_string())]);
        mock.complete("p").await.ok();
        mock.complete("p").await.ok(); // should panic
    }

    #[tokio::test]
    #[should_panic(expected = "MockBackend exhausted")]
    async fn mock_panics_immediately_when_empty() {
        let mock = MockBackend::new(vec![]);
        mock.complete("p").await.ok();
    }

    // ── build() ────────────────────────────────────────────────────────────

    #[test]
    fn build_ollama_does_not_panic() {
        let cli = cli_with_backend(Backend::Ollama);
        let _backend = build(&cli);
    }

    #[test]
    fn build_claude_does_not_panic() {
        let cli = cli_with_backend(Backend::Claude);
        let _backend = build(&cli);
    }
}

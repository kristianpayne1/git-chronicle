use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;

use crate::{cli::Cli, ChronicleError};

use super::LlmBackend;

pub struct ClaudeBackend {
    client: reqwest::Client,
    endpoint: String,
    model: String,
    api_key: String,
    retry_delay: Duration,
}

impl ClaudeBackend {
    pub fn new(config: &Cli) -> Self {
        let model = config.model.clone();
        let endpoint = std::env::var("CHRONICLE_ENDPOINT")
            .unwrap_or_else(|_| "https://api.anthropic.com".to_string());
        // Validated by Cli::validate() before build() is called.
        let api_key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_default();
        Self {
            client: reqwest::Client::new(),
            model,
            endpoint,
            api_key,
            retry_delay: Duration::from_secs(5),
        }
    }

    /// Test constructor: explicit endpoint + api_key, zero retry delay.
    #[cfg(test)]
    pub(super) fn for_test(endpoint: String, model: String, api_key: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            model,
            endpoint,
            api_key,
            retry_delay: Duration::ZERO,
        }
    }

    async fn send(&self, url: &str, payload: &Value) -> Result<reqwest::Response, ChronicleError> {
        self.client
            .post(url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(payload)
            .send()
            .await
            // API key is in headers, not in the error — safe to stringify
            .map_err(|e| ChronicleError::LlmFailure(format!("Claude request failed: {e}")))
    }
}

/// Interpret an HTTP response from the Anthropic Messages API.
/// Extracts `content[0].text`.  Never includes the API key in error text.
async fn parse_claude_response(resp: reqwest::Response) -> Result<String, ChronicleError> {
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(ChronicleError::LlmFailure(format!(
            "Claude API error {status}: {body}"
        )));
    }
    let json: Value = resp
        .json()
        .await
        .map_err(|e| ChronicleError::LlmFailure(format!("failed to parse Claude response: {e}")))?;
    json["content"][0]["text"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| {
            ChronicleError::LlmFailure(
                "missing content[0].text in Claude response".to_string(),
            )
        })
}

#[async_trait]
impl LlmBackend for ClaudeBackend {
    async fn complete(&self, prompt: &str) -> Result<String, ChronicleError> {
        let url = format!("{}/v1/messages", self.endpoint);
        let payload = serde_json::json!({
            "model": self.model,
            "max_tokens": 4096,
            "messages": [{ "role": "user", "content": prompt }],
        });

        let resp = self.send(&url, &payload).await?;
        let status = resp.status().as_u16();

        // 429 (rate limited) and 529 (overloaded) warrant a single retry
        if status == 429 || status == 529 {
            // Drain the body so the connection is released
            drop(resp);
            tokio::time::sleep(self.retry_delay).await;
            let retry_resp = self.send(&url, &payload).await?;
            return parse_claude_response(retry_resp).await;
        }

        parse_claude_response(resp).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAKE_KEY: &str = "sk-ant-test-secret-key-99999";

    fn backend(server_url: &str) -> ClaudeBackend {
        ClaudeBackend::for_test(
            server_url.to_string(),
            "claude-test".to_string(),
            FAKE_KEY.to_string(),
        )
    }

    // ── happy path ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn happy_path_extracts_content_text() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/messages")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"content":[{"type":"text","text":"great summary"}],"stop_reason":"end_turn"}"#,
            )
            .expect(1)
            .create_async()
            .await;

        let result = backend(&server.url()).complete("summarise").await.expect("ok");
        assert_eq!(result, "great summary");
        mock.assert_async().await;
    }

    // ── required headers ───────────────────────────────────────────────────

    #[tokio::test]
    async fn sends_required_headers() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/messages")
            .match_header("x-api-key", FAKE_KEY)
            .match_header("anthropic-version", "2023-06-01")
            .match_header(
                "content-type",
                mockito::Matcher::Regex("application/json".into()),
            )
            .with_status(200)
            .with_body(r#"{"content":[{"type":"text","text":"ok"}]}"#)
            .expect(1)
            .create_async()
            .await;

        backend(&server.url()).complete("test").await.expect("ok");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn sends_correct_request_body() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/messages")
            .match_body(mockito::Matcher::JsonString(
                r#"{
                    "model": "claude-test",
                    "max_tokens": 4096,
                    "messages": [{"role": "user", "content": "hello claude"}]
                }"#
                .to_string(),
            ))
            .with_status(200)
            .with_body(r#"{"content":[{"type":"text","text":"reply"}]}"#)
            .expect(1)
            .create_async()
            .await;

        backend(&server.url()).complete("hello claude").await.expect("ok");
        mock.assert_async().await;
    }

    // ── 429 / 529 retry ────────────────────────────────────────────────────

    #[tokio::test]
    async fn retry_on_429_then_succeeds() {
        let mut server = mockito::Server::new_async().await;

        // Created first → matched first (mockito is FIFO when expect limits apply)
        let rate_limit = server
            .mock("POST", "/v1/messages")
            .with_status(429)
            .with_body("rate limited")
            .expect(1)
            .create_async()
            .await;

        // Created second → matched on the retry after rate_limit is exhausted
        let success = server
            .mock("POST", "/v1/messages")
            .with_status(200)
            .with_body(r#"{"content":[{"type":"text","text":"retried ok"}]}"#)
            .expect(1)
            .create_async()
            .await;

        let result = backend(&server.url()).complete("test").await.expect("ok");
        assert_eq!(result, "retried ok");

        // Both mocks assert exactly 1 call → proves exactly 2 total requests
        rate_limit.assert_async().await;
        success.assert_async().await;
    }

    #[tokio::test]
    async fn retry_on_529_then_succeeds() {
        let mut server = mockito::Server::new_async().await;

        let overloaded = server
            .mock("POST", "/v1/messages")
            .with_status(529)
            .with_body("overloaded")
            .expect(1)
            .create_async()
            .await;

        let success = server
            .mock("POST", "/v1/messages")
            .with_status(200)
            .with_body(r#"{"content":[{"type":"text","text":"529 retry ok"}]}"#)
            .expect(1)
            .create_async()
            .await;

        let result = backend(&server.url()).complete("test").await.expect("ok");
        assert_eq!(result, "529 retry ok");
        overloaded.assert_async().await;
        success.assert_async().await;
    }

    // ── non-retryable errors ───────────────────────────────────────────────

    #[tokio::test]
    async fn non_retryable_500_surfaces_immediately() {
        let mut server = mockito::Server::new_async().await;
        // expect(1) proves no retry occurred
        let mock = server
            .mock("POST", "/v1/messages")
            .with_status(500)
            .with_body("internal server error")
            .expect(1)
            .create_async()
            .await;

        let err = backend(&server.url()).complete("test").await.unwrap_err();
        assert!(err.to_string().contains("500"), "should include status: {err}");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn missing_content_field_returns_llm_failure() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", "/v1/messages")
            .with_status(200)
            .with_body(r#"{"id":"msg_123","type":"message"}"#)
            .create_async()
            .await;

        let err = backend(&server.url()).complete("test").await.unwrap_err();
        assert!(
            err.to_string().contains("missing content[0].text"),
            "unexpected error: {err}"
        );
    }

    // ── API key redaction ──────────────────────────────────────────────────

    #[tokio::test]
    async fn api_key_absent_from_non_200_error() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", "/v1/messages")
            .with_status(401)
            .with_body("invalid key")
            .create_async()
            .await;

        let err = backend(&server.url()).complete("test").await.unwrap_err();
        assert!(
            !err.to_string().contains(FAKE_KEY),
            "API key must not appear in error: {err}"
        );
    }

    #[tokio::test]
    async fn api_key_absent_from_retry_exhausted_error() {
        let mut server = mockito::Server::new_async().await;

        // Both attempts return 429 — the retry also fails
        let _first = server
            .mock("POST", "/v1/messages")
            .with_status(429)
            .with_body("rate limited")
            .expect(1)
            .create_async()
            .await;
        let _second = server
            .mock("POST", "/v1/messages")
            .with_status(429)
            .with_body("still rate limited")
            .expect(1)
            .create_async()
            .await;

        let err = backend(&server.url()).complete("test").await.unwrap_err();
        assert!(
            !err.to_string().contains(FAKE_KEY),
            "API key must not appear in retry error: {err}"
        );
    }
}

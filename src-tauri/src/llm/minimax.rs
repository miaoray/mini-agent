use std::env;
use std::fmt::{Display, Formatter};

use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};

const CHAT_COMPLETIONS_PATH: &str = "chat/completions";

#[derive(Debug)]
pub enum MiniMaxError {
    MissingEnvVar(&'static str),
    Http(reqwest::Error),
    Json(serde_json::Error),
    InvalidUtf8(std::string::FromUtf8Error),
    MissingResponseContent,
    UnexpectedStatus(u16, String),
}

impl Display for MiniMaxError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingEnvVar(name) => write!(f, "missing required environment variable: {name}"),
            Self::Http(err) => write!(f, "http request failed: {err}"),
            Self::Json(err) => write!(f, "failed to parse json payload: {err}"),
            Self::InvalidUtf8(err) => write!(f, "stream chunk contained invalid utf-8: {err}"),
            Self::MissingResponseContent => write!(f, "assistant response content was missing"),
            Self::UnexpectedStatus(status, body) => {
                write!(f, "unexpected http status {status}: {body}")
            }
        }
    }
}

impl std::error::Error for MiniMaxError {}

impl From<reqwest::Error> for MiniMaxError {
    fn from(value: reqwest::Error) -> Self {
        Self::Http(value)
    }
}

impl From<serde_json::Error> for MiniMaxError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl From<std::string::FromUtf8Error> for MiniMaxError {
    fn from(value: std::string::FromUtf8Error) -> Self {
        Self::InvalidUtf8(value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Default)]
pub struct StreamedResponse {
    pub collected_text: String,
    pub deltas: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct MiniMaxClient {
    http_client: Client,
    api_key: String,
    base_url: String,
}

impl MiniMaxClient {
    pub fn from_env() -> Result<Self, MiniMaxError> {
        let api_key = env::var("MINIMAX_API_KEY").map_err(|_| MiniMaxError::MissingEnvVar("MINIMAX_API_KEY"))?;
        let base_url = env::var("MINIMAX_BASE_URL").map_err(|_| MiniMaxError::MissingEnvVar("MINIMAX_BASE_URL"))?;
        Ok(Self::new(api_key, base_url))
    }

    pub fn new(api_key: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            http_client: Client::new(),
            api_key: api_key.into(),
            base_url: base_url.into(),
        }
    }

    pub async fn chat_completion(
        &self,
        model: &str,
        messages: &[ChatMessage],
    ) -> Result<String, MiniMaxError> {
        let response = self.send_chat_completion_request(model, messages, false).await?;
        let payload: OpenAiChatResponse = response.json().await?;

        payload
            .choices
            .into_iter()
            .next()
            .and_then(|choice| choice.message)
            .and_then(|message| message.content)
            .filter(|content| !content.is_empty())
            .ok_or(MiniMaxError::MissingResponseContent)
    }

    pub async fn chat_completion_stream_collect(
        &self,
        model: &str,
        messages: &[ChatMessage],
    ) -> Result<StreamedResponse, MiniMaxError> {
        let response = self.send_chat_completion_request(model, messages, true).await?;
        let mut stream = response.bytes_stream();
        let mut pending = String::new();
        let mut result = StreamedResponse::default();

        while let Some(chunk) = stream.next().await {
            let bytes = chunk?;
            pending.push_str(&String::from_utf8(bytes.to_vec())?);

            while let Some(newline_idx) = pending.find('\n') {
                let line = pending[..newline_idx].to_string();
                pending = pending[newline_idx + 1..].to_string();
                if let Some(delta) = parse_sse_delta_line(&line)? {
                    result.collected_text.push_str(&delta);
                    result.deltas.push(delta);
                }
            }
        }

        if !pending.trim().is_empty() {
            if let Some(delta) = parse_sse_delta_line(&pending)? {
                result.collected_text.push_str(&delta);
                result.deltas.push(delta);
            }
        }

        Ok(result)
    }

    async fn send_chat_completion_request(
        &self,
        model: &str,
        messages: &[ChatMessage],
        stream: bool,
    ) -> Result<reqwest::Response, MiniMaxError> {
        let endpoint = build_chat_completion_endpoint(&self.base_url);
        let body = OpenAiChatRequest {
            model,
            messages,
            stream,
        };

        let response = self
            .http_client
            .post(endpoint)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let response_body = response.text().await.unwrap_or_default();
            return Err(MiniMaxError::UnexpectedStatus(status.as_u16(), response_body));
        }

        Ok(response)
    }
}

fn build_chat_completion_endpoint(base_url: &str) -> String {
    format!("{}/{}", base_url.trim_end_matches('/'), CHAT_COMPLETIONS_PATH)
}

pub fn parse_sse_delta_line(line: &str) -> Result<Option<String>, MiniMaxError> {
    let trimmed = line.trim();
    if !trimmed.starts_with("data:") {
        return Ok(None);
    }

    let payload = trimmed.trim_start_matches("data:").trim();
    if payload.is_empty() || payload == "[DONE]" {
        return Ok(None);
    }

    let event: OpenAiStreamChunk = serde_json::from_str(payload)?;
    Ok(event
        .choices
        .into_iter()
        .find_map(|choice| choice.delta.and_then(|delta| delta.content)))
}

#[derive(Debug, Serialize)]
struct OpenAiChatRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    stream: bool,
}

#[derive(Debug, Deserialize)]
struct OpenAiChatResponse {
    choices: Vec<OpenAiChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: Option<OpenAiMessage>,
}

#[derive(Debug, Deserialize)]
struct OpenAiMessage {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamChunk {
    choices: Vec<OpenAiStreamChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamChoice {
    delta: Option<OpenAiDelta>,
}

#[derive(Debug, Deserialize)]
struct OpenAiDelta {
    content: Option<String>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{ChatMessage, MiniMaxClient};

    #[tokio::test]
    async fn parses_openai_compatible_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", "Bearer test-key"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({
                    "choices": [
                        {
                            "message": { "content": "hi" }
                        }
                    ]
                })),
            )
            .mount(&server)
            .await;

        let client = MiniMaxClient::new("test-key", format!("{}/v1", server.uri()));
        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: "hello".to_string(),
        }];

        let response = client
            .chat_completion("abab6.5", &messages)
            .await
            .expect("chat completion should parse assistant response");
        assert_eq!(response, "hi");
    }

    #[tokio::test]
    async fn posts_to_expected_chat_completions_endpoint() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({
                    "choices": [
                        {
                            "message": { "content": "ok" }
                        }
                    ]
                })),
            )
            .mount(&server)
            .await;

        let client = MiniMaxClient::new("test-key", format!("{}/v1/", server.uri()));
        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: "ping".to_string(),
        }];

        let response = client
            .chat_completion("abab6.5", &messages)
            .await
            .expect("chat completion should succeed");
        assert_eq!(response, "ok");
    }

    #[tokio::test]
    async fn collects_streamed_text_deltas() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                "data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\n\n\
                 data: {\"choices\":[{\"delta\":{\"content\":\" world\"}}]}\n\n\
                 data: [DONE]\n\n",
            ))
            .mount(&server)
            .await;

        let client = MiniMaxClient::new("test-key", format!("{}/v1", server.uri()));
        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: "hello".to_string(),
        }];

        let streamed = client
            .chat_completion_stream_collect("abab6.5", &messages)
            .await
            .expect("streaming request should parse SSE-style deltas");

        assert_eq!(streamed.deltas, vec!["Hello".to_string(), " world".to_string()]);
        assert_eq!(streamed.collected_text, "Hello world");
    }
}

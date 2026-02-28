use std::env;
use std::fmt::{Display, Formatter};
use std::time::Duration;

use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};

const CHAT_COMPLETIONS_PATH: &str = "chat/completions";
const DEFAULT_HTTP_TIMEOUT_SECONDS: u64 = 60;

#[derive(Debug)]
pub enum MiniMaxError {
    MissingEnvVar(&'static str),
    Http(reqwest::Error),
    Json(serde_json::Error),
    InvalidUtf8(String),
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
        let http_client = Client::builder()
            .timeout(Duration::from_secs(DEFAULT_HTTP_TIMEOUT_SECONDS))
            .build()
            .expect("failed to build reqwest client");
        Self {
            http_client,
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
        let mut utf8_pending = Vec::new();
        let mut line_pending = String::new();
        let mut result = StreamedResponse::default();

        while let Some(chunk) = stream.next().await {
            let bytes = chunk?;
            process_sse_chunk(&bytes, &mut utf8_pending, &mut line_pending, &mut result)?;
        }

        finalize_sse_stream(&mut utf8_pending, &mut line_pending, &mut result)?;

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

#[cfg(test)]
fn collect_streamed_response_from_chunks(
    chunks: Vec<Vec<u8>>,
) -> Result<StreamedResponse, MiniMaxError> {
    let mut utf8_pending = Vec::new();
    let mut line_pending = String::new();
    let mut result = StreamedResponse::default();

    for chunk in chunks {
        process_sse_chunk(&chunk, &mut utf8_pending, &mut line_pending, &mut result)?;
    }

    finalize_sse_stream(&mut utf8_pending, &mut line_pending, &mut result)?;
    Ok(result)
}

fn process_sse_chunk(
    chunk: &[u8],
    utf8_pending: &mut Vec<u8>,
    line_pending: &mut String,
    result: &mut StreamedResponse,
) -> Result<(), MiniMaxError> {
    let decoded = decode_complete_utf8(chunk, utf8_pending, false)?;
    append_decoded_text_and_parse_lines(&decoded, line_pending, result)
}

fn finalize_sse_stream(
    utf8_pending: &mut Vec<u8>,
    line_pending: &mut String,
    result: &mut StreamedResponse,
) -> Result<(), MiniMaxError> {
    let decoded_tail = decode_complete_utf8(&[], utf8_pending, true)?;
    append_decoded_text_and_parse_lines(&decoded_tail, line_pending, result)?;

    if !line_pending.trim().is_empty() {
        if let Some(delta) = parse_sse_delta_line(line_pending)? {
            result.collected_text.push_str(&delta);
            result.deltas.push(delta);
        }
    }
    line_pending.clear();

    Ok(())
}

fn append_decoded_text_and_parse_lines(
    decoded: &str,
    line_pending: &mut String,
    result: &mut StreamedResponse,
) -> Result<(), MiniMaxError> {
    line_pending.push_str(decoded);

    while let Some(newline_idx) = line_pending.find('\n') {
        let line = line_pending[..newline_idx].to_string();
        line_pending.drain(..=newline_idx);

        if let Some(delta) = parse_sse_delta_line(&line)? {
            result.collected_text.push_str(&delta);
            result.deltas.push(delta);
        }
    }

    Ok(())
}

fn decode_complete_utf8(
    chunk: &[u8],
    utf8_pending: &mut Vec<u8>,
    flush_tail: bool,
) -> Result<String, MiniMaxError> {
    utf8_pending.extend_from_slice(chunk);
    if utf8_pending.is_empty() {
        return Ok(String::new());
    }

    match std::str::from_utf8(utf8_pending) {
        Ok(decoded) => {
            let output = decoded.to_string();
            utf8_pending.clear();
            Ok(output)
        }
        Err(err) => {
            let valid_up_to = err.valid_up_to();
            let mut output = String::new();

            if valid_up_to > 0 {
                let valid_prefix = std::str::from_utf8(&utf8_pending[..valid_up_to])
                    .expect("utf-8 prefix should always be valid");
                output.push_str(valid_prefix);
                let trailing = utf8_pending[valid_up_to..].to_vec();
                *utf8_pending = trailing;
            }

            if err.error_len().is_none() && !flush_tail {
                return Ok(output);
            }

            Err(MiniMaxError::InvalidUtf8(err.to_string()))
        }
    }
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

    use super::{collect_streamed_response_from_chunks, ChatMessage, MiniMaxClient};

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

    #[test]
    fn collects_split_multibyte_utf8_chunks() {
        let chunks = vec![
            b"data: {\"choices\":[{\"delta\":{\"content\":\"\xE4".to_vec(),
            b"\xBD\xA0\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"\xE5\xA5".to_vec(),
            b"\xBD\"}}]}\n\ndata: [DONE]\n\n".to_vec(),
        ];

        let streamed = collect_streamed_response_from_chunks(chunks)
            .expect("split UTF-8 chunks should decode and parse correctly");

        assert_eq!(streamed.deltas, vec!["你".to_string(), "好".to_string()]);
        assert_eq!(streamed.collected_text, "你好");
    }
}

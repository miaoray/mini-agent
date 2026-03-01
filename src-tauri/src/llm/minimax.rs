use std::env;
use std::fmt::{Display, Formatter};
use std::time::Duration;

use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};

const OPENAI_CHAT_COMPLETIONS_PATH: &str = "chat/completions";
const ANTHROPIC_MESSAGES_PATH: &str = "v1/messages";
const ANTHROPIC_VERSION_HEADER: &str = "2023-06-01";
const ANTHROPIC_DEFAULT_MAX_TOKENS: u32 = 4096;
const DEFAULT_HTTP_TIMEOUT_SECONDS: u64 = 60;

#[derive(Debug)]
pub enum MiniMaxError {
    MissingEnvVar(&'static str),
    Http(reqwest::Error),
    Json(serde_json::Error),
    InvalidUtf8(String),
    MissingResponseContent,
    MissingToolCallName,
    MissingToolCallArguments,
    UnexpectedStatus(u16, String),
    Callback(String),
}

impl Display for MiniMaxError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingEnvVar(name) => write!(f, "missing required environment variable: {name}"),
            Self::Http(err) => write!(f, "http request failed: {err}"),
            Self::Json(err) => write!(f, "failed to parse json payload: {err}"),
            Self::InvalidUtf8(err) => write!(f, "stream chunk contained invalid utf-8: {err}"),
            Self::MissingResponseContent => write!(f, "assistant response content was missing"),
            Self::MissingToolCallName => write!(f, "tool call function name was missing"),
            Self::MissingToolCallArguments => write!(f, "tool call function arguments were missing"),
            Self::UnexpectedStatus(status, body) => {
                write!(f, "unexpected http status {status}: {body}")
            }
            Self::Callback(err) => write!(f, "stream callback failed: {err}"),
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    pub id: String,
    pub function_name: String,
    pub function_arguments: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatCompletionOutput {
    Content(String),
    ToolCalls(Vec<ToolCall>),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProviderMode {
    OpenAiCompatible,
    AnthropicCompatible,
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
        let mode = detect_provider_mode(&self.base_url);
        let response = self.send_chat_completion_request(model, messages, false).await?;
        if mode == ProviderMode::AnthropicCompatible {
            let payload: AnthropicMessagesResponse = response.json().await?;
            return match parse_anthropic_output(payload)? {
                ChatCompletionOutput::Content(content) => Ok(content),
                ChatCompletionOutput::ToolCalls(_) => Err(MiniMaxError::MissingResponseContent),
            };
        }

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

    pub async fn chat_completion_with_tools(
        &self,
        model: &str,
        messages: &[ChatMessage],
        tools: &[serde_json::Value],
    ) -> Result<ChatCompletionOutput, MiniMaxError> {
        let mode = detect_provider_mode(&self.base_url);
        let response = self
            .send_chat_completion_request_with_tools(model, messages, tools, false)
            .await?;
        if mode == ProviderMode::AnthropicCompatible {
            let payload: AnthropicMessagesResponse = response.json().await?;
            return parse_anthropic_output(payload);
        }

        let payload: OpenAiChatResponse = response.json().await?;
        let message = payload
            .choices
            .into_iter()
            .next()
            .and_then(|choice| choice.message)
            .ok_or(MiniMaxError::MissingResponseContent)?;

        if let Some(tool_calls) = message.tool_calls {
            if !tool_calls.is_empty() {
                return Ok(ChatCompletionOutput::ToolCalls(
                    tool_calls
                        .into_iter()
                        .map(|tool_call| {
                            let function_name = tool_call
                                .function
                                .name
                                .ok_or(MiniMaxError::MissingToolCallName)?;
                            let function_arguments = tool_call
                                .function
                                .arguments
                                .ok_or(MiniMaxError::MissingToolCallArguments)?;
                            Ok(ToolCall {
                                id: tool_call.id.unwrap_or_default(),
                                function_name,
                                function_arguments,
                            })
                        })
                        .collect::<Result<Vec<_>, MiniMaxError>>()?,
                ));
            }
        }

        message
            .content
            .filter(|content| !content.is_empty())
            .map(ChatCompletionOutput::Content)
            .ok_or(MiniMaxError::MissingResponseContent)
    }

    pub async fn chat_completion_stream_collect(
        &self,
        model: &str,
        messages: &[ChatMessage],
    ) -> Result<StreamedResponse, MiniMaxError> {
        if detect_provider_mode(&self.base_url) == ProviderMode::AnthropicCompatible {
            let collected_text = self.chat_completion(model, messages).await?;
            let deltas = if collected_text.is_empty() {
                Vec::new()
            } else {
                vec![collected_text.clone()]
            };
            return Ok(StreamedResponse {
                collected_text,
                deltas,
            });
        }

        let response = self.send_chat_completion_request(model, messages, true).await?;
        self.collect_stream_with_callback(response, |_delta| Ok(())).await
    }

    pub async fn chat_completion_stream_with_callback<F>(
        &self,
        model: &str,
        messages: &[ChatMessage],
        mut on_delta: F,
    ) -> Result<String, MiniMaxError>
    where
        F: FnMut(&str) -> Result<(), String>,
    {
        if detect_provider_mode(&self.base_url) == ProviderMode::AnthropicCompatible {
            let content = self.chat_completion(model, messages).await?;
            if !content.is_empty() {
                on_delta(&content).map_err(MiniMaxError::Callback)?;
            }
            return Ok(content);
        }

        let response = self.send_chat_completion_request(model, messages, true).await?;
        let streamed = self
            .collect_stream_with_callback(response, |delta| {
                on_delta(delta).map_err(MiniMaxError::Callback)
            })
            .await?;
        Ok(streamed.collected_text)
    }

    async fn collect_stream_with_callback<F>(
        &self,
        response: reqwest::Response,
        mut on_delta: F,
    ) -> Result<StreamedResponse, MiniMaxError>
    where
        F: FnMut(&str) -> Result<(), MiniMaxError>,
    {
        let mut stream = response.bytes_stream();
        let mut utf8_pending = Vec::new();
        let mut line_pending = String::new();
        let mut result = StreamedResponse::default();

        while let Some(chunk) = stream.next().await {
            let bytes = chunk?;
            process_sse_chunk(
                &bytes,
                &mut utf8_pending,
                &mut line_pending,
                &mut result,
                &mut on_delta,
            )?;
        }

        finalize_sse_stream(
            &mut utf8_pending,
            &mut line_pending,
            &mut result,
            &mut on_delta,
        )?;

        Ok(result)
    }

    async fn send_chat_completion_request(
        &self,
        model: &str,
        messages: &[ChatMessage],
        stream: bool,
    ) -> Result<reqwest::Response, MiniMaxError> {
        let mode = detect_provider_mode(&self.base_url);
        let endpoint = build_chat_completion_endpoint(&self.base_url, mode);
        let request = self
            .http_client
            .post(endpoint)
            .bearer_auth(&self.api_key);
        let response = match mode {
            ProviderMode::OpenAiCompatible => {
                let body = OpenAiChatRequest {
                    model,
                    messages,
                    stream,
                };
                request.json(&body).send().await?
            }
            ProviderMode::AnthropicCompatible => {
                let body = AnthropicMessagesRequest {
                    model,
                    max_tokens: ANTHROPIC_DEFAULT_MAX_TOKENS,
                    messages,
                    stream: false,
                    tools: None,
                };
                request
                    .header("anthropic-version", ANTHROPIC_VERSION_HEADER)
                    .json(&body)
                    .send()
                    .await?
            }
        };

        let status = response.status();
        if !status.is_success() {
            let response_body = response.text().await.unwrap_or_default();
            return Err(MiniMaxError::UnexpectedStatus(status.as_u16(), response_body));
        }

        Ok(response)
    }

    async fn send_chat_completion_request_with_tools(
        &self,
        model: &str,
        messages: &[ChatMessage],
        tools: &[serde_json::Value],
        stream: bool,
    ) -> Result<reqwest::Response, MiniMaxError> {
        let mode = detect_provider_mode(&self.base_url);
        let endpoint = build_chat_completion_endpoint(&self.base_url, mode);
        let request = self
            .http_client
            .post(endpoint)
            .bearer_auth(&self.api_key);
        let response = match mode {
            ProviderMode::OpenAiCompatible => {
                let body = OpenAiChatRequestWithTools {
                    model,
                    messages,
                    stream,
                    tools,
                };
                request.json(&body).send().await?
            }
            ProviderMode::AnthropicCompatible => {
                let body = AnthropicMessagesRequest {
                    model,
                    max_tokens: ANTHROPIC_DEFAULT_MAX_TOKENS,
                    messages,
                    stream: false,
                    tools: if tools.is_empty() { None } else { Some(tools) },
                };
                request
                    .header("anthropic-version", ANTHROPIC_VERSION_HEADER)
                    .json(&body)
                    .send()
                    .await?
            }
        };

        let status = response.status();
        if !status.is_success() {
            let response_body = response.text().await.unwrap_or_default();
            return Err(MiniMaxError::UnexpectedStatus(status.as_u16(), response_body));
        }

        Ok(response)
    }
}

fn build_chat_completion_endpoint(base_url: &str, mode: ProviderMode) -> String {
    let trimmed = base_url.trim_end_matches('/');
    match mode {
        ProviderMode::OpenAiCompatible => format!("{trimmed}/{OPENAI_CHAT_COMPLETIONS_PATH}"),
        ProviderMode::AnthropicCompatible => format!("{trimmed}/{ANTHROPIC_MESSAGES_PATH}"),
    }
}

fn detect_provider_mode(base_url: &str) -> ProviderMode {
    if is_anthropic_like_base_url(base_url) {
        ProviderMode::AnthropicCompatible
    } else {
        ProviderMode::OpenAiCompatible
    }
}

fn is_anthropic_like_base_url(base_url: &str) -> bool {
    base_url.to_ascii_lowercase().contains("/anthropic")
}

fn parse_anthropic_output(
    payload: AnthropicMessagesResponse,
) -> Result<ChatCompletionOutput, MiniMaxError> {
    let mut text_segments = Vec::new();
    let mut tool_calls = Vec::new();

    for block in payload.content {
        match block.block_type.as_str() {
            "text" => {
                if let Some(text) = block.text {
                    if !text.is_empty() {
                        text_segments.push(text);
                    }
                }
            }
            "tool_use" => {
                let function_name = block.name.ok_or(MiniMaxError::MissingToolCallName)?;
                let function_arguments = serde_json::to_string(
                    &block
                        .input
                        .ok_or(MiniMaxError::MissingToolCallArguments)?,
                )?;
                tool_calls.push(ToolCall {
                    id: block.id.unwrap_or_default(),
                    function_name,
                    function_arguments,
                });
            }
            _ => {}
        }
    }

    if !tool_calls.is_empty() {
        return Ok(ChatCompletionOutput::ToolCalls(tool_calls));
    }

    let text = text_segments.join("");
    if text.is_empty() {
        return Err(MiniMaxError::MissingResponseContent);
    }

    Ok(ChatCompletionOutput::Content(text))
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
        process_sse_chunk(
            &chunk,
            &mut utf8_pending,
            &mut line_pending,
            &mut result,
            &mut |_delta| Ok(()),
        )?;
    }

    finalize_sse_stream(
        &mut utf8_pending,
        &mut line_pending,
        &mut result,
        &mut |_delta| Ok(()),
    )?;
    Ok(result)
}

fn process_sse_chunk(
    chunk: &[u8],
    utf8_pending: &mut Vec<u8>,
    line_pending: &mut String,
    result: &mut StreamedResponse,
    on_delta: &mut impl FnMut(&str) -> Result<(), MiniMaxError>,
) -> Result<(), MiniMaxError> {
    let decoded = decode_complete_utf8(chunk, utf8_pending, false)?;
    append_decoded_text_and_parse_lines(&decoded, line_pending, result, on_delta)
}

fn finalize_sse_stream(
    utf8_pending: &mut Vec<u8>,
    line_pending: &mut String,
    result: &mut StreamedResponse,
    on_delta: &mut impl FnMut(&str) -> Result<(), MiniMaxError>,
) -> Result<(), MiniMaxError> {
    let decoded_tail = decode_complete_utf8(&[], utf8_pending, true)?;
    append_decoded_text_and_parse_lines(&decoded_tail, line_pending, result, on_delta)?;

    if !line_pending.trim().is_empty() {
        if let Some(delta) = parse_sse_delta_line(line_pending)? {
            result.collected_text.push_str(&delta);
            result.deltas.push(delta);
            on_delta(result.deltas.last().expect("delta was just pushed"))?;
        }
    }
    line_pending.clear();

    Ok(())
}

fn append_decoded_text_and_parse_lines(
    decoded: &str,
    line_pending: &mut String,
    result: &mut StreamedResponse,
    on_delta: &mut impl FnMut(&str) -> Result<(), MiniMaxError>,
) -> Result<(), MiniMaxError> {
    line_pending.push_str(decoded);

    while let Some(newline_idx) = line_pending.find('\n') {
        let line = line_pending[..newline_idx].to_string();
        line_pending.drain(..=newline_idx);

        if let Some(delta) = parse_sse_delta_line(&line)? {
            result.collected_text.push_str(&delta);
            result.deltas.push(delta);
            on_delta(result.deltas.last().expect("delta was just pushed"))?;
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

#[derive(Debug, Serialize)]
struct OpenAiChatRequestWithTools<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    stream: bool,
    tools: &'a [serde_json::Value],
}

#[derive(Debug, Serialize)]
struct AnthropicMessagesRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    messages: &'a [ChatMessage],
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<&'a [serde_json::Value]>,
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
    tool_calls: Option<Vec<OpenAiToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OpenAiToolCall {
    id: Option<String>,
    function: OpenAiToolCallFunction,
}

#[derive(Debug, Deserialize)]
struct OpenAiToolCallFunction {
    name: Option<String>,
    arguments: Option<String>,
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

#[derive(Debug, Deserialize)]
struct AnthropicMessagesResponse {
    content: Vec<AnthropicContentBlock>,
}

#[derive(Debug, Deserialize)]
struct AnthropicContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    text: Option<String>,
    id: Option<String>,
    name: Option<String>,
    input: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{
        collect_streamed_response_from_chunks, ChatCompletionOutput, ChatMessage, MiniMaxClient,
    };

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

    #[tokio::test]
    async fn parses_anthropic_compatible_text_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/anthropic/v1/messages"))
            .and(header("authorization", "Bearer test-key"))
            .and(header("anthropic-version", "2023-06-01"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({
                    "id": "msg_1",
                    "type": "message",
                    "role": "assistant",
                    "content": [
                        { "type": "text", "text": "hi from anthropic mode" }
                    ]
                })),
            )
            .mount(&server)
            .await;

        let client = MiniMaxClient::new("test-key", format!("{}/anthropic", server.uri()));
        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: "hello".to_string(),
        }];

        let response = client
            .chat_completion("MiniMax-M2.5", &messages)
            .await
            .expect("anthropic-compatible response should parse");
        assert_eq!(response, "hi from anthropic mode");
    }

    #[tokio::test]
    async fn parses_anthropic_compatible_tool_use_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/anthropic/v1/messages"))
            .and(header("authorization", "Bearer test-key"))
            .and(header("anthropic-version", "2023-06-01"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({
                    "id": "msg_2",
                    "type": "message",
                    "role": "assistant",
                    "content": [
                        {
                            "type": "tool_use",
                            "id": "toolu_123",
                            "name": "web_search",
                            "input": { "query": "mini agent" }
                        }
                    ]
                })),
            )
            .mount(&server)
            .await;

        let client = MiniMaxClient::new("test-key", format!("{}/anthropic", server.uri()));
        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: "search it".to_string(),
        }];
        let tools = vec![json!({
            "type": "function",
            "function": {
                "name": "web_search",
                "description": "search",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" }
                    },
                    "required": ["query"]
                }
            }
        })];

        let response = client
            .chat_completion_with_tools("MiniMax-M2.5", &messages, &tools)
            .await
            .expect("anthropic-compatible tool_use response should parse");

        assert_eq!(
            response,
            ChatCompletionOutput::ToolCalls(vec![super::ToolCall {
                id: "toolu_123".to_string(),
                function_name: "web_search".to_string(),
                function_arguments: "{\"query\":\"mini agent\"}".to_string(),
            }])
        );
    }
}

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
const MAX_HTTP_RETRIES: usize = 2;
const RETRY_BASE_DELAY_MS: u64 = 250;

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
    /// Anthropic-style content blocks (text, tool_use, tool_result). When set, used for Anthropic requests instead of content.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub content_blocks: Option<Vec<serde_json::Value>>,
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

impl ChatCompletionOutput {
    pub fn as_content(&self) -> Option<&str> {
        match self {
            ChatCompletionOutput::Content(s) => Some(s),
            ChatCompletionOutput::ToolCalls(_) => None,
        }
    }
}

/// Result of chat_completion_with_tools including optional thinking for display.
#[derive(Debug, Clone)]
pub struct ChatCompletionResult {
    pub output: ChatCompletionOutput,
    pub thinking: Option<String>,
    /// Raw content blocks from Anthropic response (text, tool_use). Use for assistant message history.
    pub raw_content_blocks: Option<Vec<serde_json::Value>>,
    /// Raw request JSON for debugging
    pub raw_request: Option<String>,
    /// Raw response JSON for debugging
    pub raw_response: Option<String>,
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
            return match parse_anthropic_output(payload)?.output {
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
    ) -> Result<ChatCompletionResult, MiniMaxError> {
        let mode = detect_provider_mode(&self.base_url);
        let (raw_request, response) = self
            .send_chat_completion_request_with_tools_raw(model, messages, tools, false)
            .await?;
        let body = response.text().await?;
        let body_preview: String = body.chars().take(200).collect();
        eprintln!(
            "[mini-agent][llm] response raw_json_preview ({} chars)={:?}",
            body.len(),
            body_preview
        );

        if mode == ProviderMode::AnthropicCompatible {
            let payload: AnthropicMessagesResponse = serde_json::from_str(&body)?;
            let mut result = parse_anthropic_output(payload)?;
            result.raw_request = Some(raw_request);
            result.raw_response = Some(body);
            return Ok(result);
        }

        let payload: OpenAiChatResponse = serde_json::from_str(&body)?;
        let message = payload
            .choices
            .into_iter()
            .next()
            .and_then(|choice| choice.message)
            .ok_or(MiniMaxError::MissingResponseContent)?;

        // Extract thinking/reasoning content (for DeepSeek, etc.)
        let thinking = message.reasoning_content.filter(|r| !r.is_empty());

        if let Some(tool_calls) = message.tool_calls {
            if !tool_calls.is_empty() {
                return Ok(ChatCompletionResult {
                    output: ChatCompletionOutput::ToolCalls(
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
                    ),
                    thinking,
                    raw_content_blocks: None,
                    raw_request: Some(raw_request),
                    raw_response: Some(body),
                });
            }
        }

        message
            .content
            .filter(|content| !content.is_empty())
            .map(|c| ChatCompletionResult {
                output: ChatCompletionOutput::Content(c),
                thinking,
                raw_content_blocks: None,
                raw_request: Some(raw_request),
                raw_response: Some(body),
            })
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

    /// Streaming chat completion with tools support.
    pub async fn chat_completion_stream_with_callback_and_tools<F>(
        &self,
        model: &str,
        messages: &[ChatMessage],
        tools: &[serde_json::Value],
        mut on_delta: F,
    ) -> Result<String, MiniMaxError>
    where
        F: FnMut(&str) -> Result<(), String>,
    {
        let mode = detect_provider_mode(&self.base_url);

        // For Anthropic-compatible providers, use non-streaming
        if mode == ProviderMode::AnthropicCompatible {
            let result = self
                .chat_completion_with_tools(model, messages, tools)
                .await?;
            if let ChatCompletionOutput::Content(ref content) = result.output {
                if !content.is_empty() {
                    on_delta(content).map_err(MiniMaxError::Callback)?;
                }
            }
            return Ok(result
                .output
                .as_content()
                .unwrap_or_default()
                .to_string());
        }

        // For OpenAI-compatible providers, use streaming
        let (_raw_request, response) = self
            .send_chat_completion_request_with_tools_raw(model, messages, tools, true)
            .await?;

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
        let response = match mode {
            ProviderMode::OpenAiCompatible => {
                let body = OpenAiChatRequest {
                    model,
                    messages,
                    stream,
                };
                self.send_with_retries(|| {
                    self.http_client
                        .post(endpoint.clone())
                        .bearer_auth(&self.api_key)
                        .json(&body)
                })
                .await?
            }
            ProviderMode::AnthropicCompatible => {
                let body = AnthropicMessagesRequestBody {
                    model: model.to_string(),
                    max_tokens: ANTHROPIC_DEFAULT_MAX_TOKENS,
                    messages: chat_messages_to_anthropic(messages),
                    stream: false,
                    tools: None,
                };
                self.send_with_retries(|| {
                    self.http_client
                        .post(endpoint.clone())
                        .bearer_auth(&self.api_key)
                        .header("anthropic-version", ANTHROPIC_VERSION_HEADER)
                        .json(&body)
                })
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

    async fn send_chat_completion_request_with_tools_raw(
        &self,
        model: &str,
        messages: &[ChatMessage],
        tools: &[serde_json::Value],
        stream: bool,
    ) -> Result<(String, reqwest::Response), MiniMaxError> {
        let mode = detect_provider_mode(&self.base_url);
        let endpoint = build_chat_completion_endpoint(&self.base_url, mode);
        let (raw_request, response) = match mode {
            ProviderMode::OpenAiCompatible => {
                let body = OpenAiChatRequestWithTools {
                    model,
                    messages,
                    stream,
                    tools,
                };
                let raw = serde_json::to_string(&body).unwrap_or_default();
                let resp = self
                    .send_with_retries(|| {
                        self.http_client
                            .post(endpoint.clone())
                            .bearer_auth(&self.api_key)
                            .json(&body)
                    })
                    .await?;
                (raw, resp)
            }
            ProviderMode::AnthropicCompatible => {
                let anthropic_tools = map_openai_tools_to_anthropic(tools);
                let body = AnthropicMessagesRequestBody {
                    model: model.to_string(),
                    max_tokens: ANTHROPIC_DEFAULT_MAX_TOKENS,
                    messages: chat_messages_to_anthropic(messages),
                    stream: false,
                    tools: if anthropic_tools.is_empty() {
                        None
                    } else {
                        Some(anthropic_tools)
                    },
                };
                let raw = serde_json::to_string(&body).unwrap_or_default();
                let resp = self
                    .send_with_retries(|| {
                        self.http_client
                            .post(endpoint.clone())
                            .bearer_auth(&self.api_key)
                            .header("anthropic-version", ANTHROPIC_VERSION_HEADER)
                            .json(&body)
                    })
                    .await?;
                (raw, resp)
            }
        };

        let status = response.status();
        if !status.is_success() {
            let response_body = response.text().await.unwrap_or_default();
            return Err(MiniMaxError::UnexpectedStatus(status.as_u16(), response_body));
        }

        Ok((raw_request, response))
    }

    async fn send_with_retries<F>(&self, mut build_request: F) -> Result<reqwest::Response, MiniMaxError>
    where
        F: FnMut() -> reqwest::RequestBuilder,
    {
        let mut last_err: Option<reqwest::Error> = None;
        for attempt in 0..=MAX_HTTP_RETRIES {
            match build_request().send().await {
                Ok(response) => {
                    let status = response.status();
                    if is_retryable_status(status.as_u16()) && attempt < MAX_HTTP_RETRIES {
                        tokio::time::sleep(retry_delay_for_attempt(attempt)).await;
                        continue;
                    }
                    return Ok(response);
                }
                Err(err) => {
                    if !is_retryable_http_error(&err) || attempt >= MAX_HTTP_RETRIES {
                        return Err(MiniMaxError::Http(err));
                    }
                    last_err = Some(err);
                    tokio::time::sleep(retry_delay_for_attempt(attempt)).await;
                }
            }
        }

        Err(MiniMaxError::Http(
            last_err.expect("retry loop should capture last reqwest error before returning"),
        ))
    }
}

fn is_retryable_http_error(err: &reqwest::Error) -> bool {
    err.is_timeout() || err.is_connect() || err.is_request()
}

fn is_retryable_status(status: u16) -> bool {
    status == 429 || (500..=599).contains(&status)
}

fn retry_delay_for_attempt(attempt: usize) -> Duration {
    let multiplier = 1u64 << (attempt as u32);
    Duration::from_millis(RETRY_BASE_DELAY_MS.saturating_mul(multiplier))
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

fn map_openai_tools_to_anthropic(tools: &[serde_json::Value]) -> Vec<serde_json::Value> {
    tools
        .iter()
        .filter_map(|tool| {
            let function = tool.get("function")?.as_object()?;
            let name = function.get("name")?.as_str()?;
            if name.trim().is_empty() {
                return None;
            }

            let input_schema = function.get("parameters")?.as_object()?;
            let description = function
                .get("description")
                .and_then(|value| value.as_str())
                .filter(|value| !value.trim().is_empty());

            let mut mapped = serde_json::Map::new();
            mapped.insert(
                "name".to_string(),
                serde_json::Value::String(name.to_string()),
            );
            if let Some(description) = description {
                mapped.insert(
                    "description".to_string(),
                    serde_json::Value::String(description.to_string()),
                );
            }
            mapped.insert(
                "input_schema".to_string(),
                serde_json::Value::Object(input_schema.clone()),
            );
            Some(serde_json::Value::Object(mapped))
        })
        .collect()
}

fn parse_anthropic_output(
    payload: AnthropicMessagesResponse,
) -> Result<ChatCompletionResult, MiniMaxError> {
    let block_summary: Vec<(String, usize, usize)> = payload
        .content
        .iter()
        .map(|b| {
            (
                b.block_type.clone(),
                b.text.as_ref().map(|t| t.len()).unwrap_or(0),
                b.thinking.as_ref().map(|t| t.len()).unwrap_or(0),
            )
        })
        .collect();
    eprintln!(
        "[mini-agent][parse_anthropic] content_blocks count={} blocks={:?}",
        payload.content.len(),
        block_summary
    );

    let mut text_segments = Vec::new();
    let mut tool_calls = Vec::new();
    let mut thinking_segments = Vec::new();
    let raw_blocks: Vec<serde_json::Value> = payload
        .content
        .iter()
        .filter_map(|b| serde_json::to_value(b).ok())
        .collect();

    for block in payload.content {
        match block.block_type.as_str() {
            "text" => {
                if let Some(text) = block.text {
                    if !text.is_empty() {
                        text_segments.push(text);
                    }
                }
            }
            "thinking" => {
                if let Some(t) = block.thinking {
                    if !t.is_empty() {
                        thinking_segments.push(t);
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

    let thinking = if thinking_segments.is_empty() {
        None
    } else {
        Some(thinking_segments.join("\n\n"))
    };

    let raw_content_blocks = if raw_blocks.is_empty() {
        None
    } else {
        Some(raw_blocks)
    };

    if !tool_calls.is_empty() {
        eprintln!(
            "[mini-agent][parse_anthropic] ToolCalls count={} thinking_len={}",
            tool_calls.len(),
            thinking.as_ref().map(|t| t.len()).unwrap_or(0)
        );
        return Ok(ChatCompletionResult {
            output: ChatCompletionOutput::ToolCalls(tool_calls),
            thinking,
            raw_content_blocks,
            raw_request: None,
            raw_response: None,
        });
    }

    let text = text_segments.join("");
    eprintln!(
        "[mini-agent][parse_anthropic] parsed text_len={} thinking_len={} text_preview={:?}",
        text.len(),
        thinking.as_ref().map(|t| t.len()).unwrap_or(0),
        text.chars().take(80).collect::<String>()
    );
    if text.is_empty() {
        return Err(MiniMaxError::MissingResponseContent);
    }

    Ok(ChatCompletionResult {
        output: ChatCompletionOutput::Content(text),
        thinking,
        raw_content_blocks,
        raw_request: None,
        raw_response: None,
    })
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

fn chat_messages_to_anthropic(messages: &[ChatMessage]) -> Vec<serde_json::Value> {
    messages
        .iter()
        .map(|m| {
            let content: serde_json::Value = match &m.content_blocks {
                Some(blocks) => serde_json::Value::Array(blocks.clone()),
                None => serde_json::json!([{ "type": "text", "text": m.content }]),
            };
            serde_json::json!({ "role": m.role, "content": content })
        })
        .collect()
}

#[derive(Debug, Serialize)]
struct AnthropicMessagesRequestBody {
    model: String,
    max_tokens: u32,
    messages: Vec<serde_json::Value>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiChatResponse {
    pub choices: Vec<OpenAiChoice>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiChoice {
    pub message: Option<OpenAiMessage>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiMessage {
    pub content: Option<String>,
    pub tool_calls: Option<Vec<OpenAiToolCall>>,
    #[serde(rename = "reasoning_content")]
    pub reasoning_content: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiToolCall {
    pub id: Option<String>,
    pub function: OpenAiToolCallFunction,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiToolCallFunction {
    pub name: Option<String>,
    pub arguments: Option<String>,
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

#[derive(Debug, Serialize, Deserialize)]
struct AnthropicContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    text: Option<String>,
    thinking: Option<String>,
    id: Option<String>,
    name: Option<String>,
    input: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{
        collect_streamed_response_from_chunks, map_openai_tools_to_anthropic,
        ChatCompletionOutput, ChatMessage, MiniMaxClient,
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
            content_blocks: None,
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
            content_blocks: None,
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
            content_blocks: None,
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
            content_blocks: None,
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
            .and(body_partial_json(json!({
                "tools": [
                    {
                        "name": "web_search",
                        "description": "search",
                        "input_schema": {
                            "type": "object",
                            "properties": {
                                "query": { "type": "string" }
                            },
                            "required": ["query"]
                        }
                    }
                ]
            })))
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
            content_blocks: None,
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

        let result = client
            .chat_completion_with_tools("MiniMax-M2.5", &messages, &tools)
            .await
            .expect("anthropic-compatible tool_use response should parse");

        assert_eq!(
            result.output,
            ChatCompletionOutput::ToolCalls(vec![super::ToolCall {
                id: "toolu_123".to_string(),
                function_name: "web_search".to_string(),
                function_arguments: "{\"query\":\"mini agent\"}".to_string(),
            }])
        );
    }

    #[test]
    fn maps_valid_openai_tool_to_anthropic_tool() {
        let tools = vec![json!({
            "type": "function",
            "function": {
                "name": "web_search",
                "description": "search the web",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" }
                    },
                    "required": ["query"]
                }
            }
        })];

        let mapped = map_openai_tools_to_anthropic(&tools);
        assert_eq!(
            mapped,
            vec![json!({
                "name": "web_search",
                "description": "search the web",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" }
                    },
                    "required": ["query"]
                }
            })]
        );
    }

    #[test]
    fn skips_malformed_openai_tool_entries() {
        let tools = vec![json!({
            "type": "function",
            "function": {
                "name": "",
                "description": "invalid because empty name",
                "parameters": {
                    "type": "object",
                    "properties": {}
                }
            }
        })];

        let mapped = map_openai_tools_to_anthropic(&tools);
        assert!(mapped.is_empty());
    }

    #[test]
    fn keeps_only_valid_entries_from_mixed_tool_list() {
        let tools = vec![
            json!({
                "type": "function",
                "function": {
                    "name": "web_search",
                    "description": "search",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "query": { "type": "string" }
                        }
                    }
                }
            }),
            json!({
                "type": "function",
                "function": {
                    "name": "broken_missing_parameters"
                }
            }),
            json!({
                "type": "not_function",
                "foo": "bar"
            }),
        ];

        let mapped = map_openai_tools_to_anthropic(&tools);
        assert_eq!(
            mapped,
            vec![json!({
                "name": "web_search",
                "description": "search",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" }
                    }
                }
            })]
        );
    }
}

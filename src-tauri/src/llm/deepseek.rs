//! DeepSeek LLM provider implementation.
//! Reuses the MiniMaxClient for base functionality but adds DeepSeek-specific features:
//! - Thinking/Reasoning content support

use std::env;

use crate::llm::minimax::{self, ChatCompletionOutput, ChatCompletionResult, ChatMessage, MiniMaxError};

/// DeepSeek client that wraps MiniMaxClient with additional features
pub struct DeepSeekClient {
    inner: minimax::MiniMaxClient,
}

impl DeepSeekClient {
    pub fn from_env() -> Result<Self, MiniMaxError> {
        let api_key = env::var("DEEPSEEK_API_KEY")
            .map_err(|_| MiniMaxError::MissingEnvVar("DEEPSEEK_API_KEY"))?;
        let base_url = env::var("DEEPSEEK_BASE_URL")
            .map_err(|_| MiniMaxError::MissingEnvVar("DEEPSEEK_BASE_URL"))?;
        Ok(Self {
            inner: minimax::MiniMaxClient::new(api_key, base_url),
        })
    }

    /// Chat completion with tools - includes reasoning_content support
    pub async fn chat_completion_with_tools(
        &self,
        model: &str,
        messages: &[ChatMessage],
        tools: &[serde_json::Value],
    ) -> Result<ChatCompletionResult, MiniMaxError> {
        self.inner
            .chat_completion_with_tools(model, messages, tools)
            .await
    }

    /// Non-streaming chat completion
    pub async fn chat_completion(
        &self,
        model: &str,
        messages: &[ChatMessage],
    ) -> Result<String, MiniMaxError> {
        self.inner.chat_completion(model, messages).await
    }
}

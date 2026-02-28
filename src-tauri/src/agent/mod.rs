use serde::Serialize;

pub mod r#loop;

#[derive(Debug, Clone)]
pub struct StoredMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct ProviderRuntime {
    pub provider_id: String,
    pub base_url: String,
    pub model_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatDeltaEvent {
    pub conversation_id: String,
    pub message_id: String,
    pub delta: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatDoneEvent {
    pub conversation_id: String,
    pub message_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatErrorEvent {
    pub conversation_id: String,
    pub message_id: String,
    pub message: String,
}

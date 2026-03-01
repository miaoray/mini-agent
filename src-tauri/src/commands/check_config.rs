use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigCheckResponse {
    has_api_key: bool,
}

#[tauri::command]
pub fn check_config() -> ConfigCheckResponse {
    let has_api_key = std::env::var("MINIMAX_API_KEY")
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);

    ConfigCheckResponse { has_api_key }
}

use serde::Serialize;

use crate::db;
use crate::db::provider::Provider;

/// Check if a provider has its required environment variables configured
fn is_provider_configured(provider_id: &str) -> bool {
    let api_key_var = match provider_id {
        "deepseek" => "DEEPSEEK_API_KEY",
        "minimax" => "MINIMAX_API_KEY",
        "openai" => "OPENAI_API_KEY",
        "anthropic" => "ANTHROPIC_API_KEY",
        _ => return false,
    };

    std::env::var(api_key_var)
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub model_id: String,
    pub is_configured: bool,
}

impl From<Provider> for ProviderInfo {
    fn from(p: Provider) -> Self {
        let is_configured = is_provider_configured(&p.id);
        Self {
            id: p.id,
            name: p.name,
            model_id: p.model_id,
            is_configured,
        }
    }
}

#[tauri::command]
pub fn list_providers(state: tauri::State<'_, db::DbState>) -> Result<Vec<ProviderInfo>, String> {
    let conn = state.connection().map_err(|e| e.to_string())?;
    let providers = db::provider::list_providers(&conn).map_err(|e| e.to_string())?;

    // Filter to only show MiniMax and DeepSeek providers
    let filtered: Vec<ProviderInfo> = providers
        .into_iter()
        .filter(|p| p.id == "minimax" || p.id == "deepseek")
        .map(ProviderInfo::from)
        .collect();

    Ok(filtered)
}

#[tauri::command]
pub fn get_default_provider(state: tauri::State<'_, db::DbState>) -> Result<ProviderInfo, String> {
    let conn = state.connection().map_err(|e| e.to_string())?;

    // Try to get from app_settings first
    let default_id: Option<String> = conn
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'default_provider_id'",
            [],
            |row| row.get(0),
        )
        .ok();

    let provider_id = default_id.unwrap_or_else(|| db::provider::DEFAULT_PROVIDER_ID.to_string());

    let provider = db::provider::get_provider_by_id(&conn, &provider_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("provider {} not found", provider_id))?;

    Ok(ProviderInfo::from(provider))
}

#[tauri::command]
pub fn set_default_provider(
    state: tauri::State<'_, db::DbState>,
    provider_id: String,
) -> Result<ProviderInfo, String> {
    let conn = state.connection().map_err(|e| e.to_string())?;

    // Verify provider exists
    let provider = db::provider::get_provider_by_id(&conn, &provider_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("provider {} not found", provider_id))?;

    // Save to app_settings
    conn.execute(
        "INSERT INTO app_settings (key, value) VALUES ('default_provider_id', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [&provider_id],
    )
    .map_err(|e| e.to_string())?;

    Ok(ProviderInfo::from(provider))
}

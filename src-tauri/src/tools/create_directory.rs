use futures::future::BoxFuture;
use serde_json::{json, Value};
use std::path::{Component, Path};

use crate::tools::{ToolDef, ToolImpl};

pub struct CreateDirectoryTool;

impl CreateDirectoryTool {
    pub fn new() -> Self {
        Self
    }
}

impl ToolImpl for CreateDirectoryTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            id: "create_directory".to_string(),
            name: "create_directory".to_string(),
            description: "Request approval to create a directory".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Directory path to create"
                    }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
        }
    }

    fn execute(&self, args: Value) -> BoxFuture<'_, Result<String, String>> {
        Box::pin(async move {
            let path = args
                .get("path")
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default();
            if path.is_empty() {
                return Err("missing required argument: path".to_string());
            }

            validate_safe_path(path)?;
            Ok(json!({
                "status": "PENDING_APPROVAL",
                "payload": {
                    "path": path
                }
            })
            .to_string())
        })
    }
}

fn validate_safe_path(path: &str) -> Result<(), String> {
    let parsed = Path::new(path);
    if parsed
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err("path traversal is not allowed".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use serde_json::json;
    use uuid::Uuid;

    use super::CreateDirectoryTool;
    use crate::tools::ToolImpl;

    fn unique_temp_path(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!("{prefix}-{}", Uuid::new_v4()))
    }

    #[tokio::test]
    async fn create_directory_returns_pending_approval_and_does_not_create_directory() {
        let path = unique_temp_path("create-dir-tool");
        if path.exists() {
            std::fs::remove_dir_all(&path).expect("cleanup existing test directory should succeed");
        }

        let tool = CreateDirectoryTool::new();
        let result = tool
            .execute(json!({ "path": path.to_string_lossy() }))
            .await
            .expect("create_directory should return pending approval");

        assert!(result.contains("PENDING_APPROVAL"));
        assert!(result.contains(path.to_string_lossy().as_ref()));
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn create_directory_rejects_path_traversal() {
        let tool = CreateDirectoryTool::new();
        let result = tool.execute(json!({ "path": "../../../etc" })).await;
        assert!(result.is_err());
    }
}

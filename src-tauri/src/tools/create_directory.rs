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
    if parsed.is_absolute() {
        return Err("absolute paths are not allowed".to_string());
    }
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
    use serde_json::json;

    use super::CreateDirectoryTool;
    use crate::tools::ToolImpl;

    #[tokio::test]
    async fn create_directory_returns_pending_approval_and_does_not_create_directory() {
        let tool = CreateDirectoryTool::new();
        let path = "safe/relative/create-dir-tool";
        let result = tool
            .execute(json!({ "path": path }))
            .await
            .expect("create_directory should return pending approval");

        assert!(result.contains("PENDING_APPROVAL"));
        assert!(result.contains(path));
    }

    #[tokio::test]
    async fn create_directory_rejects_path_traversal() {
        let tool = CreateDirectoryTool::new();
        let result = tool.execute(json!({ "path": "../../../etc" })).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn create_directory_rejects_absolute_path() {
        let tool = CreateDirectoryTool::new();
        let result = tool.execute(json!({ "path": "/tmp/forbidden" })).await;
        assert!(result.is_err());
    }
}

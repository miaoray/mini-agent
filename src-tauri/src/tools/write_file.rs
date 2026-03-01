use futures::future::BoxFuture;
use serde_json::{json, Value};
use std::path::{Component, Path};

use crate::tools::{ToolDef, ToolImpl};

pub struct WriteFileTool;

impl WriteFileTool {
    pub fn new() -> Self {
        Self
    }
}

impl ToolImpl for WriteFileTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            id: "write_file".to_string(),
            name: "write_file".to_string(),
            description: "Request approval to write a file".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path to write"
                    },
                    "content": {
                        "type": "string",
                        "description": "Text content to write after approval"
                    }
                },
                "required": ["path", "content"],
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

            let content = args
                .get("content")
                .and_then(Value::as_str)
                .ok_or_else(|| "missing required argument: content".to_string())?;

            validate_safe_path(path)?;
            Ok(json!({
                "status": "PENDING_APPROVAL",
                "payload": {
                    "path": path,
                    "content": content
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

    use super::WriteFileTool;
    use crate::tools::ToolImpl;

    #[tokio::test]
    async fn write_file_returns_pending_approval_and_does_not_write_file() {
        let tool = WriteFileTool::new();
        let path = "safe/relative/write-file-tool.txt";
        let content = "hello from write_file test";
        let result = tool
            .execute(json!({ "path": path, "content": content }))
            .await
            .expect("write_file should return pending approval");

        assert!(result.contains("PENDING_APPROVAL"));
        assert!(result.contains(path));
        assert!(result.contains(content));
    }

    #[tokio::test]
    async fn write_file_rejects_path_traversal() {
        let tool = WriteFileTool::new();
        let result = tool
            .execute(json!({ "path": "../../../etc/passwd", "content": "x" }))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn write_file_rejects_absolute_path() {
        let tool = WriteFileTool::new();
        let absolute_path = if cfg!(target_os = "windows") {
            "C:\\forbidden.txt"
        } else {
            "/tmp/forbidden.txt"
        };
        let result = tool
            .execute(json!({ "path": absolute_path, "content": "x" }))
            .await;
        assert!(result.is_err());
    }
}

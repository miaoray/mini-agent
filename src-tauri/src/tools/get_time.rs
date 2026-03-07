use chrono::{DateTime, Local};
use futures::future::BoxFuture;
use serde_json::{json, Value};

use crate::tools::{ToolDef, ToolImpl};

pub struct GetTimeTool;

impl GetTimeTool {
    pub fn new() -> Self {
        Self
    }
}

impl ToolImpl for GetTimeTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            id: "get_time".to_string(),
            name: "get_time".to_string(),
            description: "Get the current date and time with timezone information. Use this when you need to know the current time or date.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {},
                "required": [],
                "additionalProperties": false
            }),
        }
    }

    fn execute(&self, _args: Value) -> BoxFuture<'_, Result<String, String>> {
        Box::pin(async move {
            let now: DateTime<Local> = Local::now();
            
            let result = json!({
                "iso": now.to_rfc3339(),
                "human_readable": now.format("%Y-%m-%d %H:%M:%S %Z").to_string(),
                "unix_timestamp": now.timestamp(),
                "timezone": now.format("%Z").to_string(),
                "utc_offset": format!("{:+05}", now.offset().local_minus_utc() / 3600)
            });
            
            Ok(result.to_string())
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::GetTimeTool;
    use crate::tools::ToolImpl;

    #[tokio::test]
    async fn get_time_returns_valid_json() {
        let tool = GetTimeTool::new();
        let result = tool.execute(Value::Object(serde_json::Map::new())).await;
        
        assert!(result.is_ok());
        let json_str = result.unwrap();
        let parsed: Value = serde_json::from_str(&json_str).expect("should return valid JSON");
        
        // Verify all expected fields are present
        assert!(parsed.get("iso").is_some());
        assert!(parsed.get("human_readable").is_some());
        assert!(parsed.get("unix_timestamp").is_some());
        assert!(parsed.get("timezone").is_some());
        assert!(parsed.get("utc_offset").is_some());
        
        // Verify iso field contains timezone info (has + or - for offset)
        let iso = parsed.get("iso").and_then(|v| v.as_str()).unwrap();
        assert!(iso.contains('+') || iso.contains('-'));
        
        // Verify unix_timestamp is a number
        assert!(parsed.get("unix_timestamp").and_then(|v| v.as_i64()).is_some());
    }

    #[tokio::test]
    async fn get_time_definition_has_required_fields() {
        let tool = GetTimeTool::new();
        let def = tool.definition();
        
        assert_eq!(def.id, "get_time");
        assert_eq!(def.name, "get_time");
        assert!(def.description.contains("current"));
        assert!(def.description.contains("time"));
    }
}

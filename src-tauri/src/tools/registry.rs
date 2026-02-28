use std::collections::HashMap;

use serde_json::{json, Value};

use super::types::ToolImpl;

#[derive(Default)]
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn ToolImpl>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<T>(&mut self, tool: T)
    where
        T: ToolImpl + 'static,
    {
        let name = tool.definition().name;
        self.tools.insert(name, Box::new(tool));
    }

    pub fn get_tools_for_llm(&self) -> Vec<Value> {
        self.tools
            .values()
            .map(|tool| {
                let def = tool.definition();
                json!({
                    "type": "function",
                    "function": {
                        "name": def.name,
                        "description": def.description,
                        "parameters": def.parameters,
                    }
                })
            })
            .collect()
    }

    pub async fn execute(&self, name: &str, args: Value) -> Result<String, String> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| format!("tool not found: {name}"))?;
        tool.execute(args).await
    }
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;
    use futures::future::BoxFuture;
    use serde_json::{json, Value};

    use super::ToolRegistry;
    use crate::tools::types::{ToolDef, ToolImpl};

    struct StubTool;

    impl ToolImpl for StubTool {
        fn definition(&self) -> ToolDef {
            ToolDef {
                id: "stub-tool-id".to_string(),
                name: "stub_tool".to_string(),
                description: "A stub tool for testing".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "x": { "type": "string" }
                    },
                    "required": ["x"]
                }),
            }
        }

        fn execute(&self, _args: Value) -> BoxFuture<'_, Result<String, String>> {
            Box::pin(async { Ok("ok".to_string()) })
        }
    }

    #[test]
    fn registry_has_tool_after_register() {
        let mut registry = ToolRegistry::new();
        registry.register(StubTool);

        let tools = registry.get_tools_for_llm();
        assert!(!tools.is_empty());
        assert_eq!(tools[0]["function"]["name"], "stub_tool");
    }

    #[test]
    fn execute_stub_tool_returns_ok() {
        let mut registry = ToolRegistry::new();
        registry.register(StubTool);

        let result = block_on(registry.execute("stub_tool", json!({ "x": "y" })));
        assert_eq!(result, Ok("ok".to_string()));
    }

    #[test]
    fn execute_unknown_tool_returns_err() {
        let registry = ToolRegistry::new();

        let result = block_on(registry.execute("unknown_tool", json!({})));
        assert!(result.is_err());
    }
}

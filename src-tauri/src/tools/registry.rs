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

    pub fn register<T>(&mut self, tool: T) -> Result<(), String>
    where
        T: ToolImpl + 'static,
    {
        let name = tool.definition().name;
        if self.tools.contains_key(&name) {
            return Err(format!(
                "tool already registered: {name}. Existing tool remains unchanged"
            ));
        }

        self.tools.insert(name, Box::new(tool));
        Ok(())
    }

    pub fn get_tools_for_llm(&self) -> Vec<Value> {
        let mut tools: Vec<_> = self.tools.values().collect();
        tools.sort_by_key(|tool| tool.definition().name);

        tools
            .into_iter()
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
    struct AlphaTool;
    struct ZetaTool;

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

    impl ToolImpl for AlphaTool {
        fn definition(&self) -> ToolDef {
            ToolDef {
                id: "alpha-tool-id".to_string(),
                name: "alpha_tool".to_string(),
                description: "Alpha tool for ordering tests".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            }
        }

        fn execute(&self, _args: Value) -> BoxFuture<'_, Result<String, String>> {
            Box::pin(async { Ok("alpha".to_string()) })
        }
    }

    impl ToolImpl for ZetaTool {
        fn definition(&self) -> ToolDef {
            ToolDef {
                id: "zeta-tool-id".to_string(),
                name: "zeta_tool".to_string(),
                description: "Zeta tool for ordering tests".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            }
        }

        fn execute(&self, _args: Value) -> BoxFuture<'_, Result<String, String>> {
            Box::pin(async { Ok("zeta".to_string()) })
        }
    }

    #[test]
    fn registry_has_tool_after_register() {
        let mut registry = ToolRegistry::new();
        registry.register(StubTool).expect("register should succeed");

        let tools = registry.get_tools_for_llm();
        assert!(!tools.is_empty());
        assert_eq!(tools[0]["function"]["name"], "stub_tool");
    }

    #[test]
    fn execute_stub_tool_returns_ok() {
        let mut registry = ToolRegistry::new();
        registry.register(StubTool).expect("register should succeed");

        let result = block_on(registry.execute("stub_tool", json!({ "x": "y" })));
        assert_eq!(result, Ok("ok".to_string()));
    }

    #[test]
    fn register_rejects_duplicate_tool_name() {
        let mut registry = ToolRegistry::new();
        registry.register(StubTool).expect("first register should succeed");

        let result = registry.register(StubTool);
        assert!(result.is_err());
        assert_eq!(
            result,
            Err("tool already registered: stub_tool. Existing tool remains unchanged".to_string())
        );

        let tools = registry.get_tools_for_llm();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["function"]["name"], "stub_tool");
    }

    #[test]
    fn get_tools_for_llm_returns_sorted_by_name() {
        let mut registry = ToolRegistry::new();
        registry.register(ZetaTool).expect("register should succeed");
        registry.register(AlphaTool).expect("register should succeed");

        let tools = registry.get_tools_for_llm();
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0]["function"]["name"], "alpha_tool");
        assert_eq!(tools[1]["function"]["name"], "zeta_tool");
    }

    #[test]
    fn execute_unknown_tool_returns_err() {
        let registry = ToolRegistry::new();

        let result = block_on(registry.execute("unknown_tool", json!({})));
        assert!(result.is_err());
    }
}

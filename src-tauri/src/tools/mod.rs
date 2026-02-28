pub mod registry;
pub mod types;
pub mod web_search;

pub use registry::ToolRegistry;
pub use types::{ToolDef, ToolImpl};
pub use web_search::WebSearchTool;

pub fn register_default_tools(registry: &mut ToolRegistry) -> Result<(), String> {
    registry.register(WebSearchTool::new())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{register_default_tools, ToolRegistry};

    #[test]
    fn register_default_tools_adds_web_search() {
        let mut registry = ToolRegistry::new();
        register_default_tools(&mut registry).expect("default tool registration should succeed");

        let tools = registry.get_tools_for_llm();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["function"]["name"], "web_search");
    }
}

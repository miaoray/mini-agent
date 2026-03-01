pub mod create_directory;
pub mod fetch_url;
pub mod registry;
pub mod types;
pub mod web_search;
pub mod write_file;

pub use create_directory::CreateDirectoryTool;
pub use fetch_url::FetchUrlTool;
pub use registry::ToolRegistry;
pub use types::{ToolDef, ToolImpl};
pub use web_search::WebSearchTool;
pub use write_file::WriteFileTool;

pub fn register_default_tools(registry: &mut ToolRegistry) -> Result<(), String> {
    registry.register(CreateDirectoryTool::new())?;
    registry.register(FetchUrlTool::new())?;
    registry.register(WebSearchTool::new())?;
    registry.register(WriteFileTool::new())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{register_default_tools, ToolRegistry};

    #[test]
    fn register_default_tools_adds_all_tools() {
        let mut registry = ToolRegistry::new();
        register_default_tools(&mut registry).expect("default tool registration should succeed");

        let tools = registry.get_tools_for_llm();
        assert_eq!(tools.len(), 4);
        assert_eq!(tools[0]["function"]["name"], "create_directory");
        assert_eq!(tools[1]["function"]["name"], "fetch_url");
        assert_eq!(tools[2]["function"]["name"], "web_search");
        assert_eq!(tools[3]["function"]["name"], "write_file");
    }
}

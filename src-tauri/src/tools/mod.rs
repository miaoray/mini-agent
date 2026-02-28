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

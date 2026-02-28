use futures::future::BoxFuture;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct ToolDef {
    pub id: String,
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

pub trait ToolImpl: Send + Sync {
    fn definition(&self) -> ToolDef;
    fn execute(&self, args: Value) -> BoxFuture<'_, Result<String, String>>;
}

//! Tool registry for managing available tools.
//!
//! Provides a central registry for all agent tools.

use std::sync::Arc;

use serdes_ai_tools::Tool;

use super::{
    delete_file_tool::DeleteFileTool, edit_file_tool::EditFileTool, grep_tool::GrepTool,
    list_files_tool::ListFilesTool, read_file_tool::ReadFileTool, shell_tool::RunShellCommandTool,
};

/// Registry of all available tools.
#[derive(Clone, Default)]
pub struct ToolRegistry {
    tools: Vec<Arc<dyn Tool>>,
}

impl std::fmt::Debug for ToolRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolRegistry")
            .field("tools", &self.names())
            .finish()
    }
}

impl ToolRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    /// Create a registry with all default tools.
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.register_defaults();
        registry
    }

    /// Register all default tools.
    pub fn register_defaults(&mut self) {
        self.register(ListFilesTool);
        self.register(ReadFileTool);
        self.register(EditFileTool);
        self.register(DeleteFileTool);
        self.register(GrepTool);
        self.register(RunShellCommandTool);
    }

    /// Register a custom tool.
    pub fn register<T: Tool + 'static>(&mut self, tool: T) {
        self.tools.push(Arc::new(tool));
    }

    /// Get all registered tools.
    pub fn tools(&self) -> &[Arc<dyn Tool>] {
        &self.tools
    }

    /// Get a tool by name.
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools
            .iter()
            .find(|t| t.definition().name() == name)
            .cloned()
    }

    /// Get all tool names.
    pub fn names(&self) -> Vec<String> {
        self.tools
            .iter()
            .map(|t| t.definition().name().to_string())
            .collect()
    }

    /// Get the number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_registry() {
        let registry = ToolRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_with_defaults() {
        let registry = ToolRegistry::with_defaults();
        assert!(!registry.is_empty());
        assert_eq!(registry.len(), 6);
    }

    #[test]
    fn test_get_tool_by_name() {
        let registry = ToolRegistry::with_defaults();

        let tool = registry.get("list_files");
        assert!(tool.is_some());
        assert_eq!(tool.unwrap().definition().name(), "list_files");

        let missing = registry.get("nonexistent");
        assert!(missing.is_none());
    }

    #[test]
    fn test_names() {
        let registry = ToolRegistry::with_defaults();
        let names = registry.names();

        assert!(names.contains(&"list_files".to_string()));
        assert!(names.contains(&"read_file".to_string()));
        assert!(names.contains(&"edit_file".to_string()));
        assert!(names.contains(&"delete_file".to_string()));
        assert!(names.contains(&"grep".to_string()));
        assert!(names.contains(&"run_shell_command".to_string()));
    }

    #[test]
    fn test_custom_tool_registration() {
        let mut registry = ToolRegistry::new();
        registry.register(ListFilesTool);

        assert_eq!(registry.len(), 1);
        assert!(registry.get("list_files").is_some());
    }
}

//! Common utilities for tools.
//!
//! Provides helper functions for file detection, path filtering, and JSON parsing.

use serde::de::DeserializeOwned;
use serde_json::Value as JsonValue;
use serdes_ai_tools::ToolError;
use tracing::debug;

// =============================================================================
// Path Filtering
// =============================================================================

/// Directory patterns to ignore.
pub static IGNORE_PATTERNS: &[&str] = &[
    // Version control
    ".git",
    ".svn",
    ".hg",
    // Dependencies
    "node_modules",
    "vendor",
    ".venv",
    "venv",
    "__pycache__",
    // Build outputs
    "target",
    "dist",
    "build",
    ".next",
    ".nuxt",
    // IDE/Editor
    ".idea",
    ".vscode",
    // Cache
    ".cache",
    ".pytest_cache",
    ".mypy_cache",
    // Package managers
    ".npm",
    ".yarn",
    ".pnpm-store",
];

/// Check if a path should be ignored.
pub fn should_ignore(path: &str) -> bool {
    let path_lower = path.to_lowercase();
    IGNORE_PATTERNS
        .iter()
        .any(|pattern| path_lower.contains(pattern))
}

// =============================================================================
// File Type Detection
// =============================================================================

/// Get file extension.
pub fn get_extension(path: &str) -> Option<&str> {
    std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
}

/// Check if path is likely a text file.
pub fn is_text_file(path: &str) -> bool {
    const TEXT_EXTENSIONS: &[&str] = &[
        "txt", "md", "rs", "py", "js", "ts", "tsx", "jsx", "json", "yaml", "yml",
        "toml", "ini", "cfg", "html", "css", "scss", "less", "sh", "bash", "zsh",
        "fish", "c", "h", "cpp", "hpp", "cc", "cxx", "go", "java", "kt", "swift",
        "rb", "php", "sql", "graphql", "proto", "xml", "svg", "dockerfile", "makefile",
    ];

    const TEXT_FILES: &[&str] = &[
        "Makefile", "Dockerfile", "Rakefile", "Gemfile", ".gitignore", ".env",
    ];

    if let Some(ext) = get_extension(path) {
        TEXT_EXTENSIONS.contains(&ext.to_lowercase().as_str())
    } else {
        let name = std::path::Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        TEXT_FILES.contains(&name)
    }
}

// =============================================================================
// JSON Type Coercion
// =============================================================================

/// Recursively coerces JSON values to match the expected types in a JSON Schema.
///
/// LLMs sometimes return "almost correct" JSON with wrong types like:
/// - `"true"` (string) instead of `true` (boolean)
/// - `"42"` (string) instead of `42` (integer)
/// - `"3.14"` (string) instead of `3.14` (number)
///
/// This function walks the schema and coerces mismatched types in `args`.
pub fn coerce_json_types(args: &mut JsonValue, schema: &JsonValue) {
    // Handle object schemas with "properties"
    if let Some(properties) = schema.get("properties").and_then(|p| p.as_object()) {
        if let Some(args_obj) = args.as_object_mut() {
            for (key, prop_schema) in properties {
                if let Some(value) = args_obj.get_mut(key) {
                    coerce_value(value, prop_schema, key);
                }
            }
        }
        return;
    }

    // Handle direct type coercion (when schema is a simple type definition)
    if schema.get("type").and_then(|t| t.as_str()).is_some() {
        coerce_value(args, schema, "root");
    }
}

/// Coerces a single value based on its schema type.
fn coerce_value(value: &mut JsonValue, schema: &JsonValue, field_name: &str) {
    let Some(type_str) = schema.get("type").and_then(|t| t.as_str()) else {
        return;
    };

    match type_str {
        "boolean" => coerce_to_boolean(value, field_name),
        "integer" => coerce_to_integer(value, field_name),
        "number" => coerce_to_number(value, field_name),
        "object" => coerce_json_types(value, schema),
        "array" => {
            if let (Some(items_schema), Some(arr)) = (schema.get("items"), value.as_array_mut()) {
                for item in arr.iter_mut() {
                    coerce_json_types(item, items_schema);
                }
            }
        }
        _ => {}
    }
}

/// Coerces string values to boolean.
fn coerce_to_boolean(value: &mut JsonValue, field_name: &str) {
    if let Some(s) = value.as_str() {
        let coerced = match s.to_lowercase().as_str() {
            "true" | "1" => Some(true),
            "false" | "0" => Some(false),
            _ => None,
        };

        if let Some(b) = coerced {
            debug!(field = field_name, original = s, coerced = b, "Coerced string to boolean");
            *value = JsonValue::Bool(b);
        }
    }
}

/// Coerces string values to integer (i64).
fn coerce_to_integer(value: &mut JsonValue, field_name: &str) {
    if let Some(s) = value.as_str() {
        if let Ok(i) = s.parse::<i64>() {
            debug!(field = field_name, original = s, coerced = i, "Coerced string to integer");
            *value = JsonValue::Number(i.into());
        }
    }
}

/// Coerces string values to number (f64).
fn coerce_to_number(value: &mut JsonValue, field_name: &str) {
    if let Some(s) = value.as_str() {
        if let Ok(f) = s.parse::<f64>() {
            if let Some(n) = serde_json::Number::from_f64(f) {
                debug!(field = field_name, original = s, coerced = f, "Coerced string to number");
                *value = JsonValue::Number(n);
            }
        }
    }
}

/// Parses tool arguments with lenient type coercion.
///
/// First coerces the JSON values to match the schema, then deserializes.
pub fn parse_tool_args_lenient<T: DeserializeOwned>(
    tool_name: &str,
    mut args: JsonValue,
    schema: &JsonValue,
) -> Result<T, ToolError> {
    coerce_json_types(&mut args, schema);

    serde_json::from_value(args.clone()).map_err(|e| {
        ToolError::execution_failed(format!(
            "{}: Invalid arguments: {}. Got: {}",
            tool_name, e, args
        ))
    })
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // should_ignore Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_should_ignore_common_dirs() {
        assert!(should_ignore(".git"));
        assert!(should_ignore("node_modules"));
        assert!(should_ignore("target"));
        assert!(should_ignore("__pycache__"));
        assert!(should_ignore(".venv"));
    }

    #[test]
    fn test_should_ignore_nested() {
        assert!(should_ignore("project/.git/HEAD"));
        assert!(should_ignore("app/node_modules/react"));
        assert!(should_ignore("target/debug/deps"));
    }

    #[test]
    fn test_should_not_ignore_normal_paths() {
        assert!(!should_ignore("src"));
        assert!(!should_ignore("src/main.rs"));
        assert!(!should_ignore("README.md"));
    }

    // -------------------------------------------------------------------------
    // is_text_file Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_is_text_file_common() {
        assert!(is_text_file("main.rs"));
        assert!(is_text_file("script.py"));
        assert!(is_text_file("app.js"));
        assert!(is_text_file("config.json"));
        assert!(is_text_file("README.md"));
    }

    #[test]
    fn test_is_text_file_special() {
        assert!(is_text_file("Makefile"));
        assert!(is_text_file("Dockerfile"));
        assert!(is_text_file(".gitignore"));
    }

    #[test]
    fn test_is_not_text_file_binary() {
        assert!(!is_text_file("image.png"));
        assert!(!is_text_file("archive.zip"));
        assert!(!is_text_file("binary.exe"));
    }

    // -------------------------------------------------------------------------
    // coerce_json_types Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_coerce_boolean_from_string() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "flag": { "type": "boolean" }
            }
        });

        let mut args = serde_json::json!({ "flag": "true" });
        coerce_json_types(&mut args, &schema);
        assert_eq!(args["flag"], serde_json::json!(true));

        let mut args = serde_json::json!({ "flag": "false" });
        coerce_json_types(&mut args, &schema);
        assert_eq!(args["flag"], serde_json::json!(false));
    }

    #[test]
    fn test_coerce_integer_from_string() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "count": { "type": "integer" }
            }
        });

        let mut args = serde_json::json!({ "count": "42" });
        coerce_json_types(&mut args, &schema);
        assert_eq!(args["count"], serde_json::json!(42));
    }

    #[test]
    fn test_coerce_number_from_string() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "value": { "type": "number" }
            }
        });

        let mut args = serde_json::json!({ "value": "3.14" });
        coerce_json_types(&mut args, &schema);
        assert_eq!(args["value"], serde_json::json!(3.14));
    }

    #[test]
    fn test_coerce_multiple_fields() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "recursive": { "type": "boolean" },
                "max_depth": { "type": "integer" }
            }
        });

        let mut args = serde_json::json!({
            "recursive": "true",
            "max_depth": "5"
        });
        coerce_json_types(&mut args, &schema);

        assert_eq!(args["recursive"], serde_json::json!(true));
        assert_eq!(args["max_depth"], serde_json::json!(5));
    }
}

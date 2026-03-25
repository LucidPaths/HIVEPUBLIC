//! File operation tools — read, write, list directory

use super::{HiveTool, RiskLevel, ToolResult};
use serde_json::json;

// ============================================
// read_file
// ============================================

pub struct ReadFileTool;

#[async_trait::async_trait]
impl HiveTool for ReadFileTool {
    fn name(&self) -> &str { "read_file" }

    fn description(&self) -> &str {
        "Read the contents of a file. Supports line-based pagination with offset and limit for large files. If a file is truncated, use offset to continue reading from where it left off."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Absolute or relative file path to read"
                },
                "offset": {
                    "type": "integer",
                    "description": "Line number to start reading from (1-based). Defaults to 1 (beginning of file)."
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of lines to return. Defaults to 500. Use smaller values for large files to stay within context limits."
                }
            },
            "required": ["path"]
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::Low }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let path = params.get("path")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: path")?;

        // Line-based pagination params (1-based offset, like editors)
        let offset = params.get("offset")
            .and_then(|v| v.as_u64())
            .unwrap_or(1)
            .max(1) as usize;

        let default_limit: usize = 500;
        let limit = params.get("limit")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(default_limit);

        match tokio::fs::read_to_string(path).await {
            Ok(content) => {
                let lines: Vec<&str> = content.lines().collect();
                let total_lines = lines.len();

                // Convert 1-based offset to 0-based index
                let start_idx = (offset - 1).min(total_lines);
                let end_idx = (start_idx + limit).min(total_lines);

                // Format with line numbers (like cat -n)
                let numbered: String = lines[start_idx..end_idx]
                    .iter()
                    .enumerate()
                    .map(|(i, line)| format!("{:>6}\t{}", start_idx + i + 1, line))
                    .collect::<Vec<_>>()
                    .join("\n");

                // Build result with metadata
                let mut result = format!(
                    "[{} — {} total lines, showing lines {}-{}]\n{}",
                    path,
                    total_lines,
                    start_idx + 1,
                    end_idx,
                    numbered,
                );

                // If there's more content, tell the model how to get it
                if end_idx < total_lines {
                    let remaining = total_lines - end_idx;
                    result.push_str(&format!(
                        "\n\n[... {} more lines remaining. Use read_file with offset: {} to continue reading]",
                        remaining,
                        end_idx + 1,
                    ));
                }

                Ok(ToolResult {
                    content: result,
                    is_error: false,
                })
            }
            Err(e) => Ok(ToolResult {
                content: format!("Failed to read file '{}': {}", path, e),
                is_error: true,
            }),
        }
    }
}

// ============================================
// write_file
// ============================================

pub struct WriteFileTool;

#[async_trait::async_trait]
impl HiveTool for WriteFileTool {
    fn name(&self) -> &str { "write_file" }

    fn description(&self) -> &str {
        "Write content to a file. Creates the file if it doesn't exist, overwrites if it does."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "File path to write to"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write to the file"
                }
            },
            "required": ["path", "content"]
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::High }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let path = params.get("path")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: path")?;

        let content = params.get("content")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: content")?;

        // Create parent directories if they don't exist
        if let Some(parent) = std::path::Path::new(path).parent() {
            if !parent.exists() {
                if let Err(e) = tokio::fs::create_dir_all(parent).await {
                    return Ok(ToolResult {
                        content: format!("Failed to create directory '{}': {}", parent.display(), e),
                        is_error: true,
                    });
                }
            }
        }

        match tokio::fs::write(path, content).await {
            Ok(()) => Ok(ToolResult {
                content: format!("Successfully wrote {} bytes to '{}'", content.len(), path),
                is_error: false,
            }),
            Err(e) => Ok(ToolResult {
                content: format!("Failed to write file '{}': {}", path, e),
                is_error: true,
            }),
        }
    }
}

// ============================================
// list_directory
// ============================================

pub struct ListDirectoryTool;

#[async_trait::async_trait]
impl HiveTool for ListDirectoryTool {
    fn name(&self) -> &str { "list_directory" }

    fn description(&self) -> &str {
        "List files and directories at the given path. Shows names, sizes, and types."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Directory path to list"
                }
            },
            "required": ["path"]
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::Low }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let path = params.get("path")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: path")?;

        let mut entries = match tokio::fs::read_dir(path).await {
            Ok(entries) => entries,
            Err(e) => return Ok(ToolResult {
                content: format!("Failed to list directory '{}': {}", path, e),
                is_error: true,
            }),
        };

        let mut items = Vec::new();
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name().to_string_lossy().to_string();
            let meta = entry.metadata().await;

            let (kind, size) = match meta {
                Ok(m) => {
                    let kind = if m.is_dir() { "dir" } else { "file" };
                    let size = if m.is_file() { m.len() } else { 0 };
                    (kind, size)
                }
                Err(_) => ("unknown", 0),
            };

            if kind == "file" {
                items.push(format!("  {} ({}, {} bytes)", name, kind, size));
            } else {
                items.push(format!("  {} ({})", name, kind));
            }
        }

        items.sort();

        let result = if items.is_empty() {
            format!("Directory '{}' is empty", path)
        } else {
            format!("Contents of '{}':\n{}", path, items.join("\n"))
        };

        Ok(ToolResult {
            content: result,
            is_error: false,
        })
    }
}

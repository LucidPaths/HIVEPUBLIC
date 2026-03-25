//! Log tools — model self-debugging via HIVE's own runtime logs
//!
//! Enables models to read HIVE's application logs, server logs, and tool audit trail.
//! This is critical for self-debugging: when a tool call fails, the model can check
//! the logs to understand what went wrong and fix the pattern.
//!
//! Tools:
//!   check_logs — read HIVE app logs, server logs, or audit trail with filtering
//!
//! Principle alignment:
//!   P4 (Errors Are Answers) — Model reads its own errors to learn from them.
//!   P3 (Simplicity)         — Plain text log files, tail + grep, no log aggregator.
//!   P5 (Fix The Pattern)    — Model sees recurring errors across sessions.

use super::{HiveTool, RiskLevel, ToolResult};
use crate::paths::get_app_data_dir;
use serde_json::json;
use std::path::PathBuf;

/// Get the app-level log file path (tool audit trail + app events)
pub fn get_app_log_path() -> PathBuf {
    get_app_data_dir().join("hive-app.log")
}

/// Append a line to the app-level log file (called from audit_log and other places).
/// This is the persistent version of eprintln — survives across sessions.
pub fn append_to_app_log(line: &str) {
    let path = get_app_log_path();
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            eprintln!("[HIVE] WARN: Failed to create log directory {}: {}", parent.display(), e);
            return;
        }
    }
    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let entry = format!("[{}] {}\n", timestamp, line);

    use std::io::Write;
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        Ok(mut file) => {
            if let Err(e) = file.write_all(entry.as_bytes()) {
                eprintln!("[HIVE] WARN: Failed to write to app log: {}", e);
            }
        }
        Err(e) => {
            eprintln!("[HIVE] WARN: Failed to open app log {}: {}", path.display(), e);
        }
    }
}

// ============================================
// check_logs
// ============================================

pub struct CheckLogsTool;

#[async_trait::async_trait]
impl HiveTool for CheckLogsTool {
    fn name(&self) -> &str { "check_logs" }

    fn description(&self) -> &str {
        "Read HIVE's own runtime logs for self-debugging. Access application logs \
         (tool audit trail, worker events, errors), server logs (llama.cpp output), \
         or all logs. Filter by keyword or time. Use this when a tool call fails \
         and you need to understand why."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "log_type": {
                    "type": "string",
                    "enum": ["app", "server", "all"],
                    "description": "Which logs to read: 'app' (tool audit + events), 'server' (llama.cpp), 'all' (both)"
                },
                "filter": {
                    "type": "string",
                    "description": "Only show lines containing this text (case-insensitive). E.g., 'ERROR', 'worker', a tool name"
                },
                "tail_lines": {
                    "type": "integer",
                    "description": "Number of lines from the end (default: 50, max: 500)"
                }
            }
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::Low }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let log_type = params.get("log_type")
            .and_then(|v| v.as_str())
            .unwrap_or("app");

        let filter = params.get("filter")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let tail_lines = params.get("tail_lines")
            .and_then(|v| v.as_u64())
            .unwrap_or(50)
            .min(500) as usize;

        let mut output = String::new();

        if log_type == "app" || log_type == "all" {
            let app_log = get_app_log_path();
            output.push_str(&format!("═══ App Log ({}) ═══\n", app_log.display()));
            output.push_str(&read_log_tail(&app_log, tail_lines, filter));
            output.push('\n');
        }

        if log_type == "server" || log_type == "all" {
            let server_log = get_app_data_dir().join("llama-server.log");
            output.push_str(&format!("═══ Server Log ({}) ═══\n", server_log.display()));
            output.push_str(&read_log_tail(&server_log, tail_lines, filter));
            output.push('\n');
        }

        if output.trim().is_empty() || output.contains("(no log file found)") && !output.contains('|') {
            output.push_str("\nNo log entries found. Logs accumulate as HIVE runs — tool calls, errors, and server events are recorded.");
        }

        // Truncate if too long
        let max_chars = 20_000;
        if output.chars().count() > max_chars {
            let truncated: String = output.chars().take(max_chars).collect();
            output = format!("{}\n\n[... truncated at {} chars. Use 'filter' to narrow results.]", truncated, max_chars);
        }

        Ok(ToolResult {
            content: output,
            is_error: false,
        })
    }
}

/// Read the last N lines of a log file, optionally filtering by keyword.
fn read_log_tail(path: &PathBuf, max_lines: usize, filter: &str) -> String {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return "(no log file found)\n".to_string(),
    };

    let filter_lower = filter.to_lowercase();
    let lines: Vec<&str> = if filter.is_empty() {
        content.lines().collect()
    } else {
        content.lines()
            .filter(|line| line.to_lowercase().contains(&filter_lower))
            .collect()
    };

    let start = lines.len().saturating_sub(max_lines);
    let tail = &lines[start..];

    if tail.is_empty() {
        if filter.is_empty() {
            "(empty log)\n".to_string()
        } else {
            format!("(no lines matching '{}')\n", filter)
        }
    } else {
        format!("{}\n({} lines shown)\n", tail.join("\n"), tail.len())
    }
}

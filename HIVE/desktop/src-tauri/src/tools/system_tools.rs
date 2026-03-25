//! System tools — command execution, system info

use super::{HiveTool, RiskLevel, ToolResult};
use serde_json::json;
use std::process::Stdio;
use tokio::process::Command;

// ============================================
// run_command
// ============================================

pub struct RunCommandTool;

#[async_trait::async_trait]
impl HiveTool for RunCommandTool {
    fn name(&self) -> &str { "run_command" }

    fn description(&self) -> &str {
        "Execute a shell command and return its output. Use for running programs, \
         scripts, or system commands. The command runs in the system's default shell."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The shell command to execute"
                },
                "working_directory": {
                    "type": "string",
                    "description": "Optional working directory for the command"
                }
            },
            "required": ["command"]
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::High }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let command = params.get("command")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: command")?;

        let working_dir = params.get("working_directory")
            .and_then(|v| v.as_str());

        // Use platform-appropriate shell
        let (shell, flag) = if cfg!(windows) {
            ("cmd", "/C")
        } else {
            ("sh", "-c")
        };

        let mut cmd = Command::new(shell);
        cmd.arg(flag).arg(command);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }

        // Hide console window on Windows
        #[cfg(windows)]
        {
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        let output = match tokio::time::timeout(
            std::time::Duration::from_secs(30),
            cmd.output(),
        ).await {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => return Ok(ToolResult {
                content: format!("Failed to execute command: {}", e),
                is_error: true,
            }),
            Err(_) => return Ok(ToolResult {
                content: "Command timed out after 30 seconds".to_string(),
                is_error: true,
            }),
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let exit_code = output.status.code().unwrap_or(-1);

        let mut result = String::new();

        if !stdout.is_empty() {
            result.push_str(&stdout);
        }

        if !stderr.is_empty() {
            if !result.is_empty() {
                result.push_str("\n--- stderr ---\n");
            }
            result.push_str(&stderr);
        }

        if result.is_empty() {
            result = format!("Command completed with exit code {}", exit_code);
        } else if exit_code != 0 {
            result.push_str(&format!("\n\nExit code: {}", exit_code));
        }

        // Truncate very long output (char count, not bytes — avoids panic on multi-byte UTF-8)
        let max_chars = 30_000;
        let char_count = result.chars().count();
        if char_count > max_chars {
            let safe_truncated: String = result.chars().take(max_chars).collect();
            result = format!(
                "{}\n\n... [output truncated, showing first {} of {} characters]",
                safe_truncated,
                max_chars,
                char_count
            );
        }

        Ok(ToolResult {
            content: result,
            is_error: exit_code != 0,
        })
    }
}

// ============================================
// system_info
// ============================================

pub struct SystemInfoTool;

#[async_trait::async_trait]
impl HiveTool for SystemInfoTool {
    fn name(&self) -> &str { "system_info" }

    fn description(&self) -> &str {
        "Get information about the system: OS, CPU, memory, disk space, current directory."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::Low }

    async fn execute(&self, _params: serde_json::Value) -> Result<ToolResult, String> {
        let mut info = Vec::new();

        info.push(format!("OS: {}", std::env::consts::OS));
        info.push(format!("Architecture: {}", std::env::consts::ARCH));

        if let Ok(cwd) = std::env::current_dir() {
            info.push(format!("Working directory: {}", cwd.display()));
        }

        if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
            info.push(format!("Home directory: {}", home));
        }

        Ok(ToolResult {
            content: info.join("\n"),
            is_error: false,
        })
    }
}

//! Agent tools — send input to running CLI agents via PTY sessions
//!
//! Lets the chat model communicate with running terminal agents (Claude Code,
//! Codex, Aider, or any CLI tool running in a NEXUS terminal pane).
//!
//! Tools:
//!   send_to_agent — write input to a running PTY session's stdin
//!
//! Principle alignment:
//!   P1 (Modularity) — Self-contained. Remove it → terminal panes still work, just no model→agent bridge.
//!   P2 (Agnostic)   — Works with any CLI agent. Same write pipe for all.
//!   P3 (Simplicity) — Thin wrapper over pty_manager::write_to_session().

use super::{HiveTool, RiskLevel, ToolResult};
use serde_json::json;

// ============================================
// send_to_agent
// ============================================

pub struct SendToAgentTool;

#[async_trait::async_trait]
impl HiveTool for SendToAgentTool {
    fn name(&self) -> &str { "send_to_agent" }

    fn description(&self) -> &str {
        "Send input to a running CLI agent in a NEXUS terminal pane. The input is written directly \
         to the agent's stdin (like typing in the terminal). Use this to delegate tasks to \
         specialized coding agents (Claude Code, Codex, Aider, etc.) or to run shell commands in a \
         persistent terminal session.\n\n\
         IMPORTANT: The input is sent as raw keystrokes. Append '\\n' (newline) to execute a command.\n\n\
         To find available sessions, use the 'list_agents' tool or check the terminal panes visible \
         in the HIVE UI. Each session has a unique session_id."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "The PTY session ID to send input to. Get this from list_agents or the terminal pane."
                },
                "input": {
                    "type": "string",
                    "description": "The text to send to the agent's stdin. Include '\\n' to press Enter."
                }
            },
            "required": ["session_id", "input"]
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::High }  // Writes to PTY stdin — command execution vector

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let session_id = params.get("session_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing required parameter: session_id".to_string())?;

        let input = params.get("input")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing required parameter: input".to_string())?;

        // Write to the PTY session
        crate::pty_manager::write_to_session(session_id, input)?;

        Ok(ToolResult {
            content: format!("Sent {} bytes to agent session {}", input.len(), session_id),
            is_error: false,
        })
    }
}

// ============================================
// read_agent_output
// ============================================

pub struct ReadAgentOutputTool;

#[async_trait::async_trait]
impl HiveTool for ReadAgentOutputTool {
    fn name(&self) -> &str { "read_agent_output" }

    fn description(&self) -> &str {
        "Read recent terminal output from a running CLI agent session. Returns the last N lines \
         of ANSI-stripped clean text from the agent's terminal (e.g., Claude Code's responses, \
         shell command results). Use this for cross-agent visibility — one model can see what \
         another agent has been outputting.\n\n\
         Use 'list_agents' first to find the session_id."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "The PTY session ID to read output from. Get this from list_agents."
                },
                "lines": {
                    "type": "integer",
                    "description": "Number of recent lines to return (default: 50, max: 500)"
                }
            },
            "required": ["session_id"]
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::Low }  // Read-only

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let session_id = params.get("session_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing required parameter: session_id".to_string())?;

        let max_lines = params.get("lines")
            .and_then(|v| v.as_u64())
            .unwrap_or(50)
            .clamp(1, 500) as usize;

        let lines = crate::pty_manager::read_session_output(session_id, max_lines)?;

        if lines.is_empty() {
            return Ok(ToolResult {
                content: format!("Session {} has no output yet.", session_id),
                is_error: false,
            });
        }

        Ok(ToolResult {
            content: format!(
                "{} lines from session {}:\n\n{}",
                lines.len(),
                session_id,
                lines.join("\n")
            ),
            is_error: false,
        })
    }
}

// ============================================
// list_agents
// ============================================

pub struct ListAgentsTool;

#[async_trait::async_trait]
impl HiveTool for ListAgentsTool {
    fn name(&self) -> &str { "list_agents" }

    fn description(&self) -> &str {
        "List all running CLI agent sessions in NEXUS terminal panes. Returns the session ID, \
         command name, and start time for each active session. Use this to find a session_id \
         before calling send_to_agent."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {}
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::Low }

    async fn execute(&self, _params: serde_json::Value) -> Result<ToolResult, String> {
        let sessions = crate::pty_manager::list_sessions_info();

        if sessions.is_empty() {
            return Ok(ToolResult {
                content: "No active agent sessions. Open a terminal pane in HIVE first.".to_string(),
                is_error: false,
            });
        }

        let mut lines = Vec::new();
        lines.push(format!("{} active agent session(s):", sessions.len()));
        for s in &sessions {
            lines.push(format!(
                "  - session_id: {}  command: {}  started: {}",
                s.id, s.command, s.started_at
            ));
        }

        Ok(ToolResult {
            content: lines.join("\n"),
            is_error: false,
        })
    }
}

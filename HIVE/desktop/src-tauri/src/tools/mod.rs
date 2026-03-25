//! HIVE Tool Framework
//!
//! MCP-compatible tool system that lets models execute actions.
//! Tools are plug-and-play: implement the HiveTool trait, register, done.
//!
//! Architecture (16 sub-modules, 44 tools):
//!   mod.rs              — Core trait, registry, types, Tauri commands, MAGMA auto-tracking
//!   file_tools.rs       — read_file, write_file, list_directory
//!   system_tools.rs     — run_command, system_info
//!   web_tools.rs        — web_fetch, web_search, web_extract, read_pdf
//!   memory_tools.rs     — memory_save/search/edit/delete, task_track, graph_query, entity_track, procedure_learn
//!   workspace_tools.rs  — repo_clone, file_tree, code_search
//!   scratchpad_tools.rs — scratchpad_create, scratchpad_write, scratchpad_read
//!   worker_tools.rs     — worker_spawn, worker_status, worker_terminate, report_to_parent
//!   agent_tools.rs      — send_to_agent, list_agents (NEXUS PTY bridge)
//!   specialist_tools.rs — read_agent_context (Cognitive Bus Phase 5B)
//!   telegram_tools.rs   — telegram_send, telegram_get_updates, telegram_bot_info
//!   discord_tools.rs    — discord_send, discord_read
//!   github_tools.rs     — github_issues, github_prs, github_repos
//!   integration_tools.rs — integration_status
//!   log_tools.rs        — check_logs
//!   plan_tools.rs       — plan_execute

mod file_tools;
mod system_tools;
mod web_tools;
mod specialist_tools;
pub mod telegram_tools;
pub mod github_tools;
pub mod discord_tools;
mod integration_tools;
mod memory_tools;
mod plan_tools;
mod workspace_tools;
pub(crate) mod scratchpad_tools;
pub(crate) mod worker_tools;
pub(crate) mod log_tools;
mod agent_tools;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

// ============================================
// Tool Types
// ============================================

/// Risk level determines the approval flow.
/// Low = auto-approve, Medium = prompt once, High/Critical = always prompt.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Low,      // read-only ops
    Medium,   // writes to known locations
    High,     // shell execution, network requests
    Critical, // system changes, destructive ops
}

/// Schema exposed to the model (MCP-compatible).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value, // JSON Schema object
    pub risk_level: RiskLevel,
}

/// Result returned from tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub content: String,   // text result for the model
    pub is_error: bool,
}

/// A tool call parsed from model output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,        // unique call ID
    pub name: String,      // tool name
    pub arguments: serde_json::Value, // parsed arguments
}

// ============================================
// Tool Trait
// ============================================

/// Implement this trait to add a new tool to HIVE.
/// Each tool is self-describing (name, description, JSON Schema params)
/// and self-executing (the execute method).
#[async_trait::async_trait]
pub trait HiveTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> serde_json::Value;
    fn risk_level(&self) -> RiskLevel;

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String>;
}

// ============================================
// Tool Registry
// ============================================

/// Central registry for all available tools.
/// Tools register once at startup; the registry is shared via Tauri state.
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn HiveTool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool. Overwrites if name already exists.
    pub fn register(&mut self, tool: impl HiveTool + 'static) {
        self.tools.insert(tool.name().to_string(), Arc::new(tool));
    }

    /// Unregister a tool by name. Used by MCP client when disconnecting servers.
    pub fn unregister(&mut self, name: &str) {
        self.tools.remove(name);
    }

    /// Get schemas for all registered tools (sent to the model).
    /// Sorted by name for deterministic ordering — enables prompt caching
    /// (tool definitions are static prefixes, cache key must be stable across turns).
    pub fn schemas(&self) -> Vec<ToolSchema> {
        let mut schemas: Vec<ToolSchema> = self.tools
            .values()
            .map(|t| ToolSchema {
                name: t.name().to_string(),
                description: t.description().to_string(),
                parameters: t.parameters_schema(),
                risk_level: t.risk_level(),
            })
            .collect();
        schemas.sort_by(|a, b| a.name.cmp(&b.name));
        schemas
    }

    /// Execute a tool by name with given parameters. Includes audit logging + MAGMA entity tracking.
    ///
    /// P4 (Errors Are Answers): Tool errors are ALWAYS returned as ToolResult { is_error: true },
    /// never as Err(String). This ensures the model sees the error and can retry or adjust,
    /// rather than crashing the entire sendMessage flow.
    pub async fn execute(&self, name: &str, params: serde_json::Value) -> Result<ToolResult, String> {
        let tool = match self.tools.get(name) {
            Some(t) => t,
            None => return Ok(ToolResult {
                content: format!("Unknown tool: '{}'. Check tool name spelling.", name),
                is_error: true,
            }),
        };
        // Convert any Err from tool.execute() into ToolResult { is_error: true }
        // so the model always gets the error as a tool result it can react to.
        let result = match tool.execute(params.clone()).await {
            Ok(r) => r,
            Err(e) => ToolResult {
                content: format!("Tool '{}' error: {}", name, e),
                is_error: true,
            },
        };
        // Audit log every tool execution
        crate::content_security::audit_log_tool_call(
            name,
            &params,
            !result.is_error,
        );
        // MAGMA entity auto-tracking: upsert entities for file/model operations.
        // This builds the entity graph passively — the model doesn't need to do anything.
        if !result.is_error {
            magma_auto_track_entity(name, &params);
        }
        Ok(result)
    }
}

/// Build the default registry with all built-in tools.
pub fn create_default_registry() -> ToolRegistry {
    let mut registry = ToolRegistry::new();

    // File tools
    registry.register(file_tools::ReadFileTool);
    registry.register(file_tools::WriteFileTool);
    registry.register(file_tools::ListDirectoryTool);

    // System tools
    registry.register(system_tools::RunCommandTool);
    registry.register(system_tools::SystemInfoTool);

    // Web tools
    registry.register(web_tools::WebFetchTool);
    registry.register(web_tools::WebSearchTool);
    registry.register(web_tools::WebExtractTool);
    registry.register(web_tools::ReadPdfTool);

    // Specialist routing (Phase 4) + agent context (Phase 5B)
    registry.register(specialist_tools::RouteToSpecialistTool);
    registry.register(specialist_tools::ReadAgentContextTool);

    // Integration tools — "doors and keys" pattern
    // These always register (they return helpful errors if no key is configured)
    // Telegram tools
    registry.register(telegram_tools::TelegramSendTool);
    registry.register(telegram_tools::TelegramGetUpdatesTool);
    registry.register(telegram_tools::TelegramBotInfoTool);
    // GitHub tools
    registry.register(github_tools::GitHubIssuesTool);
    registry.register(github_tools::GitHubPRsTool);
    registry.register(github_tools::GitHubReposTool);
    // Discord tools
    registry.register(discord_tools::DiscordSendTool);
    registry.register(discord_tools::DiscordReadTool);
    // Integration introspection
    registry.register(integration_tools::IntegrationStatusTool);
    registry.register(integration_tools::ListToolsTool);
    // Memory tools — full model agency over memory (Phase 3.5)
    registry.register(memory_tools::MemorySaveTool);
    registry.register(memory_tools::MemorySearchTool);
    registry.register(memory_tools::MemoryEditTool);
    registry.register(memory_tools::MemoryDeleteTool);
    // Task tracking (Phase 3.5.6 — cross-session task persistence)
    registry.register(memory_tools::TaskTrackTool);
    // MAGMA graph tools — model agency over the knowledge graph (Phase 4)
    registry.register(memory_tools::GraphQueryTool);
    registry.register(memory_tools::EntityTrackTool);
    registry.register(memory_tools::ProcedureLearnTool);
    // RAG file ingestion (Phase 9.3) — model can import docs into knowledge base
    registry.register(memory_tools::MemoryImportFileTool);
    // Plan execution (Phase 7 — structured multi-step tool chaining)
    registry.register(plan_tools::PlanExecuteTool);

    // Workspace tools — repo navigation (Phase 8 — autonomous research)
    registry.register(workspace_tools::RepoCloneTool);
    registry.register(workspace_tools::FileTreeTool);
    registry.register(workspace_tools::CodeSearchTool);
    // Scratchpad tools — inter-tool coordination (Phase 8)
    registry.register(scratchpad_tools::ScratchpadCreateTool);
    registry.register(scratchpad_tools::ScratchpadWriteTool);
    registry.register(scratchpad_tools::ScratchpadReadTool);
    // Worker tools — autonomous sub-agents (Phase 8)
    registry.register(worker_tools::WorkerSpawnTool);
    registry.register(worker_tools::WorkerStatusTool);
    registry.register(worker_tools::WorkerTerminateTool);
    registry.register(worker_tools::WorkerReportTool);
    // Log tools — model self-debugging (Phase 8)
    registry.register(log_tools::CheckLogsTool);
    // Agent tools — NEXUS model→agent bridge (Phase 10)
    registry.register(agent_tools::SendToAgentTool);
    registry.register(agent_tools::ReadAgentOutputTool);
    registry.register(agent_tools::ListAgentsTool);

    registry
}

/// Tauri-managed state wrapper
pub struct ToolState {
    pub registry: RwLock<ToolRegistry>,
}

impl Default for ToolState {
    fn default() -> Self {
        Self {
            registry: RwLock::new(create_default_registry()),
        }
    }
}

// ============================================
// Tauri Commands
// ============================================

/// Get all available tool schemas (for sending to the model).
#[tauri::command]
pub async fn get_available_tools(
    tool_state: tauri::State<'_, ToolState>,
) -> Result<Vec<ToolSchema>, String> {
    let registry = tool_state.registry.read().await;
    Ok(registry.schemas())
}

/// Execute a specific tool (called after model requests it and user approves).
#[tauri::command]
pub async fn execute_tool(
    tool_state: tauri::State<'_, ToolState>,
    name: String,
    arguments: serde_json::Value,
) -> Result<ToolResult, String> {
    let registry = tool_state.registry.read().await;
    registry.execute(&name, arguments).await
}

/// Append a line to the persistent app log from the frontend.
/// Used for structured tool lifecycle logging (P4: Errors Are Answers).
#[tauri::command]
pub fn log_to_app(line: String) {
    log_tools::append_to_app_log(&line);
}

/// Get all worker statuses (polled by WorkerPanel in the frontend).
#[tauri::command]
pub async fn get_worker_statuses() -> Result<Vec<serde_json::Value>, String> {
    worker_tools::get_all_worker_statuses().await
}

/// Phase 5C: Write an activity entry to the context bus.
/// Called by the frontend after tool loop completion or specialist routing.
#[tauri::command]
pub async fn context_bus_write(agent: String, content: String) -> Result<(), String> {
    scratchpad_tools::context_bus_write(&agent, &content).await;
    Ok(())
}

/// Phase 5C: Read a compact summary of the context bus for volatile context injection.
#[tauri::command]
pub async fn context_bus_summary() -> Result<String, String> {
    Ok(scratchpad_tools::context_bus_summary().await)
}

// ============================================
// MAGMA Entity Auto-Tracking
// ============================================

/// Passively upsert entities in the MAGMA graph when tools execute successfully.
/// This builds the entity graph without the model needing to do anything explicit.
/// Fire-and-forget: errors are silently ignored (P4 — never break tool execution).
fn magma_auto_track_entity(tool_name: &str, params: &serde_json::Value) {
    let (entity_type, entity_name, state) = match tool_name {
        "write_file" => {
            let path = params.get("path").and_then(|v| v.as_str()).unwrap_or_default();
            if path.is_empty() { return; }
            ("file", path.to_string(), serde_json::json!({"last_action": "write"}))
        },
        "read_file" => {
            let path = params.get("path").and_then(|v| v.as_str()).unwrap_or_default();
            if path.is_empty() { return; }
            ("file", path.to_string(), serde_json::json!({"last_action": "read"}))
        },
        "run_command" => {
            let cmd = params.get("command").and_then(|v| v.as_str()).unwrap_or_default();
            if cmd.is_empty() { return; }
            let name: String = cmd.chars().take(80).collect();
            ("command", name, serde_json::json!({"last_action": "execute"}))
        },
        "web_fetch" => {
            let url = params.get("url").and_then(|v| v.as_str()).unwrap_or_default();
            if url.is_empty() { return; }
            ("url", url.to_string(), serde_json::json!({"last_action": "fetch"}))
        },
        "memory_save" => {
            let tags = params.get("tags").and_then(|v| v.as_array());
            let topic = tags
                .and_then(|arr| arr.first())
                .and_then(|v| v.as_str())
                .unwrap_or("untagged");
            ("memory_topic", topic.to_string(), serde_json::json!({"last_action": "save"}))
        },
        _ => return, // Don't track other tools
    };

    // Fire-and-forget: open DB, upsert entity. Errors are silently ignored
    // because entity tracking must never break tool execution (P4).
    let db_path = crate::paths::get_app_data_dir().join("memory.db");
    let conn = match rusqlite::Connection::open(&db_path) {
        Ok(c) => c,
        Err(_) => return,
    };
    if let Err(e) = conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000; PRAGMA foreign_keys=ON;") {
        eprintln!("[HIVE] MAGMA auto-track: PRAGMA setup failed: {}", e);
    }

    let now = chrono::Utc::now().to_rfc3339();

    // Check if entity already exists
    let existing: Option<String> = conn.query_row(
        "SELECT id FROM entities WHERE entity_type = ?1 AND name = ?2",
        rusqlite::params![entity_type, entity_name],
        |row| row.get(0),
    ).ok();

    if let Some(existing_id) = existing {
        let _ = conn.execute(
            "UPDATE entities SET state = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![state.to_string(), now, existing_id],
        );
    } else {
        let id = format!("{}_{}", entity_type, entity_name.replace(['/', '\\', ' ', ':'], "_"));
        // Truncate ID to 128 chars to prevent bloat
        let id: String = id.chars().take(128).collect();
        let _ = conn.execute(
            "INSERT INTO entities (id, entity_type, name, state, metadata, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, '{}', ?5, ?6)",
            rusqlite::params![id, entity_type, entity_name, state.to_string(), now, now],
        );
    }
}

// ============================================
// Tests (Phase 3B — Tool Registry Integration)
// ============================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_default_has_expected_tool_count() {
        let registry = create_default_registry();
        let schemas = registry.schemas();
        // Smoke test: tools didn't silently disappear. Update count when adding/removing tools.
        assert_eq!(schemas.len(), 45,
            "Expected exactly 45 built-in tools, got {}. Update this count if you added/removed tools.", schemas.len());
    }

    #[test]
    fn registry_schemas_are_sorted_by_name() {
        let registry = create_default_registry();
        let schemas = registry.schemas();
        for window in schemas.windows(2) {
            assert!(window[0].name <= window[1].name,
                "Schemas not sorted: '{}' should come before '{}'", window[0].name, window[1].name);
        }
    }

    #[test]
    fn registry_schemas_have_valid_structure() {
        let registry = create_default_registry();
        for schema in registry.schemas() {
            assert!(!schema.name.is_empty(), "Tool name must not be empty");
            assert!(!schema.description.is_empty(),
                "Tool '{}' has empty description", schema.name);
            // Parameters must be a JSON object with "type": "object"
            assert_eq!(schema.parameters.get("type").and_then(|v| v.as_str()), Some("object"),
                "Tool '{}' parameters must have type:object", schema.name);
        }
    }

    #[test]
    fn registry_contains_core_tools() {
        let registry = create_default_registry();
        let names: Vec<String> = registry.schemas().iter().map(|s| s.name.clone()).collect();
        let expected = [
            "read_file", "write_file", "list_directory",
            "run_command", "system_info",
            "web_fetch", "web_search", "web_extract",
            "memory_save", "memory_search", "memory_edit", "memory_delete",
            "telegram_send", "discord_send",
            "worker_spawn", "worker_status", "worker_terminate",
            "scratchpad_create", "scratchpad_write", "scratchpad_read",
            "graph_query", "entity_track", "procedure_learn",
            "plan_execute", "check_logs",
            "route_to_specialist", "integration_status",
        ];
        for tool in expected {
            assert!(names.contains(&tool.to_string()),
                "Core tool '{}' missing from registry", tool);
        }
    }

    #[test]
    fn registry_risk_levels_are_set() {
        let registry = create_default_registry();
        for schema in registry.schemas() {
            // Risk level should be meaningful — critical tools are gated
            match schema.name.as_str() {
                "run_command" | "write_file" => {
                    assert!(matches!(schema.risk_level, RiskLevel::High | RiskLevel::Critical),
                        "Tool '{}' should be high/critical risk", schema.name);
                }
                "read_file" | "list_directory" => {
                    assert!(matches!(schema.risk_level, RiskLevel::Low),
                        "Tool '{}' should be low risk", schema.name);
                }
                _ => {} // Other tools — just verify the field exists (it always does by type)
            }
        }
    }

    #[tokio::test]
    async fn registry_execute_unknown_tool_returns_error() {
        let registry = create_default_registry();
        let result = registry.execute("nonexistent_tool_xyz", serde_json::json!({})).await;
        // Should NOT return Err — should return Ok(ToolResult { is_error: true }) per P4
        let tool_result = result.expect("execute should always return Ok, even for unknown tools");
        assert!(tool_result.is_error);
        assert!(tool_result.content.contains("Unknown tool"),
            "Error should mention unknown tool: {}", tool_result.content);
    }

    #[test]
    fn registry_register_and_unregister() {
        let mut registry = ToolRegistry::new();
        assert_eq!(registry.schemas().len(), 0);

        // Register a custom tool
        struct TestTool;
        #[async_trait::async_trait]
        impl HiveTool for TestTool {
            fn name(&self) -> &str { "test_tool" }
            fn description(&self) -> &str { "A test tool" }
            fn parameters_schema(&self) -> serde_json::Value {
                serde_json::json!({"type": "object", "properties": {}})
            }
            fn risk_level(&self) -> RiskLevel { RiskLevel::Low }
            async fn execute(&self, _params: serde_json::Value) -> Result<ToolResult, String> {
                Ok(ToolResult { content: "test result".to_string(), is_error: false })
            }
        }

        registry.register(TestTool);
        assert_eq!(registry.schemas().len(), 1);
        assert_eq!(registry.schemas()[0].name, "test_tool");

        registry.unregister("test_tool");
        assert_eq!(registry.schemas().len(), 0);
    }

    #[test]
    fn registry_register_overwrites_existing() {
        let mut registry = ToolRegistry::new();

        struct ToolV1;
        #[async_trait::async_trait]
        impl HiveTool for ToolV1 {
            fn name(&self) -> &str { "my_tool" }
            fn description(&self) -> &str { "Version 1" }
            fn parameters_schema(&self) -> serde_json::Value {
                serde_json::json!({"type": "object", "properties": {}})
            }
            fn risk_level(&self) -> RiskLevel { RiskLevel::Low }
            async fn execute(&self, _params: serde_json::Value) -> Result<ToolResult, String> {
                Ok(ToolResult { content: "v1".to_string(), is_error: false })
            }
        }

        struct ToolV2;
        #[async_trait::async_trait]
        impl HiveTool for ToolV2 {
            fn name(&self) -> &str { "my_tool" }
            fn description(&self) -> &str { "Version 2" }
            fn parameters_schema(&self) -> serde_json::Value {
                serde_json::json!({"type": "object", "properties": {}})
            }
            fn risk_level(&self) -> RiskLevel { RiskLevel::Medium }
            async fn execute(&self, _params: serde_json::Value) -> Result<ToolResult, String> {
                Ok(ToolResult { content: "v2".to_string(), is_error: false })
            }
        }

        registry.register(ToolV1);
        registry.register(ToolV2);
        assert_eq!(registry.schemas().len(), 1, "overwrite should not increase count");
        assert_eq!(registry.schemas()[0].description, "Version 2");
    }

    #[test]
    fn registry_unregister_nonexistent_is_silent() {
        let mut registry = ToolRegistry::new();
        registry.unregister("does_not_exist"); // Should not panic
    }

    #[test]
    fn registry_contains_phase5b_read_agent_context() {
        let registry = create_default_registry();
        let schemas = registry.schemas();
        let tool = schemas.iter().find(|s| s.name == "read_agent_context");
        assert!(tool.is_some(), "read_agent_context should be registered (Phase 5B)");
        let tool = tool.unwrap();
        assert_eq!(tool.risk_level, RiskLevel::Low,
            "read_agent_context is read-only, should be Low risk");
        assert!(tool.description.contains("agent"),
            "Description should mention agents");
        // Verify required parameter
        let required = tool.parameters.get("required")
            .and_then(|r| r.as_array())
            .expect("Should have required params");
        assert!(required.iter().any(|v| v.as_str() == Some("agent_id")),
            "agent_id should be a required parameter");
    }

    #[test]
    fn registry_contains_read_agent_output() {
        let registry = create_default_registry();
        let schemas = registry.schemas();
        let tool = schemas.iter().find(|s| s.name == "read_agent_output");
        assert!(tool.is_some(), "read_agent_output should be registered");
        let tool = tool.unwrap();
        assert_eq!(tool.risk_level, RiskLevel::Low,
            "read_agent_output is read-only, should be Low risk");
        let required = tool.parameters.get("required")
            .and_then(|r| r.as_array())
            .expect("Should have required params");
        assert!(required.iter().any(|v| v.as_str() == Some("session_id")),
            "session_id should be a required parameter");
    }
}

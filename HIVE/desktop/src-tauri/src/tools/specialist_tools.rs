//! Specialist routing + agent context tools (Phase 4 + Phase 5B).
//!
//! Tools:
//!   route_to_specialist — delegates sub-tasks to specialist agents
//!   read_agent_context  — query another agent's recent MAGMA events, scratchpads, state
//!
//! The server lifecycle (start/stop/VRAM) is managed by the TypeScript layer
//! via Tauri commands. These tools are just the messengers. P3: Simplicity Wins.

use super::{HiveTool, RiskLevel, ToolResult};
use crate::server::port_for_slot;
use rusqlite::params;
use serde_json::json;

pub struct RouteToSpecialistTool;

#[async_trait::async_trait]
impl HiveTool for RouteToSpecialistTool {
    fn name(&self) -> &str { "route_to_specialist" }

    fn description(&self) -> &str {
        "Route a sub-task to a specialist AI agent. Use when the task requires \
         specialized capabilities beyond general conversation:\n\
         - 'coder': Code generation, debugging, architecture, refactoring\n\
         - 'terminal': Shell command execution, file operations, system tasks\n\
         - 'webcrawl': Web research, documentation lookup, data gathering\n\
         - 'toolcall': Complex API interactions, multi-tool orchestration\n\n\
         The specialist runs as a separate model and may take a moment to load. \
         Provide a clear, self-contained task description — the specialist does NOT \
         see your conversation history, only the task you send."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "specialist": {
                    "type": "string",
                    "enum": ["coder", "terminal", "webcrawl", "toolcall"],
                    "description": "Which specialist to route to"
                },
                "task": {
                    "type": "string",
                    "description": "Clear, self-contained task description for the specialist"
                },
                "context": {
                    "type": "string",
                    "description": "Optional additional context (code snippets, file contents, requirements)"
                },
                "temperature": {
                    "type": "number",
                    "description": "Sampling temperature (0.0-2.0). Default: 0.7"
                },
                "max_tokens": {
                    "type": "integer",
                    "description": "Maximum tokens in response. Default: 4096"
                }
            },
            "required": ["specialist", "task"]
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::Medium }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let specialist = params.get("specialist")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: specialist")?;

        let task = params.get("task")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: task")?;

        let context = params.get("context")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let temperature = params.get("temperature")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.7);

        let max_tokens = params.get("max_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(4096);

        // Port assignment — single source of truth in server.rs (P5: Fix The Pattern)
        let valid_specialists = ["coder", "terminal", "webcrawl", "toolcall"];
        if !valid_specialists.contains(&specialist) {
            return Ok(ToolResult {
                content: format!("Unknown specialist: '{}'. Valid options: {}", specialist, valid_specialists.join(", ")),
                is_error: true,
            });
        }
        let port = port_for_slot(specialist);

        // Check if specialist server is alive
        let health_url = format!("http://127.0.0.1:{}/health", port);
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(2))
            .build()
            .map_err(|e| format!("HTTP client error: {}", e))?;

        match client.get(&health_url).send().await {
            Ok(resp) if resp.status().is_success() => {
                // Server is running — send the task
            }
            _ => {
                // Server not running — return actionable message.
                // The TS layer should intercept this and start the server.
                return Ok(ToolResult {
                    content: format!(
                        "SPECIALIST_NOT_LOADED: {} (port {}). \
                         The {} specialist server is not running. \
                         It needs to be started before routing tasks to it.",
                        specialist, port, specialist
                    ),
                    is_error: true,
                });
            }
        }

        // Build the chat request
        let mut user_content = task.to_string();
        if !context.is_empty() {
            user_content = format!("{}\n\n## Context\n{}", task, context);
        }

        // Phase 5A: Inject full HIVE identity + specialist role + MAGMA wake briefing.
        // The specialist gets the same identity as consciousness — it IS part of HIVE,
        // not a generic assistant. Wake context provides MAGMA continuity (P2, P4, P7).
        let system_content = {
            let identity = crate::harness::read_identity();
            let specialist_role = format!(
                "\n\n## Specialist Role: {}\n\
                 You are operating as the {} specialist in the HIVE orchestration harness. \
                 The consciousness model has routed this task to you for your domain expertise. \
                 Complete the task given to you. Be thorough and precise. \
                 Return your full response — it will be relayed back to the \
                 orchestrating consciousness layer.",
                specialist, specialist
            );
            let base = format!("{}{}", identity, specialist_role);
            // Try to get wake context from orchestrator (MAGMA briefing)
            let wake = crate::orchestrator::build_wake_context_for_tool(specialist, task);
            if wake.is_empty() { base } else { format!("{}\n\n{}", base, wake) }
        };
        crate::tools::log_tools::append_to_app_log(&format!(
            "SPECIALIST | harness_injected | specialist={} | identity_chars={}",
            specialist, system_content.len()
        ));

        let chat_body = json!({
            "model": "specialist",
            "messages": [
                {
                    "role": "system",
                    "content": system_content
                },
                {
                    "role": "user",
                    "content": user_content
                }
            ],
            "temperature": temperature,
            "max_tokens": max_tokens,
            "stream": false
        });

        let chat_url = format!("http://127.0.0.1:{}/v1/chat/completions", port);

        let response = match client
            .post(&chat_url)
            .json(&chat_body)
            .timeout(std::time::Duration::from_secs(120)) // specialists can take a while
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => return Ok(ToolResult {
                content: format!("Failed to communicate with {} specialist: {}", specialist, e),
                is_error: true,
            }),
        };

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Ok(ToolResult {
                content: format!("Specialist {} returned error {}: {}", specialist, status, body),
                is_error: true,
            });
        }

        // Parse OpenAI-compatible response
        let body: serde_json::Value = response.json().await
            .map_err(|e| format!("Failed to parse specialist response: {}", e))?;

        let content = body
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or("[No response from specialist]");

        // Phase 5C: Write specialist completion to context bus (fire-and-forget)
        let bus_agent = specialist.to_string();
        let bus_content = format!("Completed task ({} chars)", content.len());
        tokio::spawn(async move {
            super::scratchpad_tools::context_bus_write(&bus_agent, &bus_content).await;
        });

        Ok(ToolResult {
            content: format!("[{} specialist response]\n\n{}", specialist, content),
            is_error: false,
        })
    }
}

// ============================================
// Phase 5B: read_agent_context — cross-agent visibility
// ============================================
//
// Lets any model query another agent's recent activity. This is the missing
// piece for the Cognitive Bus — consciousness can see what specialists are
// doing, workers can see each other's progress.
//
// Data sources:
//   1. MAGMA events filtered by agent name
//   2. Active scratchpads (in-memory)
//   3. Working memory (if querying consciousness)
//   4. Worker status (if querying a worker ID)

pub struct ReadAgentContextTool;

#[async_trait::async_trait]
impl HiveTool for ReadAgentContextTool {
    fn name(&self) -> &str { "read_agent_context" }

    fn description(&self) -> &str {
        "Read another agent's recent activity and context. Returns MAGMA events, \
         scratchpad entries, and status for the specified agent. Use this to understand \
         what a specialist, worker, or the consciousness model has been doing recently.\n\n\
         Agent IDs: 'consciousness' (main model), 'coder', 'terminal', 'webcrawl', \
         'toolcall', or a worker ID (e.g., 'w-abc123')."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "agent_id": {
                    "type": "string",
                    "description": "Agent to query — 'consciousness', 'coder', 'terminal', 'webcrawl', 'toolcall', or a worker ID"
                },
                "since_minutes": {
                    "type": "integer",
                    "description": "How far back to look in minutes (default: 30, max: 1440 = 24h)"
                }
            },
            "required": ["agent_id"]
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::Low }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let agent_id = params.get("agent_id")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: agent_id")?;

        let since_minutes = params.get("since_minutes")
            .and_then(|v| v.as_i64())
            .unwrap_or(30)
            .clamp(1, 1440);

        let since = chrono::Utc::now() - chrono::Duration::minutes(since_minutes);
        let since_str = since.to_rfc3339();

        let mut output = format!("## Agent Context: {} (last {} min)\n\n", agent_id, since_minutes);

        // --- 1. MAGMA events for this agent ---
        let db_path = crate::paths::get_app_data_dir().join("memory.db");
        match rusqlite::Connection::open(&db_path) {
            Ok(conn) => {
                let _ = conn.execute_batch(
                    "PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000; PRAGMA foreign_keys=ON;"
                );

                // Query events
                match conn.prepare(
                    "SELECT event_type, content, created_at FROM events \
                     WHERE agent = ?1 AND created_at > ?2 \
                     ORDER BY created_at DESC LIMIT 20"
                ) {
                    Ok(mut stmt) => {
                        let events: Vec<(String, String, String)> = match stmt.query_map(
                            params![agent_id, since_str],
                            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                        ) {
                            Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
                            Err(_) => Vec::new(),
                        };

                        if events.is_empty() {
                            output.push_str("### MAGMA Events\nNo recent events.\n\n");
                        } else {
                            output.push_str(&format!("### MAGMA Events ({} recent)\n", events.len()));
                            for (etype, content, ts) in &events {
                                let short: String = content.chars().take(200).collect();
                                output.push_str(&format!("- [{}] {}: {}\n", ts, etype, short));
                            }
                            output.push('\n');
                        }

                        // Also check for entities modified since last wake
                        if let Ok(mut entity_stmt) = conn.prepare(
                            "SELECT name, entity_type, updated_at FROM entities \
                             WHERE updated_at > ?1 \
                             ORDER BY updated_at DESC LIMIT 10"
                        ) {
                            let entities: Vec<(String, String, String)> = match entity_stmt.query_map(
                                params![since_str],
                                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                            ) {
                                Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
                                Err(_) => Vec::new(),
                            };

                            if !entities.is_empty() {
                                output.push_str(&format!("### Recent Entities ({} modified)\n", entities.len()));
                                for (name, etype, ts) in &entities {
                                    output.push_str(&format!("- {} ({}) — updated {}\n", name, etype, ts));
                                }
                                output.push('\n');
                            }
                        }
                    }
                    Err(e) => {
                        output.push_str(&format!("### MAGMA Events\nQuery failed: {}\n\n", e));
                    }
                }
            }
            Err(e) => {
                output.push_str(&format!("### MAGMA Events\nDB unavailable: {}\n\n", e));
            }
        }

        // --- 2. Active scratchpads ---
        let pads = super::scratchpad_tools::list_scratchpads_summary().await;
        if !pads.is_empty() {
            output.push_str(&format!("### Active Scratchpads ({} total)\n", pads.len()));
            for (pad_id, sections, entry_count) in &pads {
                output.push_str(&format!(
                    "- '{}': {} sections ({}), {} entries\n",
                    pad_id, sections.len(), sections.join(", "), entry_count
                ));
            }
            output.push('\n');
        }

        // --- 3. Working memory (consciousness only) ---
        if agent_id == "consciousness" || agent_id == "main" {
            match crate::working_memory::working_memory_read() {
                Ok(wm) if !wm.is_empty() => {
                    let preview: String = wm.chars().take(500).collect();
                    let suffix = if wm.len() > 500 { "..." } else { "" };
                    output.push_str(&format!("### Working Memory\n{}{}\n\n", preview, suffix));
                }
                _ => {
                    output.push_str("### Working Memory\n(empty)\n\n");
                }
            }
        }

        // --- 4. Worker status (if querying a worker ID) ---
        if let Some(summary) = super::worker_tools::get_worker_summary(agent_id).await {
            output.push_str(&format!("### Worker Status\n{}\n\n", summary));
        } else {
            // Also list any active workers for general awareness
            let active = super::worker_tools::list_active_workers_summary().await;
            if !active.is_empty() {
                output.push_str(&format!("### Active Workers ({} running)\n", active.len()));
                for (wid, summary) in &active {
                    output.push_str(&format!("- {}: {}\n", wid, summary));
                }
                output.push('\n');
            }
        }

        Ok(ToolResult {
            content: output,
            is_error: false,
        })
    }
}

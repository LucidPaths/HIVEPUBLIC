//! Integration & introspection tools — let models discover their own capabilities.
//!
//! IntegrationStatusTool: "Can I send Telegram messages?" "Is Discord configured?"
//! ListToolsTool: "What tools do I have?" — the model can introspect its own toolset.
//!
//! These tools check API key presence and registry state. P3: Simplicity Wins.

use super::{HiveTool, RiskLevel, ToolResult};
use serde_json::json;

fn has_key(provider: &str) -> bool {
    crate::security::get_api_key_internal(provider).is_some()
}

// ============================================
// IntegrationStatusTool
// ============================================

pub struct IntegrationStatusTool;

#[async_trait::async_trait]
impl HiveTool for IntegrationStatusTool {
    fn name(&self) -> &str { "integration_status" }

    fn description(&self) -> &str {
        "Check which integrations (Telegram, Discord, GitHub) are configured and reachable. \
         Use this at the start of a session to discover your capabilities, or when a user \
         asks about available channels. Returns configuration status and connectivity for \
         each integration."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::Medium }

    async fn execute(&self, _params: serde_json::Value) -> Result<ToolResult, String> {
        let client = reqwest::Client::builder()
            .user_agent("HIVE-Desktop/1.0")
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .map_err(|e| format!("HTTP client error: {}", e))?;

        let mut lines = Vec::new();
        lines.push("=== HIVE Integration Status ===".to_string());

        // --- Telegram ---
        if let Some(token) = crate::security::get_api_key_internal("telegram") {
            let url = format!("https://api.telegram.org/bot{}/getMe", token);
            match client.get(&url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    // P6: Don't leak bot identity to models — only confirm connectivity
                    let _ = resp.bytes().await; // consume response body
                    lines.push(
                        "\nTelegram: CONNECTED\n  Bot authenticated successfully\n  Tools: telegram_send, telegram_get_updates, telegram_bot_info\n  Note: Use telegram_get_updates to discover active chat IDs".to_string()
                    );
                }
                Ok(resp) => {
                    lines.push(format!(
                        "\nTelegram: KEY_INVALID (HTTP {})\n  Token is configured but rejected by Telegram API",
                        resp.status()
                    ));
                }
                Err(e) => {
                    lines.push(format!(
                        "\nTelegram: UNREACHABLE\n  Key configured but API call failed: {}",
                        e
                    ));
                }
            }
        } else {
            lines.push("\nTelegram: NOT_CONFIGURED\n  No bot token set. User can add one in Settings → Integrations.".to_string());
        }

        // --- Discord ---
        if let Some(token) = crate::security::get_api_key_internal("discord") {
            let mut headers = reqwest::header::HeaderMap::new();
            if let Ok(auth) = reqwest::header::HeaderValue::from_str(&format!("Bot {}", token)) {
                headers.insert(reqwest::header::AUTHORIZATION, auth);
            }
            let dc_client = reqwest::Client::builder()
                .user_agent("HIVE-Desktop/1.0")
                .default_headers(headers)
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap_or(client.clone());

            match dc_client.get("https://discord.com/api/v10/users/@me").send().await {
                Ok(resp) if resp.status().is_success() => {
                    let _ = resp.bytes().await; // consume response body
                    // P6: Don't leak bot identity or channel IDs to models
                    let has_default_channel = crate::security::get_api_key_internal("discord_channel_id").is_some();
                    let channel_info = if has_default_channel {
                        "  Default channel: configured"
                    } else {
                        "  No default channel set (must provide channel_id to discord_send)"
                    };
                    lines.push(format!(
                        "\nDiscord: CONNECTED\n  Bot authenticated successfully\n  Tools: discord_send, discord_read\n{}",
                        channel_info
                    ));
                }
                Ok(resp) => {
                    lines.push(format!(
                        "\nDiscord: KEY_INVALID (HTTP {})\n  Token is configured but rejected by Discord API",
                        resp.status()
                    ));
                }
                Err(e) => {
                    lines.push(format!(
                        "\nDiscord: UNREACHABLE\n  Key configured but API call failed: {}",
                        e
                    ));
                }
            }
        } else {
            lines.push("\nDiscord: NOT_CONFIGURED\n  No bot token set. User can add one in Settings → Integrations.".to_string());
        }

        // --- GitHub ---
        if has_key("github") {
            lines.push("\nGitHub: CONFIGURED\n  Tools: github_issues, github_prs, github_repos".to_string());
        } else {
            lines.push("\nGitHub: NOT_CONFIGURED\n  No token set. User can add one in Settings → Integrations.".to_string());
        }

        // --- Vector embeddings (P2: provider-agnostic) ---
        // Any of: OpenAI, DashScope, OpenRouter (cloud) or Ollama (local) can provide embeddings.
        let embedding_providers: Vec<&str> = [
            ("openai", "OpenAI (text-embedding-3-small)"),
            ("dashscope", "DashScope (text-embedding-v3)"),
            ("openrouter", "OpenRouter (openai/text-embedding-3-small)"),
        ].iter()
            .filter(|(key, _)| has_key(key))
            .map(|(_, label)| *label)
            .collect();
        // Ollama doesn't use an API key — it's always potentially available if running
        let ollama_note = "Ollama (nomic-embed-text, local — if running)";
        if !embedding_providers.is_empty() {
            let provider_list = embedding_providers.join(", ");
            lines.push(format!("\nVector Embeddings: AVAILABLE\n  Providers: {}\n  Also: {}", provider_list, ollama_note));
        } else {
            lines.push(format!("\nVector Embeddings: CLOUD_UNCONFIGURED\n  No cloud embedding API keys set. {}\n  Memory system using FTS5 text search (still functional). Add any provider key for hybrid vector+keyword search.", ollama_note));
        }

        Ok(ToolResult {
            content: lines.join("\n"),
            is_error: false,
        })
    }
}

// ============================================
// ListToolsTool
// ============================================

pub struct ListToolsTool;

#[async_trait::async_trait]
impl HiveTool for ListToolsTool {
    fn name(&self) -> &str { "list_tools" }

    fn description(&self) -> &str {
        "List all tools currently available to you, with their descriptions and risk levels. \
         Use this when you need to check what capabilities you have, find a specific tool, \
         or remind yourself what tools exist. Supports optional category filter."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "category": {
                    "type": "string",
                    "description": "Optional filter: 'file', 'system', 'web', 'memory', 'github', 'telegram', 'discord', 'worker', 'agent', or 'all' (default)"
                }
            },
            "required": []
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::Low }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let category = params.get("category")
            .and_then(|v| v.as_str())
            .unwrap_or("all")
            .to_lowercase();

        // Build a fresh registry to get accurate built-in tool list
        let registry = super::create_default_registry();
        let schemas = registry.schemas(); // already sorted by name

        let mut lines = Vec::new();
        lines.push(format!("=== HIVE Tools ({} registered) ===\n", schemas.len()));

        for schema in &schemas {
            // Category filtering by tool name prefix
            if category != "all" {
                let matches = match category.as_str() {
                    "file" => ["read_file", "write_file", "list_directory"].contains(&schema.name.as_str()),
                    "system" => ["run_command", "system_info"].contains(&schema.name.as_str()),
                    "web" => schema.name.starts_with("web_") || schema.name == "read_pdf",
                    "memory" => schema.name.starts_with("memory_") || ["task_track", "graph_query", "entity_track", "procedure_learn"].contains(&schema.name.as_str()),
                    "github" => schema.name.starts_with("github_"),
                    "telegram" => schema.name.starts_with("telegram_"),
                    "discord" => schema.name.starts_with("discord_"),
                    "worker" => schema.name.starts_with("worker_") || schema.name.starts_with("scratchpad_"),
                    "agent" => ["send_to_agent", "list_agents"].contains(&schema.name.as_str()),
                    "research" => ["repo_clone", "file_tree", "code_search", "check_logs"].contains(&schema.name.as_str()),
                    "orchestration" => ["route_to_specialist", "plan_execute", "integration_status", "list_tools"].contains(&schema.name.as_str()),
                    _ => true, // unknown category = show all
                };
                if !matches { continue; }
            }

            let risk = match schema.risk_level {
                super::RiskLevel::Low => "low",
                super::RiskLevel::Medium => "medium",
                super::RiskLevel::High => "high",
                super::RiskLevel::Critical => "CRITICAL",
            };
            // Truncate description to first sentence for compact output
            let desc = schema.description.split(". ").next().unwrap_or(&schema.description);
            lines.push(format!("  {} [{}] — {}", schema.name, risk, desc));
        }

        if category != "all" {
            let shown = lines.len() - 1; // minus header
            lines.push(format!("\n({} tools shown, {} total. Use category='all' for full list)", shown, schemas.len()));
        }

        lines.push("\nNote: External MCP-connected tools (mcp_*) are not shown here. Use integration_status to check connected MCP servers.".to_string());

        Ok(ToolResult {
            content: lines.join("\n"),
            is_error: false,
        })
    }
}

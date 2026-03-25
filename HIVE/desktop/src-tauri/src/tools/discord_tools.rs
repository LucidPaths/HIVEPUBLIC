//! Discord Bot Tools — send messages and read channel history via Discord REST API.
//!
//! HIVE ships the door. The user provides the key (Discord bot token from Dev Portal).
//! No key = tools don't register. With key = tools appear in capability manifest.

use super::{HiveTool, RiskLevel, ToolResult};
use crate::content_security::wrap_external_content;
use serde_json::json;

const DISCORD_API_BASE: &str = "https://discord.com/api/v10";

fn get_bot_token() -> Option<String> {
    crate::security::get_api_key_internal("discord")
}

fn get_default_channel_id() -> Option<String> {
    crate::security::get_api_key_internal("discord_channel_id")
}

/// Validate that a Discord ID (channel, message, user) is a numeric snowflake.
/// Prevents API path traversal via values like `../users/@me`.
fn validate_snowflake(id: &str, label: &str) -> Result<(), String> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!("Invalid {} '{}' — must be a numeric Discord snowflake ID", label, id));
    }
    Ok(())
}

fn discord_client(token: &str) -> Result<reqwest::Client, String> {
    let auth = format!("Bot {}", token);
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::AUTHORIZATION,
        reqwest::header::HeaderValue::from_str(&auth)
            .map_err(|_| "Invalid Discord token format")?,
    );
    reqwest::Client::builder()
        .user_agent("HIVE-Desktop/1.0")
        .default_headers(headers)
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))
}

// ============================================
// discord_send — Send a message to a channel
// ============================================

pub struct DiscordSendTool;

#[async_trait::async_trait]
impl HiveTool for DiscordSendTool {
    fn name(&self) -> &str { "discord_send" }

    fn description(&self) -> &str {
        "Send a message to a Discord channel via the configured bot. \
         channel_id is optional if a default is configured in Settings → Integrations."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "The message text to send (plain text or Discord markdown)."
                },
                "channel_id": {
                    "type": "string",
                    "description": "Discord channel ID to send to. Optional if a default channel is configured."
                }
            },
            "required": ["content"]
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::High }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let token = get_bot_token().ok_or(
            "Discord bot not configured. Add your bot token in Settings → Integrations."
        )?;

        let content = params.get("content")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: content")?;

        if content.trim().is_empty() {
            return Ok(ToolResult { content: "Cannot send empty message".to_string(), is_error: true });
        }
        if content.len() > 2000 {
            return Ok(ToolResult {
                content: format!("Message too long ({} chars). Discord limit is 2000.", content.len()),
                is_error: true,
            });
        }

        let channel_id_param = params.get("channel_id").and_then(|v| v.as_str()).map(str::to_string);
        let channel_id = channel_id_param
            .or_else(get_default_channel_id)
            .ok_or("No channel_id provided and no default channel configured. Set one in Settings → Integrations.")?;

        validate_snowflake(&channel_id, "channel_id").map_err(|e| {
            format!("{}", e)
        })?;

        let client = discord_client(&token)?;
        let url = format!("{}/channels/{}/messages", DISCORD_API_BASE, channel_id);

        let response = client
            .post(&url)
            .json(&json!({ "content": content }))
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Ok(ToolResult {
                content: format!("Discord API error ({}): {}", status, body.chars().take(300).collect::<String>()),
                is_error: true,
            });
        }

        let json: serde_json::Value = response.json().await
            .map_err(|_| "Failed to parse response".to_string())?;
        let msg_id = json.get("id").and_then(|v| v.as_str()).unwrap_or("?");

        Ok(ToolResult {
            content: format!("Message sent to #{} (id: {})", channel_id, msg_id),
            is_error: false,
        })
    }
}

// ============================================
// discord_read — Read recent messages from a channel
// ============================================

pub struct DiscordReadTool;

#[async_trait::async_trait]
impl HiveTool for DiscordReadTool {
    fn name(&self) -> &str { "discord_read" }

    fn description(&self) -> &str {
        "Read recent messages from a Discord channel. Returns the last N messages (default 10, max 50)."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "channel_id": {
                    "type": "string",
                    "description": "Discord channel ID to read from. Optional if a default is configured."
                },
                "limit": {
                    "type": "integer",
                    "description": "Number of messages to fetch (1-50). Default: 10."
                }
            },
            "required": []
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::Low }  // Read-only: auto-approve

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let token = get_bot_token().ok_or(
            "Discord bot not configured. Add your bot token in Settings → Integrations."
        )?;

        let channel_id_param = params.get("channel_id").and_then(|v| v.as_str()).map(str::to_string);
        let channel_id = channel_id_param
            .or_else(get_default_channel_id)
            .ok_or("No channel_id provided and no default channel configured.")?;

        let limit = params.get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(10)
            .clamp(1, 50);

        validate_snowflake(&channel_id, "channel_id").map_err(|e| {
            format!("{}", e)
        })?;

        let client = discord_client(&token)?;
        let url = format!("{}/channels/{}/messages?limit={}", DISCORD_API_BASE, channel_id, limit);

        let response = client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Ok(ToolResult {
                content: format!("Discord API error ({}): {}", status, body.chars().take(300).collect::<String>()),
                is_error: true,
            });
        }

        let messages: Vec<serde_json::Value> = response.json().await
            .map_err(|_| "Failed to parse response".to_string())?;

        if messages.is_empty() {
            return Ok(ToolResult { content: "No messages found in channel".to_string(), is_error: false });
        }

        // Discord returns newest-first; reverse for chronological display
        let mut lines = Vec::new();
        for msg in messages.iter().rev() {
            let author = msg.get("author").cloned().unwrap_or_default();
            let username = author.get("username").and_then(|v| v.as_str()).unwrap_or("?");
            let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("(no text)");
            let ts = msg.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
            let ts_short: String = ts.chars().take(16).collect(); // "2025-01-15T12:34"
            lines.push(format!("[{}] {}: {}", ts_short, username, content));
        }

        let raw_content = format!("Last {} messages from channel {}:\n\n{}", messages.len(), channel_id, lines.join("\n"));
        Ok(ToolResult {
            content: wrap_external_content(&format!("Discord channel {}", channel_id), &raw_content),
            is_error: false,
        })
    }
}

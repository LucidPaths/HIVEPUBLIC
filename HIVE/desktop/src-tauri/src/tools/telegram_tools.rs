//! Telegram Bot Tools — "doors and keys" integration
//!
//! HIVE ships the door (this code). The user provides the key (Bot API token from @BotFather).
//! No key = tools don't register. With key = tools appear in capability manifest.
//!
//! Uses the Telegram Bot API directly via reqwest (no extra dependencies).
//! Reference: https://core.telegram.org/bots/api

use super::{HiveTool, RiskLevel, ToolResult};
use crate::content_security::wrap_user_remote_message;
use serde_json::json;

/// Escape special characters for Telegram's Markdown parse mode.
/// Telegram's legacy Markdown is strict about paired formatting chars —
/// unmatched `*`, `_`, `` ` ``, or malformed `[]()` causes HTTP 400.
/// This escapes everything so the message always sends as plain-looking text
/// rather than failing silently. Models can opt into HTML parse_mode for
/// rich formatting (HTML is more forgiving and easier to generate correctly).
fn escape_telegram_markdown(text: &str) -> String {
    // Telegram legacy Markdown special chars: * _ ` [ ]
    // We escape them with backslash so they render as literal characters.
    let mut result = String::with_capacity(text.len() + text.len() / 10);
    for ch in text.chars() {
        match ch {
            '*' | '_' | '`' | '[' | ']' => {
                result.push('\\');
                result.push(ch);
            }
            _ => result.push(ch),
        }
    }
    result
}

const TELEGRAM_API_BASE: &str = "https://api.telegram.org";

/// Get the Telegram bot token from encrypted storage.
/// Returns None if not configured (door is closed).
fn get_bot_token() -> Option<String> {
    crate::security::get_api_key_internal("telegram")
}

/// Build the Telegram API URL for a method.
fn api_url(token: &str, method: &str) -> String {
    format!("{}/bot{}/{}", TELEGRAM_API_BASE, token, method)
}

/// HTTP client for Telegram API calls.
fn telegram_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent("HIVE-Desktop/1.0")
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))
}

// ============================================
// telegram_send — Send a message via the bot
// ============================================

pub struct TelegramSendTool;

#[async_trait::async_trait]
impl HiveTool for TelegramSendTool {
    fn name(&self) -> &str { "telegram_send" }

    fn description(&self) -> &str {
        "Send a message via the configured Telegram bot. Requires a chat_id \
         (user/group ID) and the message text. Default sends as plain text. \
         Use parse_mode 'HTML' for rich formatting (bold=<b>, italic=<i>, code=<code>). \
         The bot must be configured with a token in Settings first."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "chat_id": {
                    "type": "string",
                    "description": "The Telegram chat ID to send to (user ID, group ID, or @channel_username)"
                },
                "text": {
                    "type": "string",
                    "description": "The message text to send. Sent as plain text by default. Use parse_mode 'HTML' for formatting."
                },
                "parse_mode": {
                    "type": "string",
                    "enum": ["HTML", "plain"],
                    "description": "Optional: 'HTML' for rich formatting (<b>, <i>, <code>), 'plain' for no formatting. Default: plain text."
                }
            },
            "required": ["chat_id", "text"]
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::High }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let token = get_bot_token().ok_or(
            "Telegram bot not configured. Add your bot token in Settings → Integrations."
        )?;

        let chat_id = params.get("chat_id")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: chat_id")?;

        let text = params.get("text")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: text")?;

        let parse_mode = params.get("parse_mode")
            .and_then(|v| v.as_str())
            .unwrap_or("plain");

        if text.trim().is_empty() {
            return Ok(ToolResult {
                content: "Cannot send empty message".to_string(),
                is_error: true,
            });
        }

        let client = telegram_client()?;
        let url = api_url(&token, "sendMessage");

        // Build request body based on parse_mode
        // "plain" → no parse_mode field (Telegram sends as-is, no formatting issues)
        // "HTML" → pass through (HTML is more forgiving than Markdown)
        // "Markdown" → legacy support: auto-escape special chars to prevent 400 errors
        let body = match parse_mode {
            "HTML" => json!({
                "chat_id": chat_id,
                "text": text,
                "parse_mode": "HTML",
            }),
            "Markdown" => json!({
                "chat_id": chat_id,
                "text": escape_telegram_markdown(text),
                "parse_mode": "Markdown",
            }),
            _ => json!({
                "chat_id": chat_id,
                "text": text,
                // No parse_mode → Telegram treats as plain text (never fails)
            }),
        };

        let response = client.post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                // Sanitize — reqwest errors can include the URL which contains the bot token (P6)
                let msg = format!("{}", e);
                format!("Telegram API request failed: {}", crate::providers::sanitize_api_error(&msg))
            })?;

        let status = response.status();
        let response_text = response.text().await
            .unwrap_or_else(|_| "Failed to read response".to_string());

        // Persistent log — always log send attempts so we can debug delivery issues (P4)
        let response_preview = if response_text.chars().count() > 300 {
            format!("{}...", response_text.chars().take(300).collect::<String>())
        } else {
            response_text.clone()
        };
        crate::tools::log_tools::append_to_app_log(&format!(
            "TELEGRAM_SEND | chat_id={} | status={} | parse_mode={} | text_len={} | response={}",
            chat_id, status, parse_mode, text.len(), response_preview
        ));

        // If HTML parse mode fails (malformed tags), auto-retry as plain text
        if !status.is_success() && parse_mode == "HTML" {
            let plain_body = json!({
                "chat_id": chat_id,
                "text": text,
            });
            let retry = client.post(&url)
                .json(&plain_body)
                .send()
                .await
                .map_err(|e| format!("Telegram API retry failed: {}", e))?;

            if retry.status().is_success() {
                let retry_text = retry.text().await.unwrap_or_else(|e| {
                    eprintln!("[TELEGRAM] Failed to read retry response: {}", e);
                    String::new()
                });
                if let Ok(resp) = serde_json::from_str::<serde_json::Value>(&retry_text) {
                    if let Some(msg_id) = resp.get("result").and_then(|r| r.get("message_id")) {
                        return Ok(ToolResult {
                            content: format!("Message sent as plain text — HTML formatting had errors (message_id: {})", msg_id),
                            is_error: false,
                        });
                    }
                }
                return Ok(ToolResult {
                    content: "Message sent as plain text (HTML formatting had errors)".to_string(),
                    is_error: false,
                });
            }
        }

        if !status.is_success() {
            return Ok(ToolResult {
                content: format!("Telegram API error (HTTP {}): {}", status, response_text),
                is_error: true,
            });
        }

        // Parse response to get message ID
        if let Ok(resp) = serde_json::from_str::<serde_json::Value>(&response_text) {
            if let Some(msg_id) = resp.get("result").and_then(|r| r.get("message_id")) {
                return Ok(ToolResult {
                    content: format!("Message sent successfully (message_id: {})", msg_id),
                    is_error: false,
                });
            }
        }

        Ok(ToolResult {
            content: "Message sent successfully".to_string(),
            is_error: false,
        })
    }
}

// ============================================
// telegram_get_updates — Poll for new messages
// ============================================

pub struct TelegramGetUpdatesTool;

#[async_trait::async_trait]
impl HiveTool for TelegramGetUpdatesTool {
    fn name(&self) -> &str { "telegram_get_updates" }

    fn description(&self) -> &str {
        "Check for new messages sent to the Telegram bot. Returns recent messages \
         with sender info, chat ID, and message text. Use this to see what users \
         have sent to the bot. Results are wrapped in security boundaries."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of updates to retrieve (1-100). Default: 10."
                },
                "offset": {
                    "type": "integer",
                    "description": "Update ID offset. Use to acknowledge previous updates and only get new ones."
                }
            },
            "required": []
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::Low }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let token = get_bot_token().ok_or(
            "Telegram bot not configured. Add your bot token in Settings → Integrations."
        )?;

        let limit = params.get("limit")
            .and_then(|v| v.as_i64())
            .unwrap_or(10)
            .min(100)
            .max(1);

        let client = telegram_client()?;
        let url = api_url(&token, "getUpdates");

        let mut body = json!({
            "limit": limit,
            "timeout": 0,
        });

        if let Some(offset) = params.get("offset").and_then(|v| v.as_i64()) {
            body["offset"] = json!(offset);
        }

        let response = client.post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                // Sanitize — reqwest errors can include the URL which contains the bot token (P6)
                let msg = format!("{}", e);
                format!("Telegram API request failed: {}", crate::providers::sanitize_api_error(&msg))
            })?;

        let status = response.status();
        let response_text = response.text().await
            .unwrap_or_else(|_| "Failed to read response".to_string());

        if !status.is_success() {
            return Ok(ToolResult {
                content: format!("Telegram API error (HTTP {}): {}", status, response_text),
                is_error: true,
            });
        }

        // Parse and format updates
        let resp: serde_json::Value = serde_json::from_str(&response_text)
            .map_err(|e| format!("Failed to parse Telegram response: {}", e))?;

        let updates = resp.get("result")
            .and_then(|r| r.as_array())
            .cloned()
            .unwrap_or_default();

        if updates.is_empty() {
            return Ok(ToolResult {
                content: "No new messages.".to_string(),
                is_error: false,
            });
        }

        let mut messages = Vec::new();
        let mut max_update_id: i64 = 0;

        for update in &updates {
            let update_id = update.get("update_id")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            if update_id > max_update_id {
                max_update_id = update_id;
            }

            if let Some(msg) = update.get("message") {
                let from = msg.get("from")
                    .and_then(|f| {
                        let first = f.get("first_name").and_then(|v| v.as_str()).unwrap_or("Unknown");
                        let last = f.get("last_name").and_then(|v| v.as_str()).unwrap_or("");
                        let username = f.get("username").and_then(|v| v.as_str()).unwrap_or("");
                        Some(if username.is_empty() {
                            format!("{} {}", first, last).trim().to_string()
                        } else {
                            format!("{} {} (@{})", first, last, username).trim().to_string()
                        })
                    })
                    .unwrap_or_else(|| "Unknown".to_string());

                let chat_id = msg.get("chat").and_then(|c| c.get("id"))
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "?".to_string());

                let text = msg.get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("[non-text message]");

                let date = msg.get("date")
                    .and_then(|v| v.as_i64())
                    .map(|ts| {
                        chrono::DateTime::from_timestamp(ts, 0)
                            .map(|dt| dt.format("%H:%M:%S").to_string())
                            .unwrap_or_else(|| ts.to_string())
                    })
                    .unwrap_or_else(|| "?".to_string());

                messages.push(format!(
                    "[{}] From: {} (chat_id: {})\n  {}",
                    date, from, chat_id, text
                ));
            }
        }

        let content = format!(
            "{} message(s) received (next offset: {}):\n\n{}",
            messages.len(),
            max_update_id + 1,
            messages.join("\n\n")
        );

        // Sanitize but preserve as user instructions — these are messages from the owner
        Ok(ToolResult {
            content: wrap_user_remote_message("Telegram Messages", &content),
            is_error: false,
        })
    }
}

// ============================================
// telegram_bot_info — Get bot identity and status
// ============================================

pub struct TelegramBotInfoTool;

#[async_trait::async_trait]
impl HiveTool for TelegramBotInfoTool {
    fn name(&self) -> &str { "telegram_bot_info" }

    fn description(&self) -> &str {
        "Get information about the configured Telegram bot (name, username, status). \
         Use this to verify the bot is correctly configured and connected."
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
        let token = get_bot_token().ok_or(
            "Telegram bot not configured. Add your bot token in Settings → Integrations."
        )?;

        let client = telegram_client()?;
        let url = api_url(&token, "getMe");

        let response = client.get(&url)
            .send()
            .await
            .map_err(|e| {
                // Sanitize — reqwest errors can include the URL which contains the bot token (P6)
                let msg = format!("{}", e);
                format!("Telegram API request failed: {}", crate::providers::sanitize_api_error(&msg))
            })?;

        let status = response.status();
        let response_text = response.text().await
            .unwrap_or_else(|_| "Failed to read response".to_string());

        if !status.is_success() {
            return Ok(ToolResult {
                content: format!("Telegram API error (HTTP {}): {}. Check your bot token.", status, response_text),
                is_error: true,
            });
        }

        let resp: serde_json::Value = serde_json::from_str(&response_text)
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        if let Some(bot) = resp.get("result") {
            let name = bot.get("first_name").and_then(|v| v.as_str()).unwrap_or("Unknown");
            let username = bot.get("username").and_then(|v| v.as_str()).unwrap_or("Unknown");
            let can_join = bot.get("can_join_groups").and_then(|v| v.as_bool()).unwrap_or(false);
            let can_read = bot.get("can_read_all_group_messages").and_then(|v| v.as_bool()).unwrap_or(false);

            Ok(ToolResult {
                content: format!(
                    "Telegram Bot Status: CONNECTED\n\
                     Name: {}\n\
                     Username: @{}\n\
                     Can join groups: {}\n\
                     Can read group messages: {}",
                    name, username, can_join, can_read
                ),
                is_error: false,
            })
        } else {
            Ok(ToolResult {
                content: "Bot token is valid but response format unexpected.".to_string(),
                is_error: false,
            })
        }
    }
}

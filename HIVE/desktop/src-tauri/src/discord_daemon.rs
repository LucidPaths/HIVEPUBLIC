//! Discord Daemon — background polling loop that gives HIVE ears on Discord
//!
//! Architecture:
//!   Discord REST API (poll channels) → discord_daemon.rs → Tauri event → App.tsx → agentic loop → discord_send
//!
//! The daemon is a sensory channel. When someone messages in a watched channel or DMs the bot,
//! HIVE notices, thinks (via whatever model is active), and responds.
//!
//! Lifecycle:
//!   start_discord_daemon() → spawns Tokio task → polls /channels/{id}/messages
//!   stop_discord_daemon()  → sets running=false → task exits on next poll cycle
//!
//! Events emitted:
//!   "discord-incoming" → { channel_id, guild_id, author_name, author_id, text, message_id }
//!   "discord-daemon-status" → { running, messages_processed, last_error }

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::Emitter;
use tokio::sync::Mutex;

use crate::content_security::wrap_user_remote_message;
use crate::routines::ChannelEvent;
use crate::types::SenderRole;

// ============================================
// Types
// ============================================

/// Emitted to the frontend when a Discord message arrives.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordIncoming {
    pub channel_id: String,
    pub guild_id: Option<String>,
    pub author_name: String,
    pub author_id: String,
    pub text: String,
    pub message_id: String,
    /// The message text wrapped in security boundaries
    pub wrapped_text: String,
    /// Whether this sender is Host (full access) or User (restricted)
    pub sender_role: SenderRole,
}

/// Daemon status — queryable from the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordDaemonStatus {
    pub running: bool,
    pub messages_processed: u64,
    pub errors: u64,
    pub last_error: Option<String>,
    pub last_poll: Option<String>,
    pub connected_bot: Option<String>,
    pub watched_channels: Vec<String>,
}

/// Internal mutable state shared between the daemon task and Tauri commands.
struct DaemonInner {
    handle: Option<tokio::task::JoinHandle<()>>,
    messages_processed: u64,
    errors: u64,
    last_error: Option<String>,
    last_poll: Option<String>,
    connected_bot: Option<String>,
    watched_channels: Vec<String>,   // channel IDs to poll
    host_ids: Vec<String>,           // user IDs with Host permissions (full access)
    user_ids: Vec<String>,           // user IDs with User permissions (restricted)
    // Security: empty host_ids + empty user_ids = reject ALL (closed by default)
    last_message_ids: std::collections::HashMap<String, String>, // channel_id → last seen message_id
}

/// Tauri-managed state for the Discord daemon.
pub struct DiscordDaemonState {
    pub running: Arc<AtomicBool>,
    inner: Arc<Mutex<DaemonInner>>,
}

impl Default for DiscordDaemonState {
    fn default() -> Self {
        // Load persisted access lists + watched channels from disk (survives app restart)
        let (host_ids, user_ids) = crate::paths::load_channel_access("discord");
        let watched_channels = crate::paths::load_discord_watched_channels();
        Self {
            running: Arc::new(AtomicBool::new(false)),
            inner: Arc::new(Mutex::new(DaemonInner {
                handle: None,
                messages_processed: 0,
                errors: 0,
                last_error: None,
                last_poll: None,
                connected_bot: None,
                watched_channels,
                host_ids,
                user_ids,
                last_message_ids: std::collections::HashMap::new(),
            })),
        }
    }
}

// ============================================
// Discord API helpers
// ============================================

const DISCORD_API_BASE: &str = "https://discord.com/api/v10";

fn discord_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent("HIVE-Desktop/1.0 (https://github.com/LucidPaths/HiveMind)")
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))
}

/// Fetch bot info to validate the token and get the bot's username.
async fn fetch_bot_info(token: &str) -> Result<(String, String), String> {
    let client = discord_client()?;
    let resp = client
        .get(format!("{}/users/@me", DISCORD_API_BASE))
        .header("Authorization", format!("Bot {}", token))
        .send()
        .await
        .map_err(|e| format!("Discord auth failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Discord auth error ({}): {}", status, body));
    }

    let body: serde_json::Value = resp.json().await
        .map_err(|e| format!("Discord parse failed: {}", e))?;

    let username = body.get("username")
        .and_then(|u| u.as_str())
        .unwrap_or("unknown")
        .to_string();
    let bot_id = body.get("id")
        .and_then(|u| u.as_str())
        .unwrap_or("0")
        .to_string();

    Ok((username, bot_id))
}

/// Fetch the guilds (servers) the bot is in, and return their channel IDs.
async fn fetch_bot_channels(token: &str) -> Result<Vec<(String, String)>, String> {
    let client = discord_client()?;

    // Get guilds
    let guilds_resp = client
        .get(format!("{}/users/@me/guilds", DISCORD_API_BASE))
        .header("Authorization", format!("Bot {}", token))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch guilds: {}", e))?;

    let guilds: Vec<serde_json::Value> = guilds_resp.json().await
        .map_err(|e| format!("Failed to parse guilds: {}", e))?;

    let mut channels = Vec::new();

    for guild in &guilds {
        let guild_id = guild.get("id").and_then(|i| i.as_str()).unwrap_or("");
        if guild_id.is_empty() { continue; }

        // Get channels for this guild
        if let Ok(channels_resp) = client
            .get(format!("{}/guilds/{}/channels", DISCORD_API_BASE, guild_id))
            .header("Authorization", format!("Bot {}", token))
            .send()
            .await
        {
            if let Ok(guild_channels) = channels_resp.json::<Vec<serde_json::Value>>().await {
                for ch in guild_channels {
                    // Only text channels (type 0)
                    let ch_type = ch.get("type").and_then(|t| t.as_u64()).unwrap_or(999);
                    if ch_type != 0 { continue; }

                    let ch_id = ch.get("id").and_then(|i| i.as_str()).unwrap_or("");
                    let ch_name = ch.get("name").and_then(|n| n.as_str()).unwrap_or("unknown");

                    if !ch_id.is_empty() {
                        channels.push((ch_id.to_string(), ch_name.to_string()));
                    }
                }
            }
        }
    }

    Ok(channels)
}

/// Poll a channel for new messages after a given message ID.
async fn poll_channel(
    token: &str,
    channel_id: &str,
    after_message_id: Option<&str>,
) -> Result<Vec<serde_json::Value>, String> {
    // Validate snowflake IDs before URL interpolation (P6: prevent API path traversal)
    if !channel_id.chars().all(|c| c.is_ascii_digit()) || channel_id.is_empty() {
        return Err(format!("Invalid channel_id '{}' — must be numeric", channel_id));
    }

    let client = discord_client()?;
    let mut url = format!("{}/channels/{}/messages?limit=10", DISCORD_API_BASE, channel_id);
    if let Some(after) = after_message_id {
        if !after.chars().all(|c| c.is_ascii_digit()) {
            return Err(format!("Invalid after_message_id '{}' — must be numeric", after));
        }
        url.push_str(&format!("&after={}", after));
    }

    let resp = client
        .get(&url)
        .header("Authorization", format!("Bot {}", token))
        .send()
        .await
        .map_err(|e| format!("Channel poll failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Channel poll error: {}", resp.status()));
    }

    let messages: Vec<serde_json::Value> = resp.json().await
        .map_err(|e| format!("Parse failed: {}", e))?;

    Ok(messages)
}

/// Parse a Discord message into our incoming type.
fn parse_message(msg: &serde_json::Value, bot_id: &str) -> Option<DiscordIncoming> {
    let author = msg.get("author")?;
    let author_id = author.get("id")?.as_str()?;

    // Skip messages from the bot itself
    if author_id == bot_id {
        return None;
    }

    // Skip bot messages
    if author.get("bot").and_then(|b| b.as_bool()).unwrap_or(false) {
        return None;
    }

    let text = msg.get("content")?.as_str()?;
    if text.is_empty() {
        return None;
    }

    let channel_id = msg.get("channel_id")?.as_str()?.to_string();
    let guild_id = msg.get("guild_id").and_then(|g| g.as_str()).map(|s| s.to_string());
    let message_id = msg.get("id")?.as_str()?.to_string();
    let author_name = author.get("username")
        .and_then(|u| u.as_str())
        .unwrap_or("Unknown")
        .to_string();

    let wrapped = wrap_user_remote_message(
        &format!("Discord from {} ({})", author_name, author_id),
        text,
    );

    Some(DiscordIncoming {
        channel_id,
        guild_id,
        author_name,
        author_id: author_id.to_string(),
        text: text.to_string(),
        message_id,
        wrapped_text: wrapped,
        sender_role: SenderRole::User, // default; overwritten by polling_loop after role check
    })
}

// ============================================
// The polling loop (runs as a background Tokio task)
// ============================================

async fn polling_loop(
    app: tauri::AppHandle,
    token: String,
    bot_id: String,
    running: Arc<AtomicBool>,
    inner: Arc<Mutex<DaemonInner>>,
) {
    eprintln!("[HIVE DISCORD] Daemon started, entering polling loop");
    crate::tools::log_tools::append_to_app_log("DISCORD | daemon_started | polling loop active");

    // Open a separate DB connection for routine evaluation (WAL allows concurrent readers).
    let db_path = crate::paths::get_app_data_dir().join("memory.db");
    let routines_conn = rusqlite::Connection::open(&db_path).ok();
    if let Some(ref conn) = routines_conn {
        if let Err(e) = conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000; PRAGMA foreign_keys=ON;") {
            eprintln!("[HIVE] Discord daemon: PRAGMA setup failed: {}", e);
        }
    }

    while running.load(Ordering::Relaxed) {
        // Get the channels to poll
        let channels: Vec<String>;
        let last_ids: std::collections::HashMap<String, String>;
        {
            let state = inner.lock().await;
            channels = state.watched_channels.clone();
            last_ids = state.last_message_ids.clone();
        }
        {
            let mut state = inner.lock().await;
            state.last_poll = Some(chrono::Local::now().format("%H:%M:%S").to_string());
        }

        if channels.is_empty() {
            // No channels configured — wait and retry
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            continue;
        }

        // Poll ALL channels in parallel (join_all) instead of sequential loop.
        // Before: N channels × ~100ms = N×100ms per cycle.
        // After: max(100ms per channel) ≈ 100ms regardless of N.
        // Discord rate limit is 50 req/sec for bots — parallel polls are safe.
        let poll_futures: Vec<_> = channels.iter().map(|channel_id| {
            let after = last_ids.get(channel_id).cloned();
            let tok = token.clone();
            let cid = channel_id.clone();
            async move {
                let result = poll_channel(&tok, &cid, after.as_deref()).await;
                (cid, result)
            }
        }).collect();

        let poll_results = futures_util::future::join_all(poll_futures).await;

        // Process results sequentially (state updates need ordering)
        let mut had_error = false;
        for (channel_id, result) in poll_results {
            if !running.load(Ordering::Relaxed) { break; }

            match result {
                Ok(messages) => {
                    for msg in messages.iter().rev() {
                        if let Some(incoming) = parse_message(msg, &bot_id) {
                            // Determine sender role from host_ids / user_ids.
                            // Security: if NEITHER list contains the author_id, reject.
                            // Empty lists = reject all (closed by default — P6).
                            let sender_role = {
                                let state = inner.lock().await;
                                if state.host_ids.contains(&incoming.author_id) {
                                    Some(SenderRole::Host)
                                } else if state.user_ids.contains(&incoming.author_id) {
                                    Some(SenderRole::User)
                                } else {
                                    None // not authorized
                                }
                            };

                            let role = match sender_role {
                                Some(r) => r,
                                None => {
                                    eprintln!("[HIVE DISCORD] Blocked message from unauthorized user: {} ({})", incoming.author_name, incoming.author_id);
                                    crate::tools::log_tools::append_to_app_log(&format!("DISCORD | blocked | user={} id={} (unauthorized)", incoming.author_name, incoming.author_id));
                                    // Still advance message ID to avoid re-processing
                                    {
                                        let mut state = inner.lock().await;
                                        state.last_message_ids.insert(
                                            incoming.channel_id.clone(),
                                            incoming.message_id.clone(),
                                        );
                                    }
                                    continue;
                                }
                            };

                            // Attach the role to the incoming message
                            let incoming = DiscordIncoming {
                                sender_role: role,
                                ..incoming
                            };

                            eprintln!(
                                "[HIVE DISCORD] Message from {}: {}",
                                incoming.author_name,
                                if incoming.text.chars().count() > 100 { incoming.text.chars().take(100).collect::<String>() } else { incoming.text.clone() }
                            );
                            crate::tools::log_tools::append_to_app_log(&format!(
                                "DISCORD | message | from={} role={:?} channel={} | {}",
                                incoming.author_name, role, incoming.channel_id,
                                if incoming.text.chars().count() > 80 { format!("{}...", incoming.text.chars().take(80).collect::<String>()) } else { incoming.text.clone() }
                            ));

                            {
                                let mut state = inner.lock().await;
                                state.last_message_ids.insert(
                                    incoming.channel_id.clone(),
                                    incoming.message_id.clone(),
                                );
                                state.messages_processed += 1;
                            }

                            // Emit native Discord event (backwards compat)
                            let _ = app.emit("discord-incoming", &incoming);

                            // Emit unified ChannelEvent (P5: one event path)
                            let role_str = match incoming.sender_role {
                                SenderRole::Host => "host",
                                SenderRole::User => "user",
                            };
                            let channel_event = ChannelEvent {
                                channel_type: "discord".to_string(),
                                channel_id: incoming.channel_id.clone(),
                                sender_name: incoming.author_name.clone(),
                                sender_id: incoming.author_id.clone(),
                                text: incoming.text.clone(),
                                wrapped_text: incoming.wrapped_text.clone(),
                                raw_event_id: incoming.message_id.clone(),
                                metadata: serde_json::json!({
                                    "guild_id": incoming.guild_id,
                                    "sender_role": role_str,
                                }),
                            };
                            let _ = app.emit("channel-incoming", &channel_event);

                            // Evaluate event-triggered routines
                            if let Some(ref conn) = routines_conn {
                                let triggered = crate::routines::process_channel_event(conn, &channel_event);
                                for event in triggered {
                                    let _ = app.emit("routine-triggered", &event);
                                }
                            }
                        } else if let Some(msg_id) = msg.get("id").and_then(|i| i.as_str()) {
                            let mut state = inner.lock().await;
                            state.last_message_ids.insert(channel_id.clone(), msg_id.to_string());
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[HIVE DISCORD] Poll error for channel {}: {}", channel_id, e);
                    crate::tools::log_tools::append_to_app_log(&format!("DISCORD | poll_error | channel={} | {}", channel_id, e));
                    {
                        let mut state = inner.lock().await;
                        state.errors += 1;
                        state.last_error = Some(e);
                    }
                    had_error = true;
                }
            }
        }

        // Back off on errors, otherwise normal poll interval
        if had_error {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        } else {
            // Poll every 2 seconds (was 3s — safe since parallel polls complete faster)
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    }

    eprintln!("[HIVE DISCORD] Daemon stopped");
    crate::tools::log_tools::append_to_app_log("DISCORD | daemon_stopped");
}

// ============================================
// Tauri Commands
// ============================================

/// Start the Discord polling daemon.
/// Requires a bot token stored in encrypted settings.
#[tauri::command]
pub async fn start_discord_daemon(
    app: tauri::AppHandle,
    daemon_state: tauri::State<'_, DiscordDaemonState>,
) -> Result<String, String> {
    // Atomically claim the "starting" slot — prevents concurrent starts (Q4 fix: TOCTOU)
    if daemon_state.running.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
        return Ok("Discord daemon is already running".to_string());
    }

    let token = match crate::security::get_api_key_internal("discord") {
        Some(t) => t,
        None => {
            daemon_state.running.store(false, Ordering::SeqCst);
            return Err("No Discord bot token configured. Add it in Settings → Integrations.".to_string());
        }
    };

    // Validate token
    let (bot_name, bot_id) = match fetch_bot_info(&token).await {
        Ok(info) => info,
        Err(e) => {
            daemon_state.running.store(false, Ordering::SeqCst);
            return Err(e);
        }
    };

    // Auto-discover channels the bot is in
    let channels = fetch_bot_channels(&token).await.unwrap_or_default();
    let channel_ids: Vec<String> = channels.iter().map(|(id, _)| id.clone()).collect();
    let channel_names: Vec<String> = channels.iter().map(|(_, name)| name.clone()).collect();

    eprintln!(
        "[HIVE DISCORD] Bot {} found in {} channels: {:?}",
        bot_name, channel_ids.len(), channel_names
    );

    // running was already set to true by compare_exchange above
    {
        let mut inner = daemon_state.inner.lock().await;
        inner.connected_bot = Some(bot_name.clone());
        inner.errors = 0;
        inner.last_error = None;
        inner.messages_processed = 0;
        inner.watched_channels = channel_ids.clone();
    }

    let running = daemon_state.running.clone();
    let inner = daemon_state.inner.clone();
    let handle = tokio::spawn(polling_loop(app, token, bot_id, running, inner));

    {
        let mut inner = daemon_state.inner.lock().await;
        inner.handle = Some(handle);
    }

    Ok(format!("Discord daemon started for {} ({} channels)", bot_name, channel_ids.len()))
}

/// Stop the Discord polling daemon.
#[tauri::command]
pub async fn stop_discord_daemon(
    daemon_state: tauri::State<'_, DiscordDaemonState>,
) -> Result<String, String> {
    if !daemon_state.running.load(Ordering::Relaxed) {
        return Ok("Discord daemon is not running".to_string());
    }

    daemon_state.running.store(false, Ordering::Relaxed);

    // Take the handle and await it so the old loop fully exits before we return.
    // This prevents a race where rapid stop→start could cause two loops.
    let handle = {
        let mut inner = daemon_state.inner.lock().await;
        inner.connected_bot = None;
        inner.handle.take()
    };
    if let Some(h) = handle {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), h).await;
    }

    Ok("Discord daemon stopped".to_string())
}

/// Get the current daemon status.
#[tauri::command]
pub async fn get_discord_daemon_status(
    daemon_state: tauri::State<'_, DiscordDaemonState>,
) -> Result<DiscordDaemonStatus, String> {
    let running = daemon_state.running.load(Ordering::Relaxed);
    let inner = daemon_state.inner.lock().await;

    Ok(DiscordDaemonStatus {
        running,
        messages_processed: inner.messages_processed,
        errors: inner.errors,
        last_error: inner.last_error.clone(),
        last_poll: inner.last_poll.clone(),
        connected_bot: inner.connected_bot.clone(),
        watched_channels: inner.watched_channels.clone(),
    })
}

/// Set which channels the daemon watches.
#[tauri::command]
pub async fn set_discord_watched_channels(
    daemon_state: tauri::State<'_, DiscordDaemonState>,
    channel_ids: Vec<String>,
) -> Result<String, String> {
    let mut inner = daemon_state.inner.lock().await;
    let count = channel_ids.len();
    inner.watched_channels = channel_ids;
    // Persist to disk so settings survive app restart
    crate::paths::save_discord_watched_channels(&inner.watched_channels)
        .unwrap_or_else(|e| eprintln!("[HIVE DISCORD] Failed to persist watched channels: {}", e));
    Ok(format!("Watching {} channel(s)", count))
}

/// Set the Host IDs — Discord user IDs with full desktop-equivalent permissions.
#[tauri::command]
pub async fn set_discord_host_ids(
    daemon_state: tauri::State<'_, DiscordDaemonState>,
    user_ids: Vec<String>,
) -> Result<String, String> {
    let mut inner = daemon_state.inner.lock().await;
    let count = user_ids.len();
    inner.host_ids = user_ids;
    // Persist to disk so settings survive app restart
    crate::paths::save_channel_access("discord", &inner.host_ids, &inner.user_ids)
        .unwrap_or_else(|e| eprintln!("[HIVE DISCORD] Failed to persist access lists: {}", e));
    Ok(format!("Discord host ID(s) set — {} host(s)", count))
}

/// Set the User IDs — Discord user IDs with restricted permissions.
#[tauri::command]
pub async fn set_discord_user_ids(
    daemon_state: tauri::State<'_, DiscordDaemonState>,
    user_ids: Vec<String>,
) -> Result<String, String> {
    let mut inner = daemon_state.inner.lock().await;
    let count = user_ids.len();
    inner.user_ids = user_ids;
    // Persist to disk so settings survive app restart
    crate::paths::save_channel_access("discord", &inner.host_ids, &inner.user_ids)
        .unwrap_or_else(|e| eprintln!("[HIVE DISCORD] Failed to persist access lists: {}", e));
    Ok(format!("Discord user ID(s) set — {} user(s)", count))
}

/// Get the current access lists.
#[tauri::command]
pub async fn get_discord_access_lists(
    daemon_state: tauri::State<'_, DiscordDaemonState>,
) -> Result<serde_json::Value, String> {
    let inner = daemon_state.inner.lock().await;
    Ok(serde_json::json!({
        "host_ids": inner.host_ids,
        "user_ids": inner.user_ids,
    }))
}

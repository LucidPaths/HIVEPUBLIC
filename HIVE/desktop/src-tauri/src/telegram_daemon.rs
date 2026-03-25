//! Telegram Daemon — background polling loop that gives HIVE ears
//!
//! Architecture:
//!   Telegram Bot API (long-poll, 30s) → telegram_daemon.rs → Tauri event → App.tsx → agentic loop → telegram_send
//!
//! The daemon is a sensory channel. When someone messages the bot, HIVE notices,
//! thinks (via whatever model is active), and responds. The model doesn't know or care
//! that the message came from Telegram — it just sees a user message and uses tools.
//!
//! Lifecycle:
//!   start_telegram_daemon() → spawns Tokio task → long-polls getUpdates
//!   stop_telegram_daemon()  → sets running=false → task exits on next poll cycle
//!
//! Events emitted:
//!   "telegram-incoming" → { chat_id, from_name, from_username, text, update_id }
//!   "telegram-daemon-status" → { running, messages_processed, last_error }

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

/// Emitted to the frontend when a Telegram message arrives.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramIncoming {
    pub chat_id: String,
    pub from_name: String,
    pub from_username: String,
    pub text: String,
    pub update_id: i64,
    /// The message text wrapped in security boundaries
    pub wrapped_text: String,
    /// Whether this sender is Host (full access) or User (restricted)
    pub sender_role: SenderRole,
}

/// Daemon status — queryable from the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramDaemonStatus {
    pub running: bool,
    pub messages_processed: u64,
    pub errors: u64,
    pub last_error: Option<String>,
    pub last_poll: Option<String>,
    pub connected_bot: Option<String>,
}

/// Internal mutable state shared between the daemon task and Tauri commands.
struct DaemonInner {
    handle: Option<tokio::task::JoinHandle<()>>,
    messages_processed: u64,
    errors: u64,
    last_error: Option<String>,
    last_poll: Option<String>,
    connected_bot: Option<String>,
    host_ids: Vec<String>,  // chat_ids with Host permissions (full access)
    user_ids: Vec<String>,  // chat_ids with User permissions (restricted — no dangerous tools)
    // Security: empty host_ids + empty user_ids = reject ALL (closed by default)
}

/// Tauri-managed state for the Telegram daemon.
pub struct TelegramDaemonState {
    pub running: Arc<AtomicBool>,
    inner: Arc<Mutex<DaemonInner>>,
}

impl Default for TelegramDaemonState {
    fn default() -> Self {
        // Load persisted access lists from disk (survives app restart)
        let (host_ids, user_ids) = crate::paths::load_channel_access("telegram");
        Self {
            running: Arc::new(AtomicBool::new(false)),
            inner: Arc::new(Mutex::new(DaemonInner {
                handle: None,
                messages_processed: 0,
                errors: 0,
                last_error: None,
                last_poll: None,
                connected_bot: None,
                host_ids,
                user_ids,
            })),
        }
    }
}

// ============================================
// Telegram API helpers
// ============================================

const TELEGRAM_API_BASE: &str = "https://api.telegram.org";

fn api_url(token: &str, method: &str) -> String {
    format!("{}/bot{}/{}", TELEGRAM_API_BASE, token, method)
}

fn telegram_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent("HIVE-Desktop/1.0")
        .timeout(std::time::Duration::from_secs(60)) // longer timeout for long-polling
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))
}

/// Fetch bot info to validate the token and get the bot's username.
async fn fetch_bot_info(token: &str) -> Result<String, String> {
    let client = telegram_client()?;
    let url = api_url(token, "getMe");

    let resp = client.get(&url).send().await
        .map_err(|e| format!("getMe failed: {}", e))?;

    let body: serde_json::Value = resp.json().await
        .map_err(|e| format!("getMe parse failed: {}", e))?;

    let username = body.get("result")
        .and_then(|r| r.get("username"))
        .and_then(|u| u.as_str())
        .unwrap_or("unknown");

    Ok(format!("@{}", username))
}

/// Long-poll for updates from the Telegram Bot API.
async fn poll_updates(
    token: &str,
    offset: i64,
    timeout_secs: u64,
) -> Result<Vec<serde_json::Value>, String> {
    let client = telegram_client()?;
    let url = api_url(token, "getUpdates");

    let body = serde_json::json!({
        "offset": offset,
        "timeout": timeout_secs,
        "allowed_updates": ["message"],
    });

    let resp = client.post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("getUpdates failed: {}", e))?;

    let data: serde_json::Value = resp.json().await
        .map_err(|e| format!("getUpdates parse failed: {}", e))?;

    let updates = data.get("result")
        .and_then(|r| r.as_array())
        .cloned()
        .unwrap_or_default();

    Ok(updates)
}

/// Extract a TelegramIncoming from a raw update JSON.
fn parse_update(update: &serde_json::Value) -> Option<TelegramIncoming> {
    let update_id = update.get("update_id")?.as_i64()?;

    let msg = update.get("message")?;
    let text = msg.get("text")?.as_str()?;

    let chat_id = msg.get("chat")
        .and_then(|c| c.get("id"))
        .map(|id| id.to_string())?;

    let from = msg.get("from")?;
    let first_name = from.get("first_name").and_then(|v| v.as_str()).unwrap_or("Unknown");
    let last_name = from.get("last_name").and_then(|v| v.as_str()).unwrap_or("");
    let username = from.get("username").and_then(|v| v.as_str()).unwrap_or("");

    let from_name = format!("{} {}", first_name, last_name).trim().to_string();
    let from_username = username.to_string();

    // Sanitize but preserve as user instruction — this is the owner's message via Telegram
    let wrapped = wrap_user_remote_message(
        &format!("Telegram from {} (@{})", from_name, from_username),
        text,
    );

    Some(TelegramIncoming {
        chat_id,
        from_name,
        from_username,
        text: text.to_string(),
        update_id,
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
    running: Arc<AtomicBool>,
    inner: Arc<Mutex<DaemonInner>>,
) {
    let mut offset: i64 = 0;

    eprintln!("[HIVE TELEGRAM] Daemon started, entering polling loop");
    crate::tools::log_tools::append_to_app_log("TELEGRAM | daemon_started | polling loop active");

    // Open a separate DB connection for routine evaluation (WAL allows concurrent readers).
    // This avoids holding MemoryState's std::sync::Mutex across async boundaries.
    let db_path = crate::paths::get_app_data_dir().join("memory.db");
    let routines_conn = rusqlite::Connection::open(&db_path).ok();
    if let Some(ref conn) = routines_conn {
        if let Err(e) = conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000; PRAGMA foreign_keys=ON;") {
            eprintln!("[HIVE] Telegram daemon: PRAGMA setup failed: {}", e);
        }
    }

    // Drain any pending updates without processing them, to avoid responding to stale messages.
    // Uses timeout=0 (non-blocking) to get all unacknowledged updates, then advances offset past them.
    if let Ok(stale_updates) = poll_updates(&token, 0, 0).await {
        if !stale_updates.is_empty() {
            if let Some(last) = stale_updates.last() {
                if let Some(uid) = last.get("update_id").and_then(|u| u.as_i64()) {
                    offset = uid + 1;
                    eprintln!("[HIVE TELEGRAM] Drained {} stale updates, starting from offset {}", stale_updates.len(), offset);
                }
            }
        }
    }

    while running.load(Ordering::Relaxed) {
        // Update last poll time
        {
            let mut state = inner.lock().await;
            state.last_poll = Some(chrono::Local::now().format("%H:%M:%S").to_string());
        }

        match poll_updates(&token, offset, 30).await {
            Ok(updates) => {
                for update in &updates {
                    if let Some(incoming) = parse_update(update) {
                        // Update offset to acknowledge this update
                        offset = incoming.update_id + 1;

                        // Determine sender role from host_ids / user_ids.
                        // Security: if NEITHER list contains the chat_id, reject.
                        // Empty lists = reject all (closed by default — P6).
                        let sender_role = {
                            let state = inner.lock().await;
                            if state.host_ids.contains(&incoming.chat_id) {
                                Some(SenderRole::Host)
                            } else if state.user_ids.contains(&incoming.chat_id) {
                                Some(SenderRole::User)
                            } else {
                                None // not authorized
                            }
                        };

                        let role = match sender_role {
                            Some(r) => r,
                            None => {
                                eprintln!("[HIVE TELEGRAM] Blocked message from unauthorized chat_id: {} (not in host_ids or user_ids)", incoming.chat_id);
                                crate::tools::log_tools::append_to_app_log(&format!("TELEGRAM | blocked | chat_id={} (unauthorized)", incoming.chat_id));
                                continue;
                            }
                        };

                        // Attach the role to the incoming message
                        let incoming = TelegramIncoming {
                            sender_role: role,
                            ..incoming
                        };

                        eprintln!(
                            "[HIVE TELEGRAM] Message from {} (@{}): {}",
                            incoming.from_name, incoming.from_username,
                            if incoming.text.chars().count() > 100 { incoming.text.chars().take(100).collect::<String>() } else { incoming.text.clone() }
                        );
                        crate::tools::log_tools::append_to_app_log(&format!(
                            "TELEGRAM | message | from={} role={:?} | {}",
                            incoming.from_name, role,
                            if incoming.text.chars().count() > 80 { format!("{}...", incoming.text.chars().take(80).collect::<String>()) } else { incoming.text.clone() }
                        ));

                        // Emit native Telegram event (backwards compat)
                        let _ = app.emit("telegram-incoming", &incoming);

                        // Emit unified ChannelEvent (P5: one event path)
                        let role_str = match incoming.sender_role {
                            SenderRole::Host => "host",
                            SenderRole::User => "user",
                        };
                        let channel_event = ChannelEvent {
                            channel_type: "telegram".to_string(),
                            channel_id: incoming.chat_id.clone(),
                            sender_name: incoming.from_name.clone(),
                            sender_id: incoming.from_username.clone(),
                            text: incoming.text.clone(),
                            wrapped_text: incoming.wrapped_text.clone(),
                            raw_event_id: incoming.update_id.to_string(),
                            metadata: serde_json::json!({
                                "from_username": incoming.from_username,
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

                        // Update stats
                        {
                            let mut state = inner.lock().await;
                            state.messages_processed += 1;
                        }
                    } else {
                        // Non-text update (photo, sticker, etc.) — advance offset but skip
                        if let Some(uid) = update.get("update_id").and_then(|v| v.as_i64()) {
                            offset = uid + 1;
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("[HIVE TELEGRAM] Poll error: {}", e);
                crate::tools::log_tools::append_to_app_log(&format!("TELEGRAM | poll_error | {}", e));
                {
                    let mut state = inner.lock().await;
                    state.errors += 1;
                    state.last_error = Some(e);
                }
                // Back off on error — don't hammer the API
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        }
    }

    eprintln!("[HIVE TELEGRAM] Daemon stopped");
    crate::tools::log_tools::append_to_app_log("TELEGRAM | daemon_stopped");
}

// ============================================
// Tauri Commands
// ============================================

/// Start the Telegram polling daemon.
/// Requires a bot token stored in encrypted settings.
#[tauri::command]
pub async fn start_telegram_daemon(
    app: tauri::AppHandle,
    daemon_state: tauri::State<'_, TelegramDaemonState>,
) -> Result<String, String> {
    // Atomically claim — prevents concurrent starts (Q4 fix: TOCTOU)
    if daemon_state.running.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
        return Ok("Telegram daemon is already running".to_string());
    }

    // Get bot token
    let token = match crate::security::get_api_key_internal("telegram") {
        Some(t) => t,
        None => {
            daemon_state.running.store(false, Ordering::SeqCst);
            return Err("No Telegram bot token configured. Add it in Settings → Integrations.".to_string());
        }
    };

    // Validate token by fetching bot info
    let bot_name = match fetch_bot_info(&token).await {
        Ok(name) => name,
        Err(e) => {
            daemon_state.running.store(false, Ordering::SeqCst);
            return Err(e);
        }
    };

    // running was already set to true by compare_exchange above
    {
        let mut inner = daemon_state.inner.lock().await;
        inner.connected_bot = Some(bot_name.clone());
        inner.errors = 0;
        inner.last_error = None;
        inner.messages_processed = 0;
    }

    // Spawn the background polling task
    let running = daemon_state.running.clone();
    let inner = daemon_state.inner.clone();
    let handle = tokio::spawn(polling_loop(app, token, running, inner));

    // Store the task handle so we can await it on stop
    {
        let mut inner = daemon_state.inner.lock().await;
        inner.handle = Some(handle);
    }

    Ok(format!("Telegram daemon started for {}", bot_name))
}

/// Stop the Telegram polling daemon.
#[tauri::command]
pub async fn stop_telegram_daemon(
    daemon_state: tauri::State<'_, TelegramDaemonState>,
) -> Result<String, String> {
    if !daemon_state.running.load(Ordering::Relaxed) {
        return Ok("Telegram daemon is not running".to_string());
    }

    // Signal the loop to stop
    daemon_state.running.store(false, Ordering::Relaxed);

    // Take the handle and await it so the old loop fully exits before we return.
    // This prevents a race where rapid stop→start could cause two loops.
    // Telegram long-polling has a 30s timeout, so we give it 35s.
    let handle = {
        let mut inner = daemon_state.inner.lock().await;
        inner.connected_bot = None;
        inner.handle.take()
    };
    if let Some(h) = handle {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(35), h).await;
    }

    Ok("Telegram daemon stopped".to_string())
}

/// Get the current daemon status.
#[tauri::command]
pub async fn get_telegram_daemon_status(
    daemon_state: tauri::State<'_, TelegramDaemonState>,
) -> Result<TelegramDaemonStatus, String> {
    let running = daemon_state.running.load(Ordering::Relaxed);
    let inner = daemon_state.inner.lock().await;

    Ok(TelegramDaemonStatus {
        running,
        messages_processed: inner.messages_processed,
        errors: inner.errors,
        last_error: inner.last_error.clone(),
        last_poll: inner.last_poll.clone(),
        connected_bot: inner.connected_bot.clone(),
    })
}

/// Set the Host IDs — chat_ids with full desktop-equivalent permissions.
/// Host can trigger all tools including dangerous ones (run_command, write_file).
#[tauri::command]
pub async fn set_telegram_host_ids(
    daemon_state: tauri::State<'_, TelegramDaemonState>,
    chat_ids: Vec<String>,
) -> Result<String, String> {
    let mut inner = daemon_state.inner.lock().await;
    let count = chat_ids.len();
    inner.host_ids = chat_ids;
    // Persist to disk so settings survive app restart
    crate::paths::save_channel_access("telegram", &inner.host_ids, &inner.user_ids)
        .unwrap_or_else(|e| eprintln!("[HIVE TELEGRAM] Failed to persist access lists: {}", e));
    Ok(format!("Telegram host ID(s) set — {} host(s)", count))
}

/// Set the User IDs — chat_ids with restricted permissions.
/// Users can chat and use safe tools, but dangerous/desktop-only tools are blocked.
#[tauri::command]
pub async fn set_telegram_user_ids(
    daemon_state: tauri::State<'_, TelegramDaemonState>,
    chat_ids: Vec<String>,
) -> Result<String, String> {
    let mut inner = daemon_state.inner.lock().await;
    let count = chat_ids.len();
    inner.user_ids = chat_ids;
    // Persist to disk so settings survive app restart
    crate::paths::save_channel_access("telegram", &inner.host_ids, &inner.user_ids)
        .unwrap_or_else(|e| eprintln!("[HIVE TELEGRAM] Failed to persist access lists: {}", e));
    Ok(format!("Telegram user ID(s) set — {} user(s)", count))
}

/// Get the current access lists.
#[tauri::command]
pub async fn get_telegram_access_lists(
    daemon_state: tauri::State<'_, TelegramDaemonState>,
) -> Result<serde_json::Value, String> {
    let inner = daemon_state.inner.lock().await;
    Ok(serde_json::json!({
        "host_ids": inner.host_ids,
        "user_ids": inner.user_ids,
    }))
}

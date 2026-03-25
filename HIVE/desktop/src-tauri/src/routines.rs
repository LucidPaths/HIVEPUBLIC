//! HIVE Routines Engine — autonomous standing instructions
//!
//! Architecture:
//!   Routines are persistent directives that HIVE evaluates against events and time.
//!   They bridge the gap between reactive chat and autonomous agency.
//!
//!   Two trigger types:
//!     - Cron: time-based ("every day at 9am", "every 5 minutes")
//!     - Event: channel-based ("when a Telegram message arrives", "when Discord mentions 'urgent'")
//!
//!   When triggered, the routine's action_prompt is sent through the normal agentic loop
//!   (same as if a user typed it), with optional output routing to a specific channel.
//!
//! Data flow:
//!   Cron tick / Channel event → routines.rs → Tauri event "routine-triggered" → App.tsx → sendMessage()
//!
//! Persistence:
//!   Routines live in memory.db alongside MAGMA tables. Schema version bumped to 4.
//!   Message queue also lives here for reliable channel message processing.
//!
//! Principle Lattice alignment:
//!   P1 (Bridges)    — Routines bridge time/events → agentic action. Modular, standalone module.
//!   P2 (Agnostic)   — Routines trigger sendMessage(); the model/provider is irrelevant.
//!   P3 (Simplicity) — Simple cron parser (no dep), event matching by channel type + keyword.
//!   P4 (Errors)     — Failed routines increment fail_count; dead-letter queue for messages.
//!   P5 (Fix Pattern) — Unified ChannelEvent means one event path, not N per channel.
//!   P6 (Secrets)     — Routines never contain or transmit credentials.
//!   P7 (Survives)    — Routines persist in SQLite; survive model swaps and restarts.
//!   P8 (Low/High)    — "every morning summarize X" is low-floor; cron + event combos are high-ceiling.

use chrono::{Datelike, Local, Timelike, Utc};
use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::memory::MemoryState;

// ============================================
// Types
// ============================================

/// A unified event from any channel (Telegram, Discord, future channels).
/// This is the P5-compliant "one event path" — channel daemons emit this
/// alongside their native events for backwards compatibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelEvent {
    /// Channel type: "telegram", "discord", "local"
    pub channel_type: String,
    /// Channel-specific ID (chat_id for Telegram, channel_id for Discord)
    pub channel_id: String,
    /// Human-readable sender name
    pub sender_name: String,
    /// Platform-specific sender ID
    pub sender_id: String,
    /// Raw message text
    pub text: String,
    /// Security-wrapped message text (homoglyph folded + boundary markers)
    pub wrapped_text: String,
    /// Platform-specific event ID (update_id, message_id, etc.)
    pub raw_event_id: String,
    /// Extra platform-specific data (guild_id for Discord, etc.)
    pub metadata: serde_json::Value,
}

/// A persistent routine — a standing instruction that HIVE evaluates autonomously.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Routine {
    pub id: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,

    // Trigger configuration
    pub trigger_type: TriggerType,
    /// Cron expression: "minute hour day month weekday" (5-field)
    /// Supports: *, specific values, */N (step)
    pub cron_expr: Option<String>,
    /// Event pattern: "channel:telegram", "channel:discord", "channel:*"
    pub event_pattern: Option<String>,
    /// Keyword filter — if set, event must contain this text (case-insensitive)
    pub event_keyword: Option<String>,

    // Action configuration
    /// The prompt to send through the agentic loop when triggered
    pub action_prompt: String,
    /// Where to route the response: "telegram:<chat_id>", "discord:<channel_id>", null = local
    pub response_channel: Option<String>,

    // Execution stats
    pub run_count: i64,
    pub success_count: i64,
    pub fail_count: i64,
    pub last_run: Option<String>,
    pub last_result: Option<String>,

    // Timestamps
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerType {
    Cron,
    Event,
    Both,
}

impl TriggerType {
    fn as_str(&self) -> &str {
        match self {
            TriggerType::Cron => "cron",
            TriggerType::Event => "event",
            TriggerType::Both => "both",
        }
    }

    fn from_str(s: &str) -> Self {
        match s {
            "cron" => TriggerType::Cron,
            "event" => TriggerType::Event,
            "both" => TriggerType::Both,
            _ => TriggerType::Event,
        }
    }
}

/// A queued message from a channel, awaiting processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedMessage {
    pub id: String,
    pub channel_type: String,
    pub channel_id: String,
    pub sender_name: String,
    pub sender_id: String,
    pub text: String,
    pub wrapped_text: String,
    pub status: QueueStatus,
    pub attempts: i64,
    pub max_attempts: i64,
    pub error: Option<String>,
    pub routine_id: Option<String>,
    pub created_at: String,
    pub processed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueStatus {
    Pending,
    Processing,
    Completed,
    Failed,
    Dead,
}

impl QueueStatus {
    #[allow(dead_code)] // symmetric with from_str; will be used when queue entries are serialized
    fn as_str(&self) -> &str {
        match self {
            QueueStatus::Pending => "pending",
            QueueStatus::Processing => "processing",
            QueueStatus::Completed => "completed",
            QueueStatus::Failed => "failed",
            QueueStatus::Dead => "dead",
        }
    }

    fn from_str(s: &str) -> Self {
        match s {
            "pending" => QueueStatus::Pending,
            "processing" => QueueStatus::Processing,
            "completed" => QueueStatus::Completed,
            "failed" => QueueStatus::Failed,
            "dead" => QueueStatus::Dead,
            _ => QueueStatus::Pending,
        }
    }
}

/// What triggered a routine execution — emitted as a Tauri event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineTriggered {
    pub routine_id: String,
    pub routine_name: String,
    pub action_prompt: String,
    pub response_channel: Option<String>,
    pub trigger_reason: String,
    /// If triggered by a channel event, the source event
    pub source_event: Option<ChannelEvent>,
    /// Queue message ID (for tracking completion)
    pub queue_id: Option<String>,
}

/// Summary stats for the routines engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineStats {
    pub total_routines: i64,
    pub enabled_routines: i64,
    pub total_runs: i64,
    pub total_successes: i64,
    pub total_failures: i64,
    pub queue_pending: i64,
    pub queue_processing: i64,
    pub queue_dead: i64,
}

// ============================================
// Schema (lives in memory.db, schema version 4)
// ============================================

/// Initialize routines + message queue tables. Idempotent.
/// Called from memory::init_db() when schema version < 4.
pub fn init_routines_schema(conn: &rusqlite::Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        -- ROUTINES: persistent standing instructions.
        -- Each routine has a trigger (cron, event, or both) and an action prompt.
        CREATE TABLE IF NOT EXISTS routines (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            enabled INTEGER NOT NULL DEFAULT 1,
            trigger_type TEXT NOT NULL DEFAULT 'event',
            cron_expr TEXT,
            event_pattern TEXT,
            event_keyword TEXT,
            action_prompt TEXT NOT NULL,
            response_channel TEXT,
            run_count INTEGER NOT NULL DEFAULT 0,
            success_count INTEGER NOT NULL DEFAULT 0,
            fail_count INTEGER NOT NULL DEFAULT 0,
            last_run TEXT,
            last_result TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_routines_enabled ON routines(enabled);
        CREATE INDEX IF NOT EXISTS idx_routines_trigger ON routines(trigger_type);

        -- MESSAGE QUEUE: reliable channel message processing with retry + dead-letter.
        -- Messages are enqueued by channel daemons, dequeued by the frontend agentic loop.
        CREATE TABLE IF NOT EXISTS message_queue (
            id TEXT PRIMARY KEY,
            channel_type TEXT NOT NULL,
            channel_id TEXT NOT NULL,
            sender_name TEXT NOT NULL,
            sender_id TEXT NOT NULL,
            text TEXT NOT NULL,
            wrapped_text TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending',
            attempts INTEGER NOT NULL DEFAULT 0,
            max_attempts INTEGER NOT NULL DEFAULT 5,
            error TEXT,
            routine_id TEXT,
            created_at TEXT NOT NULL,
            processed_at TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_queue_status ON message_queue(status);
        CREATE INDEX IF NOT EXISTS idx_queue_created ON message_queue(created_at);
        ",
    )
    .map_err(|e| format!("Failed to create routines schema: {}", e))
}

// ============================================
// Cron Parser (P3: no dependency, simple 5-field)
// ============================================

/// Parse a 5-field cron expression and check if the current time matches.
///
/// Supports:
///   *       — match any value
///   N       — match specific value (e.g., "9" for 9am)
///   */N     — match every N steps (e.g., "*/5" for every 5 minutes)
///   N,M     — match multiple values (e.g., "1,15" for 1st and 15th)
///
/// Fields: minute(0-59) hour(0-23) day(1-31) month(1-12) weekday(0-6, 0=Sun)
pub fn cron_matches_now(expr: &str) -> bool {
    let now = Local::now();
    cron_matches(
        expr,
        now.minute(),
        now.hour(),
        now.day(),
        now.month(),
        now.weekday().num_days_from_sunday(),
    )
}

/// Internal: check if a cron expression matches specific time components.
fn cron_matches(expr: &str, minute: u32, hour: u32, day: u32, month: u32, weekday: u32) -> bool {
    let fields: Vec<&str> = expr.trim().split_whitespace().collect();
    if fields.len() != 5 {
        eprintln!("[HIVE ROUTINES] Invalid cron expression (need 5 fields): '{}'", expr);
        return false;
    }

    field_matches(fields[0], minute)
        && field_matches(fields[1], hour)
        && field_matches(fields[2], day)
        && field_matches(fields[3], month)
        && field_matches(fields[4], weekday)
}

/// Check if a single cron field matches a value.
fn field_matches(field: &str, value: u32) -> bool {
    // Wildcard
    if field == "*" {
        return true;
    }

    // Step: */N
    if let Some(step_str) = field.strip_prefix("*/") {
        if let Ok(step) = step_str.parse::<u32>() {
            if step == 0 {
                return false;
            }
            return value % step == 0;
        }
        return false;
    }

    // List: N,M,K
    if field.contains(',') {
        return field.split(',').any(|part| {
            part.trim().parse::<u32>().map(|v| v == value).unwrap_or(false)
        });
    }

    // Range: N-M
    if field.contains('-') {
        let parts: Vec<&str> = field.split('-').collect();
        if parts.len() == 2 {
            if let (Ok(lo), Ok(hi)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                return value >= lo && value <= hi;
            }
        }
        return false;
    }

    // Specific value
    field.parse::<u32>().map(|v| v == value).unwrap_or(false)
}

// ============================================
// Event Pattern Matching
// ============================================

/// Check if a channel event matches a routine's event pattern + keyword filter.
///
/// Patterns:
///   "channel:telegram"  — match all Telegram messages
///   "channel:discord"   — match all Discord messages
///   "channel:*"         — match any channel message
///
/// If event_keyword is set, the message text must contain it (case-insensitive).
pub fn event_matches(
    event: &ChannelEvent,
    pattern: Option<&str>,
    keyword: Option<&str>,
) -> bool {
    // Check channel pattern
    let pattern_ok = match pattern {
        None => false, // No pattern = no match
        Some(p) => {
            if p == "channel:*" {
                true
            } else if let Some(channel) = p.strip_prefix("channel:") {
                event.channel_type == channel
            } else {
                false
            }
        }
    };

    if !pattern_ok {
        return false;
    }

    // Check keyword filter
    match keyword {
        None => true, // No keyword filter = match all
        Some(kw) if kw.is_empty() => true,
        Some(kw) => event.text.to_lowercase().contains(&kw.to_lowercase()),
    }
}

// ============================================
// ID generation
// ============================================

fn generate_routine_id() -> String {
    let timestamp = Utc::now().timestamp_millis();
    let random: u64 = rand::random();
    format!("routine_{:x}_{:x}", timestamp, random)
}

fn generate_queue_id() -> String {
    let timestamp = Utc::now().timestamp_millis();
    let random: u64 = rand::random();
    format!("qmsg_{:x}_{:x}", timestamp, random)
}

// ============================================
// Routines CRUD (Tauri Commands)
// ============================================

/// Create a new routine.
#[tauri::command]
pub fn routine_create(
    memory_state: tauri::State<'_, MemoryState>,
    name: String,
    description: String,
    trigger_type: String,
    cron_expr: Option<String>,
    event_pattern: Option<String>,
    event_keyword: Option<String>,
    action_prompt: String,
    response_channel: Option<String>,
) -> Result<Routine, String> {
    // Validate
    let tt = TriggerType::from_str(&trigger_type);
    match &tt {
        TriggerType::Cron | TriggerType::Both => {
            let expr = cron_expr.as_deref().unwrap_or("");
            if expr.is_empty() {
                return Err("Cron trigger requires a cron_expr".to_string());
            }
            let fields: Vec<&str> = expr.trim().split_whitespace().collect();
            if fields.len() != 5 {
                return Err(format!(
                    "Invalid cron expression: expected 5 fields, got {}. Format: 'minute hour day month weekday'",
                    fields.len()
                ));
            }
        }
        TriggerType::Event => {
            if event_pattern.as_deref().unwrap_or("").is_empty() {
                return Err("Event trigger requires an event_pattern (e.g. 'channel:telegram', 'channel:*')".to_string());
            }
        }
    }

    let db_guard = memory_state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized. Call memory_init first.")?;

    let now = Utc::now().to_rfc3339();
    let id = generate_routine_id();

    conn.execute(
        "INSERT INTO routines (id, name, description, enabled, trigger_type, cron_expr, event_pattern, event_keyword, action_prompt, response_channel, created_at, updated_at)
         VALUES (?1, ?2, ?3, 1, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            id,
            name,
            description,
            tt.as_str(),
            cron_expr,
            event_pattern,
            event_keyword,
            action_prompt,
            response_channel,
            now,
            now,
        ],
    )
    .map_err(|e| format!("Failed to create routine: {}", e))?;

    crate::tools::log_tools::append_to_app_log(&format!(
        "ROUTINES | created | id={} name={} trigger={}", id, name, tt.as_str()
    ));

    Ok(Routine {
        id,
        name,
        description,
        enabled: true,
        trigger_type: tt,
        cron_expr,
        event_pattern,
        event_keyword,
        action_prompt,
        response_channel,
        run_count: 0,
        success_count: 0,
        fail_count: 0,
        last_run: None,
        last_result: None,
        created_at: now.clone(),
        updated_at: now,
    })
}

/// List all routines.
#[tauri::command]
pub fn routine_list(
    memory_state: tauri::State<'_, MemoryState>,
) -> Result<Vec<Routine>, String> {
    let db_guard = memory_state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;

    let mut stmt = conn.prepare(
        "SELECT id, name, description, enabled, trigger_type, cron_expr, event_pattern, event_keyword,
                action_prompt, response_channel, run_count, success_count, fail_count,
                last_run, last_result, created_at, updated_at
         FROM routines ORDER BY created_at DESC"
    ).map_err(|e| format!("Query failed: {}", e))?;

    let routines = stmt
        .query_map([], |row| {
            Ok(Routine {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                enabled: row.get::<_, i64>(3)? != 0,
                trigger_type: TriggerType::from_str(
                    &row.get::<_, String>(4).unwrap_or_else(|_| "event".to_string()),
                ),
                cron_expr: row.get(5)?,
                event_pattern: row.get(6)?,
                event_keyword: row.get(7)?,
                action_prompt: row.get(8)?,
                response_channel: row.get(9)?,
                run_count: row.get(10)?,
                success_count: row.get(11)?,
                fail_count: row.get(12)?,
                last_run: row.get(13)?,
                last_result: row.get(14)?,
                created_at: row.get(15)?,
                updated_at: row.get(16)?,
            })
        })
        .map_err(|e| format!("Query failed: {}", e))?
        .filter_map(|r| match r {
            Ok(v) => Some(v),
            Err(e) => { eprintln!("[HIVE] Routine row deserialization error (skipped): {}", e); None }
        })
        .collect();

    Ok(routines)
}

/// Update a routine.
#[tauri::command]
pub fn routine_update(
    memory_state: tauri::State<'_, MemoryState>,
    id: String,
    name: Option<String>,
    description: Option<String>,
    enabled: Option<bool>,
    trigger_type: Option<String>,
    cron_expr: Option<String>,
    event_pattern: Option<String>,
    event_keyword: Option<String>,
    action_prompt: Option<String>,
    response_channel: Option<String>,
) -> Result<String, String> {
    let db_guard = memory_state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;

    // Verify the routine exists before updating (P4: Errors Are Answers)
    let exists: bool = conn.query_row(
        "SELECT COUNT(*) FROM routines WHERE id = ?1",
        params![id],
        |row| row.get::<_, i64>(0),
    ).map(|c| c > 0)
    .map_err(|e| format!("DB error: {}", e))?;

    if !exists {
        return Err(format!("Routine '{}' not found", id));
    }

    let now = Utc::now().to_rfc3339();

    // Update each provided field individually.
    // rusqlite doesn't support dynamic param lists easily, so individual updates
    // are the simplest correct approach (P3: Simplicity Wins).

    if let Some(ref n) = name {
        conn.execute(
            "UPDATE routines SET name = ?1, updated_at = ?2 WHERE id = ?3",
            params![n, now, id],
        ).map_err(|e| format!("Update failed: {}", e))?;
    }
    if let Some(ref d) = description {
        conn.execute(
            "UPDATE routines SET description = ?1, updated_at = ?2 WHERE id = ?3",
            params![d, now, id],
        ).map_err(|e| format!("Update failed: {}", e))?;
    }
    if let Some(e) = enabled {
        conn.execute(
            "UPDATE routines SET enabled = ?1, updated_at = ?2 WHERE id = ?3",
            params![e as i64, now, id],
        ).map_err(|e| format!("Update failed: {}", e))?;
    }
    if let Some(ref tt) = trigger_type {
        conn.execute(
            "UPDATE routines SET trigger_type = ?1, updated_at = ?2 WHERE id = ?3",
            params![tt, now, id],
        ).map_err(|e| format!("Update failed: {}", e))?;
    }
    if let Some(ref ce) = cron_expr {
        conn.execute(
            "UPDATE routines SET cron_expr = ?1, updated_at = ?2 WHERE id = ?3",
            params![ce, now, id],
        ).map_err(|e| format!("Update failed: {}", e))?;
    }
    if let Some(ref ep) = event_pattern {
        conn.execute(
            "UPDATE routines SET event_pattern = ?1, updated_at = ?2 WHERE id = ?3",
            params![ep, now, id],
        ).map_err(|e| format!("Update failed: {}", e))?;
    }
    if let Some(ref ek) = event_keyword {
        conn.execute(
            "UPDATE routines SET event_keyword = ?1, updated_at = ?2 WHERE id = ?3",
            params![ek, now, id],
        ).map_err(|e| format!("Update failed: {}", e))?;
    }
    if let Some(ref ap) = action_prompt {
        conn.execute(
            "UPDATE routines SET action_prompt = ?1, updated_at = ?2 WHERE id = ?3",
            params![ap, now, id],
        ).map_err(|e| format!("Update failed: {}", e))?;
    }
    if let Some(ref rc) = response_channel {
        conn.execute(
            "UPDATE routines SET response_channel = ?1, updated_at = ?2 WHERE id = ?3",
            params![rc, now, id],
        ).map_err(|e| format!("Update failed: {}", e))?;
    }

    Ok(format!("Routine {} updated", id))
}

/// Delete a routine.
#[tauri::command]
pub fn routine_delete(
    memory_state: tauri::State<'_, MemoryState>,
    id: String,
) -> Result<String, String> {
    let db_guard = memory_state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;

    let affected = conn
        .execute("DELETE FROM routines WHERE id = ?1", params![id])
        .map_err(|e| format!("Delete failed: {}", e))?;

    if affected == 0 {
        Err(format!("Routine '{}' not found", id))
    } else {
        Ok(format!("Routine '{}' deleted", id))
    }
}

/// Record that a routine ran (update stats).
#[tauri::command]
pub fn routine_record_run(
    memory_state: tauri::State<'_, MemoryState>,
    id: String,
    success: bool,
    result_summary: Option<String>,
) -> Result<String, String> {
    let db_guard = memory_state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;

    let now = Utc::now().to_rfc3339();

    if success {
        conn.execute(
            "UPDATE routines SET run_count = run_count + 1, success_count = success_count + 1,
             last_run = ?1, last_result = ?2, updated_at = ?1 WHERE id = ?3",
            params![now, result_summary, id],
        )
        .map_err(|e| format!("Update failed: {}", e))?;
    } else {
        conn.execute(
            "UPDATE routines SET run_count = run_count + 1, fail_count = fail_count + 1,
             last_run = ?1, last_result = ?2, updated_at = ?1 WHERE id = ?3",
            params![now, result_summary, id],
        )
        .map_err(|e| format!("Update failed: {}", e))?;
    }

    Ok(format!("Routine {} run recorded (success={})", id, success))
}

/// Get routines engine stats.
#[tauri::command]
pub fn routine_stats(
    memory_state: tauri::State<'_, MemoryState>,
) -> Result<RoutineStats, String> {
    let db_guard = memory_state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;

    let total: i64 = conn
        .query_row("SELECT COUNT(*) FROM routines", [], |r| r.get(0))
        .unwrap_or(0);
    let enabled: i64 = conn
        .query_row("SELECT COUNT(*) FROM routines WHERE enabled = 1", [], |r| r.get(0))
        .unwrap_or(0);
    let runs: i64 = conn
        .query_row("SELECT COALESCE(SUM(run_count), 0) FROM routines", [], |r| r.get(0))
        .unwrap_or(0);
    let successes: i64 = conn
        .query_row("SELECT COALESCE(SUM(success_count), 0) FROM routines", [], |r| r.get(0))
        .unwrap_or(0);
    let failures: i64 = conn
        .query_row("SELECT COALESCE(SUM(fail_count), 0) FROM routines", [], |r| r.get(0))
        .unwrap_or(0);
    let q_pending: i64 = conn
        .query_row("SELECT COUNT(*) FROM message_queue WHERE status = 'pending'", [], |r| r.get(0))
        .unwrap_or(0);
    let q_processing: i64 = conn
        .query_row("SELECT COUNT(*) FROM message_queue WHERE status = 'processing'", [], |r| r.get(0))
        .unwrap_or(0);
    let q_dead: i64 = conn
        .query_row("SELECT COUNT(*) FROM message_queue WHERE status = 'dead'", [], |r| r.get(0))
        .unwrap_or(0);

    Ok(RoutineStats {
        total_routines: total,
        enabled_routines: enabled,
        total_runs: runs,
        total_successes: successes,
        total_failures: failures,
        queue_pending: q_pending,
        queue_processing: q_processing,
        queue_dead: q_dead,
    })
}

// ============================================
// Message Queue Operations (Tauri Commands)
// ============================================

/// Enqueue a channel message for reliable processing.
/// Called by channel daemons instead of (or in addition to) direct event emission.
#[tauri::command]
pub fn queue_enqueue(
    memory_state: tauri::State<'_, MemoryState>,
    channel_type: String,
    channel_id: String,
    sender_name: String,
    sender_id: String,
    text: String,
    wrapped_text: String,
    routine_id: Option<String>,
) -> Result<String, String> {
    let db_guard = memory_state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;

    let id = generate_queue_id();
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO message_queue (id, channel_type, channel_id, sender_name, sender_id, text, wrapped_text, status, routine_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', ?8, ?9)",
        params![id, channel_type, channel_id, sender_name, sender_id, text, wrapped_text, routine_id, now],
    )
    .map_err(|e| format!("Enqueue failed: {}", e))?;

    Ok(id)
}

/// Dequeue the next pending message for processing.
/// Atomically marks it as "processing" and returns it.
#[tauri::command]
pub fn queue_dequeue(
    memory_state: tauri::State<'_, MemoryState>,
) -> Result<Option<QueuedMessage>, String> {
    let db_guard = memory_state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;

    let now = Utc::now().to_rfc3339();

    // Get oldest pending message
    let msg = conn.query_row(
        "SELECT id, channel_type, channel_id, sender_name, sender_id, text, wrapped_text,
                status, attempts, max_attempts, error, routine_id, created_at, processed_at
         FROM message_queue WHERE status = 'pending' ORDER BY created_at ASC LIMIT 1",
        [],
        |row| {
            Ok(QueuedMessage {
                id: row.get(0)?,
                channel_type: row.get(1)?,
                channel_id: row.get(2)?,
                sender_name: row.get(3)?,
                sender_id: row.get(4)?,
                text: row.get(5)?,
                wrapped_text: row.get(6)?,
                status: QueueStatus::from_str(&row.get::<_, String>(7)?),
                attempts: row.get(8)?,
                max_attempts: row.get(9)?,
                error: row.get(10)?,
                routine_id: row.get(11)?,
                created_at: row.get(12)?,
                processed_at: row.get(13)?,
            })
        },
    );

    match msg {
        Ok(mut m) => {
            // Mark as processing
            conn.execute(
                "UPDATE message_queue SET status = 'processing', attempts = attempts + 1, processed_at = ?1 WHERE id = ?2",
                params![now, m.id],
            )
            .map_err(|e| format!("Failed to mark processing: {}", e))?;

            m.status = QueueStatus::Processing;
            m.attempts += 1;
            m.processed_at = Some(now);
            Ok(Some(m))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(format!("Dequeue failed: {}", e)),
    }
}

/// Mark a queued message as completed.
#[tauri::command]
pub fn queue_complete(
    memory_state: tauri::State<'_, MemoryState>,
    id: String,
) -> Result<String, String> {
    let db_guard = memory_state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;

    conn.execute(
        "UPDATE message_queue SET status = 'completed' WHERE id = ?1",
        params![id],
    )
    .map_err(|e| format!("Complete failed: {}", e))?;

    Ok(format!("Message {} completed", id))
}

/// Mark a queued message as failed. If max_attempts reached, move to dead-letter.
#[tauri::command]
pub fn queue_fail(
    memory_state: tauri::State<'_, MemoryState>,
    id: String,
    error: String,
) -> Result<String, String> {
    let db_guard = memory_state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;

    // Check if we've exceeded max attempts
    let (attempts, max): (i64, i64) = conn
        .query_row(
            "SELECT attempts, max_attempts FROM message_queue WHERE id = ?1",
            params![id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|e| format!("Query failed: {}", e))?;

    let new_status = if attempts >= max { "dead" } else { "failed" };

    // Failed messages can be retried (re-queued as pending); dead ones cannot.
    let final_status = if new_status == "failed" { "pending" } else { "dead" };

    conn.execute(
        "UPDATE message_queue SET status = ?1, error = ?2 WHERE id = ?3",
        params![final_status, error, id],
    )
    .map_err(|e| format!("Fail update failed: {}", e))?;

    if final_status == "dead" {
        eprintln!("[HIVE ROUTINES] Message {} moved to dead-letter after {} attempts: {}", id, attempts, error);
        Ok(format!("Message {} dead-lettered after {} attempts", id, attempts))
    } else {
        Ok(format!("Message {} failed (attempt {}/{}), will retry", id, attempts, max))
    }
}

/// Get queue status summary.
#[tauri::command]
pub fn queue_status(
    memory_state: tauri::State<'_, MemoryState>,
) -> Result<Vec<QueuedMessage>, String> {
    let db_guard = memory_state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;

    let mut stmt = conn.prepare(
        "SELECT id, channel_type, channel_id, sender_name, sender_id, text, wrapped_text,
                status, attempts, max_attempts, error, routine_id, created_at, processed_at
         FROM message_queue WHERE status IN ('pending', 'processing', 'dead')
         ORDER BY created_at ASC LIMIT 50"
    ).map_err(|e| format!("Query failed: {}", e))?;

    let messages = stmt
        .query_map([], |row| {
            Ok(QueuedMessage {
                id: row.get(0)?,
                channel_type: row.get(1)?,
                channel_id: row.get(2)?,
                sender_name: row.get(3)?,
                sender_id: row.get(4)?,
                text: row.get(5)?,
                wrapped_text: row.get(6)?,
                status: QueueStatus::from_str(&row.get::<_, String>(7)?),
                attempts: row.get(8)?,
                max_attempts: row.get(9)?,
                error: row.get(10)?,
                routine_id: row.get(11)?,
                created_at: row.get(12)?,
                processed_at: row.get(13)?,
            })
        })
        .map_err(|e| format!("Query failed: {}", e))?
        .filter_map(|r| match r {
            Ok(v) => Some(v),
            Err(e) => { eprintln!("[HIVE] Routine row deserialization error (skipped): {}", e); None }
        })
        .collect();

    Ok(messages)
}

/// Purge completed messages older than 24 hours to prevent unbounded growth.
#[tauri::command]
pub fn queue_purge_completed(
    memory_state: tauri::State<'_, MemoryState>,
) -> Result<String, String> {
    let db_guard = memory_state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;

    let cutoff = (Utc::now() - chrono::Duration::hours(24)).to_rfc3339();

    let deleted = conn
        .execute(
            "DELETE FROM message_queue WHERE status = 'completed' AND created_at < ?1",
            params![cutoff],
        )
        .map_err(|e| format!("Purge failed: {}", e))?;

    Ok(format!("Purged {} completed messages", deleted))
}

// ============================================
// Cron Evaluation Loop
// ============================================

/// Evaluate all cron-triggered routines against the current time.
/// Returns a list of RoutineTriggered events for routines whose cron matches now.
/// Called by the background Tokio task every 60 seconds.
pub fn evaluate_cron_routines(conn: &rusqlite::Connection) -> Vec<RoutineTriggered> {
    let mut triggered = Vec::new();

    let mut stmt = match conn.prepare(
        "SELECT id, name, action_prompt, response_channel, cron_expr
         FROM routines
         WHERE enabled = 1 AND trigger_type IN ('cron', 'both') AND cron_expr IS NOT NULL"
    ) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[HIVE ROUTINES] Failed to query cron routines: {}", e);
            return triggered;
        }
    };

    let rows: Vec<(String, String, String, Option<String>, String)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .ok()
        .map(|rows| rows.filter_map(|r| match r {
            Ok(v) => Some(v),
            Err(e) => { eprintln!("[HIVE] Routine row deserialization error (skipped): {}", e); None }
        }).collect())
        .unwrap_or_default();

    for (id, name, action_prompt, response_channel, cron_expr) in rows {
        if cron_matches_now(&cron_expr) {
            eprintln!("[HIVE ROUTINES] Cron triggered: {} ({})", name, cron_expr);
            triggered.push(RoutineTriggered {
                routine_id: id,
                routine_name: name,
                action_prompt,
                response_channel,
                trigger_reason: format!("Cron match: {}", cron_expr),
                source_event: None,
                queue_id: None,
            });
        }
    }

    triggered
}

/// Evaluate all event-triggered routines against a channel event.
/// Returns a list of RoutineTriggered events for matching routines.
pub fn evaluate_event_routines(
    conn: &rusqlite::Connection,
    event: &ChannelEvent,
) -> Vec<RoutineTriggered> {
    let mut triggered = Vec::new();

    let mut stmt = match conn.prepare(
        "SELECT id, name, action_prompt, response_channel, event_pattern, event_keyword
         FROM routines
         WHERE enabled = 1 AND trigger_type IN ('event', 'both')"
    ) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[HIVE ROUTINES] Failed to query event routines: {}", e);
            return triggered;
        }
    };

    let rows: Vec<(String, String, String, Option<String>, Option<String>, Option<String>)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })
        .ok()
        .map(|rows| rows.filter_map(|r| match r {
            Ok(v) => Some(v),
            Err(e) => { eprintln!("[HIVE] Routine row deserialization error (skipped): {}", e); None }
        }).collect())
        .unwrap_or_default();

    for (id, name, action_prompt, response_channel, pattern, keyword) in rows {
        if event_matches(event, pattern.as_deref(), keyword.as_deref()) {
            eprintln!(
                "[HIVE ROUTINES] Event triggered: {} (pattern={:?}, keyword={:?})",
                name, pattern, keyword
            );

            // Inject the source message into the action prompt
            let enriched_prompt = format!(
                "{}\n\n[Source: {} message from {} in {}]\n{}",
                action_prompt,
                event.channel_type,
                event.sender_name,
                event.channel_id,
                event.wrapped_text,
            );

            triggered.push(RoutineTriggered {
                routine_id: id,
                routine_name: name,
                action_prompt: enriched_prompt,
                response_channel,
                trigger_reason: format!(
                    "Event match: {} from {} (@{})",
                    event.channel_type, event.sender_name, event.sender_id
                ),
                source_event: Some(event.clone()),
                queue_id: None,
            });
        }
    }

    triggered
}

// ============================================
// Background Cron Loop (spawned from main.rs)
// ============================================

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// State for the routines cron daemon.
pub struct RoutinesDaemonState {
    pub running: Arc<AtomicBool>,
    handle: tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl Default for RoutinesDaemonState {
    fn default() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            handle: tokio::sync::Mutex::new(None),
        }
    }
}

/// Start the routines cron evaluation daemon.
/// Runs every 60 seconds, checks cron expressions, emits "routine-triggered" events.
#[tauri::command]
pub async fn routines_daemon_start(
    app: tauri::AppHandle,
    memory_state: tauri::State<'_, MemoryState>,
    daemon_state: tauri::State<'_, RoutinesDaemonState>,
) -> Result<String, String> {
    // Atomically claim — prevents concurrent starts (Q4 fix: TOCTOU)
    if daemon_state.running.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
        return Ok("Routines daemon already running".to_string());
    }

    // Ensure DB is initialized (schema includes routines tables)
    {
        let db_guard = memory_state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
        if db_guard.is_none() {
            daemon_state.running.store(false, Ordering::SeqCst);
            return Err("Memory DB not initialized. Call memory_init first.".to_string());
        }
    }

    // running was already set to true by compare_exchange above
    let running = daemon_state.running.clone();

    // We need to re-open a separate DB connection for the background task
    // because MemoryState uses std::sync::Mutex which isn't Send across await points.
    let db_path = crate::paths::get_app_data_dir().join("memory.db");

    let handle = tokio::spawn(async move {
        use tauri::Emitter;

        eprintln!("[HIVE ROUTINES] Cron daemon started (60s tick)");
        crate::tools::log_tools::append_to_app_log("ROUTINES | cron_daemon_started | 60s tick");

        // Open our own connection (WAL mode allows concurrent readers)
        let conn = match rusqlite::Connection::open(&db_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[HIVE ROUTINES] Failed to open DB: {}", e);
                return;
            }
        };
        if let Err(e) = conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000; PRAGMA foreign_keys=ON;") {
            eprintln!("[HIVE] Routines daemon: PRAGMA setup failed: {}", e);
        }

        while running.load(Ordering::Relaxed) {
            // Evaluate cron routines
            let triggered = evaluate_cron_routines(&conn);

            for event in triggered {
                crate::tools::log_tools::append_to_app_log(&format!(
                    "ROUTINES | triggered | routine={} source=cron", event.routine_name
                ));
                if let Err(e) = app.emit("routine-triggered", &event) {
                    eprintln!("[HIVE ROUTINES] Failed to emit routine-triggered: {}", e);
                    crate::tools::log_tools::append_to_app_log(&format!("ROUTINES | emit_error | {}", e));
                }
            }

            // Also purge old completed queue messages (housekeeping)
            let cutoff = (Utc::now() - chrono::Duration::hours(24)).to_rfc3339();
            let _ = conn.execute(
                "DELETE FROM message_queue WHERE status = 'completed' AND created_at < ?1",
                params![cutoff],
            );

            // Sleep 60 seconds (cron granularity is 1 minute)
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }

        eprintln!("[HIVE ROUTINES] Cron daemon stopped");
        crate::tools::log_tools::append_to_app_log("ROUTINES | cron_daemon_stopped");
    });

    let mut h = daemon_state.handle.lock().await;
    *h = Some(handle);

    Ok("Routines cron daemon started".to_string())
}

/// Stop the routines cron daemon.
#[tauri::command]
pub async fn routines_daemon_stop(
    daemon_state: tauri::State<'_, RoutinesDaemonState>,
) -> Result<String, String> {
    if !daemon_state.running.load(Ordering::Relaxed) {
        return Ok("Routines daemon not running".to_string());
    }

    daemon_state.running.store(false, Ordering::Relaxed);

    let handle = {
        let mut h = daemon_state.handle.lock().await;
        h.take()
    };

    if let Some(h) = handle {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(65), h).await;
    }

    Ok("Routines daemon stopped".to_string())
}

/// Check if routines daemon is running.
#[tauri::command]
pub async fn routines_daemon_status(
    daemon_state: tauri::State<'_, RoutinesDaemonState>,
) -> Result<bool, String> {
    Ok(daemon_state.running.load(Ordering::Relaxed))
}

// ============================================
// Channel Event Evaluation (called from daemons)
// ============================================

/// Process a unified channel event: check event-triggered routines, enqueue if matched.
/// Called by the channel daemon wrappers when they emit ChannelEvent.
/// Returns the list of triggered routines (for immediate processing by frontend).
pub fn process_channel_event(
    conn: &rusqlite::Connection,
    event: &ChannelEvent,
) -> Vec<RoutineTriggered> {
    evaluate_event_routines(conn, event)
}

// ============================================
// Tests
// ============================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cron_wildcard() {
        assert!(cron_matches("* * * * *", 0, 0, 1, 1, 0));
        assert!(cron_matches("* * * * *", 59, 23, 31, 12, 6));
    }

    #[test]
    fn test_cron_specific() {
        assert!(cron_matches("0 9 * * *", 0, 9, 15, 6, 3)); // 9:00 AM
        assert!(!cron_matches("0 9 * * *", 0, 10, 15, 6, 3)); // 10:00 AM
        assert!(!cron_matches("0 9 * * *", 1, 9, 15, 6, 3)); // 9:01 AM
    }

    #[test]
    fn test_cron_step() {
        assert!(cron_matches("*/5 * * * *", 0, 12, 1, 1, 0)); // minute 0
        assert!(cron_matches("*/5 * * * *", 5, 12, 1, 1, 0)); // minute 5
        assert!(cron_matches("*/5 * * * *", 10, 12, 1, 1, 0)); // minute 10
        assert!(!cron_matches("*/5 * * * *", 3, 12, 1, 1, 0)); // minute 3
    }

    #[test]
    fn test_cron_list() {
        assert!(cron_matches("0 9,17 * * *", 0, 9, 1, 1, 0)); // 9 AM
        assert!(cron_matches("0 9,17 * * *", 0, 17, 1, 1, 0)); // 5 PM
        assert!(!cron_matches("0 9,17 * * *", 0, 12, 1, 1, 0)); // noon
    }

    #[test]
    fn test_cron_range() {
        assert!(cron_matches("* 9-17 * * *", 30, 9, 1, 1, 0)); // 9 AM
        assert!(cron_matches("* 9-17 * * *", 30, 12, 1, 1, 0)); // noon
        assert!(cron_matches("* 9-17 * * *", 30, 17, 1, 1, 0)); // 5 PM
        assert!(!cron_matches("* 9-17 * * *", 30, 8, 1, 1, 0)); // 8 AM
        assert!(!cron_matches("* 9-17 * * *", 30, 18, 1, 1, 0)); // 6 PM
    }

    #[test]
    fn test_cron_weekday() {
        // "Every weekday at 9am" = 0 9 * * 1-5
        assert!(cron_matches("0 9 * * 1-5", 0, 9, 1, 1, 1)); // Monday
        assert!(cron_matches("0 9 * * 1-5", 0, 9, 1, 1, 5)); // Friday
        assert!(!cron_matches("0 9 * * 1-5", 0, 9, 1, 1, 0)); // Sunday
        assert!(!cron_matches("0 9 * * 1-5", 0, 9, 1, 1, 6)); // Saturday
    }

    #[test]
    fn test_event_matching() {
        let event = ChannelEvent {
            channel_type: "telegram".to_string(),
            channel_id: "12345".to_string(),
            sender_name: "Alice".to_string(),
            sender_id: "alice123".to_string(),
            text: "urgent: server is down!".to_string(),
            wrapped_text: "urgent: server is down!".to_string(),
            raw_event_id: "1".to_string(),
            metadata: serde_json::json!({}),
        };

        // Channel match
        assert!(event_matches(&event, Some("channel:telegram"), None));
        assert!(event_matches(&event, Some("channel:*"), None));
        assert!(!event_matches(&event, Some("channel:discord"), None));

        // Keyword match
        assert!(event_matches(&event, Some("channel:telegram"), Some("urgent")));
        assert!(event_matches(&event, Some("channel:telegram"), Some("URGENT")));
        assert!(!event_matches(&event, Some("channel:telegram"), Some("not-here")));

        // No pattern = no match
        assert!(!event_matches(&event, None, None));
    }
}

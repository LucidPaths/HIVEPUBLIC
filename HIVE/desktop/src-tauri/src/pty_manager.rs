//! PTY Manager — Phase 10 NEXUS terminal pane backend
//!
//! Spawns pseudo-terminal sessions for CLI agents (shell, Claude Code, Codex, etc.)
//! and bridges them to the frontend via Tauri events.
//!
//! Architecture:
//!   - Each session: PTY pair (master + slave) → spawn child → reader thread → Tauri events
//!   - Reader thread: dedicated OS thread (blocking I/O on file descriptor, not async)
//!   - Events: "pty-output" (data), "pty-exit" (process terminated)
//!
//! Principle alignment:
//!   P1 (Modularity)  — Self-contained module. Remove it → chat panes still work.
//!   P2 (Agnostic)    — Any CLI agent is { command, args }. Shell, Claude Code, Codex — same pipe.
//!   P3 (Simplicity)  — portable-pty (Wezterm) does the heavy lifting. We write glue.
//!   P7 (Framework)   — New agent? One entry in BUILTIN_AGENTS. Done.

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{Read, Write};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde::Serialize;
use tauri::Emitter;
use uuid::Uuid;

/// Max lines stored per PTY session for cross-agent output reading.
const OUTPUT_BUFFER_MAX_LINES: usize = 500;

// ============================================
// Agent Bridge — silence-based response detection
// ============================================
// Detects when a PTY agent has finished producing output (silence after content)
// and emits an "agent-response" event so the orchestrating model can see the output.
// Reuses worker anti-spam patterns: rate limiting, semantic dedup, content gates.

/// How long output must be silent before we consider the agent "done talking".
/// Set higher than thinking→response gaps (~8-15s for extended thinking models)
/// to avoid delivering thinking blocks before the actual response arrives.
const BRIDGE_SILENCE_THRESHOLD: Duration = Duration::from_secs(12);
/// Minimum time between deliveries to prevent chat spam during bursty output.
const BRIDGE_MIN_DELIVERY_INTERVAL: Duration = Duration::from_secs(8);
/// Don't deliver trivial output (prompt chars, blank lines, status indicators).
const BRIDGE_MIN_CONTENT_CHARS: usize = 30;
/// Max chars delivered inline. Longer output gets truncated with a read_agent_output pointer.
const BRIDGE_MAX_INLINE_CHARS: usize = 4000;
/// Lines shown from start/end when truncating long output.
const BRIDGE_PREVIEW_LINES: usize = 50;
/// Jaccard word similarity threshold — above this, output is considered repetitive.
const BRIDGE_DEDUP_THRESHOLD: f64 = 0.70;
/// Monitor thread tick interval.
const BRIDGE_MONITOR_TICK: Duration = Duration::from_secs(1);

// ============================================
// Global App Handle (for emitting events to the frontend)
// ============================================

static APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();

/// Store the Tauri AppHandle so PTY reader threads can emit output events.
/// Called once from main.rs `.setup()`.
pub fn set_app_handle(handle: tauri::AppHandle) {
    let _ = APP_HANDLE.set(handle);
}

fn emit_pty_event(event_name: &str, payload: &serde_json::Value) {
    if let Some(app) = APP_HANDLE.get() {
        let _ = app.emit(event_name, payload);
    }
}

// ============================================
// Global Sessions State
// ============================================
// Sessions are stored in a global OnceLock (same pattern as worker_tools::WORKERS)
// so that both Tauri commands AND HiveTools (agent_tools::SendToAgentTool) can
// access the same session map. HiveTool::execute() doesn't receive Tauri State,
// so global access is required for cross-module tool integration.

static SESSIONS: OnceLock<Mutex<HashMap<String, PtySession>>> = OnceLock::new();

fn sessions() -> &'static Mutex<HashMap<String, PtySession>> {
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Per-session circular output buffer (ANSI-stripped clean text).
/// Separate lock from SESSIONS to avoid holding session lock during reads.
static OUTPUT_BUFFERS: OnceLock<Mutex<HashMap<String, VecDeque<String>>>> = OnceLock::new();

fn output_buffers() -> &'static Mutex<HashMap<String, VecDeque<String>>> {
    OUTPUT_BUFFERS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Per-session bridge state for silence-based response detection.
/// Tracks accumulated output, timing, and dedup state.
struct BridgeState {
    agent_name: String,
    accumulated_output: String,
    last_output_time: Instant,
    last_delivery_time: Instant,
    last_delivery_words: Vec<String>,
}

impl BridgeState {
    fn new(agent_name: &str) -> Self {
        Self {
            agent_name: agent_name.to_string(),
            accumulated_output: String::new(),
            last_output_time: Instant::now(),
            last_delivery_time: Instant::now() - BRIDGE_MIN_DELIVERY_INTERVAL, // allow immediate first delivery
            last_delivery_words: Vec::new(),
        }
    }
}

/// Global bridge state — only sessions with bridge_to_chat=true have entries.
static AGENT_BRIDGE: OnceLock<Mutex<HashMap<String, BridgeState>>> = OnceLock::new();

fn agent_bridge() -> &'static Mutex<HashMap<String, BridgeState>> {
    AGENT_BRIDGE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Flag to ensure the monitor thread is only spawned once.
static BRIDGE_MONITOR_STARTED: OnceLock<()> = OnceLock::new();

/// Start the bridge monitor thread (idempotent — only spawns once).
fn ensure_bridge_monitor() {
    BRIDGE_MONITOR_STARTED.get_or_init(|| {
        std::thread::Builder::new()
            .name("agent-bridge-monitor".to_string())
            .spawn(bridge_monitor_loop)
            .expect("Failed to spawn agent bridge monitor thread");
    });
}

/// Monitor loop: ticks every second, checks all bridged sessions for silence.
fn bridge_monitor_loop() {
    loop {
        std::thread::sleep(BRIDGE_MONITOR_TICK);

        let mut bridge = match agent_bridge().lock() {
            Ok(b) => b,
            Err(_) => continue,
        };

        let mut to_deliver: Vec<(String, String, String)> = Vec::new(); // (session_id, agent_name, content)

        for (session_id, state) in bridge.iter_mut() {
            // Gate 1: Has accumulated content?
            if state.accumulated_output.is_empty() {
                continue;
            }

            // Gate 2: Silent for long enough?
            if state.last_output_time.elapsed() < BRIDGE_SILENCE_THRESHOLD {
                continue;
            }

            // Gate 3: Rate limit — don't deliver too frequently
            if state.last_delivery_time.elapsed() < BRIDGE_MIN_DELIVERY_INTERVAL {
                continue;
            }

            // Gate 4: Minimum content length — skip trivial output
            let trimmed_len = state.accumulated_output.trim().len();
            if trimmed_len < BRIDGE_MIN_CONTENT_CHARS {
                // Clear trivial output so it doesn't keep triggering checks
                state.accumulated_output.clear();
                continue;
            }

            // Gate 5: Semantic dedup — skip repetitive output
            let current_words = extract_words(&state.accumulated_output);
            if !state.last_delivery_words.is_empty() {
                let similarity = jaccard_similarity(&current_words, &state.last_delivery_words);
                if similarity > BRIDGE_DEDUP_THRESHOLD {
                    state.accumulated_output.clear();
                    continue;
                }
            }

            // All gates passed — prepare delivery
            let content = truncate_for_delivery(&state.accumulated_output, session_id);
            to_deliver.push((session_id.clone(), state.agent_name.clone(), content));

            // Update state
            state.last_delivery_time = Instant::now();
            state.last_delivery_words = current_words;
            state.accumulated_output.clear();
        }

        // Release lock before emitting events (avoid holding lock during I/O)
        drop(bridge);

        for (session_id, agent_name, content) in to_deliver {
            emit_pty_event(
                "agent-response",
                &serde_json::json!({
                    "session_id": session_id,
                    "agent_name": agent_name,
                    "content": content,
                }),
            );
            crate::tools::log_tools::append_to_app_log(&format!(
                "PTY | bridge_deliver | id={} agent={} chars={}",
                session_id, agent_name, content.len()
            ));
        }
    }
}

/// Truncate output for chat delivery. Short output passes through;
/// long output gets first N + last N lines with a pointer to full text.
fn truncate_for_delivery(output: &str, session_id: &str) -> String {
    if output.len() <= BRIDGE_MAX_INLINE_CHARS {
        return output.trim().to_string();
    }

    let lines: Vec<&str> = output.lines().collect();
    let total = lines.len();

    if total <= BRIDGE_PREVIEW_LINES * 2 {
        // Even though char count is high, line count is low — deliver all lines
        return output.trim().to_string();
    }

    let head: String = lines[..BRIDGE_PREVIEW_LINES].join("\n");
    let tail: String = lines[total - BRIDGE_PREVIEW_LINES..].join("\n");

    format!(
        "{}\n\n... ({} lines omitted) ...\n\n{}\n\n[Full output: {} lines — use read_agent_output('{}') for complete text]",
        head, total - BRIDGE_PREVIEW_LINES * 2, tail, total, session_id
    )
}

/// Extract lowercase words (>2 chars) for Jaccard similarity. Mirrors worker_tools pattern.
fn extract_words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|w| w.to_lowercase().trim_matches(|c: char| !c.is_alphanumeric()).to_string())
        .filter(|w| w.len() > 2)
        .collect()
}

/// Jaccard similarity between two word sets. 0.0 = no overlap, 1.0 = identical.
fn jaccard_similarity(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let set_a: HashSet<&str> = a.iter().map(|s| s.as_str()).collect();
    let set_b: HashSet<&str> = b.iter().map(|s| s.as_str()).collect();
    let intersection = set_a.intersection(&set_b).count();
    let union = set_a.union(&set_b).count();
    if union == 0 { return 0.0; }
    intersection as f64 / union as f64
}

// ============================================
// Types
// ============================================

/// A live PTY session. Holds the write end + master (for resize) + child process.
/// The read end is consumed by a dedicated OS thread that emits events.
///
/// When the process exits naturally, the session transitions to `exited` status:
/// OS resources (writer, master, child) are dropped, but metadata stays in the map
/// so the user can still see the session in their terminal pane scrollback.
/// The user removes it explicitly via pty_kill or closing the pane.
pub struct PtySession {
    pub id: String,
    pub command: String,
    pub writer: Option<Box<dyn Write + Send>>,
    pub master: Option<Box<dyn portable_pty::MasterPty + Send>>,
    pub child: Option<Box<dyn portable_pty::Child + Send + Sync>>,
    pub started_at: String,
    pub exited: bool,
}

/// Lightweight info returned by pty_list (no handles, just metadata).
#[derive(Debug, Clone, Serialize)]
pub struct PtySessionInfo {
    pub id: String,
    pub command: String,
    pub started_at: String,
    pub exited: bool,
}

// ============================================
// Public API (for agent_tools and other modules)
// ============================================

/// Write data to a PTY session's stdin. Used by agent_tools::SendToAgentTool.
pub fn write_to_session(session_id: &str, data: &str) -> Result<(), String> {
    let mut map = sessions()
        .lock()
        .map_err(|e| format!("Sessions lock poisoned: {}", e))?;

    let session = map
        .get_mut(session_id)
        .ok_or_else(|| format!("PTY session not found: {}", session_id))?;

    if session.exited {
        return Err(format!("PTY session {} has exited", session_id));
    }

    let writer = session.writer.as_mut()
        .ok_or_else(|| format!("PTY session {} writer already closed", session_id))?;

    // ConPTY on Windows expects \r (carriage return) to simulate Enter,
    // not \n (line feed). Models may send \n, \r\n, or \r — normalize all
    // to \r so the PTY receives a clean Enter keystroke. Order matters:
    // replace \r\n first to avoid \r\n → \r\r.
    let pty_data = data.replace("\r\n", "\r").replace('\n', "\r");
    writer
        .write_all(pty_data.as_bytes())
        .map_err(|e| format!("Failed to write to PTY: {}", e))?;

    writer
        .flush()
        .map_err(|e| format!("Failed to flush PTY writer: {}", e))?;

    Ok(())
}

/// List all active PTY sessions (metadata only). Used by agent_tools::SendToAgentTool.
pub fn list_sessions_info() -> Vec<PtySessionInfo> {
    let map = match sessions().lock() {
        Ok(m) => m,
        Err(_) => return Vec::new(),
    };

    map.values()
        .map(|s| PtySessionInfo {
            id: s.id.clone(),
            command: s.command.clone(),
            started_at: s.started_at.clone(),
            exited: s.exited,
        })
        .collect()
}

/// Read recent output lines from a PTY session's circular buffer.
/// Returns up to `max_lines` most recent lines (ANSI-stripped clean text).
/// Used by agent_tools::ReadAgentOutputTool for cross-agent visibility.
pub fn read_session_output(session_id: &str, max_lines: usize) -> Result<Vec<String>, String> {
    let map = output_buffers()
        .lock()
        .map_err(|e| format!("Output buffers lock poisoned: {}", e))?;

    match map.get(session_id) {
        Some(buf) => {
            let total = buf.len();
            let skip = total.saturating_sub(max_lines);
            Ok(buf.iter().skip(skip).cloned().collect())
        }
        None => {
            // Check if session exists but has no output yet
            let sessions_map = sessions()
                .lock()
                .map_err(|e| format!("Sessions lock poisoned: {}", e))?;
            if sessions_map.contains_key(session_id) {
                Ok(Vec::new()) // Session exists, no output yet
            } else {
                Err(format!("PTY session not found: {}", session_id))
            }
        }
    }
}

// ============================================
// Helpers
// ============================================

/// On Windows, npm-installed CLI tools (claude, codex, aider) install as a Unix
/// shell script + a `.cmd` batch wrapper. `CreateProcessW` can only execute the
/// `.cmd` variant (OS error 193 on the bare file). This function uses `where` to
/// check if the bare command resolves to a non-executable script, and if so,
/// returns the `.cmd` variant instead.
fn resolve_command_windows(command: &str) -> String {
    #[cfg(windows)]
    {
        // Skip resolution for commands that are already explicit paths or have extensions
        if command.contains('\\') || command.contains('/') || command.contains('.') {
            return command.to_string();
        }

        // Use `where` to find the command — same approach as check_agent_available
        if let Ok(output) = std::process::Command::new("where")
            .arg(command)
            .output()
        {
            if output.status.success() {
                let paths = String::from_utf8_lossy(&output.stdout);
                // `where` returns all matches, one per line. Check if any is a .cmd/.exe
                // If the first result is a bare file (no .exe/.cmd extension), prefer .cmd
                if let Some(first) = paths.lines().next() {
                    let first_lower = first.to_lowercase();
                    if !first_lower.ends_with(".exe") && !first_lower.ends_with(".cmd") && !first_lower.ends_with(".bat") {
                        // First match is a bare script — look for .cmd in remaining matches
                        for line in paths.lines() {
                            if line.to_lowercase().ends_with(".cmd") {
                                // Found a .cmd variant, use the command name with .cmd
                                return format!("{}.cmd", command);
                            }
                        }
                    }
                }
            }
        }
        command.to_string()
    }
    #[cfg(not(windows))]
    {
        command.to_string()
    }
}

// ============================================
// Tauri Commands
// ============================================

/// Spawn a new PTY session. Returns the session ID (UUID v4).
///
/// The spawned process runs in a pseudo-terminal with the given dimensions.
/// A background OS thread reads output and emits "pty-output" events.
/// When the process exits, a "pty-exit" event is emitted.
#[tauri::command]
pub async fn pty_spawn(
    command: String,
    args: Vec<String>,
    cols: u16,
    rows: u16,
    bridge_to_chat: Option<bool>,
) -> Result<String, String> {
    // Validate command — must be a simple program name, not a path or shell expression (P6, S7 fix)
    if command.is_empty() {
        return Err("Empty command".to_string());
    }
    // Reject path separators (prevents arbitrary binary execution like /usr/bin/evil)
    // and shell metacharacters (prevents injection like "bash;rm -rf /")
    if command.contains('\0') || command.contains("..") ||
       command.contains('/') || command.contains('\\') ||
       command.contains(';') || command.contains('|') || command.contains('&') ||
       command.contains('`') || command.contains('$') || command.contains('(') {
        return Err(format!(
            "Invalid command '{}' — must be a simple program name (no paths or shell operators). \
             Configure custom agents in Settings → Agents with a plain command name like 'python3'.",
            command
        ));
    }
    // Log what's being spawned for audit trail (P4)
    crate::tools::log_tools::append_to_app_log(
        &format!("PTY | spawn | command={} args={:?}", command, args),
    );

    let session_id = Uuid::new_v4().to_string();
    let pty_system = native_pty_system();

    // Open PTY pair
    let pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("Failed to open PTY: {}", e))?;

    // Build command — on Windows, npm-installed tools (claude, codex, aider) are
    // shell scripts with a .cmd wrapper. CreateProcessW can't execute the raw script
    // (OS error 193), so we resolve to the .cmd variant when needed.
    let resolved_command = resolve_command_windows(&command);

    let mut cmd = CommandBuilder::new(&resolved_command);
    for arg in &args {
        cmd.arg(arg);
    }

    // Spawn child in the slave end
    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| format!("Failed to spawn '{}': {}", command, e))?;

    // Drop slave — we only need the master end now
    drop(pair.slave);

    // Get reader (cloned from master) for the background thread
    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| format!("Failed to clone PTY reader: {}", e))?;

    // Get writer for sending input
    let writer = pair
        .master
        .take_writer()
        .map_err(|e| format!("Failed to take PTY writer: {}", e))?;

    let started_at = chrono::Utc::now().to_rfc3339();

    // Store session in global map
    let session = PtySession {
        id: session_id.clone(),
        command: command.clone(),
        writer: Some(writer),
        master: Some(pair.master),
        child: Some(child),
        started_at: started_at.clone(),
        exited: false,
    };

    sessions()
        .lock()
        .map_err(|e| format!("Sessions lock poisoned: {}", e))?
        .insert(session_id.clone(), session);

    // Enable bridge-to-chat if requested — output will be auto-injected into orchestrator chat
    if bridge_to_chat.unwrap_or(false) {
        if let Ok(mut br) = agent_bridge().lock() {
            br.insert(session_id.clone(), BridgeState::new(&command));
        }
        ensure_bridge_monitor();
        crate::tools::log_tools::append_to_app_log(&format!(
            "PTY | bridge_enabled | id={} cmd={}", session_id, command
        ));
    }

    // Spawn dedicated OS thread for reading PTY output
    // (blocking I/O on file descriptor — not async, same as Alacritty/Wezterm)
    let sid = session_id.clone();
    let agent_name = command.clone();
    std::thread::Builder::new()
        .name(format!("pty-reader-{}", sid.chars().take(8).collect::<String>()))
        .spawn(move || {
            pty_reader_loop(sid, agent_name, reader);
        })
        .map_err(|e| format!("Failed to spawn PTY reader thread: {}", e))?;

    crate::tools::log_tools::append_to_app_log(&format!(
        "PTY | spawned | id={} cmd={} args={:?} cols={} rows={}", session_id, command, args, cols, rows
    ));

    Ok(session_id)
}

/// Mark a session as exited: drop OS resources (writer, master, child)
/// but keep metadata in the map so the pane can still show scrollback.
/// User removes it explicitly via pty_kill or closing the pane.
fn mark_session_exited(session_id: &str) {
    // Flush any remaining bridge output before marking exited
    if let Ok(mut br) = agent_bridge().lock() {
        if let Some(state) = br.remove(session_id) {
            let trimmed = state.accumulated_output.trim().to_string();
            if trimmed.len() >= BRIDGE_MIN_CONTENT_CHARS {
                let content = truncate_for_delivery(&trimmed, session_id);
                emit_pty_event(
                    "agent-response",
                    &serde_json::json!({
                        "session_id": session_id,
                        "agent_name": state.agent_name,
                        "content": content,
                    }),
                );
            }
        }
    }

    if let Ok(mut map) = sessions().lock() {
        if let Some(session) = map.get_mut(session_id) {
            crate::tools::log_tools::append_to_app_log(&format!(
                "PTY | exited | id={} cmd={}", session_id, session.command
            ));
            session.exited = true;
            // Drop OS handles — frees file descriptors and process handles
            session.writer = None;
            session.master = None;
            // Wait for child then drop
            if let Some(mut child) = session.child.take() {
                let _ = child.wait();
            }
        }
    }
}

/// Background reader loop: reads PTY output → emits "pty-output" events.
/// When the PTY closes (read returns 0 or error), emits "pty-exit".
///
/// Also accumulates output lines and periodically emits "pty-log" events
/// for optional memory logging (Phase 10.5.1). The frontend decides whether
/// to save these to HIVE's memory system.
fn pty_reader_loop(session_id: String, agent_name: String, mut reader: Box<dyn Read + Send>) {
    let mut buf = [0u8; 4096];
    let mut log_buffer = String::new();
    let mut last_flush = std::time::Instant::now();
    let flush_interval = std::time::Duration::from_secs(5);
    let max_buffer_size = 8192; // Flush at 8KB regardless of time
    let mut utf8_leftover: Vec<u8> = Vec::new(); // Reassemble split UTF-8 codepoints (B8 fix)

    loop {
        match reader.read(&mut buf) {
            Ok(0) => {
                // Flush remaining log buffer before exit
                if !log_buffer.is_empty() {
                    flush_log_buffer(&session_id, &agent_name, &mut log_buffer);
                }
                // Mark session as exited — free OS resources, keep metadata
                mark_session_exited(&session_id);
                emit_pty_event(
                    "pty-exit",
                    &serde_json::json!({
                        "session_id": session_id,
                        "exit_code": serde_json::Value::Null,
                    }),
                );
                break;
            }
            Ok(n) => {
                // Prepend any leftover bytes from a split UTF-8 codepoint (B8 fix)
                let chunk = if utf8_leftover.is_empty() {
                    buf[..n].to_vec()
                } else {
                    let mut combined = std::mem::take(&mut utf8_leftover);
                    combined.extend_from_slice(&buf[..n]);
                    combined
                };
                // Find the last valid UTF-8 boundary — save trailing incomplete bytes for next read
                let valid_end = match std::str::from_utf8(&chunk) {
                    Ok(_) => chunk.len(),
                    Err(e) => e.valid_up_to(),
                };
                let data = String::from_utf8_lossy(&chunk[..valid_end]).to_string();
                if valid_end < chunk.len() {
                    utf8_leftover = chunk[valid_end..].to_vec();
                }
                emit_pty_event(
                    "pty-output",
                    &serde_json::json!({
                        "session_id": session_id,
                        "data": data,
                    }),
                );

                // Accumulate for memory logging (strip ANSI escape codes)
                let clean = strip_ansi_escapes(&data);
                if !clean.trim().is_empty() {
                    log_buffer.push_str(&clean);

                    // Push to circular output buffer for cross-agent reading
                    if let Ok(mut bufs) = output_buffers().lock() {
                        let buf = bufs.entry(session_id.clone())
                            .or_insert_with(|| VecDeque::with_capacity(OUTPUT_BUFFER_MAX_LINES));
                        for line in clean.lines() {
                            if !line.trim().is_empty() {
                                if buf.len() >= OUTPUT_BUFFER_MAX_LINES {
                                    buf.pop_front();
                                }
                                buf.push_back(line.to_string());
                            }
                        }
                    }

                    // Accumulate in bridge buffer for silence-based response detection
                    if let Ok(mut br) = agent_bridge().lock() {
                        if let Some(state) = br.get_mut(&session_id) {
                            state.accumulated_output.push_str(&clean);
                            state.last_output_time = Instant::now();
                        }
                    }
                }

                // Flush if buffer is large or enough time has passed
                if log_buffer.len() > max_buffer_size
                    || (!log_buffer.is_empty() && last_flush.elapsed() >= flush_interval)
                {
                    flush_log_buffer(&session_id, &agent_name, &mut log_buffer);
                    last_flush = std::time::Instant::now();
                }
            }
            Err(e) => {
                if !log_buffer.is_empty() {
                    flush_log_buffer(&session_id, &agent_name, &mut log_buffer);
                }
                // Mark session as exited — free OS resources, keep metadata
                mark_session_exited(&session_id);
                emit_pty_event(
                    "pty-exit",
                    &serde_json::json!({
                        "session_id": session_id,
                        "exit_code": serde_json::Value::Null,
                        "error": e.to_string(),
                    }),
                );
                break;
            }
        }
    }
}

/// Emit accumulated log buffer as a "pty-log" event for optional memory storage.
fn flush_log_buffer(session_id: &str, agent_name: &str, buffer: &mut String) {
    let content = buffer.trim().to_string();
    if content.is_empty() {
        buffer.clear();
        return;
    }
    emit_pty_event(
        "pty-log",
        &serde_json::json!({
            "session_id": session_id,
            "agent_name": agent_name,
            "content": content,
        }),
    );
    buffer.clear();
}

/// Strip ANSI escape codes from terminal output for clean memory logging.
/// Simulates terminal carriage-return behavior: bare `\r` overwrites the current
/// line (as a terminal would), `\r\n` is treated as a normal newline.
/// This prevents spinner/thinking text from concatenating into spam.
fn strip_ansi_escapes(s: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut current_line = String::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                // CSI sequence: ESC [ ... (letter or ~)
                chars.next();
                while let Some(&nc) = chars.peek() {
                    chars.next();
                    if nc.is_ascii_alphabetic() || nc == '~' {
                        break;
                    }
                }
            } else if chars.peek() == Some(&']') {
                // OSC sequence: ESC ] ... terminated by BEL or ESC backslash
                chars.next();
                while let Some(&nc) = chars.peek() {
                    if nc == '\x07' {
                        chars.next();
                        break;
                    } else if nc == '\x1b' {
                        chars.next();
                        if chars.peek() == Some(&'\\') {
                            chars.next();
                        }
                        break;
                    } else {
                        chars.next();
                    }
                }
            } else {
                // Two-char escape: ESC + next char
                chars.next();
            }
        } else if c == '\r' {
            if chars.peek() == Some(&'\n') {
                // \r\n → normal newline
                chars.next();
                lines.push(std::mem::take(&mut current_line));
            } else {
                // Bare \r → simulate terminal overwrite (cursor to column 0)
                current_line.clear();
            }
        } else if c == '\n' {
            lines.push(std::mem::take(&mut current_line));
        } else {
            current_line.push(c);
        }
    }

    lines.push(current_line);
    lines.join("\n")
}

/// Write input data to a PTY session (keystrokes from xterm.js).
#[tauri::command]
pub async fn pty_write(
    session_id: String,
    data: String,
) -> Result<(), String> {
    write_to_session(&session_id, &data)
}

/// Resize a PTY session (when the terminal pane is resized).
#[tauri::command]
pub async fn pty_resize(
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let map = sessions()
        .lock()
        .map_err(|e| format!("Sessions lock poisoned: {}", e))?;

    let session = map
        .get(&session_id)
        .ok_or_else(|| format!("PTY session not found: {}", session_id))?;

    if session.exited {
        return Ok(()); // Silently ignore resize on exited sessions
    }

    let master = session.master.as_ref()
        .ok_or_else(|| format!("PTY session {} master already closed", session_id))?;

    master
        .resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("Failed to resize PTY: {}", e))?;

    Ok(())
}

/// Kill a PTY session and remove it from the map entirely.
/// Called when the user closes the terminal pane.
#[tauri::command]
pub async fn pty_kill(
    session_id: String,
) -> Result<(), String> {
    let mut map = sessions()
        .lock()
        .map_err(|e| format!("Sessions lock poisoned: {}", e))?;

    let mut session = map
        .remove(&session_id)
        .ok_or_else(|| format!("PTY session not found: {}", session_id))?;

    // Kill the child process if still alive
    if let Some(mut child) = session.child.take() {
        let _ = child.kill();
        let _ = child.wait();
    }

    crate::tools::log_tools::append_to_app_log(&format!(
        "PTY | killed | id={} cmd={}", session_id, session.command
    ));

    // Clean up output buffer for this session
    if let Ok(mut bufs) = output_buffers().lock() {
        bufs.remove(&session_id);
    }

    // Clean up bridge state for this session
    if let Ok(mut br) = agent_bridge().lock() {
        br.remove(&session_id);
    }

    // Writer + master dropped automatically via Option<>

    Ok(())
}

/// Kill ALL PTY sessions — called on app exit (P0a: Process Cleanup).
/// Iterates every session, kills child processes, and clears the session map.
pub fn kill_all_sessions() {
    let mut map = match sessions().lock() {
        Ok(m) => m,
        Err(_) => return, // Poisoned lock — nothing we can do on exit
    };

    for (id, session) in map.iter_mut() {
        if let Some(mut child) = session.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        // Drop writer + master so file descriptors close
        session.writer = None;
        session.master = None;

        crate::tools::log_tools::append_to_app_log(&format!(
            "PTY | shutdown_cleanup | id={} cmd={}", id, session.command
        ));
    }
    map.clear();

    // Clear all output buffers
    if let Ok(mut bufs) = output_buffers().lock() {
        bufs.clear();
    }

    // Clear all bridge states
    if let Ok(mut br) = agent_bridge().lock() {
        br.clear();
    }
}

/// List all active PTY sessions (metadata only).
#[tauri::command]
pub async fn pty_list() -> Result<Vec<PtySessionInfo>, String> {
    Ok(list_sessions_info())
}

/// Check if a CLI agent command is available on the system (which/where).
/// Returns the resolved path if found, or an empty string if not.
#[tauri::command]
pub async fn check_agent_available(command: String) -> Result<String, String> {
    let which_cmd = if cfg!(target_os = "windows") { "where" } else { "which" };

    let output = std::process::Command::new(which_cmd)
        .arg(&command)
        .output()
        .map_err(|e| format!("Failed to run {}: {}", which_cmd, e))?;

    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        // Take only the first line (Windows 'where' can return multiple)
        let first_line = path.lines().next().unwrap_or("").to_string();
        Ok(first_line)
    } else {
        Ok(String::new()) // Not found — empty string, not an error
    }
}

/// Set up the MCP bridge for a CLI agent by writing HIVE's MCP server config
/// into the agent's configuration file.
///
/// Currently supported:
///   - Claude Code: writes to ~/.claude.json → mcpServers.hive
///
/// Uses the running HIVE executable path so the MCP server resolves correctly
/// regardless of install location. Merges into existing config (doesn't overwrite).
///
/// P6 (Secrets Stay Secret): Only writes HIVE's own entry. Never reads/modifies
/// other MCP server configs. Never touches agent auth.
#[tauri::command]
pub async fn setup_mcp_bridge(agent: String) -> Result<String, String> {
    // Currently only Claude Code is supported
    if agent != "claude" && agent != "claude-code" {
        return Err(format!(
            "MCP bridge not supported for '{}'. Currently only Claude Code is supported.",
            agent
        ));
    }

    // Resolve HIVE executable path for the MCP server command
    let exe_path = std::env::current_exe()
        .map_err(|e| format!("Failed to resolve HIVE executable path: {}", e))?;
    let exe_str = exe_path.to_string_lossy().to_string();

    // Target: ~/.claude.json (Claude Code's user-level MCP config)
    let home = dirs::home_dir()
        .ok_or_else(|| "Cannot determine home directory".to_string())?;
    let config_path = home.join(".claude.json");

    // Read existing config or start with empty object
    let mut config: serde_json::Value = if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)
            .map_err(|e| format!("Failed to read {}: {}", config_path.display(), e))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse {}: {}", config_path.display(), e))?
    } else {
        serde_json::json!({})
    };

    // Ensure top-level is an object
    let root = config.as_object_mut()
        .ok_or_else(|| format!("{} is not a JSON object", config_path.display()))?;

    // Get or create mcpServers object
    let mcp_servers = root
        .entry("mcpServers")
        .or_insert_with(|| serde_json::json!({}));

    let servers = mcp_servers.as_object_mut()
        .ok_or_else(|| "mcpServers is not a JSON object".to_string())?;

    // Check if already configured
    if servers.contains_key("hive") {
        return Ok(format!(
            "HIVE MCP bridge already configured in {}",
            config_path.display()
        ));
    }

    // Add HIVE MCP server entry
    servers.insert("hive".to_string(), serde_json::json!({
        "command": exe_str,
        "args": ["--mcp"]
    }));

    // Write back with pretty formatting (preserves human readability)
    let pretty = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    std::fs::write(&config_path, &pretty)
        .map_err(|e| format!("Failed to write {}: {}", config_path.display(), e))?;

    Ok(format!(
        "HIVE MCP bridge configured in {}. Claude Code will discover HIVE's tools on next start.",
        config_path.display()
    ))
}

// ============================================
// Tests
// ============================================

#[cfg(test)]
mod tests {
    use super::*;

    // --- strip_ansi_escapes tests ---

    #[test]
    fn strip_ansi_plain_text_passthrough() {
        assert_eq!(strip_ansi_escapes("hello world"), "hello world");
    }

    #[test]
    fn strip_ansi_csi_color_codes() {
        // Bold green "ok" then reset
        let input = "\x1b[1;32mok\x1b[0m";
        assert_eq!(strip_ansi_escapes(input), "ok");
    }

    #[test]
    fn strip_ansi_cursor_movement() {
        // Move cursor up 2, then print text
        let input = "\x1b[2Ahello";
        assert_eq!(strip_ansi_escapes(input), "hello");
    }

    #[test]
    fn strip_ansi_tilde_terminated() {
        // F1 key sequence: ESC [ 11 ~
        let input = "\x1b[11~text";
        assert_eq!(strip_ansi_escapes(input), "text");
    }

    #[test]
    fn strip_ansi_two_char_escapes() {
        // ESC M = reverse line feed
        let input = "\x1bMvisible";
        assert_eq!(strip_ansi_escapes(input), "visible");
    }

    #[test]
    fn strip_ansi_carriage_returns() {
        // Bare \r simulates terminal overwrite: "line2" is overwritten by "line3"
        let input = "line1\r\nline2\rline3";
        assert_eq!(strip_ansi_escapes(input), "line1\nline3");
    }

    #[test]
    fn strip_ansi_spinner_overwrite() {
        // Simulates Claude Code thinking spinner: each \r overwrites the line
        let input = "\u{280B} Thinking...\r\u{2819} Thinking...\r\u{2839} Thinking...\rDone processing";
        assert_eq!(strip_ansi_escapes(input), "Done processing");
    }

    #[test]
    fn strip_ansi_osc_sequences() {
        // OSC set window title: ESC ] 0 ; title BEL
        let input = "\x1b]0;My Terminal\x07visible text";
        assert_eq!(strip_ansi_escapes(input), "visible text");
    }

    #[test]
    fn strip_ansi_osc_st_terminator() {
        // OSC terminated by ESC backslash (ST)
        let input = "\x1b]2;title\x1b\\visible";
        assert_eq!(strip_ansi_escapes(input), "visible");
    }

    #[test]
    fn strip_ansi_mixed_escapes() {
        let input = "\x1b[38;2;245;158;11m[Process exited]\x1b[0m\r\n";
        assert_eq!(strip_ansi_escapes(input), "[Process exited]\n");
    }

    #[test]
    fn strip_ansi_empty_string() {
        assert_eq!(strip_ansi_escapes(""), "");
    }

    #[test]
    fn strip_ansi_only_escapes() {
        let input = "\x1b[31m\x1b[0m\r";
        assert_eq!(strip_ansi_escapes(input), "");
    }

    #[test]
    fn strip_ansi_unicode_preserved() {
        let input = "\x1b[1mHIVE 🐝 ready\x1b[0m";
        assert_eq!(strip_ansi_escapes(input), "HIVE 🐝 ready");
    }

    // --- PtySessionInfo tests ---

    #[test]
    fn session_info_serializes_with_exited() {
        let info = PtySessionInfo {
            id: "abc-123".to_string(),
            command: "claude".to_string(),
            started_at: "2026-01-01T00:00:00Z".to_string(),
            exited: false,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"exited\":false"));

        let info_exited = PtySessionInfo { exited: true, ..info };
        let json2 = serde_json::to_string(&info_exited).unwrap();
        assert!(json2.contains("\"exited\":true"));
    }

    // --- Output buffer tests ---

    #[test]
    fn output_buffer_stores_and_retrieves_lines() {
        let session_id = "test-buf-1";
        // Seed a session in SESSIONS so read_session_output doesn't return "not found"
        sessions().lock().unwrap().insert(session_id.to_string(), PtySession {
            id: session_id.to_string(),
            command: "test".to_string(),
            writer: None, master: None, child: None,
            started_at: "2026-01-01T00:00:00Z".to_string(),
            exited: false,
        });
        // Push lines to buffer
        {
            let mut bufs = output_buffers().lock().unwrap();
            let buf = bufs.entry(session_id.to_string())
                .or_insert_with(|| VecDeque::with_capacity(OUTPUT_BUFFER_MAX_LINES));
            buf.push_back("line 1".to_string());
            buf.push_back("line 2".to_string());
            buf.push_back("line 3".to_string());
        }
        let lines = read_session_output(session_id, 2).unwrap();
        assert_eq!(lines, vec!["line 2", "line 3"]);
        let all = read_session_output(session_id, 100).unwrap();
        assert_eq!(all, vec!["line 1", "line 2", "line 3"]);
        // Cleanup
        sessions().lock().unwrap().remove(session_id);
        output_buffers().lock().unwrap().remove(session_id);
    }

    #[test]
    fn output_buffer_circular_eviction() {
        let session_id = "test-buf-2";
        sessions().lock().unwrap().insert(session_id.to_string(), PtySession {
            id: session_id.to_string(),
            command: "test".to_string(),
            writer: None, master: None, child: None,
            started_at: "2026-01-01T00:00:00Z".to_string(),
            exited: false,
        });
        {
            let mut bufs = output_buffers().lock().unwrap();
            let buf = bufs.entry(session_id.to_string())
                .or_insert_with(|| VecDeque::with_capacity(OUTPUT_BUFFER_MAX_LINES));
            // Fill beyond max
            for i in 0..(OUTPUT_BUFFER_MAX_LINES + 50) {
                if buf.len() >= OUTPUT_BUFFER_MAX_LINES {
                    buf.pop_front();
                }
                buf.push_back(format!("line {}", i));
            }
        }
        let lines = read_session_output(session_id, OUTPUT_BUFFER_MAX_LINES + 100).unwrap();
        assert_eq!(lines.len(), OUTPUT_BUFFER_MAX_LINES);
        assert_eq!(lines[0], "line 50"); // First 50 evicted
        // Cleanup
        sessions().lock().unwrap().remove(session_id);
        output_buffers().lock().unwrap().remove(session_id);
    }

    #[test]
    fn output_buffer_unknown_session_returns_error() {
        let result = read_session_output("nonexistent-session", 10);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    // --- Bridge helper tests ---

    #[test]
    fn extract_words_filters_short() {
        let words = extract_words("I am a big dog");
        // "I", "am", "a" are ≤2 chars, filtered out
        assert_eq!(words, vec!["big", "dog"]);
    }

    #[test]
    fn extract_words_lowercases() {
        let words = extract_words("Hello WORLD FooBar");
        assert_eq!(words, vec!["hello", "world", "foobar"]);
    }

    #[test]
    fn jaccard_identical_is_one() {
        let a = vec!["hello".to_string(), "world".to_string()];
        assert!((jaccard_similarity(&a, &a) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn jaccard_disjoint_is_zero() {
        let a = vec!["hello".to_string()];
        let b = vec!["world".to_string()];
        assert!((jaccard_similarity(&a, &b) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn jaccard_partial_overlap() {
        let a = vec!["hello".to_string(), "world".to_string(), "foo".to_string()];
        let b = vec!["hello".to_string(), "world".to_string(), "bar".to_string()];
        // intersection: {hello, world} = 2, union: {hello, world, foo, bar} = 4
        assert!((jaccard_similarity(&a, &b) - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn truncate_short_passes_through() {
        let short = "Hello, this is a response.";
        assert_eq!(truncate_for_delivery(short, "test-id"), short);
    }

    #[test]
    fn truncate_long_output_adds_pointer() {
        // Generate output that exceeds BRIDGE_MAX_INLINE_CHARS with many lines
        let lines: Vec<String> = (0..200).map(|i| format!("Line {} with some content here to take up space", i)).collect();
        let long_output = lines.join("\n");
        assert!(long_output.len() > BRIDGE_MAX_INLINE_CHARS);

        let result = truncate_for_delivery(&long_output, "sess-123");
        assert!(result.contains("lines omitted"));
        assert!(result.contains("read_agent_output('sess-123')"));
        assert!(result.contains("Line 0")); // head
        assert!(result.contains("Line 199")); // tail
    }
}

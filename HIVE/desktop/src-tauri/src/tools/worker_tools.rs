//! Worker tools — spawn autonomous model sub-agents
//!
//! Workers are independent model conversations that run in background tokio tasks.
//! Each worker has its own tool access, writes results to a shared scratchpad,
//! and notifies on completion.
//!
//! Tools:
//!   worker_spawn     — start an autonomous worker with a task
//!   worker_status    — check worker progress
//!   worker_terminate — stop a running worker
//!
//! Principle alignment:
//!   P1 (Modularity)  — Workers are isolated; their own registry, their own context.
//!   P2 (Agnostic)    — Any provider works. Model specified at spawn time.
//!   P3 (Simplicity)  — Tokio tasks, no Docker/Redis. Tool registry cloned per worker.
//!   P6 (Secrets)     — Workers can't access run_command/write_file by default.
//!   P7 (Framework)   — Workers use the same provider/tool infra. Models are swappable.

use super::{HiveTool, RiskLevel, ToolResult, ToolSchema, create_default_registry};
use crate::providers::{ChatResponse, chat_with_tools};
use serde_json::json;
use std::collections::HashMap;
use std::sync::OnceLock;
use tauri::{Emitter, Manager};
use tokio::sync::RwLock;

// ============================================
// Global App Handle (for emitting events to the frontend)
// ============================================

static APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();

/// Store the Tauri AppHandle so workers can emit completion events.
/// Called once from main.rs `.setup()`.
pub fn set_app_handle(handle: tauri::AppHandle) {
    let _ = APP_HANDLE.set(handle);
}

/// Emit a worker lifecycle event to the frontend.
fn emit_worker_event(event_name: &str, payload: &serde_json::Value) {
    if let Some(app) = APP_HANDLE.get() {
        let _ = app.emit(event_name, payload);
    }
}

// ============================================
// Global Worker State
// ============================================

static WORKERS: OnceLock<RwLock<HashMap<String, WorkerInfo>>> = OnceLock::new();

fn workers() -> &'static RwLock<HashMap<String, WorkerInfo>> {
    WORKERS.get_or_init(|| RwLock::new(HashMap::new()))
}

#[derive(Clone)]
struct WorkerInfo {
    id: String,
    model: String,
    provider: String,
    task: String,
    scratchpad_id: String,
    status: WorkerStatus,
    started_at: chrono::DateTime<chrono::Utc>,
    last_activity: chrono::DateTime<chrono::Utc>,
    turns_used: usize,
    max_turns: usize,
    summary: String,
    /// Total non-report tool executions (for progress gate)
    tools_executed: usize,
    /// tools_executed at time of last accepted report
    last_report_tool_count: usize,
    /// When last report was accepted (for rate limiting)
    last_report_time: Option<chrono::DateTime<chrono::Utc>>,
    /// Word set of last accepted report (for Jaccard dedup)
    last_report_words: Vec<String>,
    /// Wall clock time limit in seconds (primary termination — Temporal pattern)
    max_time_seconds: u64,
    /// Last time a periodic status event was emitted (for observability)
    last_status_emit: chrono::DateTime<chrono::Utc>,
    /// Set by report_to_parent(severity="done") — signals the worker loop to exit gracefully
    done_signaled: bool,
}

/// Public API for the Tauri command layer — returns all workers as JSON values.
pub async fn get_all_worker_statuses() -> Result<Vec<serde_json::Value>, String> {
    let ws = workers().read().await;
    let mut result: Vec<serde_json::Value> = ws.values().map(|w| {
        let elapsed = (chrono::Utc::now() - w.started_at).num_seconds();
        let idle_secs = (chrono::Utc::now() - w.last_activity).num_seconds();
        json!({
            "id": w.id,
            "model": w.model,
            "provider": w.provider,
            "task": if w.task.len() > 120 { format!("{}...", &w.task.chars().take(120).collect::<String>()) } else { w.task.clone() },
            "scratchpad_id": w.scratchpad_id,
            "status": w.status.to_string(),
            "started_at": w.started_at.to_rfc3339(),
            "elapsed_seconds": elapsed,
            "idle_seconds": idle_secs,
            "turns_used": w.turns_used,
            "max_turns": w.max_turns,
            "max_time_seconds": w.max_time_seconds,
            "tools_executed": w.tools_executed,
            "summary": w.summary,
        })
    }).collect();
    // Sort by started_at descending (newest first)
    result.sort_by(|a, b| {
        b.get("started_at").and_then(|v| v.as_str()).unwrap_or("")
            .cmp(a.get("started_at").and_then(|v| v.as_str()).unwrap_or(""))
    });
    Ok(result)
}

#[derive(Clone, PartialEq)]
enum WorkerStatus {
    Running,
    Completed,
    Failed(String),
    Terminated,
}

impl std::fmt::Display for WorkerStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorkerStatus::Running => write!(f, "running"),
            WorkerStatus::Completed => write!(f, "completed"),
            WorkerStatus::Failed(e) => write!(f, "failed: {}", e),
            WorkerStatus::Terminated => write!(f, "terminated"),
        }
    }
}

// ============================================
// Phase 5B: Cross-module accessors for ReadAgentContextTool
// ============================================

/// Get a formatted summary for a specific worker by ID.
/// Returns None if worker doesn't exist.
pub(crate) async fn get_worker_summary(worker_id: &str) -> Option<String> {
    let ws = workers().read().await;
    ws.get(worker_id).map(|w| {
        let elapsed = (chrono::Utc::now() - w.started_at).num_seconds();
        format!(
            "Status: {} | Model: {}/{} | Turns: {}/{} | Tools: {} | Elapsed: {}s | Task: {}",
            w.status, w.provider, w.model,
            w.turns_used, w.max_turns,
            w.tools_executed, elapsed,
            w.task.chars().take(120).collect::<String>(),
        )
    })
}

/// List all active (running) workers with brief summaries.
/// Returns: Vec<(worker_id, status_summary)>
pub(crate) async fn list_active_workers_summary() -> Vec<(String, String)> {
    let ws = workers().read().await;
    ws.iter()
        .filter(|(_, w)| matches!(w.status, WorkerStatus::Running))
        .map(|(id, w)| {
            let elapsed = (chrono::Utc::now() - w.started_at).num_seconds();
            (
                id.clone(),
                format!("{}/{} | {}s | {}", w.provider, w.model, elapsed,
                    w.task.chars().take(80).collect::<String>()),
            )
        })
        .collect()
}

/// Jaccard similarity between two word sets (0.0 = no overlap, 1.0 = identical).
/// Used by report_to_parent to detect duplicate/redundant reports.
fn jaccard_similarity(a: &[String], b: &[String]) -> f64 {
    use std::collections::HashSet;
    let set_a: HashSet<&str> = a.iter().map(|s| s.as_str()).collect();
    let set_b: HashSet<&str> = b.iter().map(|s| s.as_str()).collect();
    let intersection = set_a.intersection(&set_b).count();
    let union = set_a.union(&set_b).count();
    if union == 0 { return 0.0; }
    intersection as f64 / union as f64
}

/// Extract lowercase words (>2 chars) for Jaccard comparison.
fn extract_words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|w| w.to_lowercase().trim_matches(|c: char| !c.is_alphanumeric()).to_string())
        .filter(|w| w.len() > 2)
        .collect()
}

/// Tools that workers are NOT allowed to use (P6 security).
const WORKER_BLOCKED_TOOLS: &[&str] = &[
    "run_command",          // shell access
    "write_file",           // filesystem writes
    "worker_spawn",         // no recursive workers
    "telegram_send",        // no outbound messages
    "discord_send",         // no outbound messages
    "plan_execute",         // no nested plans
    "memory_import_file",   // arbitrary file read → persistent DB
    "send_to_agent",        // writes to PTY stdin — command execution vector
    "github_issues",        // no external issue creation
    "github_prs",           // no external PR creation
];

// ============================================
// worker_spawn
// ============================================

pub struct WorkerSpawnTool;

#[async_trait::async_trait]
impl HiveTool for WorkerSpawnTool {
    fn name(&self) -> &str { "worker_spawn" }

    fn description(&self) -> &str {
        "Spawn an autonomous worker sub-agent. The worker runs in the background with its own \
         model context and tool access, writing results to a shared scratchpad. Use this for \
         parallel analysis tasks — e.g., have one worker scan files while another reads them.\n\n\
         Workers can use: read_file, list_directory, file_tree, code_search, web_fetch, web_search, \
         memory_save, memory_search, scratchpad_write, scratchpad_read, report_to_parent.\n\
         Workers CANNOT use: run_command, write_file, worker_spawn, telegram_send, discord_send.\n\n\
         LIFECYCLE: Workers terminate via wall clock timeout (default 10 min), natural completion \
         (model stops calling tools), repetition detection (3x identical calls), or the safety \
         valve turn limit (default 100). max_turns is a SAFETY VALVE — prefer max_time_seconds \
         for controlling runtime. The worker gets a wrap-up prompt at 80% of wall clock time.\n\n\
         Workers emit periodic status events to the frontend for live observability."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "provider": {
                    "type": "string",
                    "description": "Provider API name: 'openai', 'anthropic', 'ollama', 'openrouter', 'dashscope'. Omit to use current session's provider."
                },
                "model": {
                    "type": "string",
                    "description": "API model ID (e.g., 'gpt-4o-mini', 'kimi-k2.5', 'claude-sonnet-4-5-20250929'). Omit to use current session's model. MUST be the API ID, not the display name."
                },
                "system_prompt": {
                    "type": "string",
                    "description": "System prompt for the worker — define its role and focus area"
                },
                "task": {
                    "type": "string",
                    "description": "The task for the worker to accomplish. Be specific about what to analyze and where to write results."
                },
                "scratchpad_id": {
                    "type": "string",
                    "description": "Scratchpad to write results to (created via scratchpad_create)"
                },
                "max_time_seconds": {
                    "type": "integer",
                    "description": "Wall clock time limit in seconds (default: 600 = 10 min, max: 3600). This is the PRIMARY runtime limit — worker gets a wrap-up prompt at 80%."
                },
                "max_turns": {
                    "type": "integer",
                    "description": "Safety valve turn limit (default: 100, max: 200). Use max_time_seconds to control runtime — max_turns is just a safety net."
                },
                "thinking_depth": {
                    "type": "string",
                    "enum": ["off", "low", "medium", "high"],
                    "description": "Thinking/reasoning depth for the worker (default: 'low'). Workers default to 'low' to control token burn — spawning 5 workers at 'high' would consume ~160K thinking tokens per turn. Override to 'medium' or 'high' only for complex analytical tasks."
                },
                "slot_role": {
                    "type": "string",
                    "enum": ["consciousness", "coder", "terminal", "webcrawl", "toolcall"],
                    "description": "Use the model/provider configured for this slot role. Resolves to the slot's assigned provider + model. Overridden by explicit 'provider'/'model' params."
                }
            },
            "required": ["task", "scratchpad_id"]
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::Medium }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        // Phase 5D: slot_role resolution — resolve a slot's configured model/provider.
        // Priority: explicit provider/model > slot_role > session context (P2, P3).
        let slot_assignment = params.get("slot_role")
            .and_then(|v| v.as_str())
            .and_then(|role_str| {
                let app = APP_HANDLE.get()?;
                let slots_state: tauri::State<'_, crate::slots::SlotsState> = app.state();
                let configs = slots_state.configs.lock().ok()?;
                // Match role string to SlotRole enum
                let role = match role_str {
                    "consciousness" => crate::slots::SlotRole::Consciousness,
                    "coder" => crate::slots::SlotRole::Coder,
                    "terminal" => crate::slots::SlotRole::Terminal,
                    "webcrawl" => crate::slots::SlotRole::WebCrawl,
                    "toolcall" => crate::slots::SlotRole::ToolCall,
                    _ => return None,
                };
                let config = configs.get(&role)?;
                let assignment = config.best_assignment()?;
                Some((assignment.provider.clone(), assignment.model.clone()))
            });

        // Provider and model default to the current session's values (P2: agnostic, P7: framework survives).
        // The framework sets session context at chat start — workers inherit it automatically.
        // LLMs don't need to know their own API model ID; the framework provides it.
        let session_ctx = crate::providers::get_session_model_context().await;

        let provider = match params.get("provider").and_then(|v| v.as_str()) {
            Some(p) => p.to_string(),
            None => match &slot_assignment {
                Some((p, _)) => p.clone(),
                None => match &session_ctx {
                    Some(ctx) => ctx.provider.clone(),
                    None => return Ok(ToolResult {
                        content: "No provider specified and no active session context. Pass 'provider' or 'slot_role' parameter.".to_string(),
                        is_error: true,
                    }),
                },
            },
        };

        let model = match params.get("model").and_then(|v| v.as_str()) {
            Some(m) => m.to_string(),
            None => match &slot_assignment {
                Some((_, m)) => m.clone(),
                None => match &session_ctx {
                    Some(ctx) => ctx.model_id.clone(),
                    None => return Ok(ToolResult {
                        content: "No model specified and no active session context. Pass 'model' or 'slot_role' parameter (API ID, not display name).".to_string(),
                        is_error: true,
                    }),
                },
            },
        };

        let task = params.get("task")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: task")?
            .to_string();

        let scratchpad_id = params.get("scratchpad_id")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: scratchpad_id")?
            .to_string();

        // Phase 5D: Workers get HIVE identity when no custom system_prompt is provided (P2, P7).
        // Custom system_prompt takes priority — the orchestrating model can override identity.
        let system_prompt = match params.get("system_prompt").and_then(|v| v.as_str()) {
            Some(custom) => custom.to_string(),
            None => {
                let identity = crate::harness::read_identity();
                format!(
                    "{}\n\n## Worker Role\n\
                     You are a focused research worker in the HIVE orchestration harness. \
                     Complete your assigned task using the available tools, then write your \
                     findings to the scratchpad. Be thorough and precise.",
                    identity
                )
            }
        };

        // max_time_seconds: wall clock limit (PRIMARY termination — Temporal heartbeat pattern)
        let max_time_seconds = params.get("max_time_seconds")
            .and_then(|v| v.as_u64())
            .unwrap_or(600)       // default: 10 minutes
            .max(60)              // minimum: 1 minute
            .min(3600);           // maximum: 1 hour

        // max_turns: SAFETY VALVE only (raised from 20→100, inspired by AutoGen/CrewAI patterns)
        let max_turns = params.get("max_turns")
            .and_then(|v| v.as_u64())
            .unwrap_or(100)
            .min(200) as usize;

        // thinking_depth: defaults to "low" to control token burn (P2: provider-agnostic, P3: simple).
        // Spawning 5 workers at "high" = ~160K thinking tokens/turn — "low" (2048) keeps costs sane.
        // The orchestrating model can override per-worker for complex tasks.
        let thinking_depth = params.get("thinking_depth")
            .and_then(|v| v.as_str())
            .unwrap_or("low")
            .to_string();

        // Validate provider
        let valid_providers = ["openai", "anthropic", "ollama", "openrouter", "dashscope"];
        if !valid_providers.contains(&provider.as_str()) {
            return Ok(ToolResult {
                content: format!(
                    "Invalid provider '{}'. Use one of: {}",
                    provider, valid_providers.join(", ")
                ),
                is_error: true,
            });
        }

        let worker_id = format!("w_{}_{}_{}", model.replace(['/', '.', '-'], "_"),
            chrono::Utc::now().timestamp_millis() % 100000,
            rand::random::<u16>() % 1000);

        let info = WorkerInfo {
            id: worker_id.clone(),
            model: model.clone(),
            provider: provider.clone(),
            task: task.clone(),
            scratchpad_id: scratchpad_id.clone(),
            status: WorkerStatus::Running,
            started_at: chrono::Utc::now(),
            last_activity: chrono::Utc::now(),
            turns_used: 0,
            max_turns,
            summary: String::new(),
            tools_executed: 0,
            last_report_tool_count: 0,
            last_report_time: None,
            last_report_words: Vec::new(),
            max_time_seconds,
            last_status_emit: chrono::Utc::now(),
            done_signaled: false,
        };

        workers().write().await.insert(worker_id.clone(), info);

        // Log spawn to persistent app log (includes identity injection status)
        let has_custom_prompt = params.get("system_prompt").and_then(|v| v.as_str()).is_some();
        let slot_role_used = params.get("slot_role").and_then(|v| v.as_str()).unwrap_or("none");
        crate::tools::log_tools::append_to_app_log(&format!(
            "WORKER_SPAWN | id={} | provider={} | model={} | scratchpad={} | max_time={}s | max_turns={} | thinking={} | identity={} | slot_role={} | task={}",
            worker_id, provider, model, scratchpad_id, max_time_seconds, max_turns, thinking_depth,
            if has_custom_prompt { "custom" } else { "hive" },
            slot_role_used,
            crate::content_security::safe_truncate(&task, 120),
        ));

        // Spawn background task
        let wid = worker_id.clone();
        let provider_display = provider.clone(); // for the output message (provider moves into spawn)
        let sp = system_prompt;
        let t = task.clone();
        let pid = scratchpad_id.clone();
        let td = thinking_depth.clone();

        tokio::spawn(async move {
            run_worker_loop(wid, provider, model, sp, t, pid, max_turns, max_time_seconds, td).await;
        });

        Ok(ToolResult {
            content: format!(
                "Worker '{}' spawned.\n  Provider: {}\n  Task: {}\n  Scratchpad: {}\n  Time limit: {}s ({:.0} min)\n  Safety turn limit: {}\n  Thinking depth: {}\n\n\
                 Use worker_status(worker_id=\"{}\") to check progress.\n\
                 Results will appear in scratchpad_read(scratchpad_id=\"{}\").",
                worker_id,
                provider_display,
                crate::content_security::safe_truncate(&task, 100),
                scratchpad_id,
                max_time_seconds,
                max_time_seconds as f64 / 60.0,
                max_turns,
                thinking_depth,
                worker_id,
                scratchpad_id,
            ),
            is_error: false,
        })
    }
}

/// The autonomous worker execution loop.
///
/// Termination hierarchy (checked in this order):
///   1. WallClockTimeout — max_time_seconds exceeded (PRIMARY limit)
///   2. ExternalTerminate — worker_terminate() called by parent/user
///   3. DoneSignal — report_to_parent(severity="done") triggers graceful exit
///   4. NaturalCompletion — LLM responds with text (no tool calls)
///   5. RepetitionDetection — 3x identical tool call sets = stuck
///   6. MaxTurns — safety valve only (default 100, not 20)
async fn run_worker_loop(
    worker_id: String,
    provider: String,
    model: String,
    system_prompt: String,
    task: String,
    scratchpad_id: String,
    max_turns: usize,
    max_time_seconds: u64,
    thinking_depth: String,
) {
    // Create worker's own tool registry and UNREGISTER blocked tools (P6: defense in depth)
    // This ensures both schema list AND execution are filtered from the same source.
    let mut registry = create_default_registry();
    for blocked in WORKER_BLOCKED_TOOLS {
        registry.unregister(blocked);
    }
    let worker_tools: Vec<ToolSchema> = registry.schemas();

    // Build initial messages — include worker_id so the worker can report to parent
    let mut messages: Vec<serde_json::Value> = vec![
        json!({"role": "system", "content": system_prompt}),
        json!({"role": "user", "content": format!(
            "You are worker '{worker_id}'. Your task:\n{task}\n\n\
             Write all findings to scratchpad '{scratchpad_id}' using the scratchpad_write tool. \
             Use report_to_parent(worker_id=\"{worker_id}\") to send progress updates, \
             flag errors, or report completion to the parent orchestrator. \
             When done, write a final summary to the 'summary' section.",
        )}),
    ];

    // Repetition detection — break out of stuck tool loops (parity with main chat loop).
    // Tracks full fingerprint (name + args); 3 consecutive identical calls = stuck.
    // Name-only matching was too aggressive — research workers legitimately call
    // web_search multiple times with different queries. Only identical calls = stuck.
    let mut last_tool_fingerprint = String::new();
    let mut repetition_count: usize = 0;

    let start_time = std::time::Instant::now();
    let mut wall_clock_warned = false;

    for turn in 0..max_turns {
        // === Termination check: wall clock timeout (PRIMARY — Temporal pattern) ===
        let elapsed_secs = start_time.elapsed().as_secs();
        if elapsed_secs >= max_time_seconds {
            let reason = format!("Wall clock timeout: {}s elapsed (limit: {}s)", elapsed_secs, max_time_seconds);
            write_to_scratchpad(&scratchpad_id, "result",
                &format!("[worker {}] {}", worker_id, reason)).await;
            update_worker_status(&worker_id, WorkerStatus::Completed).await;
            let summary = format!("Completed after {}s / {} turns (wall clock limit)", elapsed_secs, turn);
            {
                let mut ws = workers().write().await;
                if let Some(w) = ws.get_mut(&worker_id) {
                    w.summary = summary.clone();
                }
            }
            crate::tools::log_tools::append_to_app_log(&format!(
                "WORKER_COMPLETE | id={} | reason=wall_clock_timeout | elapsed={}s | turns={} | scratchpad={}",
                worker_id, elapsed_secs, turn, scratchpad_id,
            ));
            emit_worker_event("worker-completed", &json!({
                "worker_id": worker_id, "status": "completed",
                "summary": summary, "scratchpad_id": scratchpad_id,
                "turns_used": turn,
            }));
            write_worker_to_bus(&worker_id, "Completed (wall clock timeout)", turn);
            return;
        }

        // Update activity timestamp + check external termination + done signal
        {
            let mut ws = workers().write().await;
            if let Some(w) = ws.get_mut(&worker_id) {
                if w.status == WorkerStatus::Terminated {
                    return; // User terminated
                }
                // === Termination check: DoneSignal from report_to_parent(severity="done") ===
                if w.done_signaled {
                    w.status = WorkerStatus::Completed;
                    let elapsed = start_time.elapsed().as_secs();
                    let summary = format!("Completed after {} turns / {}s (done signal)", turn + 1, elapsed);
                    w.summary = summary.clone();
                    crate::tools::log_tools::append_to_app_log(&format!(
                        "WORKER_COMPLETE | id={} | reason=done_signal | turns={} | elapsed={}s | scratchpad={}",
                        worker_id, turn + 1, elapsed, scratchpad_id,
                    ));
                    emit_worker_event("worker-completed", &json!({
                        "worker_id": worker_id, "status": "completed",
                        "summary": summary, "scratchpad_id": scratchpad_id,
                        "turns_used": turn + 1,
                    }));
                    write_worker_to_bus(&worker_id, "Completed (done signal)", turn + 1);
                    return;
                }
                w.turns_used = turn + 1;
                w.last_activity = chrono::Utc::now();
            }
        }

        // === Periodic status emission (every 60s) — frontend observability ===
        {
            let should_emit = {
                let ws = workers().read().await;
                ws.get(&worker_id)
                    .map(|w| (chrono::Utc::now() - w.last_status_emit).num_seconds() >= 60)
                    .unwrap_or(false)
            };
            if should_emit {
                let tools_count = {
                    let ws = workers().read().await;
                    ws.get(&worker_id).map(|w| w.tools_executed).unwrap_or(0)
                };
                emit_worker_event("worker-status-update", &json!({
                    "worker_id": worker_id,
                    "turns_used": turn + 1,
                    "tools_executed": tools_count,
                    "elapsed_seconds": elapsed_secs,
                    "max_time_seconds": max_time_seconds,
                    "max_turns": max_turns,
                }));
                let mut ws = workers().write().await;
                if let Some(w) = ws.get_mut(&worker_id) {
                    w.last_status_emit = chrono::Utc::now();
                }
            }
        }

        // === Wrap-up prompts ===
        // Wall clock wrap-up at 80% of time limit (primary)
        let time_pct = (elapsed_secs as f64) / (max_time_seconds as f64);
        if time_pct >= 0.80 && !wall_clock_warned {
            wall_clock_warned = true;
            let remaining = max_time_seconds.saturating_sub(elapsed_secs);
            messages.push(json!({"role": "system", "content": format!(
                "[WRAP UP — TIME] ~{}s remaining before wall clock timeout. Write your key findings \
                 to the scratchpad NOW using scratchpad_write, then call report_to_parent(severity=\"done\").",
                remaining
            )}));
        }
        // Turn-based wrap-up at max_turns - 2 (safety valve)
        if turn == max_turns.saturating_sub(2) && max_turns > 3 && !wall_clock_warned {
            messages.push(json!({"role": "system", "content": format!(
                "[WRAP UP — TURNS] You have {} turns remaining (safety limit). Write your key findings \
                 to the scratchpad NOW using scratchpad_write, then call report_to_parent(severity=\"done\").",
                max_turns - turn
            )}));
        }

        // Context truncation: if messages grow too large, trim the middle to keep
        // system prompt + initial task + recent turns. Rough estimate: 4 chars ≈ 1 token.
        // Cap at ~100K chars (~25K tokens) to stay within most model context windows.
        let total_chars: usize = messages.iter()
            .filter_map(|m| m.get("content").and_then(|c| c.as_str()))
            .map(|c| c.len())
            .sum();
        if total_chars > 100_000 && messages.len() > 4 {
            // Keep first 2 (system + task) and last N messages that fit in budget
            let keep_front = 2;
            let mut keep_back = 1;
            let mut back_chars: usize = 0;
            for m in messages.iter().rev() {
                let mc = m.get("content").and_then(|c| c.as_str()).map(|c| c.len()).unwrap_or(0);
                if back_chars + mc > 60_000 { break; }
                back_chars += mc;
                keep_back += 1;
            }
            let trim_notice = json!({"role": "system", "content":
                "[Context truncated — older tool results removed to fit context window. Recent turns preserved.]"});
            let front: Vec<_> = messages[..keep_front].to_vec();
            let back: Vec<_> = messages[messages.len().saturating_sub(keep_back)..].to_vec();
            messages = front;
            messages.push(trim_notice);
            messages.extend(back);
        }

        // Call the LLM (single retry for transient errors — 429, 502, 503, timeouts)
        let response = {
            let mut last_err = String::new();
            let mut got_response = None;
            for attempt in 0..2u8 {
                // thinking_depth: defaults to "low" (2048 tokens) to control token burn.
                // The orchestrating model can override via worker_spawn(thinking_depth="high").
                let td = if thinking_depth == "off" { None } else { Some(thinking_depth.clone()) };
                match chat_with_tools(
                    provider.clone(),
                    model.clone(),
                    messages.clone(),
                    worker_tools.clone(),
                    None,
                    None,
                    td,
                ).await {
                    Ok(r) => { got_response = Some(r); break; }
                    Err(e) => {
                        last_err = e;
                        if attempt == 0 {
                            let lower = last_err.to_lowercase();
                            let is_transient = lower.contains("429") || lower.contains("502")
                                || lower.contains("503") || lower.contains("rate limit")
                                || lower.contains("timeout") || lower.contains("too many requests");
                            if is_transient {
                                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                                continue;
                            }
                        }
                        break;
                    }
                }
            }
            match got_response {
                Some(r) => r,
                None => {
                    update_worker_status(&worker_id, WorkerStatus::Failed(last_err.clone())).await;
                    write_to_scratchpad(&scratchpad_id, "errors",
                        &format!("[worker {}] LLM error on turn {}: {}", worker_id, turn + 1, last_err)).await;
                    crate::tools::log_tools::append_to_app_log(&format!(
                        "WORKER_FAILED | id={} | reason=llm_error | turn={} | error={} | scratchpad={}",
                        worker_id, turn + 1, crate::content_security::safe_truncate(&last_err, 200), scratchpad_id,
                    ));
                    emit_worker_event("worker-completed", &json!({
                        "worker_id": worker_id, "status": "failed",
                        "error": last_err, "scratchpad_id": scratchpad_id,
                        "turns_used": turn + 1,
                    }));
                    write_worker_to_bus(&worker_id, "Failed (LLM error)", turn + 1);
                    return;
                }
            }
        };

        match response {
            ChatResponse::Text { content, .. } => {
                // Worker finished — write final output to scratchpad
                write_to_scratchpad(&scratchpad_id, "result",
                    &format!("[worker {}] {}", worker_id, content)).await;

                let summary = crate::content_security::safe_truncate(&content, 200);

                {
                    let mut ws = workers().write().await;
                    if let Some(w) = ws.get_mut(&worker_id) {
                        w.status = WorkerStatus::Completed;
                        w.summary = summary.clone();
                        w.turns_used = turn + 1;
                    }
                }

                crate::tools::log_tools::append_to_app_log(&format!(
                    "WORKER_COMPLETE | id={} | reason=natural_completion | turns={} | elapsed={}s | scratchpad={} | summary={}",
                    worker_id, turn + 1, start_time.elapsed().as_secs(), scratchpad_id,
                    crate::content_security::safe_truncate(&summary, 120),
                ));
                emit_worker_event("worker-completed", &json!({
                    "worker_id": worker_id, "status": "completed",
                    "summary": summary, "scratchpad_id": scratchpad_id,
                    "turns_used": turn + 1,
                }));
                write_worker_to_bus(&worker_id, "Completed (natural)", turn + 1);
                return;
            }
            ChatResponse::ToolCalls { content, tool_calls, .. } => {
                // Repetition detection: same tool calls with same args 3 times → stuck
                // Includes arguments so that web_search("A"), web_search("B") ≠ loop
                let mut call_sigs: Vec<String> = tool_calls.iter().map(|tc| {
                    format!("{}({})", tc.name, serde_json::to_string(&tc.arguments).unwrap_or_default())
                }).collect();
                call_sigs.sort();
                let fingerprint = call_sigs.join(";");
                if fingerprint == last_tool_fingerprint {
                    repetition_count += 1;
                    if repetition_count >= 3 {
                        let tool_names: Vec<&str> = tool_calls.iter().map(|tc| tc.name.as_str()).collect();
                        let reason = format!("Tool loop detected: identical call set repeated {} times: {}", repetition_count, tool_names.join(", "));
                        write_to_scratchpad(&scratchpad_id, "errors",
                            &format!("[worker {}] {}", worker_id, reason)).await;
                        update_worker_status(&worker_id,
                            WorkerStatus::Failed(reason.clone())).await;
                        crate::tools::log_tools::append_to_app_log(&format!(
                            "WORKER_FAILED | id={} | reason=repetition_loop | turns={} | elapsed={}s | scratchpad={}",
                            worker_id, turn + 1, start_time.elapsed().as_secs(), scratchpad_id,
                        ));
                        emit_worker_event("worker-completed", &json!({
                            "worker_id": worker_id, "status": "failed",
                            "error": reason, "scratchpad_id": scratchpad_id,
                            "turns_used": turn + 1,
                        }));
                        write_worker_to_bus(&worker_id, "Failed (repetition loop)", turn + 1);
                        return;
                    }
                } else {
                    repetition_count = 0;
                    last_tool_fingerprint = fingerprint;
                }

                // Add assistant message with tool calls
                let tc_json: Vec<serde_json::Value> = tool_calls.iter().map(|tc| {
                    json!({
                        "id": tc.id,
                        "type": "function",
                        "function": {
                            "name": tc.name,
                            "arguments": serde_json::to_string(&tc.arguments).unwrap_or_default()
                        }
                    })
                }).collect();

                let mut assistant_msg = json!({"role": "assistant", "tool_calls": tc_json});
                if let Some(c) = content {
                    if !c.is_empty() {
                        assistant_msg["content"] = json!(c);
                    }
                }
                messages.push(assistant_msg);

                // Execute each tool call
                for tc in &tool_calls {
                    // Double-check blocked tools
                    if WORKER_BLOCKED_TOOLS.contains(&tc.name.as_str()) {
                        messages.push(json!({
                            "role": "tool",
                            "tool_call_id": tc.id,
                            "content": format!("Tool '{}' is not available to workers.", tc.name)
                        }));
                        continue;
                    }

                    let result = match registry.execute(&tc.name, tc.arguments.clone()).await {
                        Ok(r) => r,
                        Err(e) => {
                            messages.push(json!({
                                "role": "tool",
                                "tool_call_id": tc.id,
                                "content": format!("Tool error: {}", e)
                            }));
                            // Track even failed attempts (for progress gate)
                            if tc.name != "report_to_parent" {
                                let mut ws = workers().write().await;
                                if let Some(w) = ws.get_mut(&worker_id) {
                                    w.tools_executed += 1;
                                }
                            }
                            continue;
                        }
                    };

                    // Truncate large results to save context (char-safe for CJK/emoji)
                    let content = crate::content_security::safe_truncate(&result.content, 15_000);

                    messages.push(json!({
                        "role": "tool",
                        "tool_call_id": tc.id,
                        "content": content
                    }));

                    // Track tool executions (for report_to_parent progress gate)
                    if tc.name != "report_to_parent" {
                        let mut ws = workers().write().await;
                        if let Some(w) = ws.get_mut(&worker_id) {
                            w.tools_executed += 1;
                        }
                    }
                }
            }
        }
    }

    // Max turns safety valve reached (this should be rare with wall clock as primary)
    let elapsed_secs = start_time.elapsed().as_secs();
    update_worker_status(&worker_id,
        WorkerStatus::Completed).await;
    write_to_scratchpad(&scratchpad_id, "result",
        &format!("[worker {}] Reached safety turn limit ({}) after {}s. Work may be incomplete.", worker_id, max_turns, elapsed_secs)).await;

    let summary = format!("Completed after {} turns / {}s (safety turn limit)", max_turns, elapsed_secs);
    {
        let mut ws = workers().write().await;
        if let Some(w) = ws.get_mut(&worker_id) {
            w.summary = summary.clone();
        }
    }

    crate::tools::log_tools::append_to_app_log(&format!(
        "WORKER_COMPLETE | id={} | reason=safety_turn_limit | turns={} | elapsed={}s | scratchpad={}",
        worker_id, max_turns, elapsed_secs, scratchpad_id,
    ));
    emit_worker_event("worker-completed", &json!({
        "worker_id": worker_id, "status": "completed",
        "summary": summary, "scratchpad_id": scratchpad_id,
        "turns_used": max_turns,
    }));
    write_worker_to_bus(&worker_id, "Completed (safety turn limit)", max_turns);
}

/// Phase 5D: Write worker completion to context bus (fire-and-forget, P4).
fn write_worker_to_bus(worker_id: &str, reason: &str, turns: usize) {
    let agent = worker_id.to_string();
    let content = format!("{} ({} turns)", reason, turns);
    tokio::spawn(async move {
        super::scratchpad_tools::context_bus_write(&agent, &content).await;
    });
}

/// Write content to a scratchpad section (accesses the global scratchpad state).
async fn write_to_scratchpad(pad_id: &str, section: &str, content: &str) {
    use super::scratchpad_tools;
    // Use the scratchpad_write tool directly via the global state
    let tool = scratchpad_tools::ScratchpadWriteTool;
    let _ = tool.execute(json!({
        "scratchpad_id": pad_id,
        "section": section,
        "content": content,
    })).await;
}

/// Update a worker's status in the global registry.
async fn update_worker_status(worker_id: &str, status: WorkerStatus) {
    let mut ws = workers().write().await;
    if let Some(w) = ws.get_mut(worker_id) {
        w.status = status;
        w.last_activity = chrono::Utc::now();
    }
}

// ============================================
// worker_status
// ============================================

pub struct WorkerStatusTool;

#[async_trait::async_trait]
impl HiveTool for WorkerStatusTool {
    fn name(&self) -> &str { "worker_status" }

    fn description(&self) -> &str {
        "Check the status of spawned workers. Shows progress, turns used, \
         and current state. Omit worker_id to see all workers."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "worker_id": {
                    "type": "string",
                    "description": "Specific worker to check (omit for all workers)"
                }
            }
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::Low }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let filter_id = params.get("worker_id").and_then(|v| v.as_str());

        let ws = workers().read().await;

        if ws.is_empty() {
            return Ok(ToolResult {
                content: "No workers have been spawned.".to_string(),
                is_error: false,
            });
        }

        let mut output = String::new();

        let workers_to_show: Vec<&WorkerInfo> = if let Some(id) = filter_id {
            ws.values().filter(|w| w.id == id).collect()
        } else {
            ws.values().collect()
        };

        if workers_to_show.is_empty() {
            if let Some(id) = filter_id {
                return Ok(ToolResult {
                    content: format!("Worker '{}' not found.", id),
                    is_error: true,
                });
            }
        }

        for w in &workers_to_show {
            let elapsed = (chrono::Utc::now() - w.started_at).num_seconds();
            let time_pct = if w.max_time_seconds > 0 {
                (elapsed as f64 / w.max_time_seconds as f64 * 100.0).min(100.0)
            } else { 0.0 };
            output.push_str(&format!(
                "Worker: {}\n  Status: {}\n  Model: {} ({})\n  Turns: {}/{} (safety)  |  Time: {}s/{}s ({:.0}%)\n  Tools executed: {}\n  Scratchpad: {}\n  Task: {}\n",
                w.id,
                w.status,
                w.model, w.provider,
                w.turns_used, w.max_turns,
                elapsed, w.max_time_seconds, time_pct,
                w.tools_executed,
                w.scratchpad_id,
                crate::content_security::safe_truncate(&w.task, 80),
            ));
            if !w.summary.is_empty() {
                output.push_str(&format!("  Summary: {}\n", w.summary));
            }
            output.push('\n');
        }

        Ok(ToolResult {
            content: output,
            is_error: false,
        })
    }
}

// ============================================
// worker_terminate
// ============================================

pub struct WorkerTerminateTool;

#[async_trait::async_trait]
impl HiveTool for WorkerTerminateTool {
    fn name(&self) -> &str { "worker_terminate" }

    fn description(&self) -> &str {
        "Terminate a running worker. The worker will stop after its current turn completes."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "worker_id": {
                    "type": "string",
                    "description": "ID of the worker to terminate"
                },
                "reason": {
                    "type": "string",
                    "description": "Optional reason for termination"
                }
            },
            "required": ["worker_id"]
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::Low }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let worker_id = params.get("worker_id")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: worker_id")?;

        let reason = params.get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("user requested");

        let mut ws = workers().write().await;

        if let Some(w) = ws.get_mut(worker_id) {
            if w.status == WorkerStatus::Running {
                w.status = WorkerStatus::Terminated;
                w.summary = format!("Terminated: {}", reason);
                let elapsed = (chrono::Utc::now() - w.started_at).num_seconds();
                crate::tools::log_tools::append_to_app_log(&format!(
                    "WORKER_TERMINATED | id={} | reason={} | turns={} | elapsed={}s | scratchpad={}",
                    worker_id, reason, w.turns_used, elapsed, w.scratchpad_id,
                ));
                Ok(ToolResult {
                    content: format!(
                        "Worker '{}' marked for termination (reason: {}). It will stop after the current turn.",
                        worker_id, reason,
                    ),
                    is_error: false,
                })
            } else {
                Ok(ToolResult {
                    content: format!("Worker '{}' is not running (status: {}).", worker_id, w.status),
                    is_error: true,
                })
            }
        } else {
            Ok(ToolResult {
                content: format!("Worker '{}' not found.", worker_id),
                is_error: true,
            })
        }
    }
}

// ============================================
// report_to_parent (worker → parent chat channel)
// ============================================

pub struct WorkerReportTool;

#[async_trait::async_trait]
impl HiveTool for WorkerReportTool {
    fn name(&self) -> &str { "report_to_parent" }

    fn description(&self) -> &str {
        "Send a message to the parent orchestrator. Your message appears in the parent's \
         chat as a worker channel message.\n\n\
         GATES: Reports require new tool work since last report (progress proof) and are \
         rate-limited to 30s cooldown. Duplicate reports (>70% word overlap) are rejected. \
         Use severity 'error' or 'done' to bypass rate limit and progress gate.\n\n\
         Severity: 'info' for progress, 'warning' for issues, 'error' for failures \
         needing parent help, 'done' when finished.\n\n\
         Your worker_id was provided in your task assignment."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "worker_id": {
                    "type": "string",
                    "description": "Your worker ID (provided in your task assignment)"
                },
                "message": {
                    "type": "string",
                    "description": "The message to send to the parent orchestrator"
                },
                "severity": {
                    "type": "string",
                    "enum": ["info", "warning", "error", "done"],
                    "description": "Message severity (default: info)"
                }
            },
            "required": ["worker_id", "message"]
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::Low }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let worker_id = params.get("worker_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let message = params.get("message")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: message")?;
        let severity = params.get("severity")
            .and_then(|v| v.as_str())
            .unwrap_or("info");

        // === Gate checks: progress proof, rate limit, semantic dedup ===
        // 'error' and 'done' bypass rate limit + progress gate (always allowed through).
        let scratchpad_id = {
            let ws = workers().read().await;
            let pad = ws.get(worker_id).map(|w| w.scratchpad_id.clone()).unwrap_or_default();

            if let Some(w) = ws.get(worker_id) {
                let bypass = severity == "error" || severity == "done";

                // 1. Rate limit: 30s cooldown between reports
                if !bypass {
                    if let Some(last_time) = w.last_report_time {
                        let elapsed = (chrono::Utc::now() - last_time).num_seconds();
                        if elapsed < 30 {
                            return Ok(ToolResult {
                                content: format!(
                                    "Report throttled: {}s since last report (30s cooldown). \
                                     Do more tool work before reporting. Severity 'error'/'done' bypasses this.",
                                    elapsed
                                ),
                                is_error: true,
                            });
                        }
                    }
                }

                // 2. Progress gate: must have executed tools since last report
                if !bypass && w.tools_executed <= w.last_report_tool_count {
                    return Ok(ToolResult {
                        content: "Report rejected: no new tool work since last report. \
                                 Execute tools (web_search, read_file, scratchpad_write, etc.) \
                                 before sending progress updates. Severity 'error'/'done' bypasses this.".to_string(),
                        is_error: true,
                    });
                }

                // 3. Semantic dedup: reject if >70% word overlap with last report
                if !w.last_report_words.is_empty() {
                    let current_words = extract_words(message);
                    let similarity = jaccard_similarity(&current_words, &w.last_report_words);
                    if similarity > 0.70 {
                        return Ok(ToolResult {
                            content: format!(
                                "Report rejected: {:.0}% similar to your last report. \
                                 Only report genuinely new findings or different content.",
                                similarity * 100.0
                            ),
                            is_error: true,
                        });
                    }
                }
            }

            pad
        };

        // Log to persistent app log
        crate::tools::log_tools::append_to_app_log(&format!(
            "WORKER_REPORT | id={} | severity={} | message={}",
            worker_id, severity, crate::content_security::safe_truncate(message, 200),
        ));

        // Emit event → frontend injects into parent chat (like Telegram/Discord channels)
        emit_worker_event("worker-message", &json!({
            "worker_id": worker_id,
            "message": message,
            "severity": severity,
            "scratchpad_id": scratchpad_id,
        }));

        // Also persist to scratchpad for durability
        if !scratchpad_id.is_empty() {
            write_to_scratchpad(&scratchpad_id, "reports",
                &format!("[{}] [{}] {}", severity, worker_id, message)).await;
        }

        // Update report tracking state (after successful delivery)
        {
            let mut ws = workers().write().await;
            if let Some(w) = ws.get_mut(worker_id) {
                w.last_report_tool_count = w.tools_executed;
                w.last_report_time = Some(chrono::Utc::now());
                w.last_report_words = extract_words(message);
                // DoneSignal: set flag so the worker loop exits after this turn
                if severity == "done" {
                    w.done_signaled = true;
                }
            }
        }

        // Done signal response tells the worker to stop explicitly
        let done_suffix = if severity == "done" {
            " Worker will exit after this turn."
        } else {
            ""
        };

        Ok(ToolResult {
            content: format!("Reported to parent: [{}] {}{}",
                severity,
                crate::content_security::safe_truncate(message, 100),
                done_suffix,
            ),
            is_error: false,
        })
    }
}

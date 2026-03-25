//! HIVE Orchestrator — the Brain (Phase 4)
//!
//! Routes tasks to specialist slots, manages sleep/wake lifecycle,
//! tracks VRAM budget, and logs everything to MAGMA.
//!
//! This is the "consciousness layer" from HOT_SWAP_MECHANICS.md,
//! implemented as Rust state management + Tauri commands.
//!
//! Flow:
//!   User message → route_task() → determine specialist needed
//!     → wake_slot() if not loaded → inject MAGMA context
//!     → execute via provider → extract state → sleep_slot() if idle
//!
//! Principle Lattice alignment:
//!   P1 (Bridges)  — Orchestrator bridges user intent → specialist selection → execution
//!   P2 (Agnostic) — Routes to any provider; local and cloud slots are interchangeable
//!   P3 (Simplicity) — Keyword heuristic first, LLM routing only if needed
//!   P4 (Errors)   — Failed routes fall back to consciousness; always answers
//!   P7 (Survives) — Slot definitions survive model changes; orchestrator is stable

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;

use crate::slots::{SlotRole, SlotState, SlotStatus, SlotsState, VramBudget};
use crate::memory::MemoryState;

// ============================================
// Route Decision
// ============================================

/// The orchestrator's decision on how to handle a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteDecision {
    /// Which slot should handle this task.
    pub slot: SlotRole,
    /// Why this slot was chosen (for transparency / debugging).
    pub reason: String,
    /// Confidence in the routing decision (0.0–1.0).
    pub confidence: f64,
    /// Does the slot need to be woken up?
    pub needs_wake: bool,
    /// Does another slot need to sleep first (VRAM constraint)?
    pub needs_evict: Option<SlotRole>,
}

/// Result of a sleep operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SleepResult {
    pub slot: SlotRole,
    pub vram_freed_gb: f64,
    pub events_recorded: usize,
}

// ============================================
// Task Analysis (P3: simple heuristic first)
// ============================================

// ============================================
// Phase 5: 3-Layer Tiered Router
// Layer 1: Keyword rules (0ms, deterministic)
// Layer 2: Embedding similarity (5-15ms, offline)
// Layer 3: LLM structured output (future — requires async)
// ============================================

/// Phase 5 Layer 2: Pre-computed utterance embeddings per specialist.
/// Maps SlotRole → Vec of utterance embeddings. Uses MAX aggregation for routing.
/// Computed on first call, cached for process lifetime.
static SPECIALIST_VECTORS: OnceLock<HashMap<SlotRole, Vec<Vec<f64>>>> = OnceLock::new();

/// Phase 5: Synthetic utterances per specialist (same Tool2Vec pattern as Phase 4).
/// "What would a user say that needs this specialist?"
fn specialist_utterances() -> HashMap<SlotRole, Vec<&'static str>> {
    let mut m = HashMap::new();
    m.insert(SlotRole::Consciousness, vec![
        "explain this concept to me",
        "what do you think about this approach",
        "help me understand how this works",
        "analyze this situation for me",
        "let's discuss the architecture",
        "summarize what we have done so far",
        "give me your opinion on this design",
        "help me plan the implementation",
    ]);
    m.insert(SlotRole::Coder, vec![
        "write a function that does this",
        "fix this bug in the code",
        "refactor this function to be cleaner",
        "implement this interface for me",
        "add error handling to this code",
        "review this code for issues",
        "create a class that handles this",
        "debug why this test fails",
    ]);
    m.insert(SlotRole::Terminal, vec![
        "run this command for me",
        "execute the build script",
        "check the server logs please",
        "install this package dependency",
        "list the files in this directory",
        "start the development server",
        "kill the process on this port",
        "check the disk usage",
    ]);
    m.insert(SlotRole::WebCrawl, vec![
        "search the web for information about",
        "find documentation for this library",
        "look up the latest release notes for",
        "what is the current status of this project",
        "research best practices for doing this",
        "fetch this URL and extract the content",
        "find articles about this topic online",
        "check if this API endpoint is documented",
    ]);
    m.insert(SlotRole::ToolCall, vec![
        "use the tool to create a new file",
        "call the API and process the response",
        "save this information to memory",
        "send a message to the telegram channel",
        "check the integration status of services",
        "execute a multi-step workflow here",
        "automate this sequence of operations",
        "run the pipeline end to end",
    ]);
    m
}

/// Compute or retrieve cached specialist utterance embeddings.
fn get_specialist_vectors() -> &'static HashMap<SlotRole, Vec<Vec<f64>>> {
    SPECIALIST_VECTORS.get_or_init(|| {
        let utterances = specialist_utterances();
        let mut vectors = HashMap::new();

        for (role, phrases) in &utterances {
            let embeddings: Vec<Vec<f64>> = phrases.iter()
                .filter_map(|p| crate::memory::get_local_embedding(p).ok())
                .collect();

            if !embeddings.is_empty() {
                vectors.insert(*role, embeddings);
            }
        }

        vectors
    })
}

/// Phase 5 Layer 1: Keyword-based classification (deterministic, 0ms).
/// Only routes when confident (3+ keyword matches). Returns None for ambiguous cases.
fn classify_by_keywords(input: &str) -> Option<(SlotRole, f64, String)> {
    let lower = input.to_lowercase();

    let code_keywords = [
        "code", "function", "class", "debug", "refactor", "implement",
        "compile", "build", "test", "bug", "fix", "error", "syntax",
        "algorithm", "struct", "enum", "trait", "async", "api",
        "typescript", "rust", "python", "javascript", "react",
        "component", "module", "import", "export", "variable",
    ];
    let terminal_keywords = [
        "terminal", "command", "execute", "run", "shell", "bash",
        "install", "npm", "cargo", "git", "mkdir", "ls", "cd",
        "process", "kill", "docker", "ssh", "curl", "wget",
    ];
    let web_keywords = [
        "web", "scrape", "search", "crawl", "fetch", "browse",
        "website", "url", "http", "html", "download", "research",
        "documentation", "find online", "look up",
    ];
    let tool_keywords = [
        "api call", "tool", "function call", "webhook", "endpoint",
        "integration", "automate", "pipeline",
    ];

    let code_score: f64 = code_keywords.iter().filter(|kw| lower.contains(*kw)).count() as f64;
    let terminal_score: f64 = terminal_keywords.iter().filter(|kw| lower.contains(*kw)).count() as f64;
    let web_score: f64 = web_keywords.iter().filter(|kw| lower.contains(*kw)).count() as f64;
    let tool_score: f64 = tool_keywords.iter().filter(|kw| lower.contains(*kw)).count() as f64;

    let max_score = code_score.max(terminal_score).max(web_score).max(tool_score);

    if max_score == 0.0 {
        return None; // No keywords — escalate to Layer 2
    }

    let (role, score, reason) = if code_score == max_score {
        (SlotRole::Coder, code_score, format!("L1 keyword: {} code matches", code_score as usize))
    } else if terminal_score == max_score {
        (SlotRole::Terminal, terminal_score, format!("L1 keyword: {} terminal matches", terminal_score as usize))
    } else if web_score == max_score {
        (SlotRole::WebCrawl, web_score, format!("L1 keyword: {} web matches", web_score as usize))
    } else {
        (SlotRole::ToolCall, tool_score, format!("L1 keyword: {} tool matches", tool_score as usize))
    };

    let confidence = (0.3 + score * 0.2).min(0.95);
    Some((role, confidence, reason))
}

/// Phase 5 Layer 2: Embedding similarity classification (5-15ms, offline).
/// Uses MAX aggregation — cosine(query, utterance) for each utterance per specialist,
/// takes the MAX per specialist, routes to highest if above threshold.
fn classify_by_embedding(input: &str) -> Option<(SlotRole, f64, String)> {
    let query_emb = crate::memory::get_local_embedding(input).ok()?;
    let specialist_vecs = get_specialist_vectors();

    if specialist_vecs.is_empty() {
        return None; // fastembed not available
    }

    let mut best_role = SlotRole::Consciousness;
    let mut best_sim = 0.0f64;

    for (role, utterance_embs) in specialist_vecs {
        // MAX aggregation: take highest cosine similarity across all utterances
        let max_sim = utterance_embs.iter()
            .map(|utt| crate::memory::cosine_similarity(&query_emb, utt))
            .fold(0.0f64, f64::max);

        if max_sim > best_sim {
            best_sim = max_sim;
            best_role = *role;
        }
    }

    if best_sim > 0.45 {
        // Map similarity to confidence: 0.45 → 0.6, 0.82+ → 0.95
        let confidence = (0.6 + (best_sim - 0.45) * 0.95).min(0.95);
        Some((best_role, confidence, format!("L2 semantic: {:.2} cosine similarity", best_sim)))
    } else {
        None // Below threshold — all specialists equally irrelevant
    }
}

/// 3-layer tiered task classification (Phase 5 Intelligence Graduation).
///
/// Layer 1: Keyword rules — 0ms, deterministic. Routes unambiguous cases.
/// Layer 2: Embedding similarity — 5-15ms, offline. MAX aggregation per specialist.
/// Layer 3: (Future) LLM structured output — 200-800ms, requires API.
///
/// Falls back to Consciousness if no layer claims the task.
/// Returns (role, confidence, reason).
fn classify_task(input: &str) -> (SlotRole, f64, String) {
    // Layer 1: Keyword rules (0ms, deterministic)
    if let Some(result) = classify_by_keywords(input) {
        return result;
    }

    // Layer 2: Embedding similarity (5-15ms, offline)
    if let Some(result) = classify_by_embedding(input) {
        return result;
    }

    // No layer claimed the task — Consciousness handles general conversation
    (SlotRole::Consciousness, 0.8, "General conversation — no specialist needed".to_string())
}

// ============================================
// VRAM Planning
// ============================================

/// Determine what needs to happen to fit a slot into VRAM.
fn plan_vram(
    target_vram: f64,
    budget: &VramBudget,
    active_slots: &HashMap<SlotRole, SlotState>,
) -> Option<SlotRole> {
    if budget.can_fit(target_vram) {
        return None; // Fits without eviction
    }

    // Need to evict. Pick the oldest non-consciousness active slot.
    let mut candidates: Vec<_> = active_slots.iter()
        .filter(|(role, state)| {
            !role.is_always_loaded() && state.is_active() && state.vram_used_gb > 0.0
        })
        .collect();

    // Sort by last_active (oldest first — LRU eviction)
    candidates.sort_by(|a, b| {
        let a_time = a.1.last_active.as_deref().unwrap_or("");
        let b_time = b.1.last_active.as_deref().unwrap_or("");
        a_time.cmp(b_time)
    });

    // Find first candidate whose eviction frees enough
    for (role, state) in candidates {
        if budget.can_fit(target_vram - state.vram_used_gb) || state.vram_used_gb >= budget.deficit(target_vram) {
            return Some(*role);
        }
    }

    None // Can't fit even with eviction — need cloud fallback
}

// ============================================
// Wake Context Building (MAGMA integration)
// ============================================

/// Build the "Good Morning" briefing for a waking specialist.
/// Pulls relevant context from MAGMA's episodic + entity graphs.
fn build_wake_context(
    role: SlotRole,
    task: &str,
    last_sleep: Option<&str>, // ISO8601 timestamp of last sleep
    conn: &rusqlite::Connection,
) -> String {
    let now = Utc::now().to_rfc3339();
    let role_name = role.display_name();

    let mut context = format!(
        "# WAKE BRIEFING: {} Specialist\n\n**Current Task:**\n{}\n\n**Timestamp:** {}\n",
        role_name, task, now
    );

    // If we have a previous sleep timestamp, query what happened since
    if let Some(since) = last_sleep {
        // Events since last sleep
        if let Ok(mut stmt) = conn.prepare(
            "SELECT event_type, agent, content, created_at FROM events
             WHERE created_at > ?1 ORDER BY created_at ASC LIMIT 20"
        ) {
            let events: Vec<String> = stmt.query_map(
                rusqlite::params![since],
                |row| {
                    let etype: String = row.get(0)?;
                    let agent: String = row.get(1)?;
                    let content: String = row.get(2)?;
                    let time: String = row.get(3)?;
                    Ok(format!("- [{}] {} ({}): {}", time, etype, agent, content))
                }
            ).ok()
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default();

            if !events.is_empty() {
                context.push_str("\n**Events while you were asleep:**\n");
                for event in &events {
                    context.push_str(event);
                    context.push('\n');
                }
            }
        }

        // Entity changes since last sleep
        if let Ok(mut stmt) = conn.prepare(
            "SELECT entity_type, name, state FROM entities
             WHERE updated_at > ?1 ORDER BY updated_at DESC LIMIT 10"
        ) {
            let entities: Vec<String> = stmt.query_map(
                rusqlite::params![since],
                |row| {
                    let etype: String = row.get(0)?;
                    let name: String = row.get(1)?;
                    Ok(format!("- {} '{}' was modified", etype, name))
                }
            ).ok()
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default();

            if !entities.is_empty() {
                context.push_str("\n**Modified entities:**\n");
                for entity in &entities {
                    context.push_str(entity);
                    context.push('\n');
                }
            }
        }
    } else {
        context.push_str("\n**First activation** — no prior state.\n");
    }

    // Relevant procedures (learned tool chains for this role)
    if let Ok(mut stmt) = conn.prepare(
        "SELECT name, description, success_count, fail_count FROM procedures
         WHERE trigger_pattern LIKE ?1 AND success_count > fail_count
         ORDER BY success_count DESC LIMIT 3"
    ) {
        let role_pattern = format!("%{}%", role_name.to_lowercase());
        let procs: Vec<String> = stmt.query_map(
            rusqlite::params![role_pattern],
            |row| {
                let name: String = row.get(0)?;
                let desc: String = row.get(1)?;
                let success: i64 = row.get(2)?;
                let fail: i64 = row.get(3)?;
                Ok(format!("- {} ({}x success, {}x fail): {}", name, success, fail, desc))
            }
        ).ok()
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default();

        if !procs.is_empty() {
            context.push_str("\n**Learned procedures:**\n");
            for proc in &procs {
                context.push_str(proc);
                context.push('\n');
            }
        }
    }

    context
}

// ============================================
// Tauri Commands
// ============================================

/// Analyze a task and decide which slot should handle it.
/// Does NOT wake the slot — just returns the decision.
#[tauri::command]
pub fn route_task(
    slots_state: tauri::State<'_, SlotsState>,
    task: String,
) -> Result<RouteDecision, String> {
    let (role, confidence, reason) = classify_task(&task);

    let states = slots_state.states.lock().map_err(|e| format!("Lock error: {}", e))?;
    let configs = slots_state.configs.lock().map_err(|e| format!("Lock error: {}", e))?;
    let budget = slots_state.vram_budget.lock().map_err(|e| format!("Lock error: {}", e))?;

    // Check if the slot is already active
    let slot_state = states.get(&role);
    let needs_wake = slot_state.map(|s| !s.is_active()).unwrap_or(true);

    // Check if we need to evict another slot for VRAM
    let needs_evict = if needs_wake {
        let config = configs.get(&role);
        let target_vram = config
            .and_then(|c| c.best_assignment())
            .map(|a| a.vram_gb)
            .unwrap_or(0.0);

        if target_vram > 0.0 {
            plan_vram(target_vram, &budget, &states)
        } else {
            None // Cloud slot, no VRAM needed
        }
    } else {
        None
    };

    Ok(RouteDecision {
        slot: role,
        reason,
        confidence,
        needs_wake,
        needs_evict,
    })
}

/// Build wake context for a specialist from within a HiveTool (no Tauri state).
///
/// Opens its own DB connection because HiveTool::execute() doesn't receive
/// Tauri managed state. This is acceptable because:
///   1. route_to_specialist fires at most once per tool loop iteration
///   2. SQLite WAL mode handles concurrent readers without blocking
///   3. The connection is dropped immediately after building context
///
/// Best-effort — returns empty string on any failure (P4).
pub fn build_wake_context_for_tool(specialist: &str, task: &str) -> String {
    let role = match specialist {
        "coder" => SlotRole::Coder,
        "terminal" => SlotRole::Terminal,
        "webcrawl" => SlotRole::WebCrawl,
        "toolcall" => SlotRole::ToolCall,
        "consciousness" => SlotRole::Consciousness,
        _ => return String::new(),
    };
    let db_path = crate::paths::get_app_data_dir().join("memory.db");
    let conn = match rusqlite::Connection::open(&db_path) {
        Ok(c) => c,
        Err(e) => {
            crate::tools::log_tools::append_to_app_log(&format!(
                "SLOTS | wake_context_failed | specialist={} | err=db_open: {}", specialist, e
            ));
            return String::new();
        }
    };
    if let Err(e) = conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000; PRAGMA foreign_keys=ON;") {
        eprintln!("[HIVE] Orchestrator: PRAGMA setup failed: {}", e);
    }
    // No slot state available — pass None for last_sleep (gets full briefing)
    let context = build_wake_context(role, task, None, &conn);
    if !context.is_empty() {
        crate::tools::log_tools::append_to_app_log(&format!(
            "SLOTS | wake_context_built | specialist={} | chars={}", specialist, context.len()
        ));
    }
    context
}

/// Get the wake context for a slot (MAGMA briefing).
/// Called by the frontend before waking a specialist.
#[tauri::command]
pub fn get_wake_context(
    memory_state: tauri::State<'_, MemoryState>,
    slots_state: tauri::State<'_, SlotsState>,
    role: SlotRole,
    task: String,
) -> Result<String, String> {
    // Get last sleep time for this slot
    let states = slots_state.states.lock().map_err(|e| format!("Lock error: {}", e))?;
    let last_sleep = states.get(&role)
        .and_then(|s| s.last_active.clone());

    // Access MAGMA DB for context
    let db_guard = memory_state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;

    let context = build_wake_context(role, &task, last_sleep.as_deref(), conn);
    Ok(context)
}

/// Record that a slot has been woken (update state + log event).
#[tauri::command]
pub fn record_slot_wake(
    slots_state: tauri::State<'_, SlotsState>,
    role: SlotRole,
    provider: String,
    model: String,
    port: Option<u16>,
    vram_gb: f64,
) -> Result<SlotState, String> {
    let mut states = slots_state.states.lock().map_err(|e| format!("Lock error: {}", e))?;
    let mut budget = slots_state.vram_budget.lock().map_err(|e| format!("Lock error: {}", e))?;

    let now = Utc::now().to_rfc3339();

    let state = states.entry(role).or_insert(SlotState::idle(role));
    state.status = SlotStatus::Active;
    state.assignment = Some(crate::slots::SlotAssignment {
        provider,
        model,
        vram_gb,
        context_length: 0, // filled by caller
    });
    state.server_port = port;
    state.loaded_at = Some(now.clone());
    state.last_active = Some(now);
    state.vram_used_gb = vram_gb;

    budget.used_gb += vram_gb;

    Ok(state.clone())
}

/// Record that a slot has been put to sleep (update state + free VRAM).
#[tauri::command]
pub fn record_slot_sleep(
    slots_state: tauri::State<'_, SlotsState>,
    role: SlotRole,
) -> Result<SleepResult, String> {
    let mut states = slots_state.states.lock().map_err(|e| format!("Lock error: {}", e))?;
    let mut budget = slots_state.vram_budget.lock().map_err(|e| format!("Lock error: {}", e))?;

    let state = states.entry(role).or_insert(SlotState::idle(role));
    let vram_freed = state.vram_used_gb;

    // Reset to idle
    state.status = SlotStatus::Idle;
    state.assignment = None;
    state.server_port = None;
    state.vram_used_gb = 0.0;

    budget.used_gb = (budget.used_gb - vram_freed).max(0.0);

    Ok(SleepResult {
        slot: role,
        vram_freed_gb: vram_freed,
        events_recorded: 0, // filled by MAGMA integration
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- classify_task tests ---

    #[test]
    fn classify_code_task() {
        let (role, confidence, _) = classify_task("Please debug this function and fix the error");
        assert_eq!(role, SlotRole::Coder);
        assert!(confidence > 0.5);
    }

    #[test]
    fn classify_terminal_task() {
        let (role, _, _) = classify_task("Run npm install and then execute the build command");
        assert_eq!(role, SlotRole::Terminal);
    }

    #[test]
    fn classify_web_task() {
        let (role, _, _) = classify_task("Search the web for Rust documentation");
        assert_eq!(role, SlotRole::WebCrawl);
    }

    #[test]
    fn classify_tool_task() {
        let (role, _, _) = classify_task("Make an api call to the webhook endpoint");
        assert_eq!(role, SlotRole::ToolCall);
    }

    #[test]
    fn classify_general_defaults_to_consciousness() {
        let (role, confidence, _) = classify_task("What's the meaning of life?");
        assert_eq!(role, SlotRole::Consciousness);
        assert_eq!(confidence, 0.8);
    }

    #[test]
    fn classify_empty_input() {
        let (role, _, _) = classify_task("");
        assert_eq!(role, SlotRole::Consciousness);
    }

    #[test]
    fn classify_case_insensitive() {
        let (role, _, _) = classify_task("DEBUG this RUST CODE please");
        assert_eq!(role, SlotRole::Coder);
    }

    #[test]
    fn classify_confidence_scales_with_matches() {
        let (_, conf_low, _) = classify_task("fix something");  // 1 match
        let (_, conf_high, _) = classify_task("debug the function and fix the error in code");  // many matches
        assert!(conf_high > conf_low, "More matches = higher confidence");
    }

    #[test]
    fn classify_confidence_capped() {
        // Even with many matches, confidence should not exceed 0.95
        let (_, confidence, _) = classify_task(
            "code function class debug refactor implement compile build test bug fix error syntax algorithm"
        );
        assert!(confidence <= 0.95);
    }

    // --- plan_vram tests ---

    #[test]
    fn plan_vram_fits_without_eviction() {
        let budget = VramBudget { total_gb: 16.0, used_gb: 4.0, safety_buffer_gb: 2.0 };
        let active_slots = HashMap::new();
        assert!(plan_vram(4.0, &budget, &active_slots).is_none());
    }

    #[test]
    fn plan_vram_needs_eviction() {
        let budget = VramBudget { total_gb: 16.0, used_gb: 12.0, safety_buffer_gb: 2.0 };
        let mut active_slots = HashMap::new();
        active_slots.insert(SlotRole::Coder, SlotState {
            role: SlotRole::Coder,
            status: SlotStatus::Active,
            assignment: None,
            server_port: Some(8081),
            loaded_at: Some("2026-01-01T00:00:00Z".to_string()),
            last_active: Some("2026-01-01T00:00:00Z".to_string()),
            vram_used_gb: 6.0,
        });
        let result = plan_vram(4.0, &budget, &active_slots);
        assert_eq!(result, Some(SlotRole::Coder));
    }

    #[test]
    fn plan_vram_never_evicts_consciousness() {
        let budget = VramBudget { total_gb: 8.0, used_gb: 7.0, safety_buffer_gb: 2.0 };
        let mut active_slots = HashMap::new();
        active_slots.insert(SlotRole::Consciousness, SlotState {
            role: SlotRole::Consciousness,
            status: SlotStatus::Active,
            assignment: None,
            server_port: Some(8080),
            loaded_at: Some("2026-01-01T00:00:00Z".to_string()),
            last_active: Some("2026-01-01T00:00:00Z".to_string()),
            vram_used_gb: 4.0,
        });
        // Consciousness is always_loaded — should never be evicted
        let result = plan_vram(4.0, &budget, &active_slots);
        assert!(result.is_none(), "Consciousness should never be evicted");
    }

    #[test]
    fn plan_vram_evicts_oldest_first() {
        let budget = VramBudget { total_gb: 16.0, used_gb: 14.0, safety_buffer_gb: 2.0 };
        let mut active_slots = HashMap::new();
        active_slots.insert(SlotRole::Coder, SlotState {
            role: SlotRole::Coder,
            status: SlotStatus::Active,
            assignment: None,
            server_port: Some(8081),
            loaded_at: None,
            last_active: Some("2026-01-01T00:00:00Z".to_string()),  // older
            vram_used_gb: 4.0,
        });
        active_slots.insert(SlotRole::WebCrawl, SlotState {
            role: SlotRole::WebCrawl,
            status: SlotStatus::Active,
            assignment: None,
            server_port: Some(8083),
            loaded_at: None,
            last_active: Some("2026-03-01T00:00:00Z".to_string()),  // newer
            vram_used_gb: 4.0,
        });
        let result = plan_vram(2.0, &budget, &active_slots);
        assert_eq!(result, Some(SlotRole::Coder), "Should evict oldest (Coder)");
    }

    #[test]
    fn plan_vram_no_candidates() {
        let budget = VramBudget { total_gb: 8.0, used_gb: 7.0, safety_buffer_gb: 2.0 };
        // No active slots to evict
        let active_slots = HashMap::new();
        let result = plan_vram(4.0, &budget, &active_slots);
        assert!(result.is_none(), "No candidates = cloud fallback");
    }

    // --- VramBudget tests ---

    #[test]
    fn vram_budget_available() {
        let budget = VramBudget { total_gb: 16.0, used_gb: 4.0, safety_buffer_gb: 2.0 };
        assert_eq!(budget.available_gb(), 10.0);
    }

    #[test]
    fn vram_budget_available_never_negative() {
        let budget = VramBudget { total_gb: 8.0, used_gb: 10.0, safety_buffer_gb: 2.0 };
        assert_eq!(budget.available_gb(), 0.0);
    }

    #[test]
    fn vram_budget_can_fit() {
        let budget = VramBudget { total_gb: 16.0, used_gb: 4.0, safety_buffer_gb: 2.0 };
        assert!(budget.can_fit(10.0));
        assert!(!budget.can_fit(11.0));
    }

    #[test]
    fn vram_budget_deficit() {
        let budget = VramBudget { total_gb: 16.0, used_gb: 12.0, safety_buffer_gb: 2.0 };
        assert_eq!(budget.deficit(4.0), 2.0);  // available=2, need 4, deficit=2
        assert_eq!(budget.deficit(1.0), 0.0);  // fits
    }

    // --- Phase 5: Tiered router tests ---

    #[test]
    fn layer1_keywords_routes_unambiguous() {
        // Layer 1 should catch clear keyword matches
        let result = classify_by_keywords("debug this function and fix the error in code");
        assert!(result.is_some(), "Strong keyword match should route via L1");
        let (role, _, reason) = result.unwrap();
        assert_eq!(role, SlotRole::Coder);
        assert!(reason.starts_with("L1 keyword"), "Reason should indicate L1");
    }

    #[test]
    fn layer1_no_keywords_returns_none() {
        // Layer 1 should NOT route ambiguous input — escalate to L2
        let result = classify_by_keywords("What's the meaning of life?");
        assert!(result.is_none(), "No keywords → should escalate to Layer 2");
    }

    #[test]
    fn layer2_embedding_routes_semantic() {
        // Skip if fastembed not available
        if crate::memory::get_local_embedding("test").is_err() {
            eprintln!("Skipping L2 test (fastembed not available)");
            return;
        }

        // "help me deploy my application to production" has no direct keywords
        // but semantically relates to Terminal/Coder
        let result = classify_by_embedding("help me deploy my application to production");
        assert!(result.is_some(), "Semantic match should route via L2");
        let (_, _, reason) = result.unwrap();
        assert!(reason.starts_with("L2 semantic"), "Reason should indicate L2");
    }

    #[test]
    fn tiered_router_integration() {
        // classify_task should work end-to-end regardless of fastembed availability
        let (role, confidence, _) = classify_task("debug this function");
        assert_eq!(role, SlotRole::Coder, "Should route to Coder");
        assert!(confidence > 0.3);

        let (role, _, _) = classify_task("");
        assert_eq!(role, SlotRole::Consciousness, "Empty → Consciousness");
    }

    #[test]
    fn specialist_utterances_covers_all_roles() {
        let utterances = specialist_utterances();
        assert!(utterances.contains_key(&SlotRole::Consciousness));
        assert!(utterances.contains_key(&SlotRole::Coder));
        assert!(utterances.contains_key(&SlotRole::Terminal));
        assert!(utterances.contains_key(&SlotRole::WebCrawl));
        assert!(utterances.contains_key(&SlotRole::ToolCall));
        // Each should have 8 utterances
        for (role, phrases) in &utterances {
            assert!(phrases.len() >= 8, "{:?} should have at least 8 utterances", role);
        }
    }
}

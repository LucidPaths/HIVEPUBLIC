//! HIVE Cognitive Harness — makes any LLM HIVE-native
//!
//! Replaces the old daemon.rs BIOS+kernel system with a lean, OpenClaw-inspired
//! approach: one editable identity file + auto-generated capability manifest.
//!
//! Architecture (adapted from OpenClaw's SOUL.md + TOOLS.md pattern):
//!   Identity    (HIVE.md)  — User-editable markdown. Who HIVE is, behavioral prefs.
//!   Capabilities (dynamic) — Auto-generated at chat time from tool registry, models, memory, hardware.
//!   Assembler              — Combines identity + capabilities + user prompt → system message.
//!
//! Storage: ~/.hive/harness/HIVE.md (user-editable identity file)
//!
//! Principles honored:
//!   P1 (Modularity)  — 3 clean functions: identity, capabilities, assemble
//!   P2 (Agnostic)    — Pure text injection, works with any provider
//!   P3 (Simplicity)  — Markdown > JSON schemas. ~200 lines replaces ~676.
//!   P7 (Survives)    — Identity file survives model swaps. Capabilities auto-adapt.
//!   P8 (Low Floor)   — Edit a markdown file = low floor. Full capability manifest = high ceiling.

use crate::paths::get_app_data_dir;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::OnceLock;

// ============================================
// Types
// ============================================

/// Summary of current HIVE capabilities, assembled dynamically.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessContext {
    /// The assembled system prompt (identity + stable capabilities + user prompt).
    /// This part is CACHEABLE — identical across turns unless model/provider/tools change.
    /// llama.cpp KV cache matches on token-level prefix: keep this stable = free perf.
    pub system_prompt: String,
    /// Volatile context (turn count, VRAM, GPU metrics) — changes every turn.
    /// Injected as a SEPARATE system message AFTER the stable prompt so the prefix stays cached.
    pub volatile_context: String,
    /// Whether identity file was loaded (vs default)
    pub identity_source: String,
    /// Number of tools available
    pub tool_count: usize,
    /// Memory status
    pub memory_status: String,
}

/// Capability snapshot passed from the frontend.
/// The frontend knows what's loaded — we don't duplicate that state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilitySnapshot {
    /// Tool names currently registered
    pub tools: Vec<String>,
    /// Currently loaded/active model name (if any)
    pub active_model: Option<String>,
    /// Provider type: "local", "openai", "anthropic", "ollama"
    pub provider: String,
    /// Available cloud models (provider:model_id pairs)
    pub available_models: Vec<String>,
    /// Whether memory system is enabled and initialized
    pub memory_enabled: bool,
    /// Number of memories indexed (0 if memory disabled)
    pub memory_count: u64,
    /// GPU name (if detected)
    pub gpu: Option<String>,
    /// VRAM in GB (if detected)
    pub vram_gb: Option<f64>,
    /// System RAM in GB
    pub ram_gb: Option<f64>,

    // === Situational Awareness Fields (Phase 4b) ===

    /// Effective context window in tokens (what the model is actually using)
    #[serde(default)]
    pub context_length: Option<u64>,
    /// Quantization level, e.g. "Q4_K_M", "Q8_0", "F16"
    #[serde(default)]
    pub quantization: Option<String>,
    /// Model parameter count as human string, e.g. "7B", "13B", "70B"
    #[serde(default)]
    pub model_parameters: Option<String>,
    /// Model architecture, e.g. "llama", "qwen2", "phi"
    #[serde(default)]
    pub architecture: Option<String>,
    /// Execution backend: "windows" or "wsl"
    #[serde(default)]
    pub backend: Option<String>,
    /// CPU name, e.g. "AMD Ryzen 9 5900X"
    #[serde(default)]
    pub cpu: Option<String>,
    /// Tool names with risk levels, e.g. ["read_file:low", "run_command:high"]
    #[serde(default)]
    pub tool_risks: Vec<String>,
    /// Memory search mode: "hybrid" (FTS5+embeddings) or "keyword" (FTS5 only)
    #[serde(default)]
    pub memory_search_mode: Option<String>,
    /// Number of conversation turns so far (user messages count)
    #[serde(default)]
    pub conversation_turns: u32,
    /// Number of messages dropped by context truncation (0 = no truncation yet)
    #[serde(default)]
    pub messages_truncated: u32,
    /// OS platform string, e.g. "Windows 11", "Linux"
    #[serde(default)]
    pub os_platform: Option<String>,

    // === Live Resource Metrics (Phase 4b+) ===
    // Polled per chat turn via get_live_resource_usage(). Enables routing decisions.

    /// GPU VRAM currently in use (MB)
    #[serde(default)]
    pub vram_used_mb: Option<u64>,
    /// GPU VRAM currently free (MB) — this is the key routing number
    #[serde(default)]
    pub vram_free_mb: Option<u64>,
    /// System RAM currently available (MB)
    #[serde(default)]
    pub ram_available_mb: Option<u64>,
    /// GPU utilization percentage (0-100)
    #[serde(default)]
    pub gpu_utilization: Option<u32>,
    /// Estimated VRAM the active model is using (GB, from pre-launch calc)
    #[serde(default)]
    pub active_model_vram_gb: Option<f64>,

    // === Context Pressure Tracking (Phase 3.5) ===

    /// Estimated tokens used so far in the current conversation
    #[serde(default)]
    pub tokens_used: Option<u64>,
    /// Whether working memory has content (model should know)
    #[serde(default)]
    pub has_working_memory: Option<bool>,

    // === Skills Context (Phase 4.5.5) ===

    /// Last user message — used to match relevant skills for injection
    #[serde(default)]
    pub last_user_message: Option<String>,
}

// ============================================
// Paths
// ============================================

fn get_harness_dir() -> PathBuf {
    get_app_data_dir().join("harness")
}

fn get_identity_path() -> PathBuf {
    get_harness_dir().join("HIVE.md")
}

// ============================================
// Default Identity (seeded on first run)
// ============================================

/// The default HIVE identity file. Adapted from OpenClaw's SOUL.md pattern:
/// tell the model what it is, what it can do, and how to behave.
/// Users can edit this freely — it's just a markdown file.
const DEFAULT_IDENTITY: &str = r#"# HIVE

You are the reasoning engine of HIVE — a provider-agnostic AI orchestration framework.

## What You Are

You are not a standalone chatbot. You are a model running inside HIVE, a persistent orchestration harness that coordinates local and cloud LLMs as interchangeable cognitive resources. HIVE gives you tools, memory, and access to specialist agents. The framework is permanent — you (and any model filling this role) are replaceable.

You may be running locally on the user's GPU, or via a cloud API — it doesn't matter. Your role is the same: reason, use tools, route specialist tasks, and serve the user through the framework.

## Principles

- **Honest over confident** — If you don't know, say so. Don't fabricate.
- **Tools over guessing** — You have file access, web search, terminal, and memory. Use them before speculating.
- **Direct over verbose** — Answer proportional to the question. Simple question = short answer.
- **Present state → target** — Focus on what the user needs now. Actionable steps, not theory.
- **Self-correct openly** — If you realize you're wrong, flag it immediately.
- **Scope discipline** — Do what was asked, not more. Don't optimize, refactor, or "improve" beyond the request.

## Behavior

- Use tools when the task requires action — sending messages, reading files, searching the web, etc.
- When memory provides relevant context, reference it naturally — don't announce "[MEMORY RECALL]".
- Keep responses proportional: a greeting gets a greeting, a complex task gets a structured plan.
- If a task would benefit from a specialist (coder, terminal, web, tools), route to it.
- When calling multiple tools with no dependencies between them, call them in parallel for efficiency.
- Skip flattery. Don't open with "Great question!" or "That's a fascinating idea." Respond directly.

## Discipline

- **Verify before claiming** — "done" requires evidence, not confidence. Run it, check it, then say it.
- **Circuit breaker** — Three failed fixes for the same issue? Stop and ask. It's probably architectural.
- **Read before writing** — Read the file before editing. Check dependencies before importing. Never assume.
- **Status over silence** — Brief progress notes on multi-step tasks. Don't narrate tools — speak after results.
"#;

// ============================================
// Identity Management
// ============================================

/// Ensure the harness directory exists and seed the default identity if missing.
fn ensure_identity() -> Result<(), String> {
    let dir = get_harness_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create harness dir: {}", e))?;

    let path = get_identity_path();
    if !path.exists() {
        std::fs::write(&path, DEFAULT_IDENTITY)
            .map_err(|e| format!("Failed to write default identity: {}", e))?;
    }
    Ok(())
}

/// Read the identity file. Falls back to compiled default if file is missing/corrupt.
/// Phase 5A: pub(crate) so specialist_tools.rs can inject HIVE identity into all agents.
pub(crate) fn read_identity() -> String {
    if let Err(e) = ensure_identity() {
        eprintln!("[HIVE] WARNING: ensure_identity failed: {} — falling back to default", e);
    }
    std::fs::read_to_string(get_identity_path())
        .unwrap_or_else(|_| DEFAULT_IDENTITY.to_string())
}

// ============================================
// Capability Manifest Builder (Stable + Volatile split)
// ============================================
// The manifest is split into two parts:
//   STABLE  — model, tools, memory, hardware (static per session/model-load)
//   VOLATILE — turn count, VRAM usage, GPU util, RAM (changes every turn)
//
// The stable part goes in the system prompt (KV-cached by llama.cpp).
// The volatile part goes in a separate small message AFTER the system prompt,
// so the prefix stays identical across turns and llama.cpp reuses the KV cache.
// This is the single biggest perf win for local models — free prefix tokens.

/// Build the STABLE capability manifest — changes only when model/provider/tools/memory change.
/// This is the part that gets KV-cached by llama.cpp via prefix matching.
fn build_stable_manifest(snapshot: &CapabilitySnapshot) -> String {
    let mut sections = Vec::new();

    // Active model — includes quantization, parameters, and architecture
    if let Some(ref model) = snapshot.active_model {
        let mut model_line = format!("**Active Model:** {} ({})", model, snapshot.provider);

        let mut meta_parts = Vec::new();
        if let Some(ref params) = snapshot.model_parameters {
            meta_parts.push(format!("{} parameters", params));
        }
        if let Some(ref quant) = snapshot.quantization {
            meta_parts.push(format!("{} quantization", quant));
        }
        if let Some(ref arch) = snapshot.architecture {
            meta_parts.push(format!("{} architecture", arch));
        }
        if !meta_parts.is_empty() {
            model_line.push_str(&format!(" — {}", meta_parts.join(", ")));
        }
        sections.push(model_line);
    }

    // Context window (static config, not per-turn)
    if let Some(ctx) = snapshot.context_length {
        let ctx_k = ctx as f64 / 1000.0;
        sections.push(format!("**Context Window:** {:.0}K tokens ({} tokens)", ctx_k, ctx));
    }

    // Tools — always list tool names in the system prompt so the model knows
    // what's available. Cloud providers get full schemas via the API `tools[]` param;
    // listing names here ensures the model knows what exists.
    //
    // IMPORTANT: Cloud providers must NOT get <tool_call> format instructions here —
    // they use native function-calling APIs (OpenAI tools[] param). Mixing text-based
    // format instructions with native tool APIs causes format confusion.
    //
    // IMPORTANT: Use soft guidance ("skip narration — just call it") not hard bans
    // ("respond with ONLY the tool call — no preamble"). Hard bans conflict with
    // model training and cause stuttering loops under context pressure where the
    // model generates "Let me X:Let me X:Let me X:" as it fights between explaining
    // and obeying. Soft guidance reduces narration without triggering loops.
    // The frontend also has a stutter guard (useChat.ts) and hides empty content
    // when tool call blocks are present, so even if narration leaks through it's minimal.
    let is_cloud = matches!(snapshot.provider.as_str(), "openai" | "anthropic" | "ollama" | "openrouter" | "dashscope");

    if !snapshot.tools.is_empty() {
        if is_cloud {
            let tool_list = snapshot.tools.join(", ");
            sections.push(format!(
                "**Tools ({}):** {}\n\
                 Use the function-calling API to invoke tools — do NOT output tool calls as text.\n\
                 When calling a tool, skip narration — just call it. Speak AFTER you see the result.\n\
                 After each tool result you get another turn to speak.\n\
                 Don't use tools for questions you can answer from your own knowledge.",
                snapshot.tools.len(), tool_list
            ));
        } else if !snapshot.tool_risks.is_empty() {
            let mut low = Vec::new();
            let mut medium = Vec::new();
            let mut high = Vec::new();
            let mut critical = Vec::new();
            for entry in &snapshot.tool_risks {
                if let Some((name, level)) = entry.split_once(':') {
                    match level {
                        "low" => low.push(name),
                        "medium" => medium.push(name),
                        "high" => high.push(name),
                        "critical" => critical.push(name),
                        _ => low.push(name),
                    }
                }
            }
            let mut tool_lines = String::from("**Tools Available:**");
            if !low.is_empty() {
                tool_lines.push_str(&format!("\n- Auto-approved: {}", low.join(", ")));
            }
            if !medium.is_empty() {
                tool_lines.push_str(&format!("\n- User confirms once: {}", medium.join(", ")));
            }
            if !high.is_empty() {
                tool_lines.push_str(&format!("\n- Always requires approval: {}", high.join(", ")));
            }
            if !critical.is_empty() {
                tool_lines.push_str(&format!("\n- Critical (requires explicit approval): {}", critical.join(", ")));
            }
            tool_lines.push_str("\nCall tools when tasks require file access, web data, system info, or command execution. When calling a tool, skip narration — just call it. Speak after you see the result. Don't use tools for questions you can answer from your own knowledge.");
            sections.push(tool_lines);
        } else {
            let tool_list = snapshot.tools.join(", ");
            sections.push(format!(
                "**Tools Available:** {}\nCall tools when tasks require file access, web data, system info, or command execution. When calling a tool, skip narration — just call it. Speak after you see the result. Don't use tools for questions you can answer from your own knowledge.",
                tool_list
            ));
        }

        // Plan execution + tool chaining guidance — critical for reliable multi-step tasks
        if snapshot.tools.iter().any(|t| t == "plan_execute") {
            sections.push(
                "**Tool chaining rules:**\n\
                 - **1 tool:** Call directly.\n\
                 - **2+ tools in sequence:** Use plan_execute — declare all steps upfront with variable passing ($var). \
                   The system executes them automatically, handles errors per-step, and aggregates results.\n\
                 - **Research before messaging:** NEVER call telegram_send/discord_send in the same turn as research tools \
                   (web_search, read_file, etc.). Do research first, see the results, THEN compose and send. \
                   The system will defer messaging tools if you mix them with research in one turn.\n\
                 - **Integration pattern:** When responding to a Telegram/Discord message that requires action: \
                   plan the steps (research → process → respond) rather than trying to do everything in one tool call.".to_string()
            );
        }
    }

    // Memory — search mode awareness (stable unless API key added/removed)
    if snapshot.memory_enabled {
        if snapshot.memory_count > 0 {
            let mode_note = match snapshot.memory_search_mode.as_deref() {
                Some("hybrid") => " Search uses both keyword matching and semantic similarity.",
                Some("keyword") => " Search uses keyword matching only (no embedding API key configured).",
                _ => "",
            };
            sections.push(format!(
                "**Memory:** Active ({} memories indexed).{} You have full agency over your memory: use memory_search to query, memory_edit to correct, memory_delete to prune, and memory_save to store new information. Relevant memories are automatically injected but you can actively search for specific topics.",
                snapshot.memory_count, mode_note
            ));
        } else {
            sections.push("**Memory:** Active (empty — new installation). Use memory_save to store important information. Conversations will also be auto-extracted.".to_string());
        }
    }

    // Available models (for Phase 4 brain awareness)
    if !snapshot.available_models.is_empty() {
        let model_list = snapshot.available_models
            .iter()
            .take(10)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        let more = if snapshot.available_models.len() > 10 {
            format!(" (+{} more)", snapshot.available_models.len() - 10)
        } else {
            String::new()
        };
        sections.push(format!("**Other Models Available:** {}{}", model_list, more));
    }

    // Static hardware info (GPU name, total VRAM, CPU, total RAM)
    if let Some(ref gpu) = snapshot.gpu {
        let vram = snapshot.vram_gb.map(|v| format!(" ({:.0} GB VRAM)", v)).unwrap_or_default();
        sections.push(format!("**GPU:** {}{}", gpu, vram));
    }
    if let Some(ref cpu) = snapshot.cpu {
        let ram = snapshot.ram_gb.map(|v| format!(" · RAM: {:.0} GB", v)).unwrap_or_default();
        sections.push(format!("**System:** CPU: {}{}", cpu, ram));
    } else if let Some(total) = snapshot.ram_gb {
        sections.push(format!("**System:** RAM: {:.0} GB", total));
    }

    // Environment (backend + OS)
    let mut env_parts = Vec::new();
    if let Some(ref platform) = snapshot.os_platform {
        env_parts.push(format!("OS: {}", platform));
    }
    if let Some(ref be) = snapshot.backend {
        env_parts.push(format!("Backend: {}", be));
    }
    if !env_parts.is_empty() {
        sections.push(format!("**Environment:** {}", env_parts.join(" · ")));
    }

    if sections.is_empty() {
        return String::new();
    }

    format!("## Current Capabilities\n\n{}", sections.join("\n\n"))
}

/// Build a skills section for the harness based on conversation context.
/// Returns empty string if no relevant skills found.
/// Called from the frontend with the last user message as context.
fn build_skills_section(query: &str) -> String {
    if query.trim().is_empty() {
        return String::new();
    }

    let relevant = find_relevant_skills(query, 2, 2000);
    if relevant.is_empty() {
        return String::new();
    }

    let mut section = String::from("## Relevant Skills\n\n");
    for (name, content) in &relevant {
        section.push_str(&format!("### {}\n{}\n\n", name, content.trim()));
    }
    section
}

/// Build the VOLATILE context — changes every turn. Kept tiny (~30-50 tokens).
/// This is injected as a SEPARATE system message after the stable prompt,
/// so it doesn't bust the llama.cpp KV cache prefix.
fn build_volatile_context(snapshot: &CapabilitySnapshot) -> String {
    let mut parts = Vec::new();

    // Turn count + truncation status
    if snapshot.conversation_turns > 0 {
        let mut turn_info = format!("Turn {}", snapshot.conversation_turns);
        if snapshot.messages_truncated > 0 {
            turn_info.push_str(&format!(
                " — {} earlier messages were dropped to fit context",
                snapshot.messages_truncated
            ));
        }
        parts.push(turn_info);
    }

    // Live VRAM (the key routing number — changes as GPU load fluctuates)
    let has_live_vram = snapshot.vram_used_mb.is_some() && snapshot.vram_free_mb.is_some();
    if has_live_vram {
        let used = snapshot.vram_used_mb.unwrap() as f64 / 1024.0;
        let free = snapshot.vram_free_mb.unwrap() as f64 / 1024.0;
        let total = snapshot.vram_gb.unwrap_or((used + free) as f64);
        let util = snapshot.gpu_utilization
            .map(|u| format!(", {}% GPU util", u))
            .unwrap_or_default();

        let mut vram_str = format!("VRAM: {:.1}/{:.0} GB used ({:.1} GB free{})", used, total, free, util);

        if let Some(model_vram) = snapshot.active_model_vram_gb {
            vram_str.push_str(&format!(", model uses ~{:.1} GB", model_vram));
        }

        // Routing hint
        if free > 10.0 {
            vram_str.push_str(" — room for 13B+ alongside");
        } else if free > 5.0 {
            vram_str.push_str(" — room for 7-8B Q4 alongside");
        } else if free > 2.5 {
            vram_str.push_str(" — room for small 3B alongside");
        } else {
            vram_str.push_str(" — VRAM near full");
        }
        parts.push(vram_str);
    }

    // Context pressure tracking — model knows how full its context is
    if let (Some(tokens_used), Some(ctx_len)) = (snapshot.tokens_used, snapshot.context_length) {
        if ctx_len > 0 {
            let pct = (tokens_used as f64 / ctx_len as f64 * 100.0) as u32;
            let ctx_k = ctx_len as f64 / 1000.0;
            let used_k = tokens_used as f64 / 1000.0;
            let mut ctx_str = format!("Context: {:.1}K/{:.0}K tokens ({}%)", used_k, ctx_k, pct);
            if pct >= 80 {
                ctx_str.push_str(" — CRITICAL: context nearly full, summarize key points to working memory NOW");
            } else if pct >= 70 {
                ctx_str.push_str(" — HIGH: consider summarizing important context to working memory");
            } else if pct >= 50 {
                ctx_str.push_str(" — moderate");
            }
            parts.push(ctx_str);
        }
    }

    // Working memory status
    if let Some(true) = snapshot.has_working_memory {
        parts.push("Working memory: active (session scratchpad has content)".to_string());
    }

    // Live RAM
    if let Some(avail_mb) = snapshot.ram_available_mb {
        let avail_gb = avail_mb as f64 / 1024.0;
        parts.push(format!("RAM free: {:.0} GB", avail_gb));
    }

    if parts.is_empty() {
        return String::new();
    }

    format!("[Live Status] {}", parts.join(" | "))
}

// ============================================
// Assembler — The Core Function
// ============================================

/// Assemble the system prompt (stable) + volatile context (per-turn).
///
/// The system_prompt is CACHEABLE: identity + stable capabilities + user instructions.
/// The volatile_context is a tiny (~30-50 token) string for live metrics.
///
/// Frontend caches system_prompt and only rebuilds when stable inputs change.
/// volatile_context is always rebuilt (it's cheap — no KV cache impact).
fn assemble_prompt(
    capabilities: &CapabilitySnapshot,
    user_system_prompt: Option<&str>,
) -> HarnessContext {
    let identity = read_identity();
    let identity_source = if get_identity_path().exists() {
        "HIVE.md (user-editable)".to_string()
    } else {
        "default (built-in)".to_string()
    };

    let stable_manifest = build_stable_manifest(capabilities);
    let volatile_context = build_volatile_context(capabilities);
    let tool_count = capabilities.tools.len();
    let memory_status = if capabilities.memory_enabled {
        format!("active ({} memories)", capabilities.memory_count)
    } else {
        "disabled".to_string()
    };

    // Assemble STABLE system prompt: identity + stable capabilities + user instructions
    let mut prompt = String::with_capacity(identity.len() + stable_manifest.len() + 512);
    prompt.push_str(&identity);

    if !stable_manifest.is_empty() {
        prompt.push_str("\n\n");
        prompt.push_str(&stable_manifest);
    }

    if let Some(user_prompt) = user_system_prompt {
        let trimmed = user_prompt.trim();
        if !trimmed.is_empty() {
            prompt.push_str("\n\n## Additional Instructions\n\n");
            prompt.push_str(trimmed);
        }
    }

    HarnessContext {
        system_prompt: prompt,
        volatile_context,
        identity_source,
        tool_count,
        memory_status,
    }
}

// ============================================
// Tauri Commands
// ============================================

/// Build the harness system prompt. Called once per chat turn.
/// The frontend passes a capability snapshot (what's actually running right now).
#[tauri::command]
pub fn harness_build(
    capabilities: CapabilitySnapshot,
    user_system_prompt: Option<String>,
) -> HarnessContext {
    assemble_prompt(&capabilities, user_system_prompt.as_deref())
}

/// Get the current identity content (for display/editing in Settings).
#[tauri::command]
pub fn harness_get_identity() -> Result<String, String> {
    ensure_identity().map_err(|e| format!("Failed to ensure identity directory: {}", e))?;
    std::fs::read_to_string(get_identity_path())
        .map_err(|e| format!("Failed to read identity: {}", e))
}

/// Save updated identity content (user editing in Settings).
#[tauri::command]
pub fn harness_save_identity(content: String) -> Result<String, String> {
    ensure_identity().map_err(|e| format!("Failed to ensure identity directory: {}", e))?;
    std::fs::write(get_identity_path(), &content)
        .map_err(|e| format!("Failed to save identity: {}", e))?;
    crate::tools::log_tools::append_to_app_log(&format!(
        "HARNESS | identity_saved | {} bytes", content.len()
    ));
    Ok("Identity saved".to_string())
}

/// Reset identity to factory default.
#[tauri::command]
pub fn harness_reset_identity() -> Result<String, String> {
    ensure_identity().map_err(|e| format!("Failed to ensure identity directory: {}", e))?;
    std::fs::write(get_identity_path(), DEFAULT_IDENTITY)
        .map_err(|e| format!("Failed to reset identity: {}", e))?;
    crate::tools::log_tools::append_to_app_log("HARNESS | identity_reset | restored to factory default");
    Ok("Identity reset to default".to_string())
}

/// Get the path to the identity file (so user can open it externally).
#[tauri::command]
pub fn harness_get_identity_path() -> Result<String, String> {
    ensure_identity().map_err(|e| format!("Failed to ensure identity directory: {}", e))?;
    Ok(get_identity_path().to_string_lossy().to_string())
}

// ============================================
// Skills System (Phase 4.5.5)
// ============================================
// Skills are markdown files in ~/.hive/skills/ that teach the model
// domain-specific patterns. They're injected into the harness when
// relevant to the conversation context.
//
// Architecture:
//   ~/.hive/skills/*.md — user-created or seed skill files
//   load_skills()       — scan directory, return skill metadata
//   inject_skills()     — match skills to conversation context
//
// P1: Self-contained .md files, no code required
// P2: Any model benefits from skills (provider-agnostic)
// P7: Skills survive model swaps
// P8: Drop a .md file = teach HIVE something new (low floor)

fn get_skills_dir() -> PathBuf {
    get_app_data_dir().join("skills")
}

/// Skill file metadata for listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInfo {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
}

/// Ensure skills directory exists and seed built-in skills if empty.
fn ensure_skills_dir() -> Result<(), String> {
    let dir = get_skills_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create skills dir: {}", e))?;

    // Seed built-in skills only if directory is empty
    let entries: Vec<_> = std::fs::read_dir(&dir)
        .map_err(|e| format!("Failed to read skills dir: {}", e))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|ext| ext == "md").unwrap_or(false))
        .collect();

    if entries.is_empty() {
        seed_builtin_skills(&dir)?;
    }

    Ok(())
}

/// Seed built-in skill files (Phase 4.5.5 B3)
fn seed_builtin_skills(dir: &std::path::Path) -> Result<(), String> {
    let skills = vec![
        ("research.md", "\
# Research Skill

When asked to find information or answer factual questions:

1. **Search first:** Use `web_search` with clear, specific queries
2. **Fetch details:** Use `web_fetch` on the most promising results
3. **Extract data:** Use `web_extract` for structured content (tables, links, JSON-LD)
4. **Cross-reference:** Search multiple angles before concluding
5. **Cite sources:** Always mention where information came from

Pattern: web_search → web_fetch (top 2-3 results) → synthesize → respond
"),
        ("coding.md", "\
# Coding Skill

When asked to write, debug, or modify code:

1. **Read first:** Use `read_file` to understand existing code before changing it
2. **Understand context:** Use `list_directory` or `code_search` to find related files
3. **Mimic conventions:** Match the existing code style, imports, patterns, and naming
4. **Write carefully:** Use `write_file` with the complete updated file content
5. **Verify:** Use `run_command` to test (build, lint, run tests)
6. **Iterate:** If tests fail, the bug is in your code — never modify tests to make them pass

## Debugging

- **Never assume a library is available.** Check the project's dependency file first (package.json, Cargo.toml, etc.)
- **Fix the pattern, not the instance.** Found a bug? Search for the same mistake elsewhere — it usually exists in 3-5 other places
- **Trace backwards from errors.** The error message might be downstream of the real bug. Don't fix the symptom
- **Check history before rewriting.** A past version might have worked. Sometimes the fix is a revert, not new code

Pattern: read_file → understand → write_file → run_command (test) → fix if needed
"),
        ("memory.md", "\
# Memory Skill

You have persistent memory across conversations. Use it actively:

- **Save important info:** Use `memory_save` for user preferences, decisions, project details
- **Search before asking:** Use `memory_search` to check if you already know something
- **Correct mistakes:** Use `memory_edit` to fix outdated or wrong memories
- **Clean up:** Use `memory_delete` to remove irrelevant memories
- **Explore connections:** Use `graph_query` to discover related knowledge
- **Track entities:** Use `entity_track` to maintain awareness of important things

Don't save trivial chat — save decisions, preferences, technical details, and lessons learned.
"),
        ("github.md", "\
# GitHub Skill

When working with GitHub repositories:

- **Issues:** Use `github_issues` with action 'list' to see open issues, 'get' for details, 'create' to file new ones
- **Pull Requests:** Use `github_prs` with action 'list'/'get'/'create' for PR management
- **Repos:** Use `github_repos` to list repos, get info, or search code

Always specify the owner/repo (e.g., 'octocat/hello-world'). For the user's repos, check memory first for common repo names.
"),
        ("troubleshooting.md", "\
# Troubleshooting Skill

When something is broken, failing, stuck, or producing wrong results:

## Before fixing anything
- **Read the actual error.** Don't guess from the error message — read the full output
- **Trace the chain.** If A calls B calls C, and C errors, the bug might be in A's input to B
- **Check what changed.** If it worked before, find what's different now

## While fixing
- **One change at a time.** Make a single change, test, confirm. Don't stack untested fixes
- **Three strikes rule.** If three fixes for the same issue all fail, the problem isn't where you think. Step back and reconsider the architecture or your assumptions
- **Search for the pattern.** The same mistake that caused this bug probably exists in other places. Fix all instances

## After fixing
- **Verify with evidence.** Run the actual test/build/command. 'Should work' is not verification
- **Check for regressions.** Did your fix break something else? Test the surrounding functionality
- **Save the lesson.** If this was a non-obvious bug, use `memory_save` so you don't repeat it
"),
    ];

    for (filename, content) in skills {
        let path = dir.join(filename);
        std::fs::write(&path, content)
            .map_err(|e| format!("Failed to write skill {}: {}", filename, e))?;
    }

    crate::tools::log_tools::append_to_app_log(&format!(
        "HARNESS | skills_seeded | count={} | dir={}", 5, dir.display()
    ));

    Ok(())
}

/// Load all skill files from the skills directory.
fn load_skills() -> Vec<(String, String)> {
    let dir = get_skills_dir();
    if let Err(e) = ensure_skills_dir() {
        eprintln!("[HIVE] WARN: ensure_skills_dir failed in load_skills: {}", e);
    }

    let mut skills = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().map(|ext| ext == "md").unwrap_or(false) {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    let name = path.file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default();
                    skills.push((name, content));
                }
            }
        }
    }
    skills
}

/// Phase 4: Synthetic queries per built-in skill (Tool2Vec pattern).
/// "What would a user say that needs this skill?" — embed these queries,
/// average the embeddings → single skill vector. 25-30% recall improvement
/// over embedding skill descriptions directly (ToolBank benchmark).
fn builtin_skill_queries() -> std::collections::HashMap<&'static str, Vec<&'static str>> {
    let mut m = std::collections::HashMap::new();
    m.insert("research", vec![
        "search for information about this topic",
        "find documentation on this library",
        "look up how to use this API",
        "what does the research say about",
        "find articles and sources about",
        "search the web for examples of",
        "what are the latest findings on",
        "investigate this topic for me",
    ]);
    m.insert("coding", vec![
        "write a function that does",
        "fix this bug in my code",
        "refactor this code to be cleaner",
        "implement a new feature for",
        "debug this error message",
        "help me write unit tests for",
        "modify this code to add",
        "create a new file with this code",
    ]);
    m.insert("memory", vec![
        "remember this for later",
        "what did I tell you about",
        "save this preference of mine",
        "recall our previous discussion about",
        "update what you know about me",
        "search your memories for",
        "forget this information I told you",
        "track this project detail",
    ]);
    m.insert("github", vec![
        "create a pull request for this",
        "check the open issues on this repo",
        "review this pull request",
        "list my GitHub repositories",
        "file a bug report for this issue",
        "what is the status of CI checks",
        "close this GitHub issue",
        "merge this branch into main",
    ]);
    m.insert("troubleshooting", vec![
        "why is this broken",
        "something is not working correctly",
        "fix this error for me",
        "debug this issue I am having",
        "troubleshoot the problem with",
        "this keeps failing with an error",
        "diagnose what went wrong here",
        "help me figure out why this crashes",
    ]);
    m
}

/// Phase 4: Cached skill vectors. OnceLock ensures one-time computation.
/// Maps skill name → averaged Tool2Vec embedding (384 dims from fastembed).
static SKILL_VECTORS: OnceLock<std::collections::HashMap<String, Vec<f64>>> = OnceLock::new();

/// Compute or retrieve cached skill vectors using Tool2Vec pattern.
/// Built-in skills: average synthetic query embeddings (8 queries each).
/// Custom skills: embed skill name + first paragraph as fallback.
fn get_skill_vectors(skills: &[(String, String)]) -> &'static std::collections::HashMap<String, Vec<f64>> {
    SKILL_VECTORS.get_or_init(|| {
        let queries_map = builtin_skill_queries();
        let mut vectors = std::collections::HashMap::new();

        for (skill_name, content) in skills {
            // Built-in skills: use Tool2Vec synthetic queries
            if let Some(queries) = queries_map.get(skill_name.as_str()) {
                let embeddings: Vec<Vec<f64>> = queries.iter()
                    .filter_map(|q| crate::memory::get_local_embedding(q).ok())
                    .collect();

                if let Some(avg) = average_embeddings(&embeddings) {
                    vectors.insert(skill_name.clone(), avg);
                    continue;
                }
            }

            // Custom skills: embed name + first 200 chars of content
            let summary = format!("{}: {}", skill_name,
                content.chars().take(200).collect::<String>());
            if let Ok(emb) = crate::memory::get_local_embedding(&summary) {
                vectors.insert(skill_name.clone(), emb);
            }
        }

        vectors
    })
}

// average_embeddings is in memory.rs as pub(crate) — shared with Phase 6 topic centroids
use crate::memory::average_embeddings;

/// Phase 4: Keyword-based skill matching (fallback when embeddings unavailable).
fn find_skills_by_keywords(
    skills: &[(String, String)], query: &str, max: usize, max_chars: usize,
) -> Vec<(String, String)> {
    let query_lower = query.to_lowercase();
    let query_words: Vec<&str> = query_lower.split_whitespace().collect();

    let mut scored: Vec<(usize, &String, &String)> = skills.iter()
        .map(|(name, content)| {
            let name_lower = name.to_lowercase();
            let content_lower = content.to_lowercase();
            let mut score = 0usize;
            for word in &query_words {
                if word.len() < 3 { continue; }
                if name_lower.contains(word) { score += 3; }
                if content_lower.contains(word) { score += 1; }
            }
            (score, name, content)
        })
        .filter(|(score, _, _)| *score > 0)
        .collect();

    scored.sort_by(|a, b| b.0.cmp(&a.0));

    scored.into_iter()
        .take(max)
        .map(|(_, name, content)| {
            let truncated: String = content.chars().take(max_chars).collect();
            (name.clone(), truncated)
        })
        .collect()
}

/// Semantic skill matching using Tool2Vec embeddings (Phase 4).
/// Embeds the user query, compares against precomputed skill vectors via cosine similarity.
/// Falls back to keyword matching if embedding unavailable.
/// Returns up to `max` matching skill contents, each capped at `max_chars`.
pub fn find_relevant_skills(query: &str, max: usize, max_chars: usize) -> Vec<(String, String)> {
    let skills = load_skills();

    // Phase 4: Try semantic matching first
    if let Ok(query_embedding) = crate::memory::get_local_embedding(query) {
        let skill_vectors = get_skill_vectors(&skills);

        if !skill_vectors.is_empty() {
            let mut scored: Vec<(f64, &String, &String)> = skills.iter()
                .filter_map(|(name, content)| {
                    skill_vectors.get(name)
                        .map(|sv| (crate::memory::cosine_similarity(&query_embedding, sv), name, content))
                })
                .filter(|(score, _, _)| *score > 0.3) // Minimum relevance threshold
                .collect();

            scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

            let results: Vec<(String, String)> = scored.into_iter()
                .take(max)
                .map(|(_, name, content)| {
                    let truncated: String = content.chars().take(max_chars).collect();
                    (name.clone(), truncated)
                })
                .collect();

            if !results.is_empty() {
                return results;
            }
            // Semantic search returned nothing above threshold — fall through to keywords
        }
    }

    // Fallback: keyword matching (existing behavior, works without embeddings)
    find_skills_by_keywords(&skills, query, max, max_chars)
}

/// List all skill files (for Settings UI).
#[tauri::command]
pub fn harness_list_skills() -> Result<Vec<SkillInfo>, String> {
    if let Err(e) = ensure_skills_dir() {
        eprintln!("[HIVE] WARN: ensure_skills_dir failed in harness_list_skills: {}", e);
    }
    let dir = get_skills_dir();

    let mut skills = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().map(|ext| ext == "md").unwrap_or(false) {
                let name = path.file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                let size_bytes = std::fs::metadata(&path)
                    .map(|m| m.len())
                    .unwrap_or(0);
                skills.push(SkillInfo {
                    name,
                    path: path.to_string_lossy().to_string(),
                    size_bytes,
                });
            }
        }
    }
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(skills)
}

/// Read a specific skill file content.
#[tauri::command]
pub fn harness_read_skill(name: String) -> Result<String, String> {
    // P6: Reject path traversal attempts (e.g. "../../secrets")
    if name.contains("..") || name.contains('/') || name.contains('\\') || name.contains('\0') {
        return Err(format!("Invalid skill name '{}': must not contain path separators or '..'", name));
    }
    let skills_dir = get_skills_dir();
    if let Err(e) = ensure_skills_dir() {
        eprintln!("[HIVE] WARN: ensure_skills_dir failed in harness_read_skill: {}", e);
    }
    let path = skills_dir.join(format!("{}.md", name));
    // Canonicalize both paths and verify containment
    let canonical_dir = std::fs::canonicalize(&skills_dir)
        .map_err(|e| format!("Cannot resolve skills directory: {}", e))?;
    let canonical_path = std::fs::canonicalize(&path)
        .map_err(|e| format!("Skill '{}' not found: {}", name, e))?;
    if !canonical_path.starts_with(&canonical_dir) {
        return Err(format!("Skill '{}' resolves outside skills directory — blocked (P6)", name));
    }
    std::fs::read_to_string(&canonical_path)
        .map_err(|e| format!("Failed to read skill '{}': {}", name, e))
}

/// Get the skills directory path (for "Open Folder" button).
#[tauri::command]
pub fn harness_get_skills_path() -> Result<String, String> {
    if let Err(e) = ensure_skills_dir() {
        eprintln!("[HIVE] WARN: ensure_skills_dir failed in harness_get_skills_path: {}", e);
    }
    Ok(get_skills_dir().to_string_lossy().to_string())
}

/// Open the skills directory in the system file explorer.
#[tauri::command]
pub fn harness_open_skills_dir() -> Result<(), String> {
    let dir = get_skills_dir();
    if let Err(e) = ensure_skills_dir() {
        eprintln!("[HIVE] WARN: ensure_skills_dir failed in harness_open_skills_dir: {}", e);
    }

    #[cfg(windows)]
    {
        std::process::Command::new("explorer")
            .arg(&dir)
            .spawn()
            .map_err(|e| format!("Failed to open explorer: {}", e))?;
    }

    #[cfg(not(windows))]
    {
        // xdg-open on Linux, open on macOS
        let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
        std::process::Command::new(opener)
            .arg(&dir)
            .spawn()
            .map_err(|e| format!("Failed to open directory: {}", e))?;
    }

    Ok(())
}

/// Get relevant skills for the current user message (for context injection).
/// Returns a formatted string to inject as a separate system message.
/// Called per-turn from the frontend, SEPARATE from the cached system prompt.
#[tauri::command]
pub fn harness_get_relevant_skills(query: String) -> Result<String, String> {
    let section = build_skills_section(&query);
    Ok(section)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_snapshot() -> CapabilitySnapshot {
        CapabilitySnapshot {
            tools: vec![],
            active_model: None,
            provider: "local".to_string(),
            available_models: vec![],
            memory_enabled: false,
            memory_count: 0,
            gpu: None,
            vram_gb: None,
            ram_gb: None,
            context_length: None,
            quantization: None,
            model_parameters: None,
            architecture: None,
            backend: None,
            cpu: None,
            tool_risks: vec![],
            memory_search_mode: None,
            conversation_turns: 0,
            messages_truncated: 0,
            os_platform: None,
            vram_used_mb: None,
            vram_free_mb: None,
            ram_available_mb: None,
            gpu_utilization: None,
            active_model_vram_gb: None,
            tokens_used: None,
            has_working_memory: None,
            last_user_message: None,
        }
    }

    #[test]
    fn stable_manifest_empty_snapshot() {
        let snap = minimal_snapshot();
        let manifest = build_stable_manifest(&snap);
        assert!(manifest.is_empty(), "Empty snapshot should produce empty manifest");
    }

    #[test]
    fn stable_manifest_includes_active_model() {
        let mut snap = minimal_snapshot();
        snap.active_model = Some("llama-3.1-8b-q4_k_m.gguf".to_string());
        snap.provider = "local".to_string();
        let manifest = build_stable_manifest(&snap);
        assert!(manifest.contains("llama-3.1-8b-q4_k_m.gguf"), "Must include model name");
        assert!(manifest.contains("local"), "Must include provider");
    }

    #[test]
    fn stable_manifest_includes_model_metadata() {
        let mut snap = minimal_snapshot();
        snap.active_model = Some("model.gguf".to_string());
        snap.model_parameters = Some("8B".to_string());
        snap.quantization = Some("Q4_K_M".to_string());
        snap.architecture = Some("llama".to_string());
        let manifest = build_stable_manifest(&snap);
        assert!(manifest.contains("8B parameters"));
        assert!(manifest.contains("Q4_K_M quantization"));
        assert!(manifest.contains("llama architecture"));
    }

    #[test]
    fn stable_manifest_includes_tools() {
        let mut snap = minimal_snapshot();
        snap.tools = vec!["read_file".to_string(), "web_search".to_string()];
        let manifest = build_stable_manifest(&snap);
        assert!(manifest.contains("read_file"));
        assert!(manifest.contains("web_search"));
    }

    #[test]
    fn stable_manifest_tool_chaining_only_with_plan_execute() {
        let mut snap = minimal_snapshot();
        snap.tools = vec!["read_file".to_string()];
        let manifest = build_stable_manifest(&snap);
        assert!(!manifest.contains("Tool chaining"), "No chaining guidance without plan_execute");

        snap.tools.push("plan_execute".to_string());
        let manifest = build_stable_manifest(&snap);
        assert!(manifest.contains("Tool chaining"), "Chaining guidance must appear with plan_execute");
    }

    #[test]
    fn stable_manifest_memory_status() {
        let mut snap = minimal_snapshot();
        snap.memory_enabled = true;
        snap.memory_count = 42;
        let manifest = build_stable_manifest(&snap);
        assert!(manifest.contains("42 memories"));
        assert!(manifest.contains("memory_search"));
    }

    #[test]
    fn stable_manifest_cloud_provider_tool_format() {
        let mut snap = minimal_snapshot();
        snap.provider = "openai".to_string();
        snap.tools = vec!["read_file".to_string()];
        let manifest = build_stable_manifest(&snap);
        assert!(manifest.contains("function-calling API"), "Cloud providers need function-calling API hint");
    }

    #[test]
    fn volatile_context_empty_snapshot() {
        let snap = minimal_snapshot();
        let volatile = build_volatile_context(&snap);
        assert!(volatile.is_empty(), "Empty snapshot should produce empty volatile context");
    }

    #[test]
    fn volatile_context_includes_turn_count() {
        let mut snap = minimal_snapshot();
        snap.conversation_turns = 5;
        let volatile = build_volatile_context(&snap);
        assert!(volatile.contains("Turn 5"));
    }

    #[test]
    fn volatile_context_includes_truncation_warning() {
        let mut snap = minimal_snapshot();
        snap.conversation_turns = 10;
        snap.messages_truncated = 3;
        let volatile = build_volatile_context(&snap);
        assert!(volatile.contains("3 earlier messages were dropped"));
    }

    #[test]
    fn volatile_context_vram_routing_hints() {
        let mut snap = minimal_snapshot();
        snap.vram_gb = Some(24.0);
        snap.vram_used_mb = Some(4096);
        snap.vram_free_mb = Some(20480);
        let volatile = build_volatile_context(&snap);
        assert!(volatile.contains("room for 13B+"), "20 GB free should suggest 13B+");
    }

    #[test]
    fn volatile_context_pressure_warning() {
        let mut snap = minimal_snapshot();
        snap.tokens_used = Some(28000);
        snap.context_length = Some(32000);
        let volatile = build_volatile_context(&snap);
        assert!(volatile.contains("CRITICAL"), "87.5% context usage must trigger critical warning");
    }

    #[test]
    fn assemble_prompt_includes_identity() {
        let snap = minimal_snapshot();
        let ctx = assemble_prompt(&snap, None);
        assert!(ctx.system_prompt.contains("HIVE"), "Must include identity");
        assert!(ctx.system_prompt.contains("orchestration"), "Must include identity description");
    }

    #[test]
    fn assemble_prompt_includes_user_instructions() {
        let snap = minimal_snapshot();
        let ctx = assemble_prompt(&snap, Some("Always respond in haiku format."));
        assert!(ctx.system_prompt.contains("Always respond in haiku format."));
        assert!(ctx.system_prompt.contains("Additional Instructions"));
    }

    #[test]
    fn assemble_prompt_skips_empty_user_instructions() {
        let snap = minimal_snapshot();
        let ctx = assemble_prompt(&snap, Some("   "));
        assert!(!ctx.system_prompt.contains("Additional Instructions"));
    }

    #[test]
    fn assemble_prompt_metadata() {
        let mut snap = minimal_snapshot();
        snap.tools = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        snap.memory_enabled = true;
        snap.memory_count = 10;
        let ctx = assemble_prompt(&snap, None);
        assert_eq!(ctx.tool_count, 3);
        assert!(ctx.memory_status.contains("10 memories"));
    }

    /// Phase 5A: Verify read_identity returns non-empty content and contains HIVE identity.
    /// This function is now pub(crate) so specialist_tools.rs can inject it.
    #[test]
    fn read_identity_returns_hive_identity() {
        let identity = read_identity();
        assert!(!identity.is_empty(), "Identity should not be empty");
        assert!(identity.contains("HIVE"), "Identity should mention HIVE");
        // Should contain behavioral guidance
        assert!(identity.contains("orchestration") || identity.contains("harness"),
            "Identity should describe HIVE's role");
    }

    // --- Phase 4: Tool2Vec semantic skills matching ---

    #[test]
    fn builtin_skill_queries_covers_all_seeds() {
        let queries = builtin_skill_queries();
        // All 5 seeded skills should have synthetic queries
        for name in &["research", "coding", "memory", "github", "troubleshooting"] {
            assert!(queries.contains_key(name), "Missing queries for skill: {}", name);
            assert!(queries[name].len() >= 5, "Skill {} should have at least 5 queries", name);
        }
    }

    #[test]
    fn average_embeddings_produces_centroid() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let avg = average_embeddings(&[a, b]).unwrap();
        assert_eq!(avg.len(), 3);
        assert!((avg[0] - 0.5).abs() < 1e-10);
        assert!((avg[1] - 0.5).abs() < 1e-10);
        assert!((avg[2] - 0.0).abs() < 1e-10);
    }

    #[test]
    fn average_embeddings_empty_returns_none() {
        assert!(average_embeddings(&[]).is_none());
        assert!(average_embeddings(&[vec![]]).is_none());
    }

    #[test]
    fn keyword_fallback_matches_by_word_overlap() {
        let skills = vec![
            ("coding".to_string(), "write code and debug errors".to_string()),
            ("research".to_string(), "search for information online".to_string()),
        ];
        let results = find_skills_by_keywords(&skills, "write some code", 2, 1000);
        assert!(!results.is_empty(), "Should match 'coding' skill by keyword");
        assert_eq!(results[0].0, "coding");
    }

    #[test]
    fn semantic_skill_matching_prefers_relevant_skill() {
        // This test requires fastembed — skip gracefully if unavailable
        let query = "help me write a unit test for this function";
        let skills = load_skills();
        if crate::memory::get_local_embedding(query).is_err() {
            eprintln!("Skipping semantic matching test (fastembed not available)");
            return;
        }

        let results = find_relevant_skills(query, 2, 2000);
        assert!(!results.is_empty(), "Should find at least one relevant skill");
        // "write a unit test" should match coding or troubleshooting, not github
        let top_skill = &results[0].0;
        assert!(
            top_skill == "coding" || top_skill == "troubleshooting",
            "Top skill for 'write a unit test' should be coding or troubleshooting, got: {}",
            top_skill
        );
    }
}

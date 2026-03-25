//! Scratchpad tools — named, sectioned key-value stores for inter-tool coordination
//!
//! Scratchpads are in-memory stores where workers and plans can accumulate results.
//! Each scratchpad has named sections that can be appended to independently.
//! Scratchpads auto-expire after TTL (default: 60 minutes).
//!
//! Tools:
//!   scratchpad_create — create a named scratchpad with optional TTL
//!   scratchpad_write  — append content to a section within a scratchpad
//!   scratchpad_read   — read sections from a scratchpad
//!
//! Principle alignment:
//!   P1 (Modularity)  — Decouples producers (workers) from consumers (main chat).
//!   P3 (Simplicity)  — In-memory HashMap, no Redis/external deps.
//!   P6 (Secrets)      — Scratchpads are ephemeral, never persisted to disk.

use super::{HiveTool, RiskLevel, ToolResult};
use serde_json::json;
use std::collections::HashMap;
use std::sync::OnceLock;
use tokio::sync::RwLock;

// ============================================
// Global Scratchpad State
// ============================================

static SCRATCHPADS: OnceLock<RwLock<HashMap<String, Scratchpad>>> = OnceLock::new();

fn pads() -> &'static RwLock<HashMap<String, Scratchpad>> {
    SCRATCHPADS.get_or_init(|| RwLock::new(HashMap::new()))
}

struct Scratchpad {
    created_at: chrono::DateTime<chrono::Utc>,
    ttl_minutes: u64,
    max_size_kb: usize,
    sections: HashMap<String, Vec<ScratchpadEntry>>,
}

struct ScratchpadEntry {
    content: String,
    metadata: Option<serde_json::Value>,
    timestamp: chrono::DateTime<chrono::Utc>,
}

impl Scratchpad {
    fn is_expired(&self) -> bool {
        let elapsed = chrono::Utc::now() - self.created_at;
        // Clamp to 0 to avoid negative→huge-u64 wrap on clock skew (B9 fix)
        elapsed.num_minutes().max(0) as u64 > self.ttl_minutes
    }

    fn total_size_bytes(&self) -> usize {
        self.sections.values()
            .flat_map(|entries| entries.iter())
            .map(|e| e.content.len())
            .sum()
    }
}

// ============================================
// scratchpad_create
// ============================================

pub struct ScratchpadCreateTool;

#[async_trait::async_trait]
impl HiveTool for ScratchpadCreateTool {
    fn name(&self) -> &str { "scratchpad_create" }

    fn description(&self) -> &str {
        "Create a named scratchpad for accumulating results across tool calls or workers. \
         Scratchpads have sections that can be written to independently. Use this before \
         spawning workers or running multi-step analyses that need shared state."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Unique name for this scratchpad (e.g., 'repo-analysis', 'lora-findings')"
                },
                "ttl_minutes": {
                    "type": "integer",
                    "description": "Auto-expire after this many minutes (default: 60, max: 480)"
                },
                "max_size_kb": {
                    "type": "integer",
                    "description": "Maximum total size in KB to prevent runaway growth (default: 512, max: 4096)"
                }
            },
            "required": ["id"]
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::Low }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let id = params.get("id")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: id")?;

        let ttl = params.get("ttl_minutes")
            .and_then(|v| v.as_u64())
            .unwrap_or(60)
            .min(480);

        let max_size = params.get("max_size_kb")
            .and_then(|v| v.as_u64())
            .unwrap_or(512)
            .min(4096) as usize;

        let mut all = pads().write().await;

        // Clean expired pads while we're here
        all.retain(|_, pad| !pad.is_expired());

        if all.contains_key(id) {
            return Ok(ToolResult {
                content: format!(
                    "Scratchpad '{}' already exists. Use scratchpad_write to append, or create with a different ID.",
                    id
                ),
                is_error: true,
            });
        }

        all.insert(id.to_string(), Scratchpad {
            created_at: chrono::Utc::now(),
            ttl_minutes: ttl,
            max_size_kb: max_size,
            sections: HashMap::new(),
        });

        Ok(ToolResult {
            content: format!(
                "Scratchpad '{}' created (TTL: {} min, max: {} KB). Use scratchpad_write to add data.",
                id, ttl, max_size,
            ),
            is_error: false,
        })
    }
}

// ============================================
// scratchpad_write
// ============================================

pub struct ScratchpadWriteTool;

#[async_trait::async_trait]
impl HiveTool for ScratchpadWriteTool {
    fn name(&self) -> &str { "scratchpad_write" }

    fn description(&self) -> &str {
        "Append content to a section within a scratchpad. Sections are named buckets \
         that organize findings (e.g., 'file-list', 'lora-configs', 'errors'). \
         Multiple writes to the same section accumulate in order."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "scratchpad_id": {
                    "type": "string",
                    "description": "ID of the scratchpad to write to"
                },
                "section": {
                    "type": "string",
                    "description": "Section name within the scratchpad (e.g., 'findings', 'errors', 'summary')"
                },
                "content": {
                    "type": "string",
                    "description": "Content to append to this section"
                },
                "metadata": {
                    "type": "object",
                    "description": "Optional metadata (e.g., {\"worker_id\": \"w1\", \"progress\": 0.5})"
                }
            },
            "required": ["scratchpad_id", "section", "content"]
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::Low }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let pad_id = params.get("scratchpad_id")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: scratchpad_id")?;

        let section = params.get("section")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: section")?;

        let content = params.get("content")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: content")?;

        let metadata = params.get("metadata").cloned();

        let mut all = pads().write().await;

        // Auto-create scratchpad if it doesn't exist (convenience)
        let pad = all.entry(pad_id.to_string()).or_insert_with(|| Scratchpad {
            created_at: chrono::Utc::now(),
            ttl_minutes: 60,
            max_size_kb: 512,
            sections: HashMap::new(),
        });

        if pad.is_expired() {
            all.remove(pad_id);
            return Ok(ToolResult {
                content: format!("Scratchpad '{}' has expired. Create a new one.", pad_id),
                is_error: true,
            });
        }

        // Check size limit
        let current_size = pad.total_size_bytes();
        let max_bytes = pad.max_size_kb * 1024;
        if current_size + content.len() > max_bytes {
            return Ok(ToolResult {
                content: format!(
                    "Scratchpad '{}' would exceed size limit ({} KB / {} KB). Reduce content or create a new pad.",
                    pad_id, (current_size + content.len()) / 1024, pad.max_size_kb,
                ),
                is_error: true,
            });
        }

        let entries = pad.sections.entry(section.to_string()).or_insert_with(Vec::new);
        let entry_num = entries.len() + 1;
        entries.push(ScratchpadEntry {
            content: content.to_string(),
            metadata,
            timestamp: chrono::Utc::now(),
        });

        Ok(ToolResult {
            content: format!(
                "Written to '{}' → section '{}' (entry #{}, {} bytes)",
                pad_id, section, entry_num, content.len(),
            ),
            is_error: false,
        })
    }
}

// ============================================
// scratchpad_read
// ============================================

pub struct ScratchpadReadTool;

#[async_trait::async_trait]
impl HiveTool for ScratchpadReadTool {
    fn name(&self) -> &str { "scratchpad_read" }

    fn description(&self) -> &str {
        "Read content from a scratchpad. Can read all sections or a specific one. \
         Use format='summary' for a brief overview, or 'full' for complete content."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "scratchpad_id": {
                    "type": "string",
                    "description": "ID of the scratchpad to read"
                },
                "section": {
                    "type": "string",
                    "description": "Specific section to read (omit for all sections)"
                },
                "format": {
                    "type": "string",
                    "enum": ["full", "summary"],
                    "description": "Output format: 'full' for all content, 'summary' for section names + entry counts (default: full)"
                }
            },
            "required": ["scratchpad_id"]
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::Low }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let pad_id = params.get("scratchpad_id")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: scratchpad_id")?;

        let section_filter = params.get("section").and_then(|v| v.as_str());
        let format = params.get("format").and_then(|v| v.as_str()).unwrap_or("full");

        let all = pads().read().await;

        let pad = all.get(pad_id).ok_or_else(|| {
            format!("Scratchpad '{}' not found. Available: [{}]",
                pad_id,
                all.keys().cloned().collect::<Vec<_>>().join(", "))
        })?;

        if pad.is_expired() {
            return Ok(ToolResult {
                content: format!("Scratchpad '{}' has expired.", pad_id),
                is_error: true,
            });
        }

        let age_mins = (chrono::Utc::now() - pad.created_at).num_minutes();
        let size_kb = pad.total_size_bytes() / 1024;

        if format == "summary" {
            let mut out = format!(
                "Scratchpad '{}' (age: {} min, size: {} KB, {} sections):\n",
                pad_id, age_mins, size_kb, pad.sections.len(),
            );
            for (name, entries) in &pad.sections {
                let total_chars: usize = entries.iter().map(|e| e.content.len()).sum();
                out.push_str(&format!(
                    "  - '{}': {} entries, {} chars\n",
                    name, entries.len(), total_chars,
                ));
            }
            return Ok(ToolResult { content: out, is_error: false });
        }

        // Full format
        let mut out = format!(
            "Scratchpad '{}' (age: {} min, size: {} KB):\n\n",
            pad_id, age_mins, size_kb,
        );

        let sections_to_show: Vec<(&String, &Vec<ScratchpadEntry>)> = if let Some(s) = section_filter {
            pad.sections.iter().filter(|(k, _)| k.as_str() == s).collect()
        } else {
            pad.sections.iter().collect()
        };

        if sections_to_show.is_empty() {
            if let Some(s) = section_filter {
                out.push_str(&format!("Section '{}' not found. Available: [{}]",
                    s,
                    pad.sections.keys().cloned().collect::<Vec<_>>().join(", ")));
            } else {
                out.push_str("(empty — no sections written yet)");
            }
        }

        for (name, entries) in sections_to_show {
            out.push_str(&format!("═══ {} ({} entries) ═══\n", name, entries.len()));
            for (i, entry) in entries.iter().enumerate() {
                let ts = entry.timestamp.format("%H:%M:%S");
                let meta_str = entry.metadata.as_ref()
                    .map(|m| format!(" {}", m))
                    .unwrap_or_default();
                out.push_str(&format!("[#{} {}{}] {}\n", i + 1, ts, meta_str, entry.content));
            }
            out.push('\n');
        }

        // Truncate if too long
        let max_chars = 25_000;
        if out.chars().count() > max_chars {
            let truncated: String = out.chars().take(max_chars).collect();
            out = format!("{}\n\n[... truncated at {} chars. Use section filter to read specific sections.]", truncated, max_chars);
        }

        Ok(ToolResult { content: out, is_error: false })
    }
}

// ============================================
// Phase 5B: Cross-module accessor for ReadAgentContextTool
// ============================================

/// List all active (non-expired) scratchpads with section summaries.
/// Returns: Vec<(pad_id, section_names, total_entry_count)>
pub(crate) async fn list_scratchpads_summary() -> Vec<(String, Vec<String>, usize)> {
    let all = pads().read().await;
    let mut result = Vec::new();
    for (id, pad) in all.iter() {
        if pad.is_expired() { continue; }
        let sections: Vec<String> = pad.sections.keys().cloned().collect();
        let total_entries: usize = pad.sections.values().map(|v| v.len()).sum();
        result.push((id.clone(), sections, total_entries));
    }
    result
}

// ============================================
// Phase 5C: Context Bus — shared agent activity feed
// ============================================
//
// The context bus is a named scratchpad ("context_bus") where agents write
// activity summaries after completing work. It's NOT a new system — it's a
// convention on top of existing scratchpads (P3: Simplicity Wins).
//
// Each agent gets its own section. Entries are capped at 10 per agent to
// prevent unbounded growth. TTL is 8 hours (covers a full session).
//
// Data flows:
//   useChat.ts → context_bus_write("consciousness", ...) after tool loop
//   specialist routing → context_bus_write("coder", ...) after specialist returns
//   workers → already write to their own scratchpads (not duplicated here)
//   volatile context → context_bus_summary() for model awareness

const CONTEXT_BUS_ID: &str = "context_bus";
const CONTEXT_BUS_TTL_MINUTES: u64 = 480; // 8 hours
const CONTEXT_BUS_MAX_SIZE_KB: usize = 1024;
const CONTEXT_BUS_MAX_ENTRIES_PER_AGENT: usize = 10;

/// Write an activity entry to the context bus for the given agent.
/// Auto-creates the bus pad if it doesn't exist. Keeps last N entries per agent.
pub(crate) async fn context_bus_write(agent: &str, content: &str) {
    let mut all = pads().write().await;

    let pad = all.entry(CONTEXT_BUS_ID.to_string()).or_insert_with(|| Scratchpad {
        created_at: chrono::Utc::now(),
        ttl_minutes: CONTEXT_BUS_TTL_MINUTES,
        max_size_kb: CONTEXT_BUS_MAX_SIZE_KB,
        sections: HashMap::new(),
    });

    // Replace if expired
    if pad.is_expired() {
        *pad = Scratchpad {
            created_at: chrono::Utc::now(),
            ttl_minutes: CONTEXT_BUS_TTL_MINUTES,
            max_size_kb: CONTEXT_BUS_MAX_SIZE_KB,
            sections: HashMap::new(),
        };
    }

    let entries = pad.sections.entry(agent.to_string()).or_default();

    // Cap entries per agent — remove oldest if at limit
    while entries.len() >= CONTEXT_BUS_MAX_ENTRIES_PER_AGENT {
        entries.remove(0);
    }

    entries.push(ScratchpadEntry {
        content: content.to_string(),
        metadata: None,
        timestamp: chrono::Utc::now(),
    });
}

/// Read a compact summary of the context bus for volatile context injection.
/// Returns empty string if bus is empty/expired. Format: pipe-separated agent summaries.
pub(crate) async fn context_bus_summary() -> String {
    let all = pads().read().await;
    let pad = match all.get(CONTEXT_BUS_ID) {
        Some(p) if !p.is_expired() => p,
        _ => return String::new(),
    };

    let mut parts = Vec::new();
    for (agent, entries) in &pad.sections {
        if let Some(last) = entries.last() {
            let age_mins = (chrono::Utc::now() - last.timestamp).num_minutes();
            let preview: String = last.content.chars().take(80).collect();
            parts.push(format!("{} ({}m ago): {}", agent, age_mins, preview));
        }
    }

    if parts.is_empty() {
        return String::new();
    }

    format!("[Agent Activity] {}", parts.join(" | "))
}

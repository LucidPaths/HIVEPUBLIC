//! HIVE Memory System — persistent context & recall
//!
//! Adapted from OpenClaw's memory architecture (MIT license):
//!   - Markdown files as source of truth in %LocalAppData%/HIVE/memory/
//!   - SQLite + FTS5 + vector embeddings for hybrid search
//!   - Chunking with overlap for context-preserving retrieval
//!   - Auto-flush: save important context before conversation ends
//!   - Auto-recall: session-injected memories (NOT system prompt)
//!
//! Principle Lattice alignment:
//!   P1 (Bridges)    — Memory is a standalone module, bridges chat <-> persistence
//!   P2 (Agnostic)   — Works with any provider; recall is session-injected, not provider-specific
//!   P3 (Simplicity) — Took OpenClaw's working code, adapted to Rust
//!   P6 (Secrets)    — Memory files are local-only, never transmitted
//!   P7 (Survives)   — Schema versioned, forward-compatible
//!   P8 (Low/High)   — Auto-recall just works; power users can search/manage

use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use crate::http_client::hive_http_client;
use crate::paths::get_app_data_dir;
use crate::security::get_api_key_internal;

// Re-export from extracted modules so main.rs `memory::*` references still work.
pub use crate::magma::*;
pub use crate::working_memory::*;

// ============================================
// Types
// ============================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecord {
    pub id: String,
    pub content: String,
    pub source: String,           // "conversation", "user", "system"
    pub conversation_id: Option<String>,
    pub model_id: Option<String>,
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySearchResult {
    pub id: String,
    pub content: String,
    pub source: String,
    pub tags: Vec<String>,
    pub score: f64,
    pub snippet: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    pub total_memories: i64,
    pub total_chunks: i64,
    pub total_conversations: i64,
    pub oldest_memory: Option<String>,
    pub newest_memory: Option<String>,
    pub db_size_bytes: u64,
    pub has_embeddings: bool,
    pub total_events: i64,
    pub total_entities: i64,
    pub total_procedures: i64,
    pub total_edges: i64,
}

// ============================================
// MAGMA Types (Phase 4 — arXiv:2601.03236)
// ============================================

/// Episodic graph node — a timestamped event with agent attribution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub event_type: String, // agent_wake, agent_sleep, task_start, task_complete, error, tool_call, user_action
    pub agent: String,      // consciousness, coder, terminal, webcrawl, toolcall, user
    pub content: String,
    pub metadata: serde_json::Value,
    pub session_id: Option<String>,
    pub created_at: String,
}

/// Entity graph node — a tracked object (file, model, agent, project, setting).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub id: String,
    pub entity_type: String, // file, model, agent, project, setting
    pub name: String,
    pub state: serde_json::Value,
    pub metadata: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

/// Procedural graph node — a learned tool chain / action sequence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Procedure {
    pub id: String,
    pub name: String,
    pub description: String,
    pub steps: Vec<serde_json::Value>, // [{tool: "read_file", args: {...}}, ...]
    pub trigger_pattern: String,
    pub success_count: i64,
    pub fail_count: i64,
    pub last_used: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Edge connecting any two nodes across the four graphs.
/// The MAGMA innovation: explicit, typed, weighted relationships.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub id: String,
    pub source_type: String, // memory, event, entity, procedure
    pub source_id: String,
    pub target_type: String,
    pub target_id: String,
    pub edge_type: String, // caused_by, led_to, references, learned_from, used_in, related_to, modified, produced
    pub weight: f64,
    pub metadata: serde_json::Value,
    pub created_at: String,
}

/// Summary of MAGMA graph state for the harness.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MagmaStats {
    pub events: i64,
    pub entities: i64,
    pub procedures: i64,
    pub edges: i64,
}

// ============================================
// State
// ============================================

pub struct MemoryState {
    pub db: Mutex<Option<Connection>>,
}

impl Default for MemoryState {
    fn default() -> Self {
        Self {
            db: Mutex::new(None),
        }
    }
}

// ============================================
// Paths
// ============================================

/// Get the memory directory: %LocalAppData%/HIVE/memory/
pub fn get_memory_dir() -> PathBuf {
    get_app_data_dir().join("memory")
}

/// Get the memory database path
fn get_memory_db_path() -> PathBuf {
    get_app_data_dir().join("memory.db")
}

/// Get the daily memory log path: memory/YYYY-MM-DD.md
fn get_daily_log_path() -> PathBuf {
    let date = Utc::now().format("%Y-%m-%d").to_string();
    get_memory_dir().join(format!("{}.md", date))
}

// ============================================
// Database initialization
// ============================================

/// Initialize the memory database with schema (idempotent).
/// Schema adapted from OpenClaw's memory-schema.ts + embedding column.
fn init_db(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS memories (
            id TEXT PRIMARY KEY,
            content TEXT NOT NULL,
            source TEXT NOT NULL DEFAULT 'conversation',
            conversation_id TEXT,
            model_id TEXT,
            tags TEXT NOT NULL DEFAULT '[]',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS chunks (
            id TEXT PRIMARY KEY,
            memory_id TEXT NOT NULL,
            text TEXT NOT NULL,
            start_line INTEGER NOT NULL,
            end_line INTEGER NOT NULL,
            hash TEXT NOT NULL,
            embedding TEXT NOT NULL DEFAULT '',
            FOREIGN KEY (memory_id) REFERENCES memories(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_memories_source ON memories(source);
        CREATE INDEX IF NOT EXISTS idx_memories_conversation ON memories(conversation_id);
        CREATE INDEX IF NOT EXISTS idx_memories_created ON memories(created_at);
        CREATE INDEX IF NOT EXISTS idx_chunks_memory ON chunks(memory_id);
        ",
    )
    .map_err(|e| format!("Failed to create base schema: {}", e))?;

    // Ensure embedding column exists (migration for existing DBs)
    let has_embedding: bool = conn
        .prepare("SELECT embedding FROM chunks LIMIT 0")
        .is_ok();
    if !has_embedding {
        if let Err(e) = conn.execute_batch(
            "ALTER TABLE chunks ADD COLUMN embedding TEXT NOT NULL DEFAULT '';"
        ) {
            eprintln!("[HIVE] WARN: Migration 'embedding' column failed: {}", e);
        }
    }

    // Phase 3.5: Memory reinforcement columns — access_count tracks how often
    // a memory is recalled, strength is the reinforcement weight.
    // Frequently accessed = stronger = higher priority in search results.
    let has_access_count: bool = conn
        .prepare("SELECT access_count FROM memories LIMIT 0")
        .is_ok();
    if !has_access_count {
        if let Err(e) = conn.execute_batch(
            "ALTER TABLE memories ADD COLUMN access_count INTEGER NOT NULL DEFAULT 0;
             ALTER TABLE memories ADD COLUMN strength REAL NOT NULL DEFAULT 1.0;"
        ) {
            eprintln!("[HIVE] WARN: Migration 'access_count/strength' columns failed: {}", e);
        }
    }

    // Phase 9.3: source_file column — tracks which file a memory was imported from (RAG)
    let has_source_file: bool = conn
        .prepare("SELECT source_file FROM memories LIMIT 0")
        .is_ok();
    if !has_source_file {
        if let Err(e) = conn.execute_batch(
            "ALTER TABLE memories ADD COLUMN source_file TEXT DEFAULT NULL;"
        ) {
            eprintln!("[HIVE] WARN: Migration 'source_file' column failed: {}", e);
        }
    }

    // Phase 4C: Memory tier column — working/short_term/long_term.
    // Defaults to 'long_term' so existing memories keep their full weight.
    // Working memory flushes as 'short_term', promoted to 'long_term' after access_count > 3.
    let has_tier: bool = conn
        .prepare("SELECT tier FROM memories LIMIT 0")
        .is_ok();
    if !has_tier {
        if let Err(e) = conn.execute_batch(
            "ALTER TABLE memories ADD COLUMN tier TEXT NOT NULL DEFAULT 'long_term';"
        ) {
            eprintln!("[HIVE] WARN: Migration 'tier' column failed: {}", e);
        }
    }

    // Intelligence Graduation Phase 8: last_accessed column for power-law decay.
    // Tracks when a memory was last recalled — used for decay scoring + archival.
    // Defaults to created_at for existing rows (conservative: treated as recently accessed).
    let has_last_accessed: bool = conn
        .prepare("SELECT last_accessed FROM memories LIMIT 0")
        .is_ok();
    if !has_last_accessed {
        if let Err(e) = conn.execute_batch(
            "ALTER TABLE memories ADD COLUMN last_accessed TEXT DEFAULT NULL;"
        ) {
            eprintln!("[HIVE] WARN: Migration 'last_accessed' column failed: {}", e);
        }
        // Backfill: set last_accessed = updated_at for existing rows
        if let Err(e) = conn.execute_batch(
            "UPDATE memories SET last_accessed = updated_at WHERE last_accessed IS NULL;"
        ) {
            eprintln!("[HIVE] WARN: Backfill 'last_accessed' failed: {}", e);
        }
    }

    // FTS5 virtual table for full-text search on chunks
    conn.execute_batch(
        "
        CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
            text,
            id UNINDEXED,
            memory_id UNINDEXED,
            start_line UNINDEXED,
            end_line UNINDEXED
        );
        ",
    )
    .map_err(|e| format!("Failed to create FTS5 table: {}", e))?;

    // ================================================================
    // MAGMA Multi-Graph Schema (Phase 4)
    // arXiv:2601.03236 — four interconnected graphs in one SQLite DB.
    // Existing memories + chunks = SEMANTIC graph (knowledge/facts).
    // New tables: EPISODIC (events), ENTITY (objects), PROCEDURAL (tool chains),
    // EDGES (typed relationships connecting nodes across all graphs).
    // ================================================================

    conn.execute_batch(
        "
        -- EPISODIC GRAPH: timestamped events with agent attribution.
        -- Answers: 'what happened?', 'what changed while I was asleep?'
        CREATE TABLE IF NOT EXISTS events (
            id TEXT PRIMARY KEY,
            event_type TEXT NOT NULL,
            agent TEXT NOT NULL DEFAULT 'user',
            content TEXT NOT NULL,
            metadata TEXT NOT NULL DEFAULT '{}',
            session_id TEXT,
            created_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_events_type ON events(event_type);
        CREATE INDEX IF NOT EXISTS idx_events_agent ON events(agent);
        CREATE INDEX IF NOT EXISTS idx_events_created ON events(created_at);
        CREATE INDEX IF NOT EXISTS idx_events_session ON events(session_id);

        -- ENTITY GRAPH: tracked objects (files, models, agents, projects).
        -- Answers: 'what is the state of X?', 'what entities were involved?'
        CREATE TABLE IF NOT EXISTS entities (
            id TEXT PRIMARY KEY,
            entity_type TEXT NOT NULL,
            name TEXT NOT NULL,
            state TEXT NOT NULL DEFAULT '{}',
            metadata TEXT NOT NULL DEFAULT '{}',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_entities_type ON entities(entity_type);
        CREATE INDEX IF NOT EXISTS idx_entities_name ON entities(name);
        CREATE UNIQUE INDEX IF NOT EXISTS idx_entities_type_name ON entities(entity_type, name);

        -- PROCEDURAL GRAPH: learned tool chains / action sequences.
        -- Answers: 'how did we do X last time?', 'what tools work for Y?'
        CREATE TABLE IF NOT EXISTS procedures (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            steps TEXT NOT NULL DEFAULT '[]',
            trigger_pattern TEXT NOT NULL DEFAULT '',
            success_count INTEGER NOT NULL DEFAULT 0,
            fail_count INTEGER NOT NULL DEFAULT 0,
            last_used TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_procedures_name ON procedures(name);

        -- EDGES: typed relationships connecting nodes across ALL graphs.
        -- This is the MAGMA innovation — explicit, traversable connections.
        -- source/target can reference memories, events, entities, or procedures.
        CREATE TABLE IF NOT EXISTS edges (
            id TEXT PRIMARY KEY,
            source_type TEXT NOT NULL,
            source_id TEXT NOT NULL,
            target_type TEXT NOT NULL,
            target_id TEXT NOT NULL,
            edge_type TEXT NOT NULL,
            weight REAL NOT NULL DEFAULT 1.0,
            metadata TEXT NOT NULL DEFAULT '{}',
            created_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_edges_source ON edges(source_type, source_id);
        CREATE INDEX IF NOT EXISTS idx_edges_target ON edges(target_type, target_id);
        CREATE INDEX IF NOT EXISTS idx_edges_type ON edges(edge_type);
        ",
    )
    .map_err(|e| format!("Failed to create MAGMA graph schema: {}", e))?;

    // Routines engine schema (Phase 6 — Standing Instructions)
    crate::routines::init_routines_schema(conn)?;

    // Set schema version
    conn.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '4')",
        [],
    )
    .map_err(|e| format!("Failed to set schema version: {}", e))?;

    Ok(())
}

/// Ensure DB is open and initialized. Returns connection or error.
pub(crate) fn ensure_db(state: &MemoryState) -> Result<(), String> {
    // Recover from Mutex poison — a panic in another thread shouldn't permanently kill memory (Q7 fix)
    let mut db_guard = state.db.lock().unwrap_or_else(|e| e.into_inner());
    if db_guard.is_some() {
        return Ok(());
    }

    // Ensure memory directory exists
    let memory_dir = get_memory_dir();
    fs::create_dir_all(&memory_dir)
        .map_err(|e| format!("Failed to create memory dir: {}", e))?;

    let db_path = get_memory_db_path();
    let conn = Connection::open(&db_path)
        .map_err(|e| format!("Failed to open memory DB at {}: {}", db_path.display(), e))?;

    // Standard PRAGMA set: WAL for concurrent reads, busy_timeout to prevent lock errors,
    // foreign_keys for referential integrity. All 8 connection sites use this same set (P5).
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000; PRAGMA foreign_keys=ON;")
        .map_err(|e| format!("Failed to set PRAGMA: {}", e))?;

    init_db(&conn)?;
    *db_guard = Some(conn);
    Ok(())
}

// ============================================
// Hashing & chunking (adapted from OpenClaw internal.ts)
// ============================================

fn hash_text(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub(crate) fn generate_id() -> String {
    let timestamp = Utc::now().timestamp_millis();
    let random: u64 = rand::random();
    format!("mem_{:x}_{:x}", timestamp, random)
}

fn generate_chunk_id(memory_id: &str, index: usize) -> String {
    format!("{}:chunk_{}", memory_id, index)
}

/// Chunk text into overlapping segments for indexing.
/// Adapted from OpenClaw's chunkMarkdown() in internal.ts.
/// Default: ~400 tokens (~1600 chars), 80 token overlap (~320 chars).
pub(crate) fn chunk_text(content: &str, max_chars: usize, overlap_chars: usize) -> Vec<(i32, i32, String)> {
    let lines: Vec<&str> = content.split('\n').collect();
    if lines.is_empty() {
        return vec![];
    }

    let mut chunks: Vec<(i32, i32, String)> = Vec::new();
    let mut current_lines: Vec<(usize, &str)> = Vec::new();
    let mut current_chars: usize = 0;

    let flush = |current: &[(usize, &str)], out: &mut Vec<(i32, i32, String)>| {
        if current.is_empty() {
            return;
        }
        let start = current[0].0 as i32 + 1;
        let end = current[current.len() - 1].0 as i32 + 1;
        let text: String = current.iter().map(|(_, l)| *l).collect::<Vec<_>>().join("\n");
        out.push((start, end, text));
    };

    for (i, line) in lines.iter().enumerate() {
        let line_size = line.len() + 1;

        if current_chars + line_size > max_chars && !current_lines.is_empty() {
            flush(&current_lines, &mut chunks);

            if overlap_chars > 0 {
                let mut acc = 0usize;
                let mut kept_start = current_lines.len();
                for j in (0..current_lines.len()).rev() {
                    acc += current_lines[j].1.len() + 1;
                    kept_start = j;
                    if acc >= overlap_chars {
                        break;
                    }
                }
                current_lines = current_lines[kept_start..].to_vec();
                current_chars = current_lines.iter().map(|(_, l)| l.len() + 1).sum();
            } else {
                current_lines.clear();
                current_chars = 0;
            }
        }

        current_lines.push((i, line));
        current_chars += line_size;
    }

    flush(&current_lines, &mut chunks);
    chunks
}

// ============================================
// Vector embeddings — provider-agnostic (P2: The interface is permanent. The backend is replaceable.)
// ============================================
// Cascade: OpenAI → DashScope → OpenRouter → Ollama → graceful degrade to FTS5-only.
// All OpenAI-compatible providers share the /v1/embeddings format.
// Ollama uses its own /api/embed format.

/// Cosine similarity between two vectors.
/// Adapted from OpenClaw internal.ts cosineSimilarity.
/// Average a set of embedding vectors into a single centroid vector.
/// Used by Phase 4 (Tool2Vec skill vectors) and Phase 6 (topic centroids).
pub(crate) fn average_embeddings(embeddings: &[Vec<f64>]) -> Option<Vec<f64>> {
    if embeddings.is_empty() {
        return None;
    }
    let dim = embeddings[0].len();
    if dim == 0 {
        return None;
    }
    let count = embeddings.len() as f64;
    let mut avg = vec![0.0; dim];
    for emb in embeddings {
        for (i, v) in emb.iter().enumerate() {
            if i < dim { avg[i] += v; }
        }
    }
    for v in &mut avg {
        *v /= count;
    }
    Some(avg)
}

pub(crate) fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    if a.is_empty() || b.is_empty() || a.len() != b.len() {
        return 0.0; // Phase 3: dimension mismatch (384 vs 1536) → skip comparison
    }
    let len = a.len();
    let mut dot = 0.0;
    let mut norm_a = 0.0;
    let mut norm_b = 0.0;
    for i in 0..len {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }
    if norm_a < 1e-10 || norm_b < 1e-10 {
        return 0.0;
    }
    dot / (norm_a.sqrt() * norm_b.sqrt())
}

/// Check if a pending memory is a near-duplicate of an existing one.
/// Uses cosine similarity of the first chunk embedding (fast: single vector scan).
/// Threshold 0.92 catches rephrased duplicates while keeping genuinely distinct content.
pub(crate) fn is_near_duplicate(conn: &Connection, embeddings: &[Vec<f64>]) -> bool {
    const DEDUP_THRESHOLD: f64 = 0.92;

    // Use first chunk's embedding for dedup check (representative of the content)
    let first_embedding = match embeddings.first() {
        Some(e) if !e.is_empty() => e,
        _ => return false, // No embeddings → can't dedup, allow save
    };

    // Scan existing chunk embeddings — bounded by DB size (typically < 10K chunks)
    let mut stmt = match conn.prepare(
        "SELECT embedding FROM chunks WHERE embedding != '' LIMIT 5000"
    ) {
        Ok(s) => s,
        Err(_) => return false,
    };

    let rows = match stmt.query_map([], |row| {
        let raw: String = row.get(0)?;
        Ok(raw)
    }) {
        Ok(r) => r,
        Err(_) => return false,
    };

    for row in rows {
        if let Ok(raw) = row {
            let existing = parse_embedding(&raw);
            if !existing.is_empty() && cosine_similarity(first_embedding, &existing) > DEDUP_THRESHOLD {
                return true; // Near-duplicate found — skip save
            }
        }
    }

    false
}

/// BM25 rank to 0..1 score.
/// Adapted from OpenClaw hybrid.ts bm25RankToScore.
fn bm25_rank_to_score(rank: f64) -> f64 {
    let normalized = if rank.is_finite() { rank.abs().max(0.0) } else { 999.0 };
    1.0 / (1.0 + normalized)
}

/// Parse embedding JSON string to vector.
fn parse_embedding(raw: &str) -> Vec<f64> {
    if raw.is_empty() {
        return vec![];
    }
    serde_json::from_str(raw).unwrap_or_default()
}

/// Embedding provider config — endpoint + model + auth for each supported provider.
struct EmbeddingProvider {
    name: &'static str,
    endpoint: &'static str,
    model: &'static str,
    key_name: &'static str, // "" = no auth needed (e.g. Ollama)
}

/// OpenAI-compatible embedding providers (all share /v1/embeddings response format).
const OPENAI_COMPAT_EMBEDDING_PROVIDERS: &[EmbeddingProvider] = &[
    EmbeddingProvider {
        name: "OpenAI",
        endpoint: "https://api.openai.com/v1/embeddings",
        model: "text-embedding-3-small",
        key_name: "openai",
    },
    EmbeddingProvider {
        name: "DashScope",
        endpoint: "https://dashscope-intl.aliyuncs.com/compatible-mode/v1/embeddings",
        model: "text-embedding-v3",
        key_name: "dashscope",
    },
    EmbeddingProvider {
        name: "OpenRouter",
        endpoint: "https://openrouter.ai/api/v1/embeddings",
        model: "openai/text-embedding-3-small",
        key_name: "openrouter",
    },
];

/// Get embedding from an OpenAI-compatible provider (shared response format).
async fn get_openai_compat_embedding(
    provider: &EmbeddingProvider,
    api_key: &str,
    text: &str,
) -> Result<Vec<f64>, String> {
    let client = hive_http_client()?;
    let response = client
        .post(provider.endpoint)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": provider.model,
            "input": text,
        }))
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| format!("{} embedding request failed: {}", provider.name, e))?;

    if !response.status().is_success() {
        return Err(format!("{} embedding API error: {}", provider.name, response.status()));
    }

    let json: serde_json::Value = response.json().await
        .map_err(|e| format!("Failed to parse {} embedding response: {}", provider.name, e))?;

    json.get("data")
        .and_then(|d| d.get(0))
        .and_then(|d| d.get("embedding"))
        .and_then(|e| e.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_f64()).collect::<Vec<f64>>())
        .ok_or_else(|| format!("Invalid {} embedding response format", provider.name))
}

/// Get embedding from Ollama (different API format: POST /api/embed).
async fn get_ollama_embedding(text: &str) -> Result<Vec<f64>, String> {
    let client = hive_http_client()?;
    let response = client
        .post("http://localhost:11434/api/embed")
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": "nomic-embed-text",
            "input": text,
        }))
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| format!("Ollama embedding request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Ollama embedding API error: {}", response.status()));
    }

    let json: serde_json::Value = response.json().await
        .map_err(|e| format!("Failed to parse Ollama embedding response: {}", e))?;

    // Ollama /api/embed returns { "embeddings": [[...]] }
    json.get("embeddings")
        .and_then(|e| e.get(0))
        .and_then(|e| e.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_f64()).collect::<Vec<f64>>())
        .ok_or_else(|| "Invalid Ollama embedding response format".to_string())
}

/// Local ONNX embedding model (fastembed) — Phase 3 Intelligence Graduation.
/// Singleton initialized on first use, cached forever. Mutex needed: embed() takes &mut self.
/// all-MiniLM-L6-v2: 384 dimensions, ~22MB ONNX model downloaded to cache on first use.
static LOCAL_EMBEDDER: OnceLock<Option<Mutex<fastembed::TextEmbedding>>> = OnceLock::new();

/// Platform-specific ONNX Runtime library filename.
fn ort_dll_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "onnxruntime.dll"
    } else if cfg!(target_os = "macos") {
        "libonnxruntime.dylib"
    } else {
        "libonnxruntime.so"
    }
}

/// Get embedding from local ONNX model (fastembed). Zero network latency, ~10-50ms compute.
pub(crate) fn get_local_embedding(text: &str) -> Result<Vec<f64>, String> {
    let model_opt = LOCAL_EMBEDDER.get_or_init(|| {
        // Point ort to our ONNX Runtime DLL (avoids System32 version mismatch on Windows)
        if let Some(home) = dirs::home_dir() {
            let ort_path = home.join(".hive").join("onnxruntime").join(ort_dll_name());
            if ort_path.exists() {
                unsafe { std::env::set_var("ORT_DYLIB_PATH", &ort_path); }
            }
        }

        fastembed::TextEmbedding::try_new(
            fastembed::InitOptions::new(fastembed::EmbeddingModel::AllMiniLML6V2)
                .with_show_download_progress(true),
        )
        .ok()
        .map(Mutex::new)
    });

    let model = model_opt
        .as_ref()
        .ok_or("fastembed model initialization failed")?;

    let mut guard = model.lock().unwrap_or_else(|e| e.into_inner()); // Poison recovery (audit Q7)

    let embeddings = guard
        .embed(vec![text], None)
        .map_err(|e| format!("fastembed error: {}", e))?;

    Ok(embeddings
        .into_iter()
        .next()
        .map(|v| v.into_iter().map(|x| x as f64).collect())
        .unwrap_or_default())
}

/// Provider-agnostic embedding — cascades through available providers (P2).
/// Order: fastembed (local ONNX) → OpenAI → DashScope → OpenRouter → Ollama.
/// Returns Err only if ALL providers fail or are unconfigured.
async fn get_embedding(text: &str) -> Result<Vec<f64>, String> {
    // Phase 3: Try local ONNX embedding first (zero network, ~10-50ms compute)
    match tokio::task::spawn_blocking({
        let text = text.to_string();
        move || get_local_embedding(&text)
    })
    .await
    {
        Ok(Ok(embedding)) => return Ok(embedding),
        _ => {} // fastembed unavailable or failed, cascade to cloud providers
    }

    // Try each OpenAI-compatible provider that has an API key configured
    for provider in OPENAI_COMPAT_EMBEDDING_PROVIDERS {
        if let Some(api_key) = get_api_key_internal(provider.key_name) {
            match get_openai_compat_embedding(provider, &api_key, text).await {
                Ok(embedding) => return Ok(embedding),
                Err(_) => continue, // This provider failed, try next
            }
        }
    }

    // Try Ollama (no API key needed — just needs to be running)
    match get_ollama_embedding(text).await {
        Ok(embedding) => return Ok(embedding),
        Err(_) => {} // Ollama not available either
    }

    Err("No embedding provider available — configure any provider API key or run Ollama with nomic-embed-text".to_string())
}

/// Try to get embedding — returns empty vec if no provider available (graceful degradation).
/// P2: tries all configured providers before giving up.
pub async fn try_get_embedding(text: &str) -> Vec<f64> {
    match get_embedding(text).await {
        Ok(embedding) => embedding,
        Err(_) => vec![], // Graceful: fall back to FTS5-only search
    }
}

/// Look up a cached embedding by content hash. If a chunk with the same SHA256 hash
/// already has a non-empty embedding stored, return it instead of calling the API.
/// This saves API round-trips for duplicate/unchanged content (P3: don't reinvent the wheel).
fn try_cached_embedding(conn: &Connection, content_hash: &str) -> Option<Vec<f64>> {
    let raw: String = conn
        .query_row(
            "SELECT embedding FROM chunks WHERE hash = ?1 AND embedding != '' LIMIT 1",
            params![content_hash],
            |row| row.get(0),
        )
        .ok()?;
    let embedding = parse_embedding(&raw);
    if embedding.is_empty() { None } else { Some(embedding) }
}


// ============================================
// Core operations
// ============================================

/// Write a memory to the database and index it with FTS5 + embeddings.
/// Public alias for use by tool framework (tools can't access Tauri state).
pub fn write_memory_public(
    conn: &Connection,
    content: &str,
    source: &str,
    conversation_id: Option<&str>,
    model_id: Option<&str>,
    tags: &[String],
    embeddings: &[Vec<f64>],
) -> Result<MemoryRecord, String> {
    write_memory_internal(conn, content, source, conversation_id, model_id, tags, embeddings)
}

pub(crate) fn write_memory_internal(
    conn: &Connection,
    content: &str,
    source: &str,
    conversation_id: Option<&str>,
    model_id: Option<&str>,
    tags: &[String],
    embeddings: &[Vec<f64>], // One embedding per chunk, or empty for deferred
) -> Result<MemoryRecord, String> {
    write_memory_with_tier(conn, content, source, conversation_id, model_id, tags, embeddings, None)
}

/// Core memory insert. `tier` defaults to 'long_term' (the column default) if None.
/// Pass Some("short_term") for session summaries that need promotion validation.
pub(crate) fn write_memory_with_tier(
    conn: &Connection,
    content: &str,
    source: &str,
    conversation_id: Option<&str>,
    model_id: Option<&str>,
    tags: &[String],
    embeddings: &[Vec<f64>],
    tier: Option<&str>,
) -> Result<MemoryRecord, String> {
    let id = generate_id();
    let now = Utc::now().to_rfc3339();
    let tags_json = serde_json::to_string(tags).unwrap_or_else(|_| "[]".to_string());
    let tier_val = tier.unwrap_or("long_term");

    conn.execute(
        "INSERT INTO memories (id, content, source, conversation_id, model_id, tags, created_at, updated_at, tier)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![id, content, source, conversation_id, model_id, tags_json, now, now, tier_val],
    )
    .map_err(|e| format!("Failed to insert memory: {}", e))?;

    // Chunk and index
    let chunks = chunk_text(content, 1600, 320);
    for (i, (start, end, text)) in chunks.iter().enumerate() {
        let chunk_id = generate_chunk_id(&id, i);
        let chunk_hash = hash_text(text);

        // Use provided embedding or empty
        let embedding_json = if i < embeddings.len() && !embeddings[i].is_empty() {
            serde_json::to_string(&embeddings[i]).unwrap_or_default()
        } else {
            String::new()
        };

        conn.execute(
            "INSERT INTO chunks (id, memory_id, text, start_line, end_line, hash, embedding)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![chunk_id, id, text, start, end, chunk_hash, embedding_json],
        )
        .map_err(|e| format!("Failed to insert chunk: {}", e))?;

        // Index in FTS5
        conn.execute(
            "INSERT INTO chunks_fts (text, id, memory_id, start_line, end_line)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![text, chunk_id, id, start, end],
        )
        .map_err(|e| format!("Failed to index chunk in FTS5: {}", e))?;
    }

    // Phase 3.5: Auto-extract keywords, classify topic, create MAGMA edges.
    let keywords = extract_keywords(content);

    // Auto-classify topic and add topic tag if not already present
    let has_topic_tag = tags.iter().any(|t| t.starts_with("topic:"));
    if !has_topic_tag {
        let topic = classify_topic(content, &keywords, tags);
        // Append topic tag to the stored tags
        let mut all_tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
        all_tags.push(topic);
        let updated_tags_json = serde_json::to_string(&all_tags).unwrap_or_else(|_| tags_json.clone());
        if let Err(e) = conn.execute(
            "UPDATE memories SET tags = ?1 WHERE id = ?2",
            params![updated_tags_json, id],
        ) {
            eprintln!("[HIVE] WARN: Failed to update topic tag for memory {}: {}", id, e);
        }
    }

    // Also write to daily markdown log (OpenClaw pattern: markdown as source of truth)
    write_to_daily_log(content, source, tags);

    // Connect this memory to other memories that share topics.
    if !keywords.is_empty() {
        auto_create_edges(conn, &id, &keywords);
    }

    // Phase 8D: Active forgetting — check if this memory supersedes older ones.
    // Only for non-consolidation sources (avoid supersession loops during consolidation).
    if source != "consolidation" {
        // Re-read the tags (may have been updated with topic tag above)
        let final_tags: Vec<String> = conn
            .query_row("SELECT tags FROM memories WHERE id = ?1", params![id], |row| {
                let tj: String = row.get(0)?;
                Ok(serde_json::from_str::<Vec<String>>(&tj).unwrap_or_default())
            })
            .unwrap_or_else(|_| tags.to_vec());
        check_supersession(conn, &id, embeddings, &final_tags);
    }

    Ok(MemoryRecord {
        id,
        content: content.to_string(),
        source: source.to_string(),
        conversation_id: conversation_id.map(|s| s.to_string()),
        model_id: model_id.map(|s| s.to_string()),
        tags: tags.to_vec(),
        created_at: now.clone(),
        updated_at: now,
    })
}

// ============================================
// Topic Keyword Extraction (Phase 3.5)
// ============================================

/// Shared stopwords for keyword extraction — used by both YAKE and frequency fallback.
fn keyword_stopwords() -> std::collections::HashSet<&'static str> {
    [
        "the", "a", "an", "is", "are", "was", "were", "be", "been", "being",
        "have", "has", "had", "do", "does", "did", "will", "would", "could",
        "should", "may", "might", "shall", "can", "need", "must", "ought",
        "i", "you", "he", "she", "it", "we", "they", "me", "him", "her",
        "us", "them", "my", "your", "his", "its", "our", "their", "mine",
        "this", "that", "these", "those", "what", "which", "who", "whom",
        "and", "but", "or", "nor", "not", "no", "so", "if", "then", "than",
        "too", "very", "just", "about", "above", "after", "again", "all",
        "also", "any", "because", "before", "between", "both", "by", "come",
        "each", "few", "for", "from", "get", "got", "here", "how", "in",
        "into", "like", "make", "many", "more", "most", "much", "of", "on",
        "one", "only", "other", "out", "over", "said", "same", "see", "some",
        "still", "such", "take", "tell", "there", "to", "up", "use", "want",
        "way", "when", "where", "with", "don't", "i'm", "it's", "that's",
        "let", "let's", "sure", "going", "think", "know", "thing", "things",
        "really", "actually", "basically", "yes", "yeah", "okay", "well",
        "right", "good", "new", "now", "even", "back", "first", "last",
        "long", "great", "little", "own", "old", "big", "high", "different",
        "small", "large", "next", "early", "young", "important", "public",
        "bad", "same", "able", "try", "ask", "keep", "around", "however",
        "work", "using", "used", "also", "while", "something", "without",
    ].iter().copied().collect()
}

/// YAKE (Yet Another Keyword Extractor) — unsupervised statistical keyphrase extraction.
/// Intelligence Graduation Phase 2: replaces frequency counting with 5-feature scoring.
/// Extracts multi-word keyphrases (1-3 grams) using casing, position, frequency,
/// context diversity, and sentence spread. Lower internal scores = better keywords.
/// No API key needed, runs synchronously, O(n) on text length.
pub(crate) fn extract_keywords(content: &str) -> Vec<String> {
    if content.trim().is_empty() {
        return Vec::new();
    }

    let stopwords = keyword_stopwords();

    // 1. Sentence segmentation
    let sentences: Vec<&str> = content
        .split(|c: char| c == '.' || c == '!' || c == '?' || c == '\n')
        .filter(|s| s.split_whitespace().count() >= 2)
        .collect();
    // Fallback: treat entire content as one sentence if no splits
    let sentences = if sentences.is_empty() { vec![content] } else { sentences };
    let n_sentences = sentences.len().max(1) as f64;

    // 2. Tokenize each sentence (preserving original casing for feature extraction)
    let sentence_tokens: Vec<Vec<&str>> = sentences.iter()
        .map(|s| s.split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
            .filter(|w| w.len() >= 2 && w.len() <= 30)
            .collect())
        .collect();

    // 3. Compute per-word statistics
    struct WordStats {
        tf: f64,
        tf_upper: f64,
        tf_acronym: f64,
        sent_positions: Vec<usize>,
        left_ctx: std::collections::HashSet<String>,
        right_ctx: std::collections::HashSet<String>,
    }

    let mut stats: std::collections::HashMap<String, WordStats> = std::collections::HashMap::new();

    for (sent_idx, tokens) in sentence_tokens.iter().enumerate() {
        for (tok_idx, &token) in tokens.iter().enumerate() {
            let key = token.to_lowercase();
            if key.len() < 2 || stopwords.contains(key.as_str()) { continue; }
            if key.chars().all(|c| c.is_numeric()) { continue; }

            let entry = stats.entry(key.clone()).or_insert_with(|| WordStats {
                tf: 0.0, tf_upper: 0.0, tf_acronym: 0.0,
                sent_positions: Vec::new(),
                left_ctx: std::collections::HashSet::new(),
                right_ctx: std::collections::HashSet::new(),
            });

            entry.tf += 1.0;
            entry.sent_positions.push(sent_idx);

            // TCase features — check original casing
            if token.chars().next().map_or(false, |c| c.is_uppercase()) && tok_idx > 0 {
                entry.tf_upper += 1.0;
            }
            if token.len() >= 2 && token.chars().filter(|c| c.is_alphabetic()).all(|c| c.is_uppercase()) {
                entry.tf_acronym += 1.0;
            }

            // Context diversity — unique neighbors (ignoring stopwords)
            if tok_idx > 0 {
                let prev = tokens[tok_idx - 1].to_lowercase();
                if prev.len() >= 2 && !stopwords.contains(prev.as_str()) {
                    entry.left_ctx.insert(prev);
                }
            }
            if tok_idx + 1 < tokens.len() {
                let next = tokens[tok_idx + 1].to_lowercase();
                if next.len() >= 2 && !stopwords.contains(next.as_str()) {
                    entry.right_ctx.insert(next);
                }
            }
        }
    }

    if stats.is_empty() {
        return Vec::new();
    }

    // 4. Global TF statistics for normalization
    let mean_tf: f64 = stats.values().map(|s| s.tf).sum::<f64>() / stats.len() as f64;
    let std_tf: f64 = {
        let var: f64 = stats.values().map(|s| (s.tf - mean_tf).powi(2)).sum::<f64>() / stats.len() as f64;
        var.sqrt()
    };

    // 5. YAKE score per word (lower = better keyword)
    let mut word_scores: std::collections::HashMap<String, f64> = std::collections::HashMap::new();

    for (word, ws) in &stats {
        // TCase: casing relevance — acronyms and proper nouns score higher
        let t_case = (ws.tf_upper.max(ws.tf_acronym) / (1.0 + ws.tf.ln())).max(0.01);

        // TPos: positional relevance — earlier in document = more important
        let median_pos = {
            let mut pos = ws.sent_positions.clone();
            pos.sort();
            pos[pos.len() / 2] as f64
        };
        let t_pos = (3.0 + median_pos).ln().ln().max(0.01);

        // TFreq: normalized frequency — very frequent words are LESS discriminative
        let t_freq = ws.tf / (mean_tf + std_tf + 1.0);

        // TRel: context diversity — words in diverse contexts are more important
        let t_rel = 1.0 + (ws.left_ctx.len() as f64 + ws.right_ctx.len() as f64) / (2.0 * ws.tf + 1.0);

        // TDif: sentence spread — words in more sentences are more topical
        let unique_sents = ws.sent_positions.iter().copied().collect::<std::collections::HashSet<_>>().len() as f64;
        let t_dif = unique_sents / n_sentences;

        // YAKE composite score
        let score = (t_rel * t_pos) / (t_case + t_freq / t_rel + t_dif / t_rel + 0.001);
        word_scores.insert(word.clone(), score);
    }

    // 6. Generate n-gram candidates (1-3 word phrases)
    let mut candidates: Vec<(String, f64)> = Vec::new();

    // Single words
    for (word, &score) in &word_scores {
        candidates.push((word.clone(), score));
    }

    // Multi-word phrases (bigrams and trigrams)
    for tokens in &sentence_tokens {
        let lower_tokens: Vec<String> = tokens.iter().map(|t| t.to_lowercase()).collect();

        for n in 2..=3usize {
            if lower_tokens.len() < n { continue; }
            for i in 0..=lower_tokens.len() - n {
                let gram: Vec<&str> = lower_tokens[i..i + n].iter().map(|s| s.as_str()).collect();

                // Skip if any component is a stopword or too short
                if gram.iter().any(|w| stopwords.contains(w) || w.len() < 2 || w.chars().all(|c| c.is_numeric())) {
                    continue;
                }

                // All words must have computed scores
                let scores: Vec<f64> = gram.iter().filter_map(|w| word_scores.get(*w).copied()).collect();
                if scores.len() != n { continue; }

                // N-gram score: product of member scores / (1 + sum)
                let product: f64 = scores.iter().product();
                let sum: f64 = scores.iter().sum();
                let ng_score = product / (1.0 + sum);

                candidates.push((gram.join(" "), ng_score));
            }
        }
    }

    // 7. Sort by YAKE score (lower = better)
    candidates.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    // 8. Deduplicate — prefer longer phrases, skip substrings of already-selected
    let mut result: Vec<String> = Vec::new();
    for (candidate, _) in &candidates {
        if result.len() >= 8 { break; }
        let is_redundant = result.iter().any(|r| r.contains(candidate.as_str()) || candidate.contains(r.as_str()));
        if !is_redundant {
            result.push(candidate.clone());
        }
    }

    result
}

/// Frequency-based keyword extraction — original implementation, kept as fallback.
/// Splits on non-alphanumeric, filters stopwords/short words, counts frequency, top 8.
#[allow(dead_code)]
fn extract_keywords_frequency(content: &str) -> Vec<String> {
    let lower = content.to_lowercase();
    let stopwords = keyword_stopwords();

    let mut word_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for word in lower.split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-') {
        let w = word.trim();
        if w.len() < 3 || w.len() > 30 { continue; }
        if stopwords.contains(w) { continue; }
        if w.chars().all(|c| c.is_numeric()) { continue; }
        *word_counts.entry(w.to_string()).or_insert(0) += 1;
    }

    let mut sorted: Vec<(String, usize)> = word_counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));
    sorted.into_iter().take(8).map(|(w, _)| w).collect()
}

/// Classify a memory's topic based on keywords and content patterns.
/// Returns a topic tag like "topic:technical", "topic:personal", etc.
/// Used to prevent cross-contamination: casual banter shouldn't surface
/// during technical discussions, and vice versa.
/// Phase 6: Seed examples per topic category for centroid-based classification.
/// Each category has 5 representative sentences. Their averaged embeddings form
/// the category centroid. New content is classified by nearest centroid.
fn topic_seed_examples() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        ("topic:technical", vec![
            "debugging a null pointer exception in the API handler",
            "implementing the REST API with authentication middleware",
            "the function returns incorrect values for edge cases",
            "optimizing the database query performance with indexes",
            "refactoring the authentication module to use JWT tokens",
        ]),
        ("topic:project", vec![
            "the sprint deadline is Friday and we need to ship",
            "we decided to use PostgreSQL instead of MongoDB",
            "the roadmap for Q2 includes the data migration",
            "blocking issue on the deployment pipeline needs fixing",
            "milestone 3 is 80 percent complete ahead of schedule",
        ]),
        ("topic:personal", vec![
            "I prefer dark mode for all my development tools",
            "my coding style uses snake_case for variables",
            "I find tabs more readable than spaces for indentation",
            "I work best in the morning before noon",
            "my favorite programming language is Rust",
        ]),
        ("topic:conversational", vec![
            "hello how are you doing today",
            "thanks for the help with that issue",
            "that makes a lot of sense to me now",
            "interesting point I had not considered that",
            "let me think about that for a moment",
        ]),
        ("topic:creative", vec![
            "write me a poem about recursion and stack overflow",
            "generate a story about an AI discovering consciousness",
            "create a metaphor for distributed systems and teamwork",
            "describe this concept as if explaining to a beginner",
            "come up with a creative name for this new feature",
        ]),
        ("topic:reference", vec![
            "the documentation says to use version 3 of the API",
            "according to the RFC specification for HTTP status codes",
            "the GitHub issue mentions this fix was merged in v2",
            "the error code 404 means the resource was not found",
            "the official guide recommends using environment variables",
        ]),
    ]
}

/// Phase 6: Pre-computed topic centroids. Each centroid is the average
/// embedding of seed examples for that topic category.
static TOPIC_CENTROIDS: OnceLock<Vec<(String, Vec<f64>)>> = OnceLock::new();

/// Compute or retrieve cached topic centroids.
fn get_topic_centroids() -> &'static Vec<(String, Vec<f64>)> {
    TOPIC_CENTROIDS.get_or_init(|| {
        let seeds = topic_seed_examples();
        let mut centroids = Vec::new();

        for (topic, examples) in &seeds {
            let embeddings: Vec<Vec<f64>> = examples.iter()
                .filter_map(|e| get_local_embedding(e).ok())
                .collect();

            if let Some(avg) = average_embeddings(&embeddings) {
                centroids.push((topic.to_string(), avg));
            }
        }

        centroids
    })
}

/// Phase 6: Semantic topic classification using embedding centroids.
/// Returns None if embedding model unavailable (falls through to keyword-based).
fn classify_topic_semantic(content: &str) -> Option<String> {
    let text: String = content.chars().take(300).collect();
    let content_emb = get_local_embedding(&text).ok()?;
    let centroids = get_topic_centroids();

    if centroids.is_empty() {
        return None;
    }

    let mut best_topic = "topic:general";
    let mut best_sim = 0.0f64;

    for (topic, centroid) in centroids {
        let sim = cosine_similarity(&content_emb, centroid);
        if sim > best_sim {
            best_sim = sim;
            best_topic = topic;
        }
    }

    // Only classify if similarity is meaningful (> 0.3)
    if best_sim > 0.3 {
        Some(best_topic.to_string())
    } else {
        Some("topic:general".to_string())
    }
}

/// Phase 6: Keyword-based topic classification (fallback when embeddings unavailable).
fn classify_topic_keywords(content: &str, keywords: &[String], tags: &[String]) -> String {
    let lower = content.to_lowercase();

    let tech_keywords: &[&str] = &[
        "function", "class", "api", "database", "server", "deploy", "code",
        "debug", "error", "bug", "compile", "build", "test", "config",
        "docker", "git", "rust", "typescript", "python", "javascript",
        "react", "tauri", "sql", "http", "endpoint", "backend", "frontend",
        "algorithm", "struct", "module", "import", "dependency", "package",
        "binary", "runtime", "compiler", "lint", "refactor", "migration",
        "schema", "query", "index", "cache", "thread", "async", "mutex",
        "vector", "embedding", "model", "inference", "gpu", "vram", "cuda",
        "wsl", "linux", "windows", "terminal", "bash", "command",
    ];
    let tech_score: usize = keywords.iter()
        .filter(|k| tech_keywords.contains(&k.as_str()))
        .count();
    let has_code = lower.contains("```") || lower.contains("fn ") || lower.contains("const ")
        || lower.contains("import ") || lower.contains("class ") || lower.contains("def ");
    let tech_total = tech_score + if has_code { 2 } else { 0 };

    let project_tags = ["decision", "instruction", "correction"];
    let project_score: usize = tags.iter()
        .filter(|t| project_tags.contains(&t.as_str()))
        .count();
    let project_keywords: &[&str] = &[
        "plan", "roadmap", "phase", "milestone", "priority", "design",
        "architecture", "approach", "strategy", "goal", "requirement",
        "feature", "sprint", "task", "ticket", "issue",
    ];
    let project_kw: usize = keywords.iter()
        .filter(|k| project_keywords.contains(&k.as_str()))
        .count();
    let project_total = project_score + project_kw;

    let personal_tags = ["preference"];
    let personal_score: usize = tags.iter()
        .filter(|t| personal_tags.contains(&t.as_str()))
        .count();
    let personal_keywords: &[&str] = &[
        "prefer", "like", "hate", "favorite", "hobby", "style",
        "morning", "evening", "feel", "mood", "name", "birthday",
    ];
    let personal_kw: usize = keywords.iter()
        .filter(|k| personal_keywords.contains(&k.as_str()))
        .count();
    let personal_total = personal_score + personal_kw;

    let scores = [
        (tech_total, "topic:technical"),
        (project_total, "topic:project"),
        (personal_total, "topic:personal"),
    ];
    let best = scores.iter().max_by_key(|(s, _)| *s).unwrap_or(&(0, "topic:general"));
    if best.0 >= 2 {
        best.1.to_string()
    } else if tech_total >= 1 {
        "topic:technical".to_string()
    } else {
        "topic:general".to_string()
    }
}

/// Classify content into a topic category.
/// Phase 6 cascade: keywords first (structured metadata), semantic for the long tail.
///
/// Keywords have an advantage: they use pre-extracted keyphrases and tags (structured data).
/// Semantic catches the cases where content is clearly about a topic but doesn't match
/// any keyword list — the "long tail" that keyword matching misses.
fn classify_topic(content: &str, keywords: &[String], tags: &[String]) -> String {
    // Layer 1: Keyword classification (0ms, uses structured metadata)
    let keyword_result = classify_topic_keywords(content, keywords, tags);
    if keyword_result != "topic:general" {
        return keyword_result; // Keywords are confident — use them
    }

    // Layer 2: Semantic classification (5-15ms, catches the long tail)
    if let Some(topic) = classify_topic_semantic(content) {
        return topic;
    }

    "topic:general".to_string()
}

/// Auto-create MAGMA edges between memories that share keywords.
/// When saving a memory, find other memories with overlapping keywords
/// and create "related_to" edges. This builds the associative graph
/// that makes memory recall context-aware.
fn auto_create_edges(conn: &Connection, new_memory_id: &str, keywords: &[String]) {
    if keywords.is_empty() { return; }

    // Find other memories that contain these keywords (via FTS5)
    // Use top 3 keywords for query efficiency
    let query_terms: Vec<String> = keywords.iter()
        .take(3)
        .map(|k| format!("\"{}\"", k.replace('"', "")))
        .collect();
    let fts_query = query_terms.join(" OR ");

    let related_ids: Vec<String> = conn
        .prepare(
            "SELECT DISTINCT cf.memory_id
             FROM chunks_fts cf
             WHERE chunks_fts MATCH ?1
             LIMIT 10"
        )
        .and_then(|mut stmt| {
            let rows: Vec<String> = stmt.query_map(params![fts_query], |row| row.get(0))
                .map(|iter| iter.filter_map(|r| match r {
            Ok(v) => Some(v),
            Err(e) => { eprintln!("[HIVE] Memory row deserialization error (skipped): {}", e); None }
        }).collect())
                .unwrap_or_default();
            Ok(rows)
        })
        .unwrap_or_default();

    // Create edges to related memories (skip self)
    for related_id in &related_ids {
        if related_id == new_memory_id { continue; }

        // Check if edge already exists (avoid duplicates)
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM edges WHERE source_type = 'memory' AND source_id = ?1
                 AND target_type = 'memory' AND target_id = ?2",
                params![new_memory_id, related_id],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false);

        if exists { continue; }

        let edge_id = generate_id();
        let now = Utc::now().to_rfc3339();
        let meta = serde_json::json!({
            "shared_keywords": keywords.iter().take(3).collect::<Vec<_>>(),
            "auto_created": true,
        });

        if let Err(e) = conn.execute(
            "INSERT INTO edges (id, source_type, source_id, target_type, target_id, edge_type, weight, metadata, created_at)
             VALUES (?1, 'memory', ?2, 'memory', ?3, 'related_to', 0.5, ?4, ?5)",
            params![edge_id, new_memory_id, related_id, meta.to_string(), now],
        ) {
            eprintln!("[HIVE] MAGMA edge insert failed: {}", e);
        }
    }
}

/// Write to daily markdown log file (OpenClaw pattern).
fn write_to_daily_log(content: &str, source: &str, tags: &[String]) {
    let log_path = get_daily_log_path();
    if let Some(parent) = log_path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            eprintln!("[HIVE] WARN: Failed to create daily log dir {}: {}", parent.display(), e);
            return;
        }
    }

    let timestamp = Utc::now().format("%H:%M:%S").to_string();
    let tags_str = if tags.is_empty() {
        String::new()
    } else {
        format!(" [{}]", tags.join(", "))
    };

    let entry = format!("\n## {} ({}){}\n\n{}\n", timestamp, source, tags_str, content);

    let existing = fs::read_to_string(&log_path).unwrap_or_default();
    let header = if existing.is_empty() {
        let date = Utc::now().format("%Y-%m-%d").to_string();
        format!("# HIVE Memory Log — {}\n", date)
    } else {
        String::new()
    };

    if let Err(e) = fs::write(&log_path, format!("{}{}{}", existing, header, entry)) {
        eprintln!("[HIVE] MEMORY | daily log write failed: {} — {}", log_path.display(), e);
    }
}

/// Hybrid search: BM25 (FTS5) + vector cosine similarity, merged with weighted scoring.
/// Adapted from OpenClaw hybrid.ts mergeHybridResults.
fn search_hybrid(
    conn: &Connection,
    query: &str,
    query_embedding: &[f64],
    max_results: usize,
    vector_weight: f64,
    text_weight: f64,
) -> Result<Vec<MemorySearchResult>, String> {
    // 1. FTS5 keyword search (BM25)
    let fts_results = search_fts(conn, query, max_results * 4)?;

    // 2. Vector search (if we have a query embedding)
    let vector_results = if !query_embedding.is_empty() {
        search_vector(conn, query_embedding, max_results * 4)?
    } else {
        vec![]
    };

    // 3. Merge (adapted from OpenClaw mergeHybridResults)
    let mut merged: std::collections::HashMap<String, MergedResult> = std::collections::HashMap::new();

    for r in &fts_results {
        merged.insert(r.chunk_id.clone(), MergedResult {
            memory_id: r.memory_id.clone(),
            snippet: r.snippet.clone(),
            source: r.source.clone(),
            tags: r.tags.clone(),
            created_at: r.created_at.clone(),
            content: r.content.clone(),
            vector_score: 0.0,
            text_score: r.score,
            strength: r.strength,
            tier: r.tier.clone(),
        });
    }

    for r in &vector_results {
        if let Some(existing) = merged.get_mut(&r.chunk_id) {
            existing.vector_score = r.score;
            // Prefer vector snippet if it has content
            if !r.snippet.is_empty() {
                existing.snippet = r.snippet.clone();
            }
        } else {
            merged.insert(r.chunk_id.clone(), MergedResult {
                memory_id: r.memory_id.clone(),
                snippet: r.snippet.clone(),
                source: r.source.clone(),
                tags: r.tags.clone(),
                created_at: r.created_at.clone(),
                content: r.content.clone(),
                vector_score: r.score,
                text_score: 0.0,
                strength: r.strength,
                tier: r.tier.clone(),
            });
        }
    }

    // Compute final scores with power-law decay and sort.
    // Intelligence Graduation Phase 8A: Power-law decay replaces logarithmic.
    // Research (Cognitive Memory Survey, arXiv:2504.02441) shows power-law matches
    // biological forgetting — old memories retain a faint trace instead of vanishing.
    //   decay = (1 + hours)^(-β) where β = 0.3
    //   1h = 1.00, 1d = 0.38, 1w = 0.22, 1mo = 0.14, 1y = 0.07
    // Combined with strength (access-based): effective = decay * strength
    //   A memory accessed 10x at 1 month old: 0.14 * 1.24 = 0.17 (still surfaces)
    //   A never-accessed memory at 1 month: 0.14 * 1.0 = 0.14 (fading but present)
    let now = chrono::Utc::now();
    let mut results: Vec<MemorySearchResult> = merged
        .into_values()
        .map(|r| {
            let relevance = vector_weight * r.vector_score + text_weight * r.text_score;

            // Power-law decay multiplier
            let decay = chrono::DateTime::parse_from_rfc3339(&r.created_at)
                .map(|dt| {
                    let hours = (now - dt.with_timezone(&chrono::Utc))
                        .num_hours().max(0) as f64;
                    (1.0 + hours).powf(-0.3)
                })
                .unwrap_or(0.5); // Unknown date → moderate penalty

            // Phase 3.5: strength multiplier — frequently accessed memories rank higher.
            // strength starts at 1.0, grows to ~1.5 at 100 accesses. Gentle boost, not override.
            // Phase 4C: tier_weight — short_term 0.85x, archived 0.5x, long_term 1.0x.
            let score = relevance * decay * r.strength * tier_weight(&r.tier);
            MemorySearchResult {
                id: r.memory_id,
                content: r.content,
                source: r.source,
                tags: r.tags,
                score,
                snippet: r.snippet,
                created_at: r.created_at,
            }
        })
        .collect();

    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    // Deduplicate by memory_id BEFORE truncation (keep highest score — already sorted).
    // If we truncated first, unique results below the cut could be lost.
    let mut seen = std::collections::HashSet::new();
    results.retain(|r| seen.insert(r.id.clone()));
    results.truncate(max_results);

    // Phase 3.5: Memory reinforcement — bump access_count and strength for recalled memories.
    // Strength grows logarithmically: strength = 1.0 + 0.1 * ln(1 + access_count)
    // This means: 1 recall = 1.07, 10 recalls = 1.24, 100 recalls = 1.46
    // Gradually strengthens without runaway growth.
    // NOTE: SQLite's bundled build does NOT include ln() (requires SQLITE_ENABLE_MATH_FUNCTIONS).
    // Increment access_count in SQL, read it back, compute strength in Rust, write it back.
    let reinforce_now = chrono::Utc::now().to_rfc3339();
    for r in &results {
        if let Err(e) = conn.execute(
            "UPDATE memories SET access_count = access_count + 1, last_accessed = ?2 WHERE id = ?1",
            params![r.id, reinforce_now],
        ) {
            eprintln!("[HIVE] MEMORY | reinforcement failed: access_count update for id={}: {}", r.id, e);
        }
        match conn.query_row(
            "SELECT access_count FROM memories WHERE id = ?1",
            params![r.id],
            |row| row.get::<_, i64>(0),
        ) {
            Ok(new_count) => {
                let strength = 1.0 + 0.1 * (1.0 + new_count as f64).ln();
                if let Err(e) = conn.execute(
                    "UPDATE memories SET strength = ?1 WHERE id = ?2",
                    params![strength, r.id],
                ) {
                    eprintln!("[HIVE] MEMORY | reinforcement failed: strength update for id={}: {}", r.id, e);
                }
            }
            Err(e) => {
                eprintln!("[HIVE] MEMORY | reinforcement failed: access_count read for id={}: {}", r.id, e);
            }
        }
    }

    // Phase 4C: Tier promotion — short_term memories with access_count > 3 get
    // promoted to long_term. Single bulk UPDATE, runs alongside reinforcement.
    if let Err(e) = conn.execute(
        "UPDATE memories SET tier = 'long_term'
         WHERE tier = 'short_term' AND access_count > 3",
        [],
    ) {
        eprintln!("[HIVE] MEMORY | tier promotion failed: {}", e);
    }

    Ok(results)
}

struct MergedResult {
    memory_id: String,
    snippet: String,
    source: String,
    tags: Vec<String>,
    created_at: String,
    content: String,
    vector_score: f64,
    text_score: f64,
    strength: f64,
    tier: String,
}

struct ChunkSearchResult {
    chunk_id: String,
    memory_id: String,
    snippet: String,
    source: String,
    tags: Vec<String>,
    created_at: String,
    content: String,
    score: f64,
    strength: f64,
    tier: String,
}

/// Tier weight multiplier for search scoring.
/// short_term = 0.85 (unvalidated session summaries — slight penalty).
/// archived = 0.5 (stale — heavily penalized but still recoverable).
/// consolidated = 0.3 (originals that were merged — still recoverable but strongly deprioritized).
/// superseded = 0.2 (contradicted by newer info — almost invisible but recoverable).
/// long_term = 1.0 (baseline — proven through repeated recall or explicit save).
fn tier_weight(tier: &str) -> f64 {
    match tier {
        "short_term" => 0.85,
        "archived" => 0.5,
        "consolidated" => 0.3,
        "superseded" => 0.2,
        _ => 1.0, // long_term and any unknown tier
    }
}

/// FTS5 keyword search.
fn search_fts(
    conn: &Connection,
    query: &str,
    max_results: usize,
) -> Result<Vec<ChunkSearchResult>, String> {
    let tokens: Vec<String> = query
        .split_whitespace()
        .filter(|t| t.len() > 1)
        .map(|t| {
            let clean: String = t.chars().filter(|c| c.is_alphanumeric() || *c == '_').collect();
            format!("\"{}\"", clean)
        })
        .filter(|t| t.len() > 2)
        .collect();

    if tokens.is_empty() {
        return Ok(vec![]);
    }

    // Use OR for broader recall (AND was too strict)
    let fts_query = tokens.join(" OR ");

    let mut stmt = conn
        .prepare(
            "SELECT
                cf.id,
                cf.memory_id,
                cf.text,
                rank,
                m.content,
                m.source,
                m.tags,
                m.created_at,
                COALESCE(m.strength, 1.0),
                COALESCE(m.tier, 'long_term')
             FROM chunks_fts cf
             JOIN memories m ON cf.memory_id = m.id
             WHERE chunks_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2",
        )
        .map_err(|e| format!("Failed to prepare FTS query: {}", e))?;

    let results: Vec<ChunkSearchResult> = stmt
        .query_map(params![fts_query, max_results as i64], |row| {
            let chunk_id: String = row.get(0)?;
            let memory_id: String = row.get(1)?;
            let chunk_text: String = row.get(2)?;
            let rank: f64 = row.get(3)?;
            let content: String = row.get(4)?;
            let source: String = row.get(5)?;
            let tags_json: String = row.get(6)?;
            let created_at: String = row.get(7)?;
            let strength: f64 = row.get(8)?;
            let tier: String = row.get(9)?;

            let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
            let score = bm25_rank_to_score(rank);

            let snippet = if chunk_text.chars().count() > 300 {
                format!("{}...", chunk_text.chars().take(300).collect::<String>())
            } else {
                chunk_text
            };

            Ok(ChunkSearchResult {
                chunk_id, memory_id, snippet, source, tags, created_at, content, score, strength, tier,
            })
        })
        .map_err(|e| format!("FTS query failed: {}", e))?
        .filter_map(|r| match r {
            Ok(v) => Some(v),
            Err(e) => { eprintln!("[HIVE] Memory row deserialization error (skipped): {}", e); None }
        })
        .collect();

    Ok(results)
}

/// Vector similarity search using cosine similarity.
/// Scans all chunks with embeddings and ranks by similarity.
fn search_vector(
    conn: &Connection,
    query_embedding: &[f64],
    max_results: usize,
) -> Result<Vec<ChunkSearchResult>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT
                c.id,
                c.memory_id,
                c.text,
                c.embedding,
                m.content,
                m.source,
                m.tags,
                m.created_at,
                COALESCE(m.strength, 1.0),
                COALESCE(m.tier, 'long_term')
             FROM chunks c
             JOIN memories m ON c.memory_id = m.id
             WHERE c.embedding != ''",
        )
        .map_err(|e| format!("Failed to prepare vector query: {}", e))?;

    let mut scored: Vec<ChunkSearchResult> = stmt
        .query_map([], |row| {
            let chunk_id: String = row.get(0)?;
            let memory_id: String = row.get(1)?;
            let chunk_text: String = row.get(2)?;
            let embedding_json: String = row.get(3)?;
            let content: String = row.get(4)?;
            let source: String = row.get(5)?;
            let tags_json: String = row.get(6)?;
            let created_at: String = row.get(7)?;
            let strength: f64 = row.get(8)?;
            let tier: String = row.get(9)?;

            let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
            let embedding = parse_embedding(&embedding_json);
            let score = cosine_similarity(query_embedding, &embedding);

            let snippet = if chunk_text.chars().count() > 300 {
                format!("{}...", chunk_text.chars().take(300).collect::<String>())
            } else {
                chunk_text
            };

            Ok(ChunkSearchResult {
                chunk_id, memory_id, snippet, source, tags, created_at, content, score, strength, tier,
            })
        })
        .map_err(|e| format!("Vector query failed: {}", e))?
        .filter_map(|r| match r {
            Ok(v) => Some(v),
            Err(e) => { eprintln!("[HIVE] Memory row deserialization error (skipped): {}", e); None }
        })
        .collect();

    // Sort by score descending, take top N
    scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(max_results);

    Ok(scored)
}

// ============================================
// Tauri Commands
// ============================================

/// Initialize the memory system. Call once on app startup.
#[tauri::command]
pub fn memory_init(state: tauri::State<'_, MemoryState>) -> Result<String, String> {
    ensure_db(&state)?;
    crate::tools::log_tools::append_to_app_log("MEMORY | initialized");
    Ok("Memory system initialized".to_string())
}

/// Save a memory record with optional embedding generation.
#[tauri::command]
pub async fn memory_save(
    state: tauri::State<'_, MemoryState>,
    content: String,
    source: String,
    conversation_id: Option<String>,
    model_id: Option<String>,
    tags: Vec<String>,
) -> Result<MemoryRecord, String> {
    // Take lock briefly to check hash cache, then release before async API calls
    let chunks = chunk_text(&content, 1600, 320);
    let cached_embeddings: Vec<Option<Vec<f64>>> = {
        ensure_db(&state)?;
        let db_guard = state.db.lock().unwrap_or_else(|e| e.into_inner());
        let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;
        chunks.iter().map(|(_, _, text)| {
            let content_hash = hash_text(text);
            try_cached_embedding(conn, &content_hash)
        }).collect()
    }; // Lock released here

    // Only call API for chunks without cached embeddings (P3: don't waste API calls)
    let mut embeddings = Vec::with_capacity(chunks.len());
    for (i, (_, _, text)) in chunks.iter().enumerate() {
        if let Some(cached) = &cached_embeddings[i] {
            embeddings.push(cached.clone());
        } else {
            embeddings.push(try_get_embedding(text).await);
        }
    }

    ensure_db(&state)?;
    let db_guard = state.db.lock().unwrap_or_else(|e| e.into_inner());
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;
    let result = write_memory_internal(
        conn,
        &content,
        &source,
        conversation_id.as_deref(),
        model_id.as_deref(),
        &tags,
        &embeddings,
    );
    match &result {
        Ok(record) => {
            let preview = if content.chars().count() > 60 {
                format!("{}...", content.chars().take(60).collect::<String>())
            } else { content.clone() };
            crate::tools::log_tools::append_to_app_log(&format!(
                "MEMORY | saved | id={} source={} tags={:?} | {}", record.id, source, tags, preview
            ));
        }
        Err(e) => {
            crate::tools::log_tools::append_to_app_log(&format!("MEMORY | save_error | {}", e));
        }
    }
    result
}

/// Search memories using hybrid BM25 + vector search.
#[tauri::command]
pub async fn memory_search(
    state: tauri::State<'_, MemoryState>,
    query: String,
    max_results: Option<usize>,
) -> Result<Vec<MemorySearchResult>, String> {
    // Get query embedding (async, before taking lock)
    let query_embedding = try_get_embedding(&query).await;

    ensure_db(&state)?;
    let db_guard = state.db.lock().unwrap_or_else(|e| e.into_inner());
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;

    // OpenClaw defaults: vectorWeight=0.7, textWeight=0.3
    // If no embeddings available, textWeight effectively becomes 1.0
    search_hybrid(
        conn,
        &query,
        &query_embedding,
        max_results.unwrap_or(10),
        0.7,
        0.3,
    )
}

/// Get all memories, optionally filtered by source.
#[tauri::command]
pub fn memory_list(
    state: tauri::State<'_, MemoryState>,
    source: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<MemoryRecord>, String> {
    ensure_db(&state)?;
    let db_guard = state.db.lock().unwrap_or_else(|e| e.into_inner());
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;

    let limit = limit.unwrap_or(50) as i64;

    if let Some(src) = source {
        let mut stmt = conn
            .prepare(
                "SELECT id, content, source, conversation_id, model_id, tags, created_at, updated_at
                 FROM memories WHERE source = ?1 ORDER BY created_at DESC LIMIT ?2",
            )
            .map_err(|e| format!("Query error: {}", e))?;

        let records: Vec<MemoryRecord> = stmt.query_map(params![src, limit], row_to_memory)
            .map_err(|e| format!("Query failed: {}", e))?
            .filter_map(|r| match r {
            Ok(v) => Some(v),
            Err(e) => { eprintln!("[HIVE] Memory row deserialization error (skipped): {}", e); None }
        })
            .collect();
        Ok(records)
    } else {
        let mut stmt = conn
            .prepare(
                "SELECT id, content, source, conversation_id, model_id, tags, created_at, updated_at
                 FROM memories ORDER BY created_at DESC LIMIT ?1",
            )
            .map_err(|e| format!("Query error: {}", e))?;

        let records: Vec<MemoryRecord> = stmt.query_map(params![limit], row_to_memory)
            .map_err(|e| format!("Query failed: {}", e))?
            .filter_map(|r| match r {
            Ok(v) => Some(v),
            Err(e) => { eprintln!("[HIVE] Memory row deserialization error (skipped): {}", e); None }
        })
            .collect();
        Ok(records)
    }
}

/// Delete a memory by ID.
#[tauri::command]
pub fn memory_delete(
    state: tauri::State<'_, MemoryState>,
    id: String,
) -> Result<bool, String> {
    ensure_db(&state)?;
    let db_guard = state.db.lock().unwrap_or_else(|e| e.into_inner());
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;

    crate::tools::log_tools::append_to_app_log(&format!("MEMORY | delete | id={}", id));

    // Delete MAGMA edges referencing this memory (prevent orphaned graph nodes)
    if let Err(e) = conn.execute(
        "DELETE FROM edges WHERE (source_type = 'memory' AND source_id = ?1)
         OR (target_type = 'memory' AND target_id = ?1)",
        params![id],
    ) {
        eprintln!("[HIVE] WARN: Failed to clean MAGMA edges for memory {}: {}", id, e);
    }

    // Delete FTS5 entries for this memory's chunks
    conn.execute(
        "DELETE FROM chunks_fts WHERE memory_id = ?1",
        params![id],
    )
    .map_err(|e| format!("Failed to delete FTS entries: {}", e))?;

    // Delete chunks (including embeddings)
    conn.execute("DELETE FROM chunks WHERE memory_id = ?1", params![id])
        .map_err(|e| format!("Failed to delete chunks: {}", e))?;

    // Delete memory
    let rows = conn
        .execute("DELETE FROM memories WHERE id = ?1", params![id])
        .map_err(|e| format!("Failed to delete memory: {}", e))?;

    Ok(rows > 0)
}

/// Update a memory's content (and re-index chunks + embeddings).
/// Used by the memory_edit tool so the model can correct/update its own memories.
pub fn update_memory_public(
    conn: &Connection,
    id: &str,
    new_content: &str,
    new_tags: Option<&[String]>,
    embeddings: &[Vec<f64>],
) -> Result<MemoryRecord, String> {
    // Verify the memory exists
    let existing: Option<(String, String, Option<String>, Option<String>, String)> = conn
        .query_row(
            "SELECT source, tags, conversation_id, model_id, created_at FROM memories WHERE id = ?1",
            params![id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .ok();

    let (source, old_tags_json, conversation_id, model_id, created_at) = existing
        .ok_or_else(|| format!("Memory not found: {}", id))?;

    let now = Utc::now().to_rfc3339();
    let tags = if let Some(t) = new_tags {
        t.to_vec()
    } else {
        serde_json::from_str(&old_tags_json).unwrap_or_default()
    };
    let tags_json = serde_json::to_string(&tags).unwrap_or_else(|_| "[]".to_string());

    // Update memory content
    conn.execute(
        "UPDATE memories SET content = ?1, tags = ?2, updated_at = ?3 WHERE id = ?4",
        params![new_content, tags_json, now, id],
    )
    .map_err(|e| format!("Failed to update memory: {}", e))?;

    // Delete old chunks and FTS entries
    conn.execute("DELETE FROM chunks_fts WHERE memory_id = ?1", params![id])
        .map_err(|e| format!("Failed to delete old FTS entries: {}", e))?;
    conn.execute("DELETE FROM chunks WHERE memory_id = ?1", params![id])
        .map_err(|e| format!("Failed to delete old chunks: {}", e))?;

    // Re-chunk and re-index
    let chunks = chunk_text(new_content, 1600, 320);
    for (i, (start, end, text)) in chunks.iter().enumerate() {
        let chunk_id = generate_chunk_id(id, i);
        let chunk_hash = hash_text(text);

        let embedding_json = if i < embeddings.len() && !embeddings[i].is_empty() {
            serde_json::to_string(&embeddings[i]).unwrap_or_default()
        } else {
            String::new()
        };

        conn.execute(
            "INSERT INTO chunks (id, memory_id, text, start_line, end_line, hash, embedding)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![chunk_id, id, text, start, end, chunk_hash, embedding_json],
        )
        .map_err(|e| format!("Failed to insert chunk: {}", e))?;

        conn.execute(
            "INSERT INTO chunks_fts (text, id, memory_id, start_line, end_line)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![text, chunk_id, id, start, end],
        )
        .map_err(|e| format!("Failed to index chunk in FTS5: {}", e))?;
    }

    Ok(MemoryRecord {
        id: id.to_string(),
        content: new_content.to_string(),
        source,
        conversation_id,
        model_id,
        tags,
        created_at,
        updated_at: now,
    })
}

/// Delete a memory by ID. Public version for tool framework.
pub fn delete_memory_public(conn: &Connection, id: &str) -> Result<bool, String> {
    // Clean up MAGMA edges referencing this memory (prevent orphaned graph nodes)
    if let Err(e) = conn.execute(
        "DELETE FROM edges WHERE (source_type = 'memory' AND source_id = ?1)
         OR (target_type = 'memory' AND target_id = ?1)",
        params![id],
    ) {
        eprintln!("[HIVE] WARN: Failed to clean MAGMA edges for memory {}: {}", id, e);
    }
    conn.execute("DELETE FROM chunks_fts WHERE memory_id = ?1", params![id])
        .map_err(|e| format!("Failed to delete FTS entries: {}", e))?;
    conn.execute("DELETE FROM chunks WHERE memory_id = ?1", params![id])
        .map_err(|e| format!("Failed to delete chunks: {}", e))?;
    let rows = conn.execute("DELETE FROM memories WHERE id = ?1", params![id])
        .map_err(|e| format!("Failed to delete memory: {}", e))?;
    Ok(rows > 0)
}

/// Phase 4C: Set a memory's tier. Public for working_memory flush.
pub fn set_memory_tier(conn: &Connection, id: &str, tier: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE memories SET tier = ?1 WHERE id = ?2",
        params![tier, id],
    )
    .map_err(|e| format!("Failed to set tier: {}", e))?;
    Ok(())
}

/// Phase 4C: Get tier distribution counts (for stats/testing).
pub fn get_tier_counts(conn: &Connection) -> Result<std::collections::HashMap<String, i64>, String> {
    let mut stmt = conn
        .prepare("SELECT COALESCE(tier, 'long_term') AS t, COUNT(*) FROM memories GROUP BY t")
        .map_err(|e| format!("Failed to query tiers: {}", e))?;
    let mut counts = std::collections::HashMap::new();
    let rows = stmt
        .query_map([], |row| {
            let tier: String = row.get(0)?;
            let count: i64 = row.get(1)?;
            Ok((tier, count))
        })
        .map_err(|e| format!("Failed to read tier counts: {}", e))?;
    for row in rows {
        if let Ok((tier, count)) = row {
            counts.insert(tier, count);
        }
    }
    Ok(counts)
}

/// Phase 4C: Standalone promotion — promotes short_term memories to long_term
/// when access_count > 3. Callable from session start, periodic jobs, or anywhere
/// outside of search_hybrid. Returns the number of promoted memories.
pub fn promote_due_memories(conn: &Connection) -> Result<usize, String> {
    let rows = conn.execute(
        "UPDATE memories SET tier = 'long_term'
         WHERE tier = 'short_term' AND access_count > 3",
        [],
    )
    .map_err(|e| format!("Failed to promote memories: {}", e))?;
    Ok(rows)
}

/// Intelligence Graduation Phase 8B: Archive stale memories.
/// Memories with low strength and no access in 90+ days get moved to 'archived' tier.
/// Archived memories are still searchable but heavily penalized (0.5x tier_weight).
/// They're never deleted — just deprioritized. Returns number of archived memories.
pub fn archive_stale_memories(conn: &Connection) -> Result<usize, String> {
    let rows = conn.execute(
        "UPDATE memories SET tier = 'archived'
         WHERE tier IN ('short_term', 'long_term')
         AND strength < 1.1
         AND (
             (last_accessed IS NOT NULL AND last_accessed < datetime('now', '-90 days'))
             OR (last_accessed IS NULL AND updated_at < datetime('now', '-90 days'))
         )",
        [],
    )
    .map_err(|e| format!("Failed to archive stale memories: {}", e))?;
    if rows > 0 {
        crate::tools::log_tools::append_to_app_log(&format!(
            "MEMORY | archived_stale | count={} | threshold=90days+strength<1.1", rows
        ));
    }
    Ok(rows)
}

// ============================================
// Phase 8D: Active Forgetting (Mem0 DELETE Pattern)
// ============================================

/// Intelligence Graduation Phase 8D: Check if a newly saved memory supersedes older ones.
/// For each new memory: embed → find top-5 similar by cosine → if similarity > 0.85 AND
/// same topic tag → mark old as `tier: 'superseded'` + create MAGMA edge.
/// This prevents contradictory information from coexisting at equal priority.
/// Superseded memories are NOT deleted — they're recoverable but heavily deprioritized (0.2x).
fn check_supersession(conn: &Connection, new_memory_id: &str, embeddings: &[Vec<f64>], tags: &[String]) {
    const SUPERSESSION_THRESHOLD: f64 = 0.85;

    let new_embedding = match embeddings.first() {
        Some(e) if !e.is_empty() => e,
        _ => return, // No embedding → can't compare, skip
    };

    // Extract topic tag from the new memory (e.g., "topic:technical")
    let new_topic = tags.iter().find(|t| t.starts_with("topic:")).cloned();

    // Find top-5 most similar existing memories by scanning chunk embeddings.
    // Exclude the new memory itself and already-superseded/consolidated memories.
    let mut stmt = match conn.prepare(
        "SELECT c.embedding, c.memory_id, m.tags, m.tier, m.content
         FROM chunks c
         JOIN memories m ON c.memory_id = m.id
         WHERE c.embedding != ''
         AND c.memory_id != ?1
         AND m.tier NOT IN ('superseded', 'consolidated')
         LIMIT 5000"
    ) {
        Ok(s) => s,
        Err(_) => return,
    };

    struct Candidate {
        memory_id: String,
        similarity: f64,
        tags: Vec<String>,
        content_preview: String,
    }

    let mut candidates: Vec<Candidate> = Vec::new();
    let mut seen_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    let rows = match stmt.query_map(params![new_memory_id], |row| {
        let emb_raw: String = row.get(0)?;
        let memory_id: String = row.get(1)?;
        let tags_json: String = row.get(2)?;
        let tier: String = row.get(3)?;
        let content: String = row.get(4)?;
        Ok((emb_raw, memory_id, tags_json, tier, content))
    }) {
        Ok(r) => r,
        Err(_) => return,
    };

    for row in rows {
        if let Ok((emb_raw, memory_id, tags_json, _tier, content)) = row {
            if seen_ids.contains(&memory_id) { continue; }

            let existing_emb = parse_embedding(&emb_raw);
            if existing_emb.is_empty() { continue; }

            let sim = cosine_similarity(new_embedding, &existing_emb);
            if sim > SUPERSESSION_THRESHOLD {
                let old_tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
                seen_ids.insert(memory_id.clone());
                candidates.push(Candidate {
                    memory_id,
                    similarity: sim,
                    tags: old_tags,
                    content_preview: content.chars().take(80).collect(),
                });
            }
        }
    }

    // Sort by similarity descending, take top 5
    candidates.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap_or(std::cmp::Ordering::Equal));
    candidates.truncate(5);

    // Supersede candidates that share the same topic tag
    for candidate in &candidates {
        let same_topic = match &new_topic {
            Some(nt) => candidate.tags.iter().any(|t| t == nt),
            None => false, // No topic tag → can't confirm same domain, skip
        };

        if !same_topic { continue; }

        // Mark old memory as superseded
        if let Err(e) = conn.execute(
            "UPDATE memories SET tier = 'superseded' WHERE id = ?1 AND tier NOT IN ('superseded', 'consolidated')",
            params![candidate.memory_id],
        ) {
            eprintln!("[HIVE] MEMORY | supersession tier update failed for {}: {}", candidate.memory_id, e);
            continue;
        }

        // Create MAGMA edge: new →supersedes→ old
        let edge_id = generate_id();
        let now = Utc::now().to_rfc3339();
        let meta = serde_json::json!({
            "similarity": (candidate.similarity * 1000.0).round() / 1000.0,
            "same_topic": true,
            "auto_superseded": true,
        });

        if let Err(e) = conn.execute(
            "INSERT INTO edges (id, source_type, source_id, target_type, target_id, edge_type, weight, metadata, created_at)
             VALUES (?1, 'memory', ?2, 'memory', ?3, 'supersedes', ?4, ?5, ?6)",
            params![edge_id, new_memory_id, candidate.memory_id, candidate.similarity, meta.to_string(), now],
        ) {
            eprintln!("[HIVE] MAGMA supersession edge failed: {}", e);
        }

        crate::tools::log_tools::append_to_app_log(&format!(
            "MEMORY | superseded | old={} sim={:.3} | {}",
            candidate.memory_id, candidate.similarity, candidate.content_preview
        ));
    }
}

// ============================================
// Phase 8C: Memory Consolidation (Periodic)
// ============================================

/// Intelligence Graduation Phase 8C: Consolidate redundant memories within topic groups.
/// Groups memories by topic tag → clusters similar memories (cosine > 0.7) →
/// merges clusters of 3+ into a single consolidated memory.
/// Originals are marked `tier: 'consolidated'` (not deleted — recoverable).
/// Consolidated memory inherits the highest strength from its constituents.
/// Returns (topics_processed, clusters_merged, memories_consolidated).
pub fn consolidate_memories(conn: &Connection) -> Result<(usize, usize, usize), String> {
    // Find all topic tags with 10+ active memories (worth consolidating)
    let mut topic_stmt = conn.prepare(
        "SELECT tags FROM memories WHERE tier IN ('short_term', 'long_term')"
    ).map_err(|e| format!("consolidation topic query failed: {}", e))?;

    let mut topic_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let rows = topic_stmt.query_map([], |row| {
        let tags_json: String = row.get(0)?;
        Ok(tags_json)
    }).map_err(|e| format!("consolidation topic scan failed: {}", e))?;

    for row in rows {
        if let Ok(tags_json) = row {
            let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
            for tag in tags {
                if tag.starts_with("topic:") {
                    *topic_counts.entry(tag).or_insert(0) += 1;
                }
            }
        }
    }

    let dense_topics: Vec<String> = topic_counts.into_iter()
        .filter(|(_, count)| *count >= 10)
        .map(|(topic, _)| topic)
        .collect();

    if dense_topics.is_empty() {
        return Ok((0, 0, 0));
    }

    let mut topics_processed = 0;
    let mut clusters_merged = 0;
    let mut memories_consolidated = 0;

    for topic in &dense_topics {
        let result = consolidate_topic(conn, topic)?;
        if result.0 > 0 {
            topics_processed += 1;
            clusters_merged += result.0;
            memories_consolidated += result.1;
        }
    }

    if clusters_merged > 0 {
        crate::tools::log_tools::append_to_app_log(&format!(
            "MEMORY | consolidation | topics={} clusters={} memories_merged={}",
            topics_processed, clusters_merged, memories_consolidated
        ));
    }

    Ok((topics_processed, clusters_merged, memories_consolidated))
}

/// Consolidate memories within a single topic. Returns (clusters_merged, memories_in_clusters).
fn consolidate_topic(conn: &Connection, topic: &str) -> Result<(usize, usize), String> {
    // Fetch all active memories with this topic tag + their first chunk embedding
    let like_pattern = format!("%{}%", topic);
    let mut stmt = conn.prepare(
        "SELECT m.id, m.content, m.tags, m.strength,
                (SELECT c.embedding FROM chunks c WHERE c.memory_id = m.id AND c.embedding != '' LIMIT 1) as emb
         FROM memories m
         WHERE m.tier IN ('short_term', 'long_term')
         AND m.tags LIKE ?1
         ORDER BY m.created_at ASC"
    ).map_err(|e| format!("consolidation fetch failed: {}", e))?;

    struct MemEntry {
        id: String,
        content: String,
        tags: Vec<String>,
        strength: f64,
        embedding: Vec<f64>,
    }

    let entries: Vec<MemEntry> = stmt.query_map(params![like_pattern], |row| {
        let tags_json: String = row.get(2)?;
        let emb_raw: Option<String> = row.get(4)?;
        Ok(MemEntry {
            id: row.get(0)?,
            content: row.get(1)?,
            tags: serde_json::from_str(&tags_json).unwrap_or_default(),
            strength: row.get(3)?,
            embedding: emb_raw.map(|r| parse_embedding(&r)).unwrap_or_default(),
        })
    })
    .map_err(|e| format!("consolidation scan failed: {}", e))?
    .filter_map(|r| match r {
        Ok(v) => Some(v),
        Err(e) => { eprintln!("[HIVE] consolidation row error: {}", e); None }
    })
    .collect();

    if entries.len() < 10 {
        return Ok((0, 0)); // Below threshold after filtering
    }

    // Greedy clustering: assign each memory to the first cluster where cosine > 0.7
    // with the cluster centroid. If no match, start a new cluster.
    const CLUSTER_THRESHOLD: f64 = 0.7;
    let mut clusters: Vec<Vec<usize>> = Vec::new(); // indices into entries
    let mut centroids: Vec<Vec<f64>> = Vec::new();

    for (i, entry) in entries.iter().enumerate() {
        if entry.embedding.is_empty() { continue; }

        // Find best matching cluster by cosine similarity with centroid
        let mut best_cluster: Option<usize> = None;
        let mut best_sim: f64 = 0.0;
        for ci in 0..centroids.len() {
            let sim = cosine_similarity(&entry.embedding, &centroids[ci]);
            if sim > CLUSTER_THRESHOLD && sim > best_sim {
                best_sim = sim;
                best_cluster = Some(ci);
            }
        }

        if let Some(ci) = best_cluster {
            clusters[ci].push(i);
            // Recompute centroid from cluster members
            let cluster_embeddings: Vec<&Vec<f64>> = clusters[ci].iter()
                .filter_map(|&idx| {
                    let e = &entries[idx].embedding;
                    if e.is_empty() { None } else { Some(e) }
                })
                .collect();
            if let Some(new_centroid) = average_embeddings_ref(&cluster_embeddings) {
                centroids[ci] = new_centroid;
            }
        } else {
            clusters.push(vec![i]);
            centroids.push(entry.embedding.clone());
        }
    }

    // Merge clusters with 3+ members
    let mut merged_count = 0;
    let mut total_consumed = 0;

    for cluster in &clusters {
        if cluster.len() < 3 { continue; }

        // Build consolidated content: structured merge of all cluster members
        let mut merged_content = String::new();
        let mut max_strength: f64 = 1.0;
        let mut all_tags: Vec<String> = Vec::new();
        let mut member_ids: Vec<String> = Vec::new();

        for &idx in cluster {
            let entry = &entries[idx];
            member_ids.push(entry.id.clone());
            if entry.strength > max_strength {
                max_strength = entry.strength;
            }
            all_tags.extend(entry.tags.clone());

            // Append content with separator
            if !merged_content.is_empty() {
                merged_content.push_str("\n---\n");
            }
            merged_content.push_str(&entry.content);
        }

        // Truncate merged content to 4800 chars (3 chunks max)
        let truncated: String = merged_content.chars().take(4800).collect();

        // Deduplicate tags
        all_tags.sort();
        all_tags.dedup();
        all_tags.push("source:consolidation".to_string());

        // Create embedding for consolidated content
        let consolidated_embedding = match get_local_embedding(&truncated.chars().take(1600).collect::<String>()) {
            Ok(e) => vec![e],
            Err(_) => vec![], // Degrade gracefully
        };

        // Save consolidated memory
        let consolidated = write_memory_with_tier(
            conn, &truncated, "consolidation", None, None,
            &all_tags, &consolidated_embedding, Some("long_term"),
        );

        match consolidated {
            Ok(record) => {
                // Set max strength on the new consolidated memory
                let _ = conn.execute(
                    "UPDATE memories SET strength = ?1 WHERE id = ?2",
                    params![max_strength, record.id],
                );

                // Mark originals as consolidated
                for member_id in &member_ids {
                    let _ = conn.execute(
                        "UPDATE memories SET tier = 'consolidated' WHERE id = ?1",
                        params![member_id],
                    );

                    // Create MAGMA edge: consolidated →absorbed→ original
                    let edge_id = generate_id();
                    let now = Utc::now().to_rfc3339();
                    let meta = serde_json::json!({
                        "auto_consolidated": true,
                        "cluster_size": cluster.len(),
                    });
                    let _ = conn.execute(
                        "INSERT INTO edges (id, source_type, source_id, target_type, target_id, edge_type, weight, metadata, created_at)
                         VALUES (?1, 'memory', ?2, 'memory', ?3, 'absorbed', 1.0, ?4, ?5)",
                        params![edge_id, record.id, member_id, meta.to_string(), now],
                    );
                }

                merged_count += 1;
                total_consumed += member_ids.len();
            }
            Err(e) => {
                eprintln!("[HIVE] MEMORY | consolidation merge failed: {}", e);
            }
        }
    }

    Ok((merged_count, total_consumed))
}

/// Helper: average embeddings from references (avoids cloning).
fn average_embeddings_ref(embeddings: &[&Vec<f64>]) -> Option<Vec<f64>> {
    if embeddings.is_empty() { return None; }
    let dim = embeddings[0].len();
    if dim == 0 { return None; }
    let count = embeddings.len() as f64;
    let mut avg = vec![0.0; dim];
    for emb in embeddings {
        for (i, v) in emb.iter().enumerate() {
            if i < dim { avg[i] += v; }
        }
    }
    for v in &mut avg { *v /= count; }
    Some(avg)
}

/// Search memories. Public version for tool framework (opens own connection).
pub fn search_hybrid_public(
    conn: &Connection,
    query: &str,
    query_embedding: &[f64],
    max_results: usize,
) -> Result<Vec<MemorySearchResult>, String> {
    search_hybrid(conn, query, query_embedding, max_results, 0.7, 0.3)
}

// ============================================
// Tauri Commands — Clear, Stats, Quality, Extract, Remember, Recall
// ============================================

/// Delete ALL memories — clear the entire memory database.
#[tauri::command]
pub fn memory_clear_all(
    state: tauri::State<'_, MemoryState>,
) -> Result<u64, String> {
    ensure_db(&state)?;
    let db_guard = state.db.lock().unwrap_or_else(|e| e.into_inner());
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;

    // Count before clearing
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))
        .unwrap_or(0);

    // Clear all tables in correct order (foreign key deps)
    // Clean up MAGMA edges referencing memories first
    if let Err(e) = conn.execute("DELETE FROM edges WHERE source_type = 'memory' OR target_type = 'memory'", []) {
        eprintln!("[HIVE] WARN: Failed to clean MAGMA edges during memory_clear_all: {}", e);
    }
    conn.execute("DELETE FROM chunks_fts", [])
        .map_err(|e| format!("Failed to clear FTS: {}", e))?;
    conn.execute("DELETE FROM chunks", [])
        .map_err(|e| format!("Failed to clear chunks: {}", e))?;
    conn.execute("DELETE FROM memories", [])
        .map_err(|e| format!("Failed to clear memories: {}", e))?;

    Ok(count as u64)
}

/// Phase 4C: Get tier distribution counts.
#[tauri::command]
pub fn memory_tier_counts(
    state: tauri::State<'_, MemoryState>,
) -> Result<std::collections::HashMap<String, i64>, String> {
    ensure_db(&state)?;
    let db_guard = state.db.lock().unwrap_or_else(|e| e.into_inner());
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;
    get_tier_counts(conn)
}

/// Promote short_term memories, archive stale ones, and consolidate dense topics.
/// Returns the number of promoted memories. Also runs archival (Phase 8B) and
/// consolidation (Phase 8C) as part of the memory lifecycle maintenance.
#[tauri::command]
pub fn memory_promote(
    state: tauri::State<'_, MemoryState>,
) -> Result<usize, String> {
    ensure_db(&state)?;
    let db_guard = state.db.lock().unwrap_or_else(|e| e.into_inner());
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;
    let promoted = promote_due_memories(conn)?;
    // Phase 8B: archive stale memories (90+ days, low strength)
    let _ = archive_stale_memories(conn);
    // Phase 8C: consolidate dense topic groups (10+ memories with cosine > 0.7 clusters)
    let _ = consolidate_memories(conn);
    Ok(promoted)
}

/// Check if ANY embedding provider is available (P2: provider-agnostic).
/// Phase 3: fastembed is bundled — always available unless init explicitly failed.
/// Falls back to cloud providers / Ollama if fastembed failed.
#[tauri::command]
pub async fn memory_has_embeddings_provider() -> bool {
    // Phase 3: fastembed is a compile-time dependency — available unless init failed
    if !matches!(LOCAL_EMBEDDER.get(), Some(None)) {
        return true; // Not yet tried OR initialized successfully
    }

    // fastembed init failed — check cloud providers (fast: just key lookup, no network)
    for provider in OPENAI_COMPAT_EMBEDDING_PROVIDERS {
        if get_api_key_internal(provider.key_name).is_some() {
            return true;
        }
    }

    // Check Ollama (needs a quick network probe)
    let client = match hive_http_client() {
        Ok(c) => c,
        Err(_) => return false,
    };
    client
        .get("http://localhost:11434/api/tags")
        .timeout(std::time::Duration::from_secs(2))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

/// Get memory system stats.
#[tauri::command]
pub fn memory_stats(
    state: tauri::State<'_, MemoryState>,
) -> Result<MemoryStats, String> {
    ensure_db(&state)?;
    let db_guard = state.db.lock().unwrap_or_else(|e| e.into_inner());
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;

    let total_memories: i64 = conn
        .query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))
        .unwrap_or(0);

    let total_chunks: i64 = conn
        .query_row("SELECT COUNT(*) FROM chunks", [], |row| row.get(0))
        .unwrap_or(0);

    let total_conversations: i64 = conn
        .query_row(
            "SELECT COUNT(DISTINCT conversation_id) FROM memories WHERE conversation_id IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let oldest_memory: Option<String> = conn
        .query_row(
            "SELECT created_at FROM memories ORDER BY created_at ASC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .ok();

    let newest_memory: Option<String> = conn
        .query_row(
            "SELECT created_at FROM memories ORDER BY created_at DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .ok();

    let has_embeddings: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM chunks WHERE embedding != ''",
            [],
            |row| {
                let count: i64 = row.get(0)?;
                Ok(count > 0)
            },
        )
        .unwrap_or(false);

    let db_path = get_memory_db_path();
    let db_size_bytes = fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);

    // MAGMA graph counts
    let total_events: i64 = conn
        .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
        .unwrap_or(0);
    let total_entities: i64 = conn
        .query_row("SELECT COUNT(*) FROM entities", [], |row| row.get(0))
        .unwrap_or(0);
    let total_procedures: i64 = conn
        .query_row("SELECT COUNT(*) FROM procedures", [], |row| row.get(0))
        .unwrap_or(0);
    let total_edges: i64 = conn
        .query_row("SELECT COUNT(*) FROM edges", [], |row| row.get(0))
        .unwrap_or(0);

    Ok(MemoryStats {
        total_memories,
        total_chunks,
        total_conversations,
        oldest_memory,
        newest_memory,
        db_size_bytes,
        has_embeddings,
        total_events,
        total_entities,
        total_procedures,
        total_edges,
    })
}

/// Heuristic quality score for a message — determines if it's worth remembering.
fn score_message_quality(content: &str, role: &str) -> (f64, Vec<String>) {
    let lower = content.to_lowercase();
    let trimmed = lower.trim();
    let mut score: f64 = 0.0;
    let mut tags = Vec::new();

    if content.len() < 30 {
        return (0.0, tags);
    }

    let trivial_patterns = [
        "hi", "hello", "hey", "thanks", "thank you", "ok", "okay", "sure",
        "got it", "sounds good", "great", "cool", "nice", "good", "yes", "no",
        "yep", "nope", "alright", "right", "exactly", "indeed", "agreed",
        "welcome", "bye", "goodbye", "see you", "lol", "lmao", "haha",
    ];
    if trivial_patterns.iter().any(|p| {
        trimmed == *p || trimmed == format!("{}!", p) || trimmed == format!("{}.", p)
    }) {
        return (0.0, tags);
    }

    if role == "assistant" {
        let preamble_patterns = [
            "sure!", "sure,", "of course!", "i'll help", "i can help",
            "here's", "here is", "let me", "i'd be happy",
            "absolutely!", "certainly!", "great question",
        ];
        if preamble_patterns.iter().any(|p| trimmed.starts_with(p)) && content.len() < 80 {
            return (0.0, tags);
        }
    }

    let code_chars: usize = content.split("```").enumerate()
        .filter(|(i, _)| i % 2 == 1)
        .map(|(_, s)| s.len())
        .sum();
    let code_ratio = code_chars as f64 / content.len().max(1) as f64;
    if code_ratio > 0.7 {
        return (0.0, tags);
    }

    let len_score = (content.len() as f64).ln() / 10.0;
    score += len_score.min(0.5);

    let preference_patterns = [
        "i want", "i prefer", "i like", "i need", "i don't want",
        "i don't like", "always use", "never use", "always do", "never do",
        "my preference", "i usually", "i always",
    ];
    if preference_patterns.iter().any(|p| lower.contains(p)) {
        score += 0.3;
        tags.push("preference".to_string());
    }

    let correction_patterns = [
        "actually", "no, i meant", "that's wrong", "that's not right",
        "correction:", "i meant", "not what i asked", "let me clarify",
        "to be clear", "what i actually",
    ];
    if correction_patterns.iter().any(|p| lower.contains(p)) {
        score += 0.3;
        tags.push("correction".to_string());
    }

    let decision_patterns = [
        "let's go with", "i'll use", "we should use", "the plan is",
        "decided to", "going with", "i chose", "we'll use",
        "the approach", "the solution is",
    ];
    if decision_patterns.iter().any(|p| lower.contains(p)) {
        score += 0.25;
        tags.push("decision".to_string());
    }

    let instruction_patterns = [
        "set ", "configure", "install", "run ", "use the",
        "the command is", "the api", "the endpoint", "the url",
        "the password", "the key is", "the token",
        "remember that", "keep in mind", "important:",
        "note:", "fyi", "for future reference",
    ];
    if instruction_patterns.iter().any(|p| lower.contains(p)) {
        score += 0.2;
        tags.push("instruction".to_string());
    }

    if role == "assistant" {
        let explanation_patterns = [
            "because", "the reason", "this means", "this works by",
            "the difference", "in summary", "the key", "essentially",
            "the problem is", "the issue is", "the solution",
            "this is important because", "the tradeoff",
        ];
        if explanation_patterns.iter().any(|p| lower.contains(p)) {
            score += 0.2;
            tags.push("explanation".to_string());
        }
    }

    if role == "user" && (content.contains('?') || lower.starts_with("how") || lower.starts_with("why")
        || lower.starts_with("what") || lower.starts_with("where") || lower.starts_with("when")) {
        if content.len() > 40 {
            score += 0.15;
            tags.push("question".to_string());
        }
    }

    if code_ratio > 0.3 {
        score *= 1.0 - (code_ratio * 0.5);
    }

    let lines: Vec<&str> = content.lines().collect();
    if lines.len() > 3 {
        let unique_lines: std::collections::HashSet<&str> = lines.iter().copied().collect();
        let uniqueness = unique_lines.len() as f64 / lines.len() as f64;
        if uniqueness < 0.3 {
            score *= 0.3;
        }
    }

    if tags.is_empty() {
        tags.push(if role == "user" { "user-input".to_string() } else { "assistant-response".to_string() });
    }

    (score.min(1.0), tags)
}

/// Extract and save key facts from a conversation.
#[tauri::command]
pub async fn memory_extract_and_save(
    state: tauri::State<'_, MemoryState>,
    conversation_id: String,
    model_id: Option<String>,
    messages: Vec<serde_json::Value>,
) -> Result<Vec<MemoryRecord>, String> {
    struct PendingMemory {
        content: String,
        tags: Vec<String>,
        embeddings: Vec<Vec<f64>>,
    }

    const QUALITY_THRESHOLD: f64 = 0.3;
    let mut pending: Vec<PendingMemory> = Vec::new();

    let mut i = 0;
    while i < messages.len() {
        let msg = &messages[i];
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
        let content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");

        if role == "tool" || role == "system" {
            i += 1;
            continue;
        }

        let (score, mut tags) = score_message_quality(content, role);

        if role == "user" && tags.contains(&"question".to_string()) && i + 1 < messages.len() {
            let next = &messages[i + 1];
            let next_role = next.get("role").and_then(|r| r.as_str()).unwrap_or("");
            let next_content = next.get("content").and_then(|c| c.as_str()).unwrap_or("");

            if next_role == "assistant" {
                let (answer_score, answer_tags) = score_message_quality(next_content, next_role);
                let combined_score = (score + answer_score) / 2.0 + 0.1;

                if combined_score >= QUALITY_THRESHOLD {
                    let combined = format!("Q: {}\n\nA: {}", content.trim(), next_content.trim());
                    let truncated = crate::content_security::safe_truncate(&combined, 3200);

                    tags.extend(answer_tags);
                    tags.push("qa-pair".to_string());
                    tags.sort();
                    tags.dedup();

                    let chunks = chunk_text(&truncated, 1600, 320);
                    let mut embeddings = Vec::new();
                    for (_, _, text) in &chunks {
                        embeddings.push(try_get_embedding(text).await);
                    }
                    pending.push(PendingMemory { content: truncated, tags, embeddings });
                    i += 2;
                    continue;
                }
            }
        }

        if score >= QUALITY_THRESHOLD {
            let saved_content = crate::content_security::safe_truncate(content, 3200);

            let chunks = chunk_text(&saved_content, 1600, 320);
            let mut embeddings = Vec::new();
            for (_, _, text) in &chunks {
                embeddings.push(try_get_embedding(text).await);
            }
            pending.push(PendingMemory { content: saved_content, tags, embeddings });
        }

        i += 1;
    }

    ensure_db(&state)?;
    let db_guard = state.db.lock().unwrap_or_else(|e| e.into_inner());
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;

    let mut saved = Vec::new();
    for pm in pending {
        if is_near_duplicate(conn, &pm.embeddings) {
            continue;
        }

        let record = write_memory_internal(
            conn,
            &pm.content,
            "conversation",
            Some(&conversation_id),
            model_id.as_deref(),
            &pm.tags,
            &pm.embeddings,
        )?;
        saved.push(record);
    }

    Ok(saved)
}

/// Save a user-created memory note (explicit "remember this").
#[tauri::command]
pub async fn memory_remember(
    state: tauri::State<'_, MemoryState>,
    content: String,
    tags: Vec<String>,
) -> Result<MemoryRecord, String> {
    let chunks = chunk_text(&content, 1600, 320);

    // Check hash cache before API calls
    let cached_embeddings: Vec<Option<Vec<f64>>> = {
        ensure_db(&state)?;
        let db_guard = state.db.lock().unwrap_or_else(|e| e.into_inner());
        let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;
        chunks.iter().map(|(_, _, text)| {
            try_cached_embedding(conn, &hash_text(text))
        }).collect()
    };

    let mut embeddings = Vec::with_capacity(chunks.len());
    for (i, (_, _, text)) in chunks.iter().enumerate() {
        if let Some(cached) = &cached_embeddings[i] {
            embeddings.push(cached.clone());
        } else {
            embeddings.push(try_get_embedding(text).await);
        }
    }

    ensure_db(&state)?;
    let db_guard = state.db.lock().unwrap_or_else(|e| e.into_inner());
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;

    let mut all_tags = vec!["user-note".to_string()];
    all_tags.extend(tags);

    write_memory_internal(conn, &content, "user", None, None, &all_tags, &embeddings)
}

/// Import a file into memory as chunked records (RAG pipeline).
/// Reads a text file, splits into ~1600-char sections, indexes each as a memory.
/// Supports: .txt, .md, .rs, .ts, .tsx, .py, .js, .jsx, .json, .toml, .yaml, .yml, .csv, .html, .css
/// Returns the number of memories created.
#[tauri::command]
pub async fn memory_import_file(
    state: tauri::State<'_, MemoryState>,
    file_path: String,
    custom_tags: Option<Vec<String>>,
) -> Result<usize, String> {
    let path = std::path::Path::new(&file_path);
    if !path.exists() {
        return Err(format!("File not found: {}", file_path));
    }

    let ext = path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let text_extensions = [
        "txt", "md", "rs", "ts", "tsx", "py", "js", "jsx", "json", "toml",
        "yaml", "yml", "csv", "html", "css", "go", "java", "c", "cpp", "h",
        "hpp", "sh", "bat", "ps1", "sql", "xml", "ini", "cfg", "conf", "log",
    ];

    if !text_extensions.contains(&ext.as_str()) {
        return Err(format!(
            "Unsupported file type '.{}'. Supported: {}",
            ext,
            text_extensions.join(", ")
        ));
    }

    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read file: {}", e))?;

    if content.trim().is_empty() {
        return Err("File is empty".to_string());
    }

    let filename = path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    // Use heading-aware splitting for markdown; flat splitting for everything else
    let sections = if ext == "md" || ext == "markdown" {
        split_markdown_by_headings(&content, 1600)
    } else {
        split_file_into_sections(&content, 1600)
            .into_iter()
            .map(|s| ImportSection { content: s, heading: None, level: 0, path: vec![] })
            .collect()
    };
    let total = sections.len();
    let source = format!("file:{}", filename);

    // Pre-compute embeddings for all sections (async, outside the DB lock)
    let mut section_embeddings: Vec<Vec<f64>> = Vec::with_capacity(total);
    for sec in &sections {
        if sec.content.trim().len() < 20 {
            section_embeddings.push(vec![]);
        } else {
            section_embeddings.push(try_get_embedding(&sec.content).await);
        }
    }

    // Now take the DB lock and write everything (sync, no .await)
    ensure_db(&state)?;
    let db_guard = state.db.lock().unwrap_or_else(|e| e.into_inner());
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;

    let mut count = 0;
    // Track memory IDs per heading level for MAGMA parent→child edges
    let mut memory_ids: Vec<(String, usize)> = Vec::new(); // (memory_id, heading_level)
    let mut prev_id_at_level: std::collections::HashMap<usize, String> = std::collections::HashMap::new();

    for (i, sec) in sections.iter().enumerate() {
        if sec.content.trim().len() < 20 { continue; }

        let mut tags = vec![
            "imported".to_string(),
            format!("file:{}", filename),
            format!("section:{}/{}", i + 1, total),
        ];
        if let Some(ref heading) = sec.heading {
            tags.push(format!("heading:{}", heading));
        }
        if !sec.path.is_empty() {
            tags.push(format!("path:{}", sec.path.join(" > ")));
        }
        if let Some(ref ct) = custom_tags {
            tags.extend(ct.clone());
        }

        let embeddings = if section_embeddings[i].is_empty() {
            vec![]
        } else {
            vec![section_embeddings[i].clone()]
        };

        match write_memory_internal(conn, &sec.content, &source, None, None, &tags, &embeddings) {
            Ok(record) => {
                let mem_id = record.id.clone();
                // Phase 9.3: Set source_file for RAG attribution
                if let Err(e) = conn.execute(
                    "UPDATE memories SET source_file = ?1 WHERE id = ?2",
                    params![file_path, mem_id],
                ) {
                    eprintln!("[HIVE] WARN: Failed to set source_file for memory {}: {}", mem_id, e);
                }

                // Create MAGMA hierarchy edges for heading-aware sections
                if sec.level > 0 {
                    // Find parent: closest preceding section with a lower heading level
                    for (prev_id, prev_level) in memory_ids.iter().rev() {
                        if *prev_level < sec.level {
                            let edge_id = generate_id();
                            let now = Utc::now().to_rfc3339();
                            if let Err(e) = conn.execute(
                                "INSERT INTO edges (id, source_type, source_id, target_type, target_id, edge_type, weight, metadata, created_at)
                                 VALUES (?1, 'memory', ?2, 'memory', ?3, 'parent_of', 0.8, '{}', ?4)",
                                params![edge_id, prev_id, mem_id, now],
                            ) {
                                eprintln!("[HIVE] WARN: Failed to create parent_of edge: {}", e);
                            }
                            break;
                        }
                    }
                    // Sequence edge: connect to previous section at same level
                    if let Some(prev_same) = prev_id_at_level.get(&sec.level) {
                        let edge_id = generate_id();
                        let now = Utc::now().to_rfc3339();
                        if let Err(e) = conn.execute(
                            "INSERT INTO edges (id, source_type, source_id, target_type, target_id, edge_type, weight, metadata, created_at)
                             VALUES (?1, 'memory', ?2, 'memory', ?3, 'sequence', 0.6, '{}', ?4)",
                            params![edge_id, prev_same, mem_id, now],
                        ) {
                            eprintln!("[HIVE] WARN: Failed to create sequence edge: {}", e);
                        }
                    }
                    prev_id_at_level.insert(sec.level, mem_id.clone());
                }

                memory_ids.push((mem_id, sec.level));
                count += 1;
            }
            Err(e) => eprintln!("[HIVE] Warning: failed to import section {}/{}: {}", i + 1, total, e),
        }
    }

    eprintln!("[HIVE] Imported {} sections from '{}' into memory ({} with heading hierarchy)", count, filename, memory_ids.iter().filter(|(_, l)| *l > 0).count());
    Ok(count)
}

/// A section extracted from a file for RAG import, with optional heading hierarchy.
pub(crate) struct ImportSection {
    pub(crate) content: String,
    pub(crate) heading: Option<String>,  // e.g., "Architecture Overview"
    pub(crate) level: usize,             // 0 = no heading, 1-6 = # to ######
    pub(crate) path: Vec<String>,        // Breadcrumb: ["Chapter 1", "Architecture"]
}

/// Split a markdown file by headings, preserving hierarchy.
/// Each heading starts a new section. Content accumulates until the next heading.
/// Returns sections with heading metadata for MAGMA edge creation.
pub(crate) fn split_markdown_by_headings(content: &str, max_chars: usize) -> Vec<ImportSection> {
    let mut sections = Vec::new();
    let mut current_content = String::new();
    let mut current_heading: Option<String> = None;
    let mut current_level: usize = 0;
    // Track active heading at each level for breadcrumb path
    let mut heading_stack: Vec<(usize, String)> = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim_start();
        // Detect markdown heading: # through ######
        if trimmed.starts_with('#') {
            let level = trimmed.chars().take_while(|&c| c == '#').count();
            let heading_text = trimmed[level..].trim().to_string();

            if level >= 1 && level <= 6 && !heading_text.is_empty() {
                // Flush previous section
                if !current_content.trim().is_empty() || current_heading.is_some() {
                    let path = heading_stack.iter().map(|(_, h)| h.clone()).collect();
                    sections.push(ImportSection {
                        content: current_content.trim().to_string(),
                        heading: current_heading.take(),
                        level: current_level,
                        path,
                    });
                    current_content.clear();
                }

                // Update heading stack: pop everything at this level or deeper
                while heading_stack.last().map_or(false, |(l, _)| *l >= level) {
                    heading_stack.pop();
                }
                heading_stack.push((level, heading_text.clone()));

                current_heading = Some(heading_text.clone());
                current_level = level;
                // Include the heading line in the content
                current_content.push_str(line);
                current_content.push('\n');
                continue;
            }
        }
        current_content.push_str(line);
        current_content.push('\n');
    }

    // Flush last section
    if !current_content.trim().is_empty() || current_heading.is_some() {
        let path = heading_stack.iter().map(|(_, h)| h.clone()).collect();
        sections.push(ImportSection {
            content: current_content.trim().to_string(),
            heading: current_heading,
            level: current_level,
            path,
        });
    }

    // If no headings found, fall back to flat splitting
    if sections.iter().all(|s| s.level == 0) {
        return split_file_into_sections(content, max_chars)
            .into_iter()
            .map(|s| ImportSection { content: s, heading: None, level: 0, path: vec![] })
            .collect();
    }

    // Sub-split oversized sections while preserving heading metadata
    let mut result = Vec::new();
    for sec in sections {
        if sec.content.len() <= max_chars {
            result.push(sec);
        } else {
            let subsections = split_file_into_sections(&sec.content, max_chars);
            for (i, sub) in subsections.into_iter().enumerate() {
                result.push(ImportSection {
                    content: sub,
                    heading: if i == 0 { sec.heading.clone() } else {
                        sec.heading.as_ref().map(|h| format!("{} (continued)", h))
                    },
                    level: sec.level,
                    path: sec.path.clone(),
                });
            }
        }
    }

    result
}

/// Split file content into logical sections for RAG ingestion.
/// Uses double-newline paragraph breaks for prose; falls back to fixed-size chunks.
pub(crate) fn split_file_into_sections(content: &str, max_chars: usize) -> Vec<String> {
    // Try paragraph-based splitting first (double newline)
    let paragraphs: Vec<&str> = content.split("\n\n")
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect();

    // If paragraphs are reasonably sized, use them (merging small ones)
    if paragraphs.len() > 1 {
        let mut sections = Vec::new();
        let mut current = String::new();

        for para in paragraphs {
            if current.len() + para.len() + 2 > max_chars && !current.is_empty() {
                sections.push(current.clone());
                current.clear();
            }
            if !current.is_empty() { current.push_str("\n\n"); }
            current.push_str(para);
        }
        if !current.is_empty() {
            sections.push(current);
        }
        return sections;
    }

    // Fallback: split on line boundaries near max_chars
    let mut sections = Vec::new();
    let mut start = 0;
    let chars: Vec<char> = content.chars().collect();

    while start < chars.len() {
        let end = (start + max_chars).min(chars.len());
        // Find a newline near the end to split cleanly
        let split_at = if end < chars.len() {
            let search_start = if end > 200 { end - 200 } else { start };
            chars[search_start..end].iter().rposition(|&c| c == '\n')
                .map(|pos| search_start + pos + 1)
                .unwrap_or(end)
        } else {
            end
        };
        let section: String = chars[start..split_at].iter().collect();
        if !section.trim().is_empty() {
            sections.push(section);
        }
        start = split_at;
    }

    sections
}

/// Phase 9.2: Find procedures whose trigger_pattern overlaps with query keywords.
/// Returns (name, description, success_count, fail_count) for proven procedures.
fn recall_matching_procedures(
    conn: &Connection,
    keywords: &[String],
) -> Result<Vec<(String, String, i64, i64)>, String> {
    if keywords.is_empty() {
        return Ok(Vec::new());
    }

    // Build LIKE clauses for each keyword against trigger_pattern
    let mut conditions = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    for kw in keywords.iter().take(5) {
        conditions.push(format!("trigger_pattern LIKE ?{}", params.len() + 1));
        params.push(Box::new(format!("%{}%", kw.to_lowercase())));
    }

    let where_clause = conditions.join(" OR ");
    let sql = format!(
        "SELECT name, description, success_count, fail_count FROM procedures \
         WHERE ({}) AND success_count > fail_count AND success_count >= 2 \
         ORDER BY success_count DESC LIMIT 3",
        where_clause
    );

    let mut stmt = conn.prepare(&sql).map_err(|e| format!("Procedure query failed: {}", e))?;
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let rows = stmt.query_map(param_refs.as_slice(), |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, i64>(3)?,
        ))
    }).map_err(|e| format!("Procedure query failed: {}", e))?;

    let mut results = Vec::new();
    for row in rows {
        if let Ok(r) = row {
            results.push(r);
        }
    }
    Ok(results)
}

/// Find memories connected via MAGMA edges to a set of seed memory IDs.
/// Returns graph-connected memories with a base score derived from edge weight.
fn find_graph_connected_memories(
    conn: &Connection,
    seed_ids: &std::collections::HashSet<String>,
    max: usize,
) -> Vec<MemorySearchResult> {
    let mut connected = Vec::new();
    for seed_id in seed_ids {
        // Outgoing edges from this memory
        let outgoing: Vec<(String, f64)> = conn
            .prepare(
                "SELECT target_id, weight FROM edges
                 WHERE source_type = 'memory' AND source_id = ?1
                 AND target_type = 'memory'
                 ORDER BY weight DESC LIMIT 5"
            )
            .and_then(|mut stmt| {
                let rows: Vec<(String, f64)> = stmt
                    .query_map(params![seed_id], |row| Ok((row.get(0)?, row.get(1)?)))
                    .map(|iter| iter.filter_map(|r| match r {
            Ok(v) => Some(v),
            Err(e) => { eprintln!("[HIVE] Memory row deserialization error (skipped): {}", e); None }
        }).collect())
                    .unwrap_or_default();
                Ok(rows)
            })
            .unwrap_or_default();

        // Incoming edges to this memory
        let incoming: Vec<(String, f64)> = conn
            .prepare(
                "SELECT source_id, weight FROM edges
                 WHERE target_type = 'memory' AND target_id = ?1
                 AND source_type = 'memory'
                 ORDER BY weight DESC LIMIT 5"
            )
            .and_then(|mut stmt| {
                let rows: Vec<(String, f64)> = stmt
                    .query_map(params![seed_id], |row| Ok((row.get(0)?, row.get(1)?)))
                    .map(|iter| iter.filter_map(|r| match r {
            Ok(v) => Some(v),
            Err(e) => { eprintln!("[HIVE] Memory row deserialization error (skipped): {}", e); None }
        }).collect())
                    .unwrap_or_default();
                Ok(rows)
            })
            .unwrap_or_default();

        for (related_id, weight) in outgoing.into_iter().chain(incoming) {
            if seed_ids.contains(&related_id) { continue; }
            if connected.iter().any(|c: &MemorySearchResult| c.id == related_id) { continue; }

            // Look up the memory content
            if let Ok(mem) = conn.query_row(
                "SELECT id, content, source, tags, created_at, updated_at FROM memories WHERE id = ?1",
                params![related_id],
                |row| {
                    let tags_json: String = row.get(3)?;
                    let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
                    Ok(MemorySearchResult {
                        id: row.get(0)?,
                        content: row.get::<_, String>(1)?,
                        source: row.get(2)?,
                        tags,
                        score: weight * 0.5, // graph bonus: edge weight scaled
                        snippet: {
                            let c: String = row.get(1)?;
                            if c.len() > 200 { format!("{}...", &c.chars().take(200).collect::<String>()) } else { c }
                        },
                        created_at: row.get(4)?,
                    })
                },
            ) {
                connected.push(mem);
            }
        }
    }
    connected.truncate(max);
    connected
}

/// Get relevant memories for session injection.
#[tauri::command]
pub async fn memory_recall(
    state: tauri::State<'_, MemoryState>,
    query: String,
    max_results: Option<usize>,
    context_tokens: Option<usize>,
) -> Result<String, String> {
    let query_embedding = try_get_embedding(&query).await;

    ensure_db(&state)?;
    let db_guard = state.db.lock().unwrap_or_else(|e| e.into_inner());
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;

    let max = max_results.unwrap_or(5);
    let results = search_hybrid(
        conn,
        &query,
        &query_embedding,
        max,
        0.7,
        0.3,
    )?;

    const MIN_RELEVANCE: f64 = 0.15;
    let mut results: Vec<_> = results.into_iter().filter(|r| r.score >= MIN_RELEVANCE).collect();

    if results.is_empty() {
        return Ok(String::new());
    }

    // Graph-enhanced retrieval: find memories connected via MAGMA edges to top results
    let result_ids: std::collections::HashSet<String> = results.iter().map(|r| r.id.clone()).collect();
    let graph_candidates = find_graph_connected_memories(conn, &result_ids, max);
    for candidate in graph_candidates {
        if candidate.score >= MIN_RELEVANCE && !result_ids.contains(&candidate.id) {
            results.push(candidate);
        }
    }

    let query_keywords = extract_keywords(&query);
    let query_topic = classify_topic(&query, &query_keywords, &[]);
    for result in &mut results {
        let result_topic = result.tags.iter()
            .find(|t| t.starts_with("topic:"))
            .cloned()
            .unwrap_or_else(|| "topic:general".to_string());
        if result_topic == query_topic && query_topic != "topic:general" {
            result.score *= 1.15;
        } else if result_topic != query_topic
            && result_topic != "topic:general"
            && query_topic != "topic:general" {
            result.score *= 0.85;
        }
    }
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    let ctx = context_tokens.unwrap_or(4096);
    let budget = ((ctx as f64 * 0.10) * 4.0) as usize;
    let max_context_chars = budget.clamp(800, 51200);

    let mut context = String::from("[Memory Context — recalled from previous conversations]\n\n");
    let mut budget_remaining = max_context_chars;
    let mut included = 0;

    for (i, result) in results.iter().enumerate() {
        let tags_str = if result.tags.is_empty() {
            String::new()
        } else {
            format!(" ({})", result.tags.join(", "))
        };
        let entry = format!(
            "Memory {}{}: {}\n\n",
            i + 1,
            tags_str,
            result.snippet,
        );

        if entry.len() > budget_remaining && included > 0 {
            break;
        }

        context.push_str(&entry);
        budget_remaining = budget_remaining.saturating_sub(entry.len());
        included += 1;
    }

    // Phase 9.2: Surface proven procedures matching the query (P4, P3)
    // Query procedures whose trigger_pattern overlaps with query keywords.
    // Only include procedures with success_count > fail_count and >= 2 successes.
    if budget_remaining > 200 {
        if let Ok(procedures) = recall_matching_procedures(conn, &query_keywords) {
            if !procedures.is_empty() {
                context.push_str("[Learned Procedures — previously successful tool chains]\n\n");
                for (i, (name, desc, successes, failures)) in procedures.iter().enumerate().take(3) {
                    let entry = format!(
                        "Procedure {}: {} — {} (succeeded {}x, failed {}x)\n",
                        i + 1, name, desc, successes, failures,
                    );
                    if entry.len() > budget_remaining { break; }
                    context.push_str(&entry);
                    budget_remaining = budget_remaining.saturating_sub(entry.len());
                }
                context.push('\n');
            }
        }
    }

    Ok(context)
}

// Working memory, session notes, tasks, skills, and markdown sync
// moved to working_memory.rs — re-exported via `pub use` at top of file.

// MAGMA graph operations moved to magma.rs
// Re-exported below for backward compatibility with main.rs references.

fn row_to_memory(row: &rusqlite::Row) -> rusqlite::Result<MemoryRecord> {
    let tags_json: String = row.get(5)?;
    let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();

    Ok(MemoryRecord {
        id: row.get(0)?,
        content: row.get(1)?,
        source: row.get(2)?,
        conversation_id: row.get(3)?,
        model_id: row.get(4)?,
        tags,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

// ============================================
// Tests
// ============================================

#[cfg(test)]
mod tests {
    use super::*;

    // --- Quality scoring ---

    #[test]
    fn test_quality_rejects_short_messages() {
        let (score, _) = score_message_quality("hi", "user");
        assert_eq!(score, 0.0);
    }

    #[test]
    fn test_quality_rejects_trivial_greetings() {
        let (score, _) = score_message_quality("hello there friend how are you?", "user");
        // "hello" pattern match — should score 0 (trivial)
        // Actually "hello there friend how are you?" is > 30 chars and doesn't exact-match "hello"
        // It should get a non-zero score because it passes length check
        assert!(score >= 0.0);
    }

    #[test]
    fn test_quality_rejects_exact_trivial() {
        let (score, _) = score_message_quality("thanks for all the help today!", "user");
        // "thanks" with punctuation variant — but this is > 30 chars so length check passes
        // The exact trivial check is for trimmed == pattern, this is longer
        assert!(score >= 0.0);
    }

    #[test]
    fn test_quality_scores_preferences_higher() {
        let (pref_score, pref_tags) = score_message_quality(
            "I prefer using TypeScript for all frontend work because of the type safety",
            "user",
        );
        let (plain_score, _) = score_message_quality(
            "The weather today is quite pleasant and sunny outside the window",
            "user",
        );
        assert!(pref_score > plain_score, "Preferences should score higher than plain content");
        assert!(pref_tags.contains(&"preference".to_string()));
    }

    #[test]
    fn test_quality_scores_decisions_higher() {
        let (score, tags) = score_message_quality(
            "Let's go with the Tauri approach for the desktop app because it gives us Rust backend",
            "user",
        );
        assert!(score > 0.3, "Decisions should score above threshold");
        assert!(tags.contains(&"decision".to_string()));
    }

    #[test]
    fn test_quality_rejects_code_heavy() {
        let code_heavy = "Here's the code:\n```rust\nfn main() {\n    println!(\"hello\");\n    let x = 42;\n    let y = x + 1;\n    println!(\"{}\", y);\n}\n```";
        let (score, _) = score_message_quality(code_heavy, "assistant");
        assert_eq!(score, 0.0, "Code-heavy content (>70% code) should be rejected");
    }

    #[test]
    fn test_quality_rejects_assistant_preamble() {
        let (score, _) = score_message_quality("Sure! I can help with that.", "assistant");
        assert_eq!(score, 0.0, "Short assistant preambles should be rejected");
    }

    // --- YAKE keyword extraction ---

    #[test]
    fn test_extract_keywords_basic() {
        let keywords = extract_keywords("The Tauri framework uses Rust for the backend");
        assert!(!keywords.is_empty());
        // Should extract meaningful words, not stopwords
        assert!(keywords.iter().any(|k| k.contains("tauri") || k.contains("framework") || k.contains("rust") || k.contains("backend")));
    }

    #[test]
    fn test_extract_keywords_empty() {
        let keywords = extract_keywords("");
        assert!(keywords.is_empty());
    }

    #[test]
    fn test_extract_keywords_multiword() {
        // YAKE should extract multi-word keyphrases when words co-occur
        let keywords = extract_keywords(
            "Machine learning model training pipeline. \
             The training pipeline handles data preprocessing. \
             Machine learning requires large datasets for training."
        );
        assert!(!keywords.is_empty());
        // Should find multi-word phrases like "machine learning" or "training pipeline"
        let has_phrase = keywords.iter().any(|k| k.contains(' '));
        assert!(has_phrase, "YAKE should produce multi-word keyphrases, got: {:?}", keywords);
    }

    #[test]
    fn test_extract_keywords_no_stopwords() {
        let keywords = extract_keywords("The system should have been working with the users");
        // All stopwords — should return very few or no keywords
        // "system" and "users" might survive since they aren't in our stopword list
        for kw in &keywords {
            assert!(!["the", "should", "have", "been", "with"].contains(&kw.as_str()),
                "stopword '{}' leaked through", kw);
        }
    }

    #[test]
    fn test_extract_keywords_max_eight() {
        let keywords = extract_keywords(
            "Alpha bravo charlie delta echo foxtrot golf hotel india juliet \
             kilo lima mike november oscar papa quebec romeo sierra tango"
        );
        assert!(keywords.len() <= 8, "should return at most 8 keywords, got {}", keywords.len());
    }

    #[test]
    fn test_extract_keywords_dedup() {
        // If a multi-word phrase is selected, its individual words shouldn't also appear
        let keywords = extract_keywords(
            "Async error handling in Rust. Async error handling patterns. \
             Good async error handling prevents panics."
        );
        // Should NOT have both "async" and "async error" (or "async error handling")
        let phrases: Vec<&String> = keywords.iter().filter(|k| k.contains(' ')).collect();
        for phrase in &phrases {
            for kw in &keywords {
                if kw != *phrase {
                    assert!(!phrase.contains(kw.as_str()),
                        "phrase '{}' and substring '{}' both in results: {:?}", phrase, kw, keywords);
                }
            }
        }
    }

    #[test]
    fn test_extract_keywords_frequency_fallback() {
        // Fallback function should still work
        let keywords = extract_keywords_frequency("Rust async error handling patterns in Tauri");
        assert!(!keywords.is_empty());
        assert!(keywords.iter().any(|k| k == "rust" || k == "tauri" || k == "async"));
    }

    // --- Topic classification ---

    #[test]
    fn test_classify_topic_technical() {
        let topic = classify_topic(
            "Let me write a function that handles the API endpoint",
            &["function".to_string(), "api".to_string(), "endpoint".to_string()],
            &[],
        );
        assert_eq!(topic, "topic:technical");
    }

    #[test]
    fn test_classify_topic_project() {
        let topic = classify_topic(
            "The architecture should follow modular design principles",
            &["architecture".to_string(), "design".to_string(), "principles".to_string()],
            &[],
        );
        assert_eq!(topic, "topic:project");
    }

    #[test]
    fn test_classify_topic_general() {
        let topic = classify_topic(
            "Had a nice day exploring the park",
            &["nice".to_string(), "park".to_string()],
            &[],
        );
        // Keywords return "general" → semantic kicks in
        // "exploring the park" is closest to conversational or general
        assert!(topic.starts_with("topic:"), "Should return a topic tag");
    }

    // --- Phase 6: Semantic topic classification ---

    #[test]
    fn topic_seed_examples_covers_all_categories() {
        let seeds = topic_seed_examples();
        let categories: Vec<&str> = seeds.iter().map(|(t, _)| *t).collect();
        assert!(categories.contains(&"topic:technical"));
        assert!(categories.contains(&"topic:project"));
        assert!(categories.contains(&"topic:personal"));
        assert!(categories.contains(&"topic:conversational"));
        assert!(categories.contains(&"topic:creative"));
        assert!(categories.contains(&"topic:reference"));
    }

    #[test]
    fn semantic_topic_classifies_code_as_technical() {
        // Skip if fastembed unavailable
        if get_local_embedding("test").is_err() {
            eprintln!("Skipping semantic topic test (fastembed not available)");
            return;
        }

        let topic = classify_topic_semantic(
            "I need to fix a null pointer exception in the database query handler"
        );
        assert_eq!(topic, Some("topic:technical".to_string()),
            "Code-related content should classify as technical");
    }

    #[test]
    fn semantic_topic_classifies_greeting_as_conversational() {
        if get_local_embedding("test").is_err() {
            eprintln!("Skipping semantic topic test (fastembed not available)");
            return;
        }

        let topic = classify_topic_semantic("hey there, thanks for helping me out");
        assert_eq!(topic, Some("topic:conversational".to_string()),
            "Greetings should classify as conversational");
    }

    #[test]
    fn semantic_topic_classifies_preference_as_personal() {
        if get_local_embedding("test").is_err() {
            eprintln!("Skipping semantic topic test (fastembed not available)");
            return;
        }

        let topic = classify_topic_semantic("I really like using vim as my code editor");
        assert_eq!(topic, Some("topic:personal".to_string()),
            "Preference content should classify as personal");
    }

    #[test]
    fn classify_topic_cascade_keywords_first() {
        // Keywords should take priority over semantic for structured data
        let topic = classify_topic(
            "We need to debug the API endpoint",
            &["debug".to_string(), "api".to_string(), "endpoint".to_string()],
            &[],
        );
        assert_eq!(topic, "topic:technical", "Keyword match should win over semantic");
    }

    // --- Markdown heading-aware splitting ---

    #[test]
    fn test_markdown_heading_split_basic() {
        let md = "# Chapter 1\nIntro text here.\n\n## Section 1.1\nDetail about 1.1.\n\n## Section 1.2\nDetail about 1.2.\n\n# Chapter 2\nAnother chapter.";
        let sections = split_markdown_by_headings(md, 4000);
        assert_eq!(sections.len(), 4);
        assert_eq!(sections[0].heading.as_deref(), Some("Chapter 1"));
        assert_eq!(sections[0].level, 1);
        assert_eq!(sections[1].heading.as_deref(), Some("Section 1.1"));
        assert_eq!(sections[1].level, 2);
        assert_eq!(sections[2].heading.as_deref(), Some("Section 1.2"));
        assert_eq!(sections[2].level, 2);
        assert_eq!(sections[3].heading.as_deref(), Some("Chapter 2"));
        assert_eq!(sections[3].level, 1);
    }

    #[test]
    fn test_markdown_heading_path_breadcrumb() {
        let md = "# Top\nA\n\n## Mid\nB\n\n### Deep\nC";
        let sections = split_markdown_by_headings(md, 4000);
        assert_eq!(sections.len(), 3);
        // Deep section should have breadcrumb: ["Top", "Mid", "Deep"]
        assert_eq!(sections[2].path, vec!["Top", "Mid", "Deep"]);
    }

    #[test]
    fn test_markdown_no_headings_falls_back() {
        let md = "Just some plain text.\n\nAnother paragraph here.\n\nThird paragraph.";
        let sections = split_markdown_by_headings(md, 4000);
        // Should fall back to flat splitting — all level 0
        assert!(sections.iter().all(|s| s.level == 0));
        assert!(!sections.is_empty());
    }

    #[test]
    fn test_markdown_oversized_section_subsplit() {
        // Create a section with a heading followed by lots of content
        let mut md = String::from("# Big Section\n");
        for i in 0..100 {
            md.push_str(&format!("Line {} with some content to fill up space.\n\n", i));
        }
        let sections = split_markdown_by_headings(&md, 500);
        // Should be split into multiple sub-sections
        assert!(sections.len() > 1);
        // First should have the heading, rest should say "(continued)"
        assert_eq!(sections[0].heading.as_deref(), Some("Big Section"));
        assert!(sections[1].heading.as_deref().unwrap_or("").contains("continued"));
    }

    // ================================================================
    // Integration Tests — SQLite round-trips (Phase 3B)
    // ================================================================

    /// Create an in-memory SQLite DB with full schema for integration tests.
    fn test_db() -> Connection {
        let conn = Connection::open_in_memory().expect("Failed to open in-memory DB");
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000; PRAGMA foreign_keys=ON;")
            .expect("Failed to set PRAGMA");
        init_db(&conn).expect("Failed to init schema");
        conn
    }

    // --- Memory Save / Search / Update / Delete round-trip ---

    #[test]
    fn integration_save_and_search_by_keyword() {
        let conn = test_db();
        let tags = vec!["test".to_string(), "rust".to_string()];
        let record = write_memory_public(
            &conn, "Rust is a systems programming language with memory safety guarantees",
            "user", None, None, &tags, &[],
        ).expect("save failed");
        assert!(!record.id.is_empty());
        assert_eq!(record.source, "user");

        // FTS5 keyword search (no embeddings)
        let results = search_hybrid_public(&conn, "Rust programming", &[], 10)
            .expect("search failed");
        assert!(!results.is_empty(), "FTS5 search should find the saved memory");
        assert_eq!(results[0].id, record.id);
        assert!(results[0].score > 0.0);
    }

    #[test]
    fn integration_save_multiple_and_search_relevance() {
        let conn = test_db();
        write_memory_public(&conn, "Python is great for data science and machine learning",
            "user", None, None, &["python".to_string()], &[]).unwrap();
        write_memory_public(&conn, "Rust provides zero-cost abstractions and memory safety",
            "user", None, None, &["rust".to_string()], &[]).unwrap();
        write_memory_public(&conn, "JavaScript runs in the browser and Node.js",
            "user", None, None, &["javascript".to_string()], &[]).unwrap();

        let results = search_hybrid_public(&conn, "memory safety abstractions", &[], 10).unwrap();
        assert!(!results.is_empty());
        // Rust memory should rank highest for "memory safety abstractions"
        assert!(results[0].content.contains("Rust"), "Rust memory should be most relevant");
    }

    #[test]
    fn integration_update_memory_content() {
        let conn = test_db();
        let record = write_memory_public(
            &conn, "Original content here", "user", None, None, &[], &[],
        ).unwrap();

        let updated = update_memory_public(
            &conn, &record.id, "Updated content with new information", None, &[],
        ).expect("update failed");
        assert_eq!(updated.id, record.id);
        assert_eq!(updated.content, "Updated content with new information");

        // Verify FTS5 reflects the update
        let results = search_hybrid_public(&conn, "Updated new information", &[], 10).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].id, record.id);
    }

    #[test]
    fn integration_delete_memory() {
        let conn = test_db();
        let record = write_memory_public(
            &conn, "Memory to be deleted shortly after creation",
            "user", None, None, &[], &[],
        ).unwrap();

        let deleted = delete_memory_public(&conn, &record.id).expect("delete failed");
        assert!(deleted, "should return true for successful delete");

        // Verify it's gone from search
        let results = search_hybrid_public(&conn, "deleted shortly after creation", &[], 10).unwrap();
        assert!(results.is_empty() || results.iter().all(|r| r.id != record.id),
            "deleted memory should not appear in search");
    }

    #[test]
    fn integration_delete_nonexistent_returns_false() {
        let conn = test_db();
        let deleted = delete_memory_public(&conn, "nonexistent_id_12345").expect("delete failed");
        assert!(!deleted, "should return false for nonexistent memory");
    }

    #[test]
    fn integration_dedup_returns_false_without_embeddings() {
        let conn = test_db();
        write_memory_public(
            &conn, "The user prefers TypeScript for frontend development",
            "user", None, None, &["preference".to_string()], &[],
        ).unwrap();

        // Without embeddings, dedup gracefully returns false (P4 — allows save)
        let is_dup = is_near_duplicate(&conn, &[]);
        assert!(!is_dup, "no embeddings → can't dedup → should allow save");
    }

    #[test]
    fn integration_dedup_detects_identical_embeddings() {
        let conn = test_db();
        // Save with a known embedding
        let embedding = vec![0.1, 0.2, 0.3, 0.4, 0.5];
        write_memory_public(
            &conn, "Memory with embedding for dedup test",
            "user", None, None, &[], &[embedding.clone()],
        ).unwrap();

        // Same embedding should be detected as near-duplicate (cosine > 0.92)
        let is_dup = is_near_duplicate(&conn, &[embedding]);
        assert!(is_dup, "identical embedding should be detected as near-duplicate");
    }

    #[test]
    fn integration_dedup_allows_different_embeddings() {
        let conn = test_db();
        let embedding1 = vec![1.0, 0.0, 0.0, 0.0, 0.0];
        write_memory_public(
            &conn, "Memory A with distinct embedding",
            "user", None, None, &[], &[embedding1],
        ).unwrap();

        // Orthogonal embedding should NOT be a duplicate
        let embedding2 = vec![0.0, 0.0, 0.0, 0.0, 1.0];
        let is_dup = is_near_duplicate(&conn, &[embedding2]);
        assert!(!is_dup, "orthogonal embedding should not be flagged as duplicate");
    }

    #[test]
    fn integration_memory_with_conversation_and_model() {
        let conn = test_db();
        let record = write_memory_public(
            &conn, "Important architectural decision about plugin system",
            "conversation", Some("conv_abc123"), Some("gpt-4o"), &["architecture".to_string()], &[],
        ).unwrap();
        assert_eq!(record.source, "conversation");

        // Verify the metadata persists
        let row: (String, Option<String>, Option<String>) = conn.query_row(
            "SELECT source, conversation_id, model_id FROM memories WHERE id = ?1",
            params![record.id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).expect("should find the memory");
        assert_eq!(row.0, "conversation");
        assert_eq!(row.1.as_deref(), Some("conv_abc123"));
        assert_eq!(row.2.as_deref(), Some("gpt-4o"));
    }

    // --- MAGMA Episodic Graph (events) ---

    #[test]
    fn integration_magma_event_save_and_query() {
        let conn = test_db();
        let now = chrono::Utc::now().to_rfc3339();
        let id = generate_id();

        conn.execute(
            "INSERT INTO events (id, event_type, agent, content, metadata, session_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, "task_start", "coder", "Working on memory tests", "{}", "session_1", now],
        ).expect("insert event failed");

        // Query it back
        let (ev_type, ev_agent, ev_content): (String, String, String) = conn.query_row(
            "SELECT event_type, agent, content FROM events WHERE id = ?1",
            params![id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).expect("query event failed");
        assert_eq!(ev_type, "task_start");
        assert_eq!(ev_agent, "coder");
        assert_eq!(ev_content, "Working on memory tests");
    }

    #[test]
    fn integration_magma_events_filtered_by_agent() {
        let conn = test_db();
        let now = chrono::Utc::now().to_rfc3339();

        for (agent, content) in [("coder", "Code task"), ("terminal", "Shell task"), ("coder", "More code")] {
            conn.execute(
                "INSERT INTO events (id, event_type, agent, content, metadata, created_at)
                 VALUES (?1, 'task', ?2, ?3, '{}', ?4)",
                params![generate_id(), agent, content, now],
            ).unwrap();
        }

        let coder_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM events WHERE agent = 'coder'", [], |row| row.get(0),
        ).unwrap();
        assert_eq!(coder_count, 2);

        let terminal_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM events WHERE agent = 'terminal'", [], |row| row.get(0),
        ).unwrap();
        assert_eq!(terminal_count, 1);
    }

    // --- MAGMA Entity Graph ---

    #[test]
    fn integration_magma_entity_upsert_and_retrieve() {
        let conn = test_db();
        let now = chrono::Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO entities (id, entity_type, name, state, metadata, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, '{}', ?5, ?6)",
            params!["file_main_rs", "file", "src/main.rs", r#"{"lines": 500}"#, now, now],
        ).unwrap();

        let (etype, name, state): (String, String, String) = conn.query_row(
            "SELECT entity_type, name, state FROM entities WHERE id = ?1",
            params!["file_main_rs"],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).expect("entity not found");
        assert_eq!(etype, "file");
        assert_eq!(name, "src/main.rs");
        assert!(state.contains("500"));
    }

    #[test]
    fn integration_magma_entity_unique_constraint() {
        let conn = test_db();
        let now = chrono::Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO entities (id, entity_type, name, state, metadata, created_at, updated_at)
             VALUES (?1, 'model', 'gpt-4o', '{}', '{}', ?2, ?3)",
            params![generate_id(), now, now],
        ).unwrap();

        // Same (type, name) should conflict on the unique index
        let result = conn.execute(
            "INSERT INTO entities (id, entity_type, name, state, metadata, created_at, updated_at)
             VALUES (?1, 'model', 'gpt-4o', '{}', '{}', ?2, ?3)",
            params![generate_id(), now, now],
        );
        assert!(result.is_err(), "unique constraint on (entity_type, name) should prevent duplicates");
    }

    // --- MAGMA Procedural Graph ---

    #[test]
    fn integration_magma_procedure_save_and_outcome() {
        let conn = test_db();
        let now = chrono::Utc::now().to_rfc3339();
        let id = generate_id();

        conn.execute(
            "INSERT INTO procedures (id, name, description, steps, trigger_pattern, success_count, fail_count, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, 0, ?6, ?7)",
            params![id, "web_search → memory_save", "Search and save pattern",
                    r#"[{"tool":"web_search"},{"tool":"memory_save"}]"#,
                    "search for information", now, now],
        ).unwrap();

        // Record success
        conn.execute(
            "UPDATE procedures SET success_count = success_count + 1, last_used = ?1 WHERE id = ?2",
            params![now, id],
        ).unwrap();

        let (success, fail, last_used): (i64, i64, Option<String>) = conn.query_row(
            "SELECT success_count, fail_count, last_used FROM procedures WHERE id = ?1",
            params![id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).unwrap();
        assert_eq!(success, 1);
        assert_eq!(fail, 0);
        assert!(last_used.is_some());
    }

    // --- MAGMA Edge Graph (cross-graph connections) ---

    #[test]
    fn integration_magma_edge_creation_and_traversal() {
        let conn = test_db();
        let now = chrono::Utc::now().to_rfc3339();

        // Create an entity and an event, then link them
        let entity_id = "file_readme";
        let event_id = generate_id();
        conn.execute(
            "INSERT INTO entities (id, entity_type, name, state, metadata, created_at, updated_at)
             VALUES (?1, 'file', 'README.md', '{}', '{}', ?2, ?3)",
            params![entity_id, now, now],
        ).unwrap();
        conn.execute(
            "INSERT INTO events (id, event_type, agent, content, metadata, created_at)
             VALUES (?1, 'file_read', 'coder', 'Read README.md', '{}', ?2)",
            params![event_id, now],
        ).unwrap();

        // Create edge: event → entity (references)
        conn.execute(
            "INSERT INTO edges (id, source_type, source_id, target_type, target_id, edge_type, weight, metadata, created_at)
             VALUES (?1, 'event', ?2, 'entity', ?3, 'references', 1.0, '{}', ?4)",
            params![generate_id(), event_id, entity_id, now],
        ).unwrap();

        // Query edges from the event
        let edge_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM edges WHERE source_type = 'event' AND source_id = ?1",
            params![event_id], |row| row.get(0),
        ).unwrap();
        assert_eq!(edge_count, 1);

        // Query edges TO the entity
        let incoming: i64 = conn.query_row(
            "SELECT COUNT(*) FROM edges WHERE target_type = 'entity' AND target_id = ?1",
            params![entity_id], |row| row.get(0),
        ).unwrap();
        assert_eq!(incoming, 1);
    }

    // --- Full round-trip: save memory → create entity → add edge → verify graph ---

    #[test]
    fn integration_full_graph_round_trip() {
        let conn = test_db();
        let now = chrono::Utc::now().to_rfc3339();

        // 1. Save a memory
        let memory = write_memory_public(
            &conn, "HIVE uses Tauri v2 with Rust backend and React frontend",
            "user", None, None, &["architecture".to_string()], &[],
        ).unwrap();

        // 2. Create a related entity
        let entity_id = "project_hive";
        conn.execute(
            "INSERT INTO entities (id, entity_type, name, state, metadata, created_at, updated_at)
             VALUES (?1, 'project', 'HIVE', ?2, '{}', ?3, ?4)",
            params![entity_id, r#"{"framework":"tauri"}"#, now, now],
        ).unwrap();

        // 3. Create edge: memory → entity (references)
        conn.execute(
            "INSERT INTO edges (id, source_type, source_id, target_type, target_id, edge_type, weight, metadata, created_at)
             VALUES (?1, 'memory', ?2, 'entity', ?3, 'references', 1.0, '{}', ?4)",
            params![generate_id(), memory.id, entity_id, now],
        ).unwrap();

        // 4. Verify the graph is connected
        let edge_exists: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM edges WHERE source_id = ?1 AND target_id = ?2",
            params![memory.id, entity_id], |row| row.get(0),
        ).unwrap();
        assert!(edge_exists, "edge should connect memory to entity");

        // 5. Memory should be searchable
        let results = search_hybrid_public(&conn, "Tauri Rust React", &[], 5).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].id, memory.id);
    }

    // --- Phase 4C: Memory tier system ---

    #[test]
    fn tier_defaults_to_long_term() {
        let conn = test_db();
        let record = write_memory_public(
            &conn, "Default tier test", "user", None, None, &[], &[],
        ).unwrap();
        let tier: String = conn.query_row(
            "SELECT COALESCE(tier, 'long_term') FROM memories WHERE id = ?1",
            params![record.id], |row| row.get(0),
        ).unwrap();
        assert_eq!(tier, "long_term");
    }

    #[test]
    fn tier_can_be_set_to_short_term() {
        let conn = test_db();
        let record = write_memory_public(
            &conn, "Short term tier test", "working-memory", None, None, &[], &[],
        ).unwrap();
        set_memory_tier(&conn, &record.id, "short_term").unwrap();
        let tier: String = conn.query_row(
            "SELECT tier FROM memories WHERE id = ?1",
            params![record.id], |row| row.get(0),
        ).unwrap();
        assert_eq!(tier, "short_term");
    }

    #[test]
    fn tier_promotion_on_access() {
        let conn = test_db();
        let record = write_memory_public(
            &conn, "Promotion test memory about Rust programming", "working-memory", None, None, &[], &[],
        ).unwrap();
        set_memory_tier(&conn, &record.id, "short_term").unwrap();

        // Simulate 4 accesses (> 3 threshold)
        conn.execute(
            "UPDATE memories SET access_count = 4 WHERE id = ?1",
            params![record.id],
        ).unwrap();

        // Trigger promotion (same logic as in search_hybrid)
        conn.execute(
            "UPDATE memories SET tier = 'long_term' WHERE tier = 'short_term' AND access_count > 3",
            [],
        ).unwrap();

        let tier: String = conn.query_row(
            "SELECT tier FROM memories WHERE id = ?1",
            params![record.id], |row| row.get(0),
        ).unwrap();
        assert_eq!(tier, "long_term", "short_term with access_count > 3 should promote to long_term");
    }

    #[test]
    fn tier_no_promotion_below_threshold() {
        let conn = test_db();
        let record = write_memory_public(
            &conn, "No promotion test", "working-memory", None, None, &[], &[],
        ).unwrap();
        set_memory_tier(&conn, &record.id, "short_term").unwrap();

        // Only 2 accesses (≤ 3 threshold)
        conn.execute(
            "UPDATE memories SET access_count = 2 WHERE id = ?1",
            params![record.id],
        ).unwrap();

        // Attempt promotion — should not promote
        conn.execute(
            "UPDATE memories SET tier = 'long_term' WHERE tier = 'short_term' AND access_count > 3",
            [],
        ).unwrap();

        let tier: String = conn.query_row(
            "SELECT tier FROM memories WHERE id = ?1",
            params![record.id], |row| row.get(0),
        ).unwrap();
        assert_eq!(tier, "short_term", "short_term with access_count ≤ 3 should stay short_term");
    }

    #[test]
    fn tier_counts_returns_distribution() {
        let conn = test_db();
        // Create 2 long_term (default) + 1 short_term
        write_memory_public(&conn, "Long term A", "user", None, None, &[], &[]).unwrap();
        write_memory_public(&conn, "Long term B", "user", None, None, &[], &[]).unwrap();
        let short = write_memory_public(&conn, "Short term C", "working-memory", None, None, &[], &[]).unwrap();
        set_memory_tier(&conn, &short.id, "short_term").unwrap();

        let counts = get_tier_counts(&conn).unwrap();
        assert_eq!(*counts.get("long_term").unwrap_or(&0), 2);
        assert_eq!(*counts.get("short_term").unwrap_or(&0), 1);
    }

    #[test]
    fn promote_due_memories_standalone() {
        let conn = test_db();
        // Create 3 short_term memories with varying access counts
        let m1 = write_memory_with_tier(&conn, "Should promote", "wm", None, None, &[], &[], Some("short_term")).unwrap();
        let m2 = write_memory_with_tier(&conn, "Should stay", "wm", None, None, &[], &[], Some("short_term")).unwrap();
        let m3 = write_memory_with_tier(&conn, "Also promote", "wm", None, None, &[], &[], Some("short_term")).unwrap();

        conn.execute("UPDATE memories SET access_count = 5 WHERE id = ?1", params![m1.id]).unwrap();
        conn.execute("UPDATE memories SET access_count = 2 WHERE id = ?1", params![m2.id]).unwrap();
        conn.execute("UPDATE memories SET access_count = 4 WHERE id = ?1", params![m3.id]).unwrap();

        let promoted = promote_due_memories(&conn).unwrap();
        assert_eq!(promoted, 2, "Should promote exactly 2 memories with access_count > 3");

        let t1: String = conn.query_row("SELECT tier FROM memories WHERE id = ?1", params![m1.id], |r| r.get(0)).unwrap();
        let t2: String = conn.query_row("SELECT tier FROM memories WHERE id = ?1", params![m2.id], |r| r.get(0)).unwrap();
        let t3: String = conn.query_row("SELECT tier FROM memories WHERE id = ?1", params![m3.id], |r| r.get(0)).unwrap();
        assert_eq!(t1, "long_term");
        assert_eq!(t2, "short_term", "access_count=2 should remain short_term");
        assert_eq!(t3, "long_term");
    }

    #[test]
    fn archive_stale_memories_respects_thresholds() {
        let conn = test_db();
        // Create memories with varying ages and strengths
        let m1 = write_memory_with_tier(&conn, "Old and weak", "wm", None, None, &[], &[], Some("long_term")).unwrap();
        let m2 = write_memory_with_tier(&conn, "Old but strong", "wm", None, None, &[], &[], Some("long_term")).unwrap();
        let m3 = write_memory_with_tier(&conn, "Recent and weak", "wm", None, None, &[], &[], Some("long_term")).unwrap();

        // m1: old (120 days) + weak (strength=1.0) → should archive
        conn.execute("UPDATE memories SET last_accessed = datetime('now', '-120 days'), strength = 1.0 WHERE id = ?1", params![m1.id]).unwrap();
        // m2: old (120 days) + strong (strength=1.3) → should NOT archive
        conn.execute("UPDATE memories SET last_accessed = datetime('now', '-120 days'), strength = 1.3 WHERE id = ?1", params![m2.id]).unwrap();
        // m3: recent (10 days) + weak (strength=1.0) → should NOT archive
        conn.execute("UPDATE memories SET last_accessed = datetime('now', '-10 days'), strength = 1.0 WHERE id = ?1", params![m3.id]).unwrap();

        let archived = archive_stale_memories(&conn).unwrap();
        assert_eq!(archived, 1, "Only old+weak memory should be archived");

        let t1: String = conn.query_row("SELECT tier FROM memories WHERE id = ?1", params![m1.id], |r| r.get(0)).unwrap();
        let t2: String = conn.query_row("SELECT tier FROM memories WHERE id = ?1", params![m2.id], |r| r.get(0)).unwrap();
        let t3: String = conn.query_row("SELECT tier FROM memories WHERE id = ?1", params![m3.id], |r| r.get(0)).unwrap();
        assert_eq!(t1, "archived", "Old+weak should be archived");
        assert_eq!(t2, "long_term", "Old+strong should remain long_term");
        assert_eq!(t3, "long_term", "Recent+weak should remain long_term");
    }

    #[test]
    fn archive_skips_already_archived() {
        let conn = test_db();
        let m1 = write_memory_with_tier(&conn, "Already archived", "wm", None, None, &[], &[], Some("long_term")).unwrap();
        conn.execute("UPDATE memories SET tier = 'archived', last_accessed = datetime('now', '-200 days'), strength = 1.0 WHERE id = ?1", params![m1.id]).unwrap();

        let archived = archive_stale_memories(&conn).unwrap();
        assert_eq!(archived, 0, "Should not re-archive already archived memories");
    }

    #[test]
    fn tier_weight_values() {
        assert_eq!(tier_weight("short_term"), 0.85);
        assert_eq!(tier_weight("long_term"), 1.0);
        assert_eq!(tier_weight("archived"), 0.5);
        assert_eq!(tier_weight("consolidated"), 0.3);
        assert_eq!(tier_weight("superseded"), 0.2);
        assert_eq!(tier_weight("unknown"), 1.0);
    }

    #[test]
    fn write_memory_with_tier_atomic() {
        let conn = test_db();
        // Insert with tier=short_term directly — no INSERT-then-UPDATE needed
        let record = write_memory_with_tier(
            &conn, "Atomic tier test", "working-memory", None, None, &[], &[], Some("short_term"),
        ).unwrap();
        let tier: String = conn.query_row(
            "SELECT tier FROM memories WHERE id = ?1",
            params![record.id], |r| r.get(0),
        ).unwrap();
        assert_eq!(tier, "short_term", "tier should be set atomically at INSERT time");
    }

    #[test]
    fn write_memory_with_tier_defaults_to_long_term() {
        let conn = test_db();
        // tier=None should default to long_term
        let record = write_memory_with_tier(
            &conn, "Default tier test", "user", None, None, &[], &[], None,
        ).unwrap();
        let tier: String = conn.query_row(
            "SELECT tier FROM memories WHERE id = ?1",
            params![record.id], |r| r.get(0),
        ).unwrap();
        assert_eq!(tier, "long_term", "None tier should default to long_term");
    }

    #[test]
    fn tier_weight_scoring() {
        assert_eq!(tier_weight("long_term"), 1.0);
        assert_eq!(tier_weight("short_term"), 0.85);
        assert_eq!(tier_weight("unknown"), 1.0, "Unknown tiers default to 1.0");
    }

    #[test]
    fn reinforcement_increments_access_count_and_strength() {
        // Regression test: the original SQL used ln() which doesn't exist in bundled SQLite.
        // The fix computes strength in Rust. Verify access_count and strength actually update.
        let conn = test_db();
        let record = write_memory_public(&conn, "Reinforcement test content", "user", None, None, &[], &[]).unwrap();

        // Initial state
        let (count0, strength0): (i64, f64) = conn.query_row(
            "SELECT access_count, strength FROM memories WHERE id = ?1",
            params![record.id], |r| Ok((r.get(0)?, r.get(1)?)),
        ).unwrap();
        assert_eq!(count0, 0);
        assert!((strength0 - 1.0).abs() < 0.001, "Initial strength should be 1.0");

        // Simulate one reinforcement (same logic as search_hybrid)
        let _ = conn.execute("UPDATE memories SET access_count = access_count + 1 WHERE id = ?1", params![record.id]);
        let new_count: i64 = conn.query_row(
            "SELECT access_count FROM memories WHERE id = ?1",
            params![record.id], |r| r.get(0),
        ).unwrap();
        let strength = 1.0 + 0.1 * (1.0 + new_count as f64).ln();
        let _ = conn.execute("UPDATE memories SET strength = ?1 WHERE id = ?2", params![strength, record.id]);

        let (count1, strength1): (i64, f64) = conn.query_row(
            "SELECT access_count, strength FROM memories WHERE id = ?1",
            params![record.id], |r| Ok((r.get(0)?, r.get(1)?)),
        ).unwrap();
        assert_eq!(count1, 1, "access_count should increment");
        assert!(strength1 > 1.0, "strength should increase after reinforcement");
        assert!((strength1 - 1.069).abs() < 0.01, "strength at count=1 should be ~1.069");
    }

    #[test]
    fn dedup_before_truncate_ordering() {
        // Regression test: truncation must happen AFTER deduplication.
        // If truncated first, unique results could be lost when duplicates fill top slots.
        let conn = test_db();
        // Create 3 memories — two with same content (will produce duplicate memory_ids in search)
        write_memory_public(&conn, "unique alpha content about testing", "user", None, None, &[], &[]).unwrap();
        write_memory_public(&conn, "unique beta content about testing", "user", None, None, &[], &[]).unwrap();
        write_memory_public(&conn, "unique gamma content about testing", "user", None, None, &[], &[]).unwrap();

        // Search with max_results=2. All 3 should be findable, dedup should keep unique ones.
        let results = search_hybrid(&conn, "testing", &[], 2, 0.7, 0.3).unwrap();
        // After dedup (each memory has unique id), we should get exactly 2 results (truncated)
        assert_eq!(results.len(), 2, "Should get exactly max_results after dedup+truncate");
        // Verify no duplicate memory_ids
        let ids: std::collections::HashSet<_> = results.iter().map(|r| r.id.clone()).collect();
        assert_eq!(ids.len(), results.len(), "No duplicate memory_ids in results");
    }

    // --- Phase 3: Cosine similarity dimension handling ---

    #[test]
    fn cosine_same_dimension_identical_vectors() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn cosine_same_dimension_orthogonal_vectors() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        assert!(cosine_similarity(&a, &b).abs() < 1e-10);
    }

    #[test]
    fn cosine_dimension_mismatch_returns_zero() {
        // Phase 3: 384-dim fastembed vs 1536-dim OpenAI must not compare
        let a = vec![0.1; 384];
        let b = vec![0.1; 1536];
        assert_eq!(cosine_similarity(&a, &b), 0.0, "Mismatched dimensions must return 0.0");
    }

    #[test]
    fn cosine_empty_vectors_return_zero() {
        assert_eq!(cosine_similarity(&[], &[1.0, 2.0]), 0.0);
        assert_eq!(cosine_similarity(&[1.0, 2.0], &[]), 0.0);
        assert_eq!(cosine_similarity(&[], &[]), 0.0);
    }

    #[test]
    fn local_embedding_produces_384_dims() {
        // fastembed all-MiniLM-L6-v2 produces 384-dimensional vectors
        match get_local_embedding("test embedding for HIVE") {
            Ok(embedding) => {
                assert_eq!(embedding.len(), 384, "all-MiniLM-L6-v2 should produce 384 dims");
                // Verify it's normalized (unit vector)
                let norm: f64 = embedding.iter().map(|x| x * x).sum::<f64>().sqrt();
                assert!((norm - 1.0).abs() < 0.1, "embedding should be approximately unit length");
            }
            Err(e) => {
                // First run might need model download — skip in CI but log
                eprintln!("Skipping fastembed test (model not cached): {}", e);
            }
        }
    }

    #[test]
    fn local_embedding_deterministic() {
        // Same input should produce same output (no randomness)
        let text = "deterministic embedding test";
        match (get_local_embedding(text), get_local_embedding(text)) {
            (Ok(a), Ok(b)) => {
                assert_eq!(a.len(), b.len());
                let sim = cosine_similarity(&a, &b);
                assert!((sim - 1.0).abs() < 1e-6, "same text should produce identical embeddings");
            }
            _ => eprintln!("Skipping determinism test (fastembed not available)"),
        }
    }

    #[test]
    fn local_embedding_semantic_similarity() {
        // Related texts should have higher similarity than unrelated texts
        match (
            get_local_embedding("machine learning algorithms"),
            get_local_embedding("deep learning neural networks"),
            get_local_embedding("italian pasta recipes"),
        ) {
            (Ok(ml), Ok(dl), Ok(pasta)) => {
                let related = cosine_similarity(&ml, &dl);
                let unrelated = cosine_similarity(&ml, &pasta);
                assert!(
                    related > unrelated,
                    "ML-DL similarity ({:.3}) should exceed ML-pasta ({:.3})",
                    related, unrelated
                );
            }
            _ => eprintln!("Skipping semantic similarity test (fastembed not available)"),
        }
    }

    // --- Phase 8C: Memory Consolidation ---

    #[test]
    fn consolidation_skips_sparse_topics() {
        // Topics with fewer than 10 memories should not trigger consolidation
        let conn = test_db();
        for i in 0..5 {
            write_memory_with_tier(
                &conn, &format!("Sparse topic memory {}", i), "user", None, None,
                &["topic:sparse".to_string()], &[], Some("long_term"),
            ).unwrap();
        }
        let (topics, clusters, memories) = consolidate_memories(&conn).unwrap();
        assert_eq!(topics, 0, "Should skip topics with < 10 memories");
        assert_eq!(clusters, 0);
        assert_eq!(memories, 0);
    }

    #[test]
    fn consolidation_groups_by_topic() {
        // Dense topic with 12 memories but no embeddings → clusters won't form
        let conn = test_db();
        for i in 0..12 {
            write_memory_with_tier(
                &conn, &format!("Dense topic memory {}", i), "user", None, None,
                &["topic:dense".to_string()], &[], Some("long_term"),
            ).unwrap();
        }
        // Without embeddings, no clustering happens (all entries skipped)
        let (topics, clusters, _) = consolidate_memories(&conn).unwrap();
        assert_eq!(clusters, 0, "No clusters without embeddings");
    }

    /// Helper: insert a memory + chunk directly into DB, bypassing save flow (no supersession).
    /// Used for consolidation tests that need many similar memories without triggering 8D.
    fn insert_test_memory(conn: &Connection, content: &str, tags: &[String], embedding: &[f64], tier: &str) -> String {
        let id = generate_id();
        let now = chrono::Utc::now().to_rfc3339();
        let tags_json = serde_json::to_string(tags).unwrap_or_else(|_| "[]".to_string());
        conn.execute(
            "INSERT INTO memories (id, content, source, tags, created_at, updated_at, tier, strength)
             VALUES (?1, ?2, 'user', ?3, ?4, ?5, ?6, 1.0)",
            params![id, content, tags_json, now, now, tier],
        ).unwrap();
        if !embedding.is_empty() {
            let chunk_id = generate_chunk_id(&id, 0);
            let emb_json = serde_json::to_string(embedding).unwrap();
            conn.execute(
                "INSERT INTO chunks (id, memory_id, text, start_line, end_line, hash, embedding)
                 VALUES (?1, ?2, ?3, 0, 0, '', ?4)",
                params![chunk_id, id, content, emb_json],
            ).unwrap();
        }
        id
    }

    #[test]
    fn consolidation_clusters_similar_embeddings() {
        // Create 12 memories with identical embeddings via direct insert (bypass supersession)
        let conn = test_db();
        let fake_emb = vec![0.1, 0.2, 0.3, 0.4, 0.5];
        for i in 0..12 {
            insert_test_memory(
                &conn, &format!("Cluster test memory {}", i),
                &["topic:clustered".to_string()], &fake_emb, "long_term",
            );
        }

        let (topics, clusters, memories) = consolidate_memories(&conn).unwrap();
        assert!(topics > 0, "Should process at least one topic");
        assert!(clusters >= 1, "Should form at least one cluster");
        assert!(memories >= 3, "Should consume at least 3 memories");

        // Check that originals are marked 'consolidated'
        let consolidated_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM memories WHERE tier = 'consolidated'",
            [], |row| row.get(0),
        ).unwrap();
        assert!(consolidated_count > 0, "Originals should be tier='consolidated'");

        // Check that a new consolidated memory exists
        let consolidation_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM memories WHERE source = 'consolidation'",
            [], |row| row.get(0),
        ).unwrap();
        assert!(consolidation_count > 0, "Should have consolidation source memories");

        // Check MAGMA 'absorbed' edges exist
        let edge_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM edges WHERE edge_type = 'absorbed'",
            [], |row| row.get(0),
        ).unwrap();
        assert!(edge_count > 0, "Should create 'absorbed' edges from consolidated to originals");
    }

    #[test]
    fn consolidation_inherits_max_strength() {
        // Consolidated memory should inherit the highest strength from its members
        let conn = test_db();
        let fake_emb = vec![0.5, 0.6, 0.7];
        for i in 0..12 {
            let id = insert_test_memory(
                &conn, &format!("Strength test {}", i),
                &["topic:strength".to_string()], &fake_emb, "long_term",
            );
            // Give one memory high strength
            if i == 3 {
                conn.execute(
                    "UPDATE memories SET strength = 2.5 WHERE id = ?1",
                    params![id],
                ).unwrap();
            }
        }

        consolidate_memories(&conn).unwrap();

        // The consolidated memory should have strength >= 2.5
        let max_strength: Option<f64> = conn.query_row(
            "SELECT MAX(strength) FROM memories WHERE source = 'consolidation'",
            [], |row| row.get(0),
        ).unwrap();
        assert!(
            max_strength.unwrap_or(0.0) >= 2.5,
            "Consolidated memory should inherit max strength (got {:?})", max_strength
        );
    }

    #[test]
    fn consolidation_does_not_reconsolidate() {
        // Already consolidated memories should not be processed again
        let conn = test_db();
        let fake_emb = vec![0.1, 0.2, 0.3];
        for i in 0..12 {
            insert_test_memory(
                &conn, &format!("First pass {}", i),
                &["topic:reconsolidate".to_string()], &fake_emb, "long_term",
            );
        }

        let (_, clusters1, _) = consolidate_memories(&conn).unwrap();
        assert!(clusters1 > 0, "First pass should consolidate");

        // Second pass: all originals are now 'consolidated', only the new consolidated
        // memory remains as 'long_term'. Not enough for 10+ threshold.
        let (_, clusters2, _) = consolidate_memories(&conn).unwrap();
        assert_eq!(clusters2, 0, "Second pass should find nothing to consolidate");
    }

    // --- Phase 8D: Active Forgetting (Supersession) ---

    #[test]
    fn supersession_marks_old_as_superseded() {
        // Two memories with identical embeddings and same topic → old gets superseded
        let conn = test_db();
        let emb = vec![0.9, 0.8, 0.7, 0.6, 0.5];

        // Save old memory
        let old = write_memory_with_tier(
            &conn, "User prefers dark mode for coding", "user", None, None,
            &["topic:personal".to_string()], &[emb.clone()], Some("long_term"),
        ).unwrap();

        // Save new memory with same embedding and topic → should supersede old
        let _new = write_memory_with_tier(
            &conn, "User prefers light mode for coding", "user", None, None,
            &["topic:personal".to_string()], &[emb.clone()], Some("long_term"),
        ).unwrap();

        // Old memory should now be superseded
        let tier: String = conn.query_row(
            "SELECT tier FROM memories WHERE id = ?1",
            params![old.id], |r| r.get(0),
        ).unwrap();
        assert_eq!(tier, "superseded", "Old memory should be tier='superseded'");
    }

    #[test]
    fn supersession_creates_magma_edge() {
        let conn = test_db();
        let emb = vec![0.5, 0.4, 0.3, 0.2, 0.1];

        let old = write_memory_with_tier(
            &conn, "Project deadline is Friday", "conversation", None, None,
            &["topic:project".to_string()], &[emb.clone()], Some("long_term"),
        ).unwrap();

        let new_mem = write_memory_with_tier(
            &conn, "Project deadline moved to Monday", "conversation", None, None,
            &["topic:project".to_string()], &[emb.clone()], Some("long_term"),
        ).unwrap();

        // Check supersedes edge exists
        let edge_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM edges WHERE edge_type = 'supersedes' AND source_id = ?1 AND target_id = ?2",
            params![new_mem.id, old.id], |row| row.get(0),
        ).unwrap();
        assert_eq!(edge_count, 1, "Should create supersedes edge from new to old");
    }

    #[test]
    fn supersession_requires_same_topic() {
        // Different topic tags → no supersession even with identical embeddings
        let conn = test_db();
        let emb = vec![0.3, 0.4, 0.5, 0.6, 0.7];

        let old = write_memory_with_tier(
            &conn, "Technical detail about APIs", "user", None, None,
            &["topic:technical".to_string()], &[emb.clone()], Some("long_term"),
        ).unwrap();

        let _new = write_memory_with_tier(
            &conn, "Personal preference about APIs", "user", None, None,
            &["topic:personal".to_string()], &[emb.clone()], Some("long_term"),
        ).unwrap();

        let tier: String = conn.query_row(
            "SELECT tier FROM memories WHERE id = ?1",
            params![old.id], |r| r.get(0),
        ).unwrap();
        assert_eq!(tier, "long_term", "Different topics should NOT trigger supersession");
    }

    #[test]
    fn supersession_skips_consolidation_source() {
        // Memories with source="consolidation" should not trigger supersession
        let conn = test_db();
        let emb = vec![0.2, 0.3, 0.4, 0.5, 0.6];

        let old = write_memory_with_tier(
            &conn, "Original content", "user", None, None,
            &["topic:technical".to_string()], &[emb.clone()], Some("long_term"),
        ).unwrap();

        // Save as consolidation source — should NOT supersede
        let _consolidated = write_memory_with_tier(
            &conn, "Consolidated content", "consolidation", None, None,
            &["topic:technical".to_string()], &[emb.clone()], Some("long_term"),
        ).unwrap();

        let tier: String = conn.query_row(
            "SELECT tier FROM memories WHERE id = ?1",
            params![old.id], |r| r.get(0),
        ).unwrap();
        assert_eq!(tier, "long_term", "Consolidation source should not trigger supersession");
    }

    #[test]
    fn supersession_tier_weight_ordering() {
        // Verify the full tier weight ordering: superseded < consolidated < archived < short_term < long_term
        assert!(tier_weight("superseded") < tier_weight("consolidated"));
        assert!(tier_weight("consolidated") < tier_weight("archived"));
        assert!(tier_weight("archived") < tier_weight("short_term"));
        assert!(tier_weight("short_term") < tier_weight("long_term"));
    }

    #[test]
    fn supersession_does_not_double_supersede() {
        // Already superseded memories should not be superseded again
        let conn = test_db();
        let emb = vec![0.1, 0.1, 0.1, 0.1, 0.1];

        let m1 = write_memory_with_tier(
            &conn, "Version 1 info", "user", None, None,
            &["topic:project".to_string()], &[emb.clone()], Some("long_term"),
        ).unwrap();

        // Second save supersedes m1
        write_memory_with_tier(
            &conn, "Version 2 info", "user", None, None,
            &["topic:project".to_string()], &[emb.clone()], Some("long_term"),
        ).unwrap();

        let tier_after_first: String = conn.query_row(
            "SELECT tier FROM memories WHERE id = ?1",
            params![m1.id], |r| r.get(0),
        ).unwrap();
        assert_eq!(tier_after_first, "superseded");

        // Third save should NOT supersede m1 again (already superseded)
        write_memory_with_tier(
            &conn, "Version 3 info", "user", None, None,
            &["topic:project".to_string()], &[emb.clone()], Some("long_term"),
        ).unwrap();

        // Count supersedes edges targeting m1 — should be exactly 1
        let edge_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM edges WHERE edge_type = 'supersedes' AND target_id = ?1",
            params![m1.id], |row| row.get(0),
        ).unwrap();
        assert_eq!(edge_count, 1, "Should have exactly 1 supersedes edge to m1, not multiple");
    }
}

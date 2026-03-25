//! Working Memory, Session Notes, Task Tracking, Skills, and Markdown Sync
//!
//! Extracted from memory.rs for modularity (P1).
//! - Working memory: per-session scratchpad (read/write/append/clear/flush)
//! - Session notes: continuity handoff between sessions
//! - Task tracking: MAGMA entities with status (open/in_progress/done/blocked)
//! - Skills: tool schemas registered as MAGMA entities with keyword edges
//! - Markdown sync: bidirectional daily log ↔ DB reimport

use chrono::Utc;
use rusqlite::params;
use std::fs;
use std::path::PathBuf;

use crate::memory::{
    chunk_text, ensure_db, extract_keywords, generate_id, get_memory_dir, is_near_duplicate,
    try_get_embedding, write_memory_internal, write_memory_with_tier, Entity, MemoryRecord, MemoryState,
};
use crate::paths::get_app_data_dir;

// ============================================
// Working Memory (Per-Session Scratchpad)
// ============================================

/// Get the working memory file path for the current session.
fn get_working_memory_path() -> PathBuf {
    get_app_data_dir().join("harness").join("working_memory.md")
}

/// Read working memory contents. Returns empty string if no working memory exists.
#[tauri::command]
pub fn working_memory_read() -> Result<String, String> {
    let path = get_working_memory_path();
    if path.exists() {
        fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read working memory: {}", e))
    } else {
        Ok(String::new())
    }
}

/// Write/overwrite working memory contents.
#[tauri::command]
pub fn working_memory_write(content: String) -> Result<(), String> {
    let path = get_working_memory_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create working memory dir: {}", e))?;
    }
    fs::write(&path, &content)
        .map_err(|e| format!("Failed to write working memory: {}", e))
}

/// Append to working memory (adds a timestamped section).
#[tauri::command]
pub fn working_memory_append(content: String) -> Result<(), String> {
    let path = get_working_memory_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create working memory dir: {}", e))?;
    }
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let timestamp = Utc::now().format("%H:%M:%S").to_string();
    let entry = format!("\n## {}\n{}\n", timestamp, content);
    let header = if existing.is_empty() {
        let date = Utc::now().format("%Y-%m-%d").to_string();
        format!("# Working Memory — {}\n", date)
    } else {
        String::new()
    };
    fs::write(&path, format!("{}{}{}", existing, header, entry))
        .map_err(|e| format!("Failed to append to working memory: {}", e))
}

/// Clear working memory (session end).
#[tauri::command]
pub fn working_memory_clear() -> Result<(), String> {
    let path = get_working_memory_path();
    if path.exists() {
        fs::remove_file(&path)
            .map_err(|e| format!("Failed to clear working memory: {}", e))?;
    }
    Ok(())
}

/// Flush working memory to short-term memory (end of session).
/// Saves the working memory contents as a memory record before clearing.
#[tauri::command]
pub async fn working_memory_flush(
    state: tauri::State<'_, MemoryState>,
) -> Result<Option<MemoryRecord>, String> {
    let content = working_memory_read()?;
    if content.trim().is_empty() {
        return Ok(None);
    }

    // Generate embeddings
    let chunks = chunk_text(&content, 1600, 320);
    let mut embeddings = Vec::new();
    for (_, _, text) in &chunks {
        embeddings.push(try_get_embedding(text).await);
    }

    ensure_db(&state)?;
    let db_guard = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;

    // Anti-trash: skip if near-duplicate of existing memory
    if is_near_duplicate(conn, &embeddings) {
        drop(db_guard);
        working_memory_clear()?;
        return Ok(None);
    }

    // Strengthen related long-term memories (summary → reinforcement feedback loop)
    let keywords = extract_keywords(&content);
    if !keywords.is_empty() {
        let keyword_query = keywords.iter().take(5).cloned().collect::<Vec<_>>().join(" OR ");
        if let Err(e) = conn.execute(
            "UPDATE memories SET strength = MIN(strength + 0.05, 2.0)
             WHERE id IN (
               SELECT m.id FROM memories m
               JOIN chunks_fts ON chunks_fts.memory_id = CAST(m.id AS TEXT)
               WHERE chunks_fts MATCH ?1
               LIMIT 10
             )",
            rusqlite::params![keyword_query],
        ) {
            eprintln!("[HIVE] MEMORY | reinforcement update failed: {}", e);
        }
    }

    let tags = vec!["working-memory".to_string(), "session-summary".to_string()];
    // Phase 4C: Atomic tier — insert as short_term directly (no INSERT-then-UPDATE race).
    // Promoted to long_term after access_count > 3 (validated through repeated recall).
    let record = write_memory_with_tier(
        conn,
        &content,
        "working-memory",
        None,
        None,
        &tags,
        &embeddings,
        Some("short_term"),
    )?;

    // Clear working memory after flush
    drop(db_guard);
    working_memory_clear()?;

    Ok(Some(record))
}

// ============================================
// Session Handoff Notes (Phase 3.5.6)
// ============================================

/// Get the session notes file path.
fn get_session_notes_path() -> PathBuf {
    get_app_data_dir().join("harness").join("SESSION_NOTES.md")
}

/// Read session handoff notes from previous session.
/// Returns empty string if no notes exist.
#[tauri::command]
pub fn session_notes_read() -> Result<String, String> {
    let path = get_session_notes_path();
    if path.exists() {
        fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read session notes: {}", e))
    } else {
        Ok(String::new())
    }
}

/// Write session handoff notes (AI writes continuity notes for next session).
#[tauri::command]
pub fn session_notes_write(content: String) -> Result<(), String> {
    let path = get_session_notes_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create session notes dir: {}", e))?;
    }
    fs::write(&path, &content)
        .map_err(|e| format!("Failed to write session notes: {}", e))
}

// ============================================
// Cross-Session Task Tracking (Phase 3.5.6)
// ============================================
// Tasks are MAGMA entities (entity_type: "task") with structured state.
// Persists across sessions so HIVE remembers ongoing work.

/// Create or update a tracked task. Returns the task entity.
#[tauri::command]
pub fn memory_task_upsert(
    state: tauri::State<'_, MemoryState>,
    name: String,
    description: String,
    status: Option<String>, // "open", "in_progress", "done", "blocked"
    notes: Option<String>,
) -> Result<Entity, String> {
    ensure_db(&state)?;
    let db_guard = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;

    let now = Utc::now().to_rfc3339();
    let task_status = status.unwrap_or_else(|| "open".to_string());
    let task_notes = notes.unwrap_or_default();

    let task_state = serde_json::json!({
        "description": description,
        "status": task_status,
        "notes": task_notes,
    });

    // Check if task already exists
    let existing: Option<(String, String)> = conn.query_row(
        "SELECT id, created_at FROM entities WHERE entity_type = 'task' AND name = ?1",
        params![name],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).ok();

    let (id, created_at) = if let Some((eid, orig_created)) = existing {
        conn.execute(
            "UPDATE entities SET state = ?1, updated_at = ?2 WHERE id = ?3",
            params![task_state.to_string(), now, eid],
        ).map_err(|e| format!("Failed to update task: {}", e))?;
        (eid, orig_created)
    } else {
        let new_id = generate_id();
        conn.execute(
            "INSERT INTO entities (id, entity_type, name, state, metadata, created_at, updated_at)
             VALUES (?1, 'task', ?2, ?3, '{}', ?4, ?5)",
            params![new_id, name, task_state.to_string(), now, now],
        ).map_err(|e| format!("Failed to insert task: {}", e))?;
        (new_id, now.clone())
    };

    Ok(Entity {
        id,
        entity_type: "task".to_string(),
        name,
        state: task_state,
        metadata: serde_json::json!({}),
        created_at,
        updated_at: now,
    })
}

/// List tasks, optionally filtered by status. Returns all if no status filter.
#[tauri::command]
pub fn memory_task_list(
    state: tauri::State<'_, MemoryState>,
    status_filter: Option<String>, // "open", "in_progress", "done", "blocked", or None for all
) -> Result<Vec<Entity>, String> {
    ensure_db(&state)?;
    let db_guard = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;

    // Query helper: parse entity rows
    fn parse_task_row(row: &rusqlite::Row) -> rusqlite::Result<Entity> {
        Ok(Entity {
            id: row.get(0)?,
            entity_type: row.get(1)?,
            name: row.get(2)?,
            state: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or(serde_json::json!({})),
            metadata: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or(serde_json::json!({})),
            created_at: row.get(5)?,
            updated_at: row.get(6)?,
        })
    }

    let tasks: Vec<Entity> = if let Some(ref status) = status_filter {
        let pattern = format!("%\"status\":\"{}\"%", status);
        let mut stmt = conn.prepare(
            "SELECT id, entity_type, name, state, metadata, created_at, updated_at
             FROM entities WHERE entity_type = 'task' AND state LIKE ?1
             ORDER BY updated_at DESC"
        ).map_err(|e| format!("Failed to prepare query: {}", e))?;
        let result: Vec<Entity> = stmt.query_map(params![pattern], parse_task_row)
            .map_err(|e| format!("Failed to query tasks: {}", e))?
            .filter_map(|r| r.ok())
            .collect();
        result
    } else {
        let mut stmt = conn.prepare(
            "SELECT id, entity_type, name, state, metadata, created_at, updated_at
             FROM entities WHERE entity_type = 'task'
             ORDER BY updated_at DESC"
        ).map_err(|e| format!("Failed to prepare query: {}", e))?;
        let result: Vec<Entity> = stmt.query_map([], parse_task_row)
            .map_err(|e| format!("Failed to query tasks: {}", e))?
            .filter_map(|r| r.ok())
            .collect();
        result
    };

    Ok(tasks)
}

// ============================================
// Skills as Graph Nodes (Phase 3.5.5)
// ============================================
// Tools/skills are registered as MAGMA entities connected to topic keywords.
// This enables the "discover skills by association" pattern: when a conversation
// touches "database," the memory_search and run_command skills surface.
// At 20 tools this is infrastructure; at 100+ it prevents context overload.

/// Sync tool schemas into the MAGMA graph as entities with keyword edges.
/// Called once when tools are loaded (not per-turn). Idempotent.
#[tauri::command]
pub fn memory_sync_skills(
    state: tauri::State<'_, MemoryState>,
    tools: Vec<serde_json::Value>,
) -> Result<usize, String> {
    ensure_db(&state)?;
    let db_guard = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;

    let now = Utc::now().to_rfc3339();
    let mut synced = 0;

    for tool in &tools {
        let name = tool.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let desc = tool.get("description").and_then(|v| v.as_str()).unwrap_or("");
        let risk = tool.get("risk_level").and_then(|v| v.as_str()).unwrap_or("low");
        if name.is_empty() { continue; }

        let entity_id = format!("skill_{}", name);

        // Upsert entity
        let existing: Option<String> = conn.query_row(
            "SELECT id FROM entities WHERE entity_type = 'skill' AND name = ?1",
            params![name],
            |row| row.get(0),
        ).ok();

        let state_json = serde_json::json!({
            "description": desc,
            "risk_level": risk,
        }).to_string();

        if let Some(eid) = existing {
            conn.execute(
                "UPDATE entities SET state = ?1, updated_at = ?2 WHERE id = ?3",
                params![state_json, now, eid],
            ).map_err(|e| format!("Failed to update skill entity: {}", e))?;
        } else {
            conn.execute(
                "INSERT INTO entities (id, entity_type, name, state, metadata, created_at, updated_at)
                 VALUES (?1, 'skill', ?2, ?3, '{}', ?4, ?5)",
                params![entity_id, name, state_json, now, now],
            ).map_err(|e| format!("Failed to insert skill entity: {}", e))?;
        }

        // Extract keywords from description and create edges to topic keywords
        let keywords = extract_keywords(desc);
        for keyword in keywords.iter().take(5) {
            // Check if edge already exists
            let edge_exists: bool = conn.query_row(
                "SELECT COUNT(*) FROM edges WHERE source_type = 'skill' AND source_id = ?1
                 AND target_type = 'keyword' AND target_id = ?2 AND edge_type = 'relevant_to'",
                params![entity_id, keyword],
                |row| row.get::<_, i64>(0),
            ).unwrap_or(0) > 0;

            if !edge_exists {
                let edge_id = generate_id();
                if let Err(e) = conn.execute(
                    "INSERT INTO edges (id, source_type, source_id, target_type, target_id, edge_type, weight, metadata, created_at)
                     VALUES (?1, 'skill', ?2, 'keyword', ?3, 'relevant_to', 0.8, '{}', ?4)",
                    params![edge_id, entity_id, keyword, now],
                ) {
                    eprintln!("[HIVE] MEMORY | skill-keyword edge insert failed: {}", e);
                }
            }
        }

        synced += 1;
    }

    Ok(synced)
}

/// Given a query, find skills that are relevant via graph traversal.
/// Returns skill names ordered by relevance (keyword overlap → edge weight).
#[tauri::command]
pub fn memory_discover_skills(
    state: tauri::State<'_, MemoryState>,
    query: String,
) -> Result<Vec<String>, String> {
    ensure_db(&state)?;
    let db_guard = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;

    let query_keywords = extract_keywords(&query);
    if query_keywords.is_empty() {
        return Ok(Vec::new());
    }

    // Find skills connected to query keywords via edges
    let placeholders: Vec<String> = query_keywords.iter().enumerate()
        .map(|(i, _)| format!("?{}", i + 1))
        .collect();
    let sql = format!(
        "SELECT e.source_id, SUM(e.weight) as total_weight
         FROM edges e
         WHERE e.source_type = 'skill'
           AND e.target_type = 'keyword'
           AND e.edge_type = 'relevant_to'
           AND e.target_id IN ({})
         GROUP BY e.source_id
         ORDER BY total_weight DESC
         LIMIT 10",
        placeholders.join(", ")
    );

    let mut stmt = conn.prepare(&sql)
        .map_err(|e| format!("Failed to query skill edges: {}", e))?;

    let params: Vec<Box<dyn rusqlite::types::ToSql>> = query_keywords.iter()
        .map(|k| Box::new(k.clone()) as Box<dyn rusqlite::types::ToSql>)
        .collect();

    let skill_ids: Vec<String> = stmt.query_map(
        rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())),
        |row| row.get(0),
    )
    .map_err(|e| format!("Failed to read skill edges: {}", e))?
    .filter_map(|r| r.ok())
    .collect();

    // Map entity IDs back to skill names
    let skill_names: Vec<String> = skill_ids.iter()
        .filter_map(|id| {
            conn.query_row(
                "SELECT name FROM entities WHERE id = ?1 AND entity_type = 'skill'",
                params![id],
                |row| row.get(0),
            ).ok()
        })
        .collect();

    Ok(skill_names)
}

// ============================================
// Markdown ↔ DB Bidirectional Sync (Phase 3.5.5)
// ============================================
// Daily markdown logs are the human-readable source of truth.
// This sync allows: edit/add entries in markdown → reindex in DB.
// Obsidian-compatible: memory/*.md files are valid markdown.

/// Parse a daily log markdown file into (source, tags, content) entries.
fn parse_daily_log(text: &str) -> Vec<(String, Vec<String>, String)> {
    let mut entries = Vec::new();
    let mut current_source = String::new();
    let mut current_tags: Vec<String> = Vec::new();
    let mut current_content = String::new();
    let mut in_entry = false;

    for line in text.lines() {
        if line.starts_with("## ") {
            // Save previous entry
            if in_entry && !current_content.trim().is_empty() {
                entries.push((current_source.clone(), current_tags.clone(), current_content.trim().to_string()));
            }
            // Parse header: "## HH:MM:SS (source) [tag1, tag2]"
            let header = &line[3..];
            // Extract source from (...)
            current_source = header
                .split('(').nth(1)
                .and_then(|s| s.split(')').next())
                .unwrap_or("markdown")
                .to_string();
            // Extract tags from [...]
            current_tags = header
                .split('[').nth(1)
                .and_then(|s| s.split(']').next())
                .map(|s| s.split(',').map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect())
                .unwrap_or_default();
            current_content = String::new();
            in_entry = true;
        } else if line.starts_with("# ") {
            // File header line — skip
        } else if in_entry {
            current_content.push_str(line);
            current_content.push('\n');
        }
    }
    // Don't forget the last entry
    if in_entry && !current_content.trim().is_empty() {
        entries.push((current_source, current_tags, current_content.trim().to_string()));
    }
    entries
}

/// Reimport all markdown memory files into the DB.
/// Entries that already exist (by content hash) are skipped.
/// New or edited entries are indexed. Returns count of new entries added.
#[tauri::command]
pub async fn memory_reimport_markdown(
    state: tauri::State<'_, MemoryState>,
) -> Result<usize, String> {
    let memory_dir = get_memory_dir();
    if !memory_dir.exists() {
        return Ok(0);
    }

    // Collect all .md files
    let md_files: Vec<PathBuf> = fs::read_dir(&memory_dir)
        .map_err(|e| format!("Failed to read memory dir: {}", e))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|ext| ext == "md").unwrap_or(false))
        .collect();

    // Parse all entries from all files
    let mut all_entries: Vec<(String, Vec<String>, String)> = Vec::new();
    for path in &md_files {
        let text = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
        all_entries.extend(parse_daily_log(&text));
    }

    if all_entries.is_empty() {
        return Ok(0);
    }

    // Get embeddings for all entries (async, before lock)
    let mut entry_data: Vec<(String, Vec<String>, String, Vec<Vec<f64>>)> = Vec::new();
    for (source, tags, content) in all_entries {
        let chunks = chunk_text(&content, 1600, 320);
        let mut embeddings = Vec::new();
        for (_, _, text) in &chunks {
            embeddings.push(try_get_embedding(text).await);
        }
        entry_data.push((source, tags, content, embeddings));
    }

    ensure_db(&state)?;
    let db_guard = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;

    let mut added = 0;
    for (source, tags, content, embeddings) in entry_data {
        // Skip if near-duplicate already exists in DB
        if is_near_duplicate(conn, &embeddings) {
            continue;
        }

        // Also skip exact content match (cheaper check for non-embedded content)
        let exact_exists: bool = conn.query_row(
            "SELECT COUNT(*) FROM memories WHERE content = ?1",
            params![content],
            |row| row.get::<_, i64>(0),
        ).unwrap_or(0) > 0;
        if exact_exists {
            continue;
        }

        let mut all_tags = tags;
        all_tags.push("reimported".to_string());
        write_memory_internal(conn, &content, &source, None, None, &all_tags, &embeddings)?;
        added += 1;
    }

    Ok(added)
}

/// Get the memory directory path (for UI to show to user).
#[tauri::command]
pub fn memory_get_directory() -> Result<String, String> {
    let dir = get_memory_dir();
    Ok(dir.to_string_lossy().to_string())
}

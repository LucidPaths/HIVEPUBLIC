//! MAGMA Graph Operations (Phase 4 — arXiv:2601.03236)
//!
//! Four interconnected graphs sharing one SQLite DB:
//!   Semantic  = existing memories + chunks (hybrid search) — in memory.rs
//!   Episodic  = events table (timestamped agent actions)
//!   Entity    = entities table (tracked objects: files, models, agents)
//!   Procedural = procedures table (learned tool chains)
//!   Edges     = edges table (typed relationships across all graphs)

use chrono::Utc;
use rusqlite::params;
use serde_json;

use crate::memory::{
    ensure_db, generate_id, Edge, Entity, Event, MagmaStats, MemoryState, Procedure,
};

// ============================================
// Tauri Commands
// ============================================

/// Record an event in the episodic graph.
#[tauri::command]
pub fn magma_add_event(
    state: tauri::State<'_, MemoryState>,
    event_type: String,
    agent: String,
    content: String,
    metadata: Option<serde_json::Value>,
    session_id: Option<String>,
) -> Result<Event, String> {
    ensure_db(&state)?;
    let db_guard = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;

    let id = generate_id();
    let now = Utc::now().to_rfc3339();
    let meta = metadata.unwrap_or(serde_json::json!({}));

    conn.execute(
        "INSERT INTO events (id, event_type, agent, content, metadata, session_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![id, event_type, agent, content, meta.to_string(), session_id, now],
    )
    .map_err(|e| format!("Failed to insert event: {}", e))?;

    Ok(Event {
        id,
        event_type,
        agent,
        content,
        metadata: meta,
        session_id,
        created_at: now,
    })
}

/// Query events since a timestamp (for wake briefings).
#[tauri::command]
pub fn magma_events_since(
    state: tauri::State<'_, MemoryState>,
    since: String,
    agent: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<Event>, String> {
    ensure_db(&state)?;
    let db_guard = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;

    let max = limit.unwrap_or(50);

    let mut events = Vec::new();
    if let Some(agent_filter) = agent {
        let mut stmt = conn.prepare(
            "SELECT id, event_type, agent, content, metadata, session_id, created_at
             FROM events WHERE created_at > ?1 AND agent = ?2
             ORDER BY created_at ASC LIMIT ?3"
        ).map_err(|e| format!("Query error: {}", e))?;

        let rows = stmt.query_map(params![since, agent_filter, max as i64], row_to_event)
            .map_err(|e| format!("Query error: {}", e))?;
        for row in rows { if let Ok(e) = row { events.push(e); } }
    } else {
        let mut stmt = conn.prepare(
            "SELECT id, event_type, agent, content, metadata, session_id, created_at
             FROM events WHERE created_at > ?1
             ORDER BY created_at ASC LIMIT ?2"
        ).map_err(|e| format!("Query error: {}", e))?;

        let rows = stmt.query_map(params![since, max as i64], row_to_event)
            .map_err(|e| format!("Query error: {}", e))?;
        for row in rows { if let Ok(e) = row { events.push(e); } }
    }

    Ok(events)
}

/// Register or update an entity in the entity graph.
/// Upserts by (entity_type, name) — same entity updates in place.
#[tauri::command]
pub fn magma_upsert_entity(
    state: tauri::State<'_, MemoryState>,
    entity_type: String,
    name: String,
    entity_state: Option<serde_json::Value>,
    metadata: Option<serde_json::Value>,
) -> Result<Entity, String> {
    ensure_db(&state)?;
    let db_guard = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;

    let now = Utc::now().to_rfc3339();
    let st = entity_state.unwrap_or(serde_json::json!({}));
    let meta = metadata.unwrap_or(serde_json::json!({}));

    // Check if entity already exists
    let existing: Option<String> = conn.query_row(
        "SELECT id FROM entities WHERE entity_type = ?1 AND name = ?2",
        params![entity_type, name],
        |row| row.get(0),
    ).ok();

    let (id, actual_created_at) = if let Some(existing_id) = existing {
        // Fetch original created_at before updating
        let original_created: String = conn.query_row(
            "SELECT created_at FROM entities WHERE id = ?1",
            params![existing_id],
            |row| row.get(0),
        ).map_err(|e| format!("Failed to read entity: {}", e))?;
        // Update existing
        conn.execute(
            "UPDATE entities SET state = ?1, metadata = ?2, updated_at = ?3 WHERE id = ?4",
            params![st.to_string(), meta.to_string(), now, existing_id],
        ).map_err(|e| format!("Failed to update entity: {}", e))?;
        (existing_id, original_created)
    } else {
        // Insert new
        let new_id = generate_id();
        conn.execute(
            "INSERT INTO entities (id, entity_type, name, state, metadata, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![new_id, entity_type, name, st.to_string(), meta.to_string(), now, now],
        ).map_err(|e| format!("Failed to insert entity: {}", e))?;
        (new_id, now.clone())
    };

    Ok(Entity {
        id,
        entity_type,
        name,
        state: st,
        metadata: meta,
        created_at: actual_created_at,
        updated_at: now,
    })
}

/// Get an entity by type and name.
#[tauri::command]
pub fn magma_get_entity(
    state: tauri::State<'_, MemoryState>,
    entity_type: String,
    name: String,
) -> Result<Option<Entity>, String> {
    ensure_db(&state)?;
    let db_guard = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;

    let result = conn.query_row(
        "SELECT id, entity_type, name, state, metadata, created_at, updated_at
         FROM entities WHERE entity_type = ?1 AND name = ?2",
        params![entity_type, name],
        row_to_entity,
    ).ok();

    Ok(result)
}

/// List entities by type.
#[tauri::command]
pub fn magma_list_entities(
    state: tauri::State<'_, MemoryState>,
    entity_type: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<Entity>, String> {
    ensure_db(&state)?;
    let db_guard = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;

    let max = limit.unwrap_or(100);
    let mut entities = Vec::new();

    if let Some(et) = entity_type {
        let mut stmt = conn.prepare(
            "SELECT id, entity_type, name, state, metadata, created_at, updated_at
             FROM entities WHERE entity_type = ?1 ORDER BY updated_at DESC LIMIT ?2"
        ).map_err(|e| format!("Query error: {}", e))?;
        let rows = stmt.query_map(params![et, max as i64], row_to_entity)
            .map_err(|e| format!("Query error: {}", e))?;
        for row in rows { if let Ok(e) = row { entities.push(e); } }
    } else {
        let mut stmt = conn.prepare(
            "SELECT id, entity_type, name, state, metadata, created_at, updated_at
             FROM entities ORDER BY updated_at DESC LIMIT ?1"
        ).map_err(|e| format!("Query error: {}", e))?;
        let rows = stmt.query_map(params![max as i64], row_to_entity)
            .map_err(|e| format!("Query error: {}", e))?;
        for row in rows { if let Ok(e) = row { entities.push(e); } }
    }

    Ok(entities)
}

/// Record a learned procedure (tool chain).
#[tauri::command]
pub fn magma_save_procedure(
    state: tauri::State<'_, MemoryState>,
    name: String,
    description: String,
    steps: Vec<serde_json::Value>,
    trigger_pattern: Option<String>,
) -> Result<Procedure, String> {
    ensure_db(&state)?;
    let db_guard = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;

    let now = Utc::now().to_rfc3339();
    let trigger = trigger_pattern.unwrap_or_default();
    let steps_json = serde_json::to_string(&steps).unwrap_or_else(|_| "[]".to_string());

    // P5: Upsert by name — prevent duplicate procedure accumulation.
    // If a procedure with the same name exists, increment success_count and update.
    let existing: Option<(String, i64, i64, Option<String>, String)> = conn.query_row(
        "SELECT id, success_count, fail_count, last_used, created_at FROM procedures WHERE name = ?1",
        params![name],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
    ).ok();

    if let Some((existing_id, success_count, fail_count, last_used, created_at)) = existing {
        conn.execute(
            "UPDATE procedures SET description = ?1, steps = ?2, trigger_pattern = ?3, \
             success_count = ?4, updated_at = ?5 WHERE id = ?6",
            params![description, steps_json, trigger, success_count + 1, now, existing_id],
        ).map_err(|e| format!("Failed to update procedure: {}", e))?;

        Ok(Procedure {
            id: existing_id,
            name,
            description,
            steps,
            trigger_pattern: trigger,
            success_count: success_count + 1,
            fail_count,
            last_used,
            created_at,
            updated_at: now,
        })
    } else {
        let id = generate_id();
        conn.execute(
            "INSERT INTO procedures (id, name, description, steps, trigger_pattern, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, name, description, steps_json, trigger, now, now],
        ).map_err(|e| format!("Failed to save procedure: {}", e))?;

        Ok(Procedure {
            id,
            name,
            description,
            steps,
            trigger_pattern: trigger,
            success_count: 0,
            fail_count: 0,
            last_used: None,
            created_at: now.clone(),
            updated_at: now,
        })
    }
}

/// Increment success/fail count for a procedure (reinforcement learning signal).
#[tauri::command]
pub fn magma_record_procedure_outcome(
    state: tauri::State<'_, MemoryState>,
    procedure_id: String,
    success: bool,
) -> Result<(), String> {
    ensure_db(&state)?;
    let db_guard = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;

    let now = Utc::now().to_rfc3339();
    let rows_changed = if success {
        conn.execute(
            "UPDATE procedures SET success_count = success_count + 1, last_used = ?1, updated_at = ?2 WHERE id = ?3",
            params![now, now, procedure_id],
        ).map_err(|e| format!("Failed to update procedure: {}", e))?
    } else {
        conn.execute(
            "UPDATE procedures SET fail_count = fail_count + 1, last_used = ?1, updated_at = ?2 WHERE id = ?3",
            params![now, now, procedure_id],
        ).map_err(|e| format!("Failed to update procedure: {}", e))?
    };

    if rows_changed == 0 {
        return Err(format!("Procedure '{}' not found — reinforcement signal lost", procedure_id));
    }

    Ok(())
}

/// Create a typed edge between any two nodes in the MAGMA graphs.
#[tauri::command]
pub fn magma_add_edge(
    state: tauri::State<'_, MemoryState>,
    source_type: String,
    source_id: String,
    target_type: String,
    target_id: String,
    edge_type: String,
    weight: Option<f64>,
    metadata: Option<serde_json::Value>,
) -> Result<Edge, String> {
    ensure_db(&state)?;
    let db_guard = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;

    let id = generate_id();
    let now = Utc::now().to_rfc3339();
    let w = weight.unwrap_or(1.0);
    let meta = metadata.unwrap_or(serde_json::json!({}));

    conn.execute(
        "INSERT INTO edges (id, source_type, source_id, target_type, target_id, edge_type, weight, metadata, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![id, source_type, source_id, target_type, target_id, edge_type, w, meta.to_string(), now],
    ).map_err(|e| format!("Failed to insert edge: {}", e))?;

    Ok(Edge {
        id,
        source_type,
        source_id,
        target_type,
        target_id,
        edge_type,
        weight: w,
        metadata: meta,
        created_at: now,
    })
}

/// Traverse edges from a node — find all connected nodes within N hops.
/// This is MAGMA's graph-based retrieval: expand from seed along weighted edges.
#[tauri::command]
pub fn magma_traverse(
    state: tauri::State<'_, MemoryState>,
    node_type: String,
    node_id: String,
    max_depth: Option<usize>,
    edge_types: Option<Vec<String>>,
) -> Result<Vec<Edge>, String> {
    ensure_db(&state)?;
    let db_guard = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;

    let depth = max_depth.unwrap_or(2);
    let mut result_edges = Vec::new();
    let mut visited: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    let mut frontier: Vec<(String, String)> = vec![(node_type, node_id)];

    for _ in 0..depth {
        let mut next_frontier = Vec::new();
        for (ntype, nid) in &frontier {
            if !visited.insert((ntype.clone(), nid.clone())) {
                continue;
            }

            // Outgoing edges from this node
            let mut stmt = conn.prepare(
                "SELECT id, source_type, source_id, target_type, target_id, edge_type, weight, metadata, created_at
                 FROM edges WHERE source_type = ?1 AND source_id = ?2
                 ORDER BY weight DESC LIMIT 20"
            ).map_err(|e| format!("Query error: {}", e))?;

            let rows = stmt.query_map(params![ntype, nid], row_to_edge)
                .map_err(|e| format!("Query error: {}", e))?;

            for row in rows {
                if let Ok(edge) = row {
                    // Filter by edge type if specified
                    if let Some(ref types) = edge_types {
                        if !types.contains(&edge.edge_type) {
                            continue;
                        }
                    }
                    next_frontier.push((edge.target_type.clone(), edge.target_id.clone()));
                    result_edges.push(edge);
                }
            }

            // Also check incoming edges (bidirectional traversal)
            let mut stmt2 = conn.prepare(
                "SELECT id, source_type, source_id, target_type, target_id, edge_type, weight, metadata, created_at
                 FROM edges WHERE target_type = ?1 AND target_id = ?2
                 ORDER BY weight DESC LIMIT 20"
            ).map_err(|e| format!("Query error: {}", e))?;

            let rows2 = stmt2.query_map(params![ntype, nid], row_to_edge)
                .map_err(|e| format!("Query error: {}", e))?;

            for row in rows2 {
                if let Ok(edge) = row {
                    if let Some(ref types) = edge_types {
                        if !types.contains(&edge.edge_type) {
                            continue;
                        }
                    }
                    next_frontier.push((edge.source_type.clone(), edge.source_id.clone()));
                    result_edges.push(edge);
                }
            }
        }
        frontier = next_frontier;
    }

    Ok(result_edges)
}

/// Get MAGMA graph statistics (for harness manifest).
#[tauri::command]
pub fn magma_stats(
    state: tauri::State<'_, MemoryState>,
) -> Result<MagmaStats, String> {
    ensure_db(&state)?;
    let db_guard = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    let conn = db_guard.as_ref().ok_or("Memory DB not initialized")?;

    Ok(MagmaStats {
        events: conn.query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0)).unwrap_or(0),
        entities: conn.query_row("SELECT COUNT(*) FROM entities", [], |row| row.get(0)).unwrap_or(0),
        procedures: conn.query_row("SELECT COUNT(*) FROM procedures", [], |row| row.get(0)).unwrap_or(0),
        edges: conn.query_row("SELECT COUNT(*) FROM edges", [], |row| row.get(0)).unwrap_or(0),
    })
}

// ============================================
// Row Helpers
// ============================================

fn row_to_event(row: &rusqlite::Row) -> rusqlite::Result<Event> {
    let meta_str: String = row.get(4)?;
    Ok(Event {
        id: row.get(0)?,
        event_type: row.get(1)?,
        agent: row.get(2)?,
        content: row.get(3)?,
        metadata: serde_json::from_str(&meta_str).unwrap_or(serde_json::json!({})),
        session_id: row.get(5)?,
        created_at: row.get(6)?,
    })
}

pub(crate) fn row_to_entity(row: &rusqlite::Row) -> rusqlite::Result<Entity> {
    let state_str: String = row.get(3)?;
    let meta_str: String = row.get(4)?;
    Ok(Entity {
        id: row.get(0)?,
        entity_type: row.get(1)?,
        name: row.get(2)?,
        state: serde_json::from_str(&state_str).unwrap_or(serde_json::json!({})),
        metadata: serde_json::from_str(&meta_str).unwrap_or(serde_json::json!({})),
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn row_to_edge(row: &rusqlite::Row) -> rusqlite::Result<Edge> {
    let meta_str: String = row.get(7)?;
    Ok(Edge {
        id: row.get(0)?,
        source_type: row.get(1)?,
        source_id: row.get(2)?,
        target_type: row.get(3)?,
        target_id: row.get(4)?,
        edge_type: row.get(5)?,
        weight: row.get(6)?,
        metadata: serde_json::from_str(&meta_str).unwrap_or(serde_json::json!({})),
        created_at: row.get(8)?,
    })
}

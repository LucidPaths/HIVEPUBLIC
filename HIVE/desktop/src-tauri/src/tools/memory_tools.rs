//! Memory tools — give the model full agency over its memory
//!
//! Phase 3.5: The model needs to SEE, SEARCH, EDIT, and DELETE its own memories.
//! Without these, the model is blind to its own history — it gets auto-injected
//! context but can't actively query, correct mistakes, or prune outdated info.
//!
//! Tools:
//!   memory_save    — save new memories (existing)
//!   memory_search  — search memories by query (hybrid BM25 + vector)
//!   memory_edit    — update an existing memory's content
//!   memory_delete  — remove an outdated or incorrect memory
//!   task_track     — cross-session task management via MAGMA entities
//!   graph_query    — traverse and explore the MAGMA knowledge graph
//!   entity_track   — model curates entities (upsert, connect, delete)
//!   procedure_learn — record/recall/reinforce tool chain patterns
//!
//! Opens its own DB connection (same path, WAL mode = safe concurrent access)
//! because the HiveTool trait doesn't have access to Tauri managed state.

use super::{HiveTool, RiskLevel, ToolResult};
use serde_json::json;

/// Helper: open a connection to the memory DB with WAL mode.
fn open_memory_db() -> Result<rusqlite::Connection, String> {
    let db_path = crate::paths::get_app_data_dir().join("memory.db");
    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| format!("Failed to open memory DB: {}", e))?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000; PRAGMA foreign_keys=ON;")
        .map_err(|e| format!("Failed to set PRAGMA: {}", e))?;
    Ok(conn)
}

// ============================================
// memory_save — save new memories
// ============================================

pub struct MemorySaveTool;

#[async_trait::async_trait]
impl HiveTool for MemorySaveTool {
    fn name(&self) -> &str { "memory_save" }

    fn description(&self) -> &str {
        "Save information to persistent memory. Use this when the user asks you to \
         remember something, save a note, or store information for future sessions. \
         The memory will persist across conversations and can be recalled later."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "The information to remember. Write it as a clear, self-contained statement."
                },
                "tags": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional tags to categorize this memory (e.g. ['preference', 'project', 'design'])"
                }
            },
            "required": ["content"]
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::Low }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let content = params.get("content")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: content")?;

        if content.trim().is_empty() {
            return Ok(ToolResult {
                content: "Cannot save empty memory.".to_string(),
                is_error: true,
            });
        }

        let tags: Vec<String> = params.get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();

        let conn = open_memory_db()?;

        // Generate embedding for the content (async)
        let embedding = crate::memory::try_get_embedding(content).await;

        let record = crate::memory::write_memory_public(
            &conn,
            content,
            "model-tool",
            None,
            None,
            &tags,
            &[embedding],
        )?;

        Ok(ToolResult {
            content: format!(
                "Saved to memory (id: {}). Tags: [{}]. This will be available in future sessions.",
                record.id,
                if tags.is_empty() { "none".to_string() } else { tags.join(", ") }
            ),
            is_error: false,
        })
    }
}

// ============================================
// memory_search — query own memories
// ============================================

pub struct MemorySearchTool;

#[async_trait::async_trait]
impl HiveTool for MemorySearchTool {
    fn name(&self) -> &str { "memory_search" }

    fn description(&self) -> &str {
        "Search your persistent memory for relevant information from past conversations. \
         Use this when you need to recall something specific — a user preference, a past \
         decision, project details, or anything discussed previously. Returns the most \
         relevant memories ranked by similarity and recency."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "What to search for. Can be a topic, keyword, or natural language question."
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default: 5, max: 20)"
                }
            },
            "required": ["query"]
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::Low }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let query = params.get("query")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: query")?;

        if query.trim().is_empty() {
            return Ok(ToolResult {
                content: "Cannot search with empty query.".to_string(),
                is_error: true,
            });
        }

        let max_results = params.get("max_results")
            .and_then(|v| v.as_u64())
            .map(|v| (v as usize).min(20))
            .unwrap_or(5);

        let conn = open_memory_db()?;

        // Generate query embedding (async)
        let query_embedding = crate::memory::try_get_embedding(query).await;

        let results = crate::memory::search_hybrid_public(
            &conn,
            query,
            &query_embedding,
            max_results,
        )?;

        if results.is_empty() {
            return Ok(ToolResult {
                content: "No memories found matching your query.".to_string(),
                is_error: false,
            });
        }

        let mut output = format!("Found {} memories:\n\n", results.len());
        for (i, r) in results.iter().enumerate() {
            let tags_str = if r.tags.is_empty() {
                String::new()
            } else {
                format!(" [{}]", r.tags.join(", "))
            };
            // Phase 9.3: Include source file attribution when memory was imported from a file
            let source_file: String = conn.query_row(
                "SELECT COALESCE(source_file, '') FROM memories WHERE id = ?1",
                rusqlite::params![r.id],
                |row| row.get(0),
            ).unwrap_or_default();
            let source_str = if source_file.is_empty() {
                String::new()
            } else {
                let fname = std::path::Path::new(&source_file)
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or(source_file.clone());
                format!(" [Source: {}]", fname)
            };
            output.push_str(&format!(
                "{}. (id: {}, score: {:.2}, {}{}{})\n{}\n\n",
                i + 1,
                r.id,
                r.score,
                r.created_at.chars().take(10).collect::<String>(),
                tags_str,
                source_str,
                r.snippet,
            ));
        }

        Ok(ToolResult {
            content: output,
            is_error: false,
        })
    }
}

// ============================================
// memory_edit — update existing memories
// ============================================

pub struct MemoryEditTool;

#[async_trait::async_trait]
impl HiveTool for MemoryEditTool {
    fn name(&self) -> &str { "memory_edit" }

    fn description(&self) -> &str {
        "Edit an existing memory to correct or update it. Use this when a memory contains \
         outdated information, a mistake, or needs to be updated with new details. \
         Use memory_search first to find the memory ID you want to edit."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "The memory ID to edit (from memory_search results)"
                },
                "content": {
                    "type": "string",
                    "description": "The updated content for this memory"
                },
                "tags": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional: new tags to replace existing ones"
                }
            },
            "required": ["id", "content"]
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::Medium }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let id = params.get("id")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: id")?;

        let content = params.get("content")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: content")?;

        if content.trim().is_empty() {
            return Ok(ToolResult {
                content: "Cannot set memory to empty content. Use memory_delete to remove it.".to_string(),
                is_error: true,
            });
        }

        let tags: Option<Vec<String>> = params.get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect());

        let conn = open_memory_db()?;

        // Generate new embeddings
        let embedding = crate::memory::try_get_embedding(content).await;

        let record = crate::memory::update_memory_public(
            &conn,
            id,
            content,
            tags.as_deref(),
            &[embedding],
        )?;

        let tags_str = if record.tags.is_empty() {
            "none".to_string()
        } else {
            record.tags.join(", ")
        };

        Ok(ToolResult {
            content: format!(
                "Memory updated (id: {}). Tags: [{}]. Changes are immediately searchable.",
                record.id, tags_str,
            ),
            is_error: false,
        })
    }
}

// ============================================
// memory_delete — remove memories
// ============================================

pub struct MemoryDeleteTool;

#[async_trait::async_trait]
impl HiveTool for MemoryDeleteTool {
    fn name(&self) -> &str { "memory_delete" }

    fn description(&self) -> &str {
        "Delete a memory that is outdated, incorrect, or no longer needed. \
         Use memory_search first to find the memory ID you want to delete. \
         This action is permanent."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "The memory ID to delete (from memory_search results)"
                }
            },
            "required": ["id"]
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::Medium }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let id = params.get("id")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: id")?;

        let conn = open_memory_db()?;

        let deleted = crate::memory::delete_memory_public(&conn, id)?;

        if deleted {
            Ok(ToolResult {
                content: format!("Memory deleted (id: {}). It has been permanently removed.", id),
                is_error: false,
            })
        } else {
            Ok(ToolResult {
                content: format!("Memory not found (id: {}). It may have already been deleted.", id),
                is_error: true,
            })
        }
    }
}

// ============================================
// task_track — cross-session task management
// ============================================

pub struct TaskTrackTool;

#[async_trait::async_trait]
impl HiveTool for TaskTrackTool {
    fn name(&self) -> &str { "task_track" }

    fn description(&self) -> &str {
        "Track ongoing tasks across sessions. Use this to create, update, or list tasks \
         that span multiple conversations. Tasks persist across app restarts. \
         Actions: 'create' (new task), 'update' (change status/notes), 'list' (show tasks)."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["create", "update", "list"],
                    "description": "Action to perform: create, update, or list tasks"
                },
                "name": {
                    "type": "string",
                    "description": "Task name (required for create/update)"
                },
                "description": {
                    "type": "string",
                    "description": "Task description (required for create)"
                },
                "status": {
                    "type": "string",
                    "enum": ["open", "in_progress", "done", "blocked"],
                    "description": "Task status (for create/update, defaults to 'open')"
                },
                "notes": {
                    "type": "string",
                    "description": "Additional notes (for create/update)"
                },
                "filter": {
                    "type": "string",
                    "enum": ["open", "in_progress", "done", "blocked", "all"],
                    "description": "Status filter for list action (defaults to showing open + in_progress)"
                }
            },
            "required": ["action"]
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::Low }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let action = params.get("action")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: action")?;

        let conn = open_memory_db()?;
        let now = chrono::Utc::now().to_rfc3339();

        match action {
            "create" | "update" => {
                let name = params.get("name")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required parameter: name")?;
                let desc = params.get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let status = params.get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("open");
                let notes = params.get("notes")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let task_state = json!({
                    "description": desc,
                    "status": status,
                    "notes": notes,
                });

                // Upsert
                let existing: Option<String> = conn.query_row(
                    "SELECT id FROM entities WHERE entity_type = 'task' AND name = ?1",
                    rusqlite::params![name],
                    |row| row.get(0),
                ).ok();

                if let Some(eid) = existing {
                    conn.execute(
                        "UPDATE entities SET state = ?1, updated_at = ?2 WHERE id = ?3",
                        rusqlite::params![task_state.to_string(), now, eid],
                    ).map_err(|e| format!("Failed to update task: {}", e))?;
                    Ok(ToolResult {
                        content: format!("Task '{}' updated — status: {}", name, status),
                        is_error: false,
                    })
                } else {
                    let new_id = format!("task_{}", name.replace(' ', "_").to_lowercase());
                    conn.execute(
                        "INSERT INTO entities (id, entity_type, name, state, metadata, created_at, updated_at)
                         VALUES (?1, 'task', ?2, ?3, '{}', ?4, ?5)",
                        rusqlite::params![new_id, name, task_state.to_string(), now, now],
                    ).map_err(|e| format!("Failed to create task: {}", e))?;
                    Ok(ToolResult {
                        content: format!("Task '{}' created — status: {}", name, status),
                        is_error: false,
                    })
                }
            },
            "list" => {
                let filter = params.get("filter")
                    .and_then(|v| v.as_str())
                    .unwrap_or("active"); // "active" = open + in_progress

                let tasks: Vec<(String, String)> = if filter == "all" {
                    let mut stmt = conn.prepare(
                        "SELECT name, state FROM entities WHERE entity_type = 'task' ORDER BY updated_at DESC"
                    ).map_err(|e| format!("Query failed: {}", e))?;
                    let result: Vec<(String, String)> = stmt.query_map([], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                    }).map_err(|e| format!("Query failed: {}", e))?
                    .filter_map(|r| r.ok())
                    .collect();
                    result
                } else {
                    // Filter: "active" shows open+in_progress, or specific status
                    let status_values = if filter == "active" {
                        vec!["open", "in_progress"]
                    } else {
                        vec![filter]
                    };
                    let mut all: Vec<(String, String)> = Vec::new();
                    for sv in status_values {
                        let pattern = format!("%\"status\":\"{}\"%", sv);
                        if let Ok(mut stmt) = conn.prepare(
                            "SELECT name, state FROM entities WHERE entity_type = 'task' AND state LIKE ?1 ORDER BY updated_at DESC"
                        ) {
                            if let Ok(rows) = stmt.query_map(
                                rusqlite::params![pattern], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                            ) {
                                let results: Vec<(String, String)> = rows.filter_map(|r| r.ok()).collect();
                                all.extend(results);
                            }
                        }
                    }
                    all
                };

                if tasks.is_empty() {
                    Ok(ToolResult {
                        content: format!("No tasks found (filter: {})", filter),
                        is_error: false,
                    })
                } else {
                    let mut output = format!("Tasks ({}):\n", filter);
                    for (name, state_str) in &tasks {
                        let state: serde_json::Value = serde_json::from_str(state_str).unwrap_or(json!({}));
                        let status = state.get("status").and_then(|v| v.as_str()).unwrap_or("?");
                        let desc = state.get("description").and_then(|v| v.as_str()).unwrap_or("");
                        let notes = state.get("notes").and_then(|v| v.as_str()).unwrap_or("");
                        output.push_str(&format!("- [{}] {} — {}", status, name, desc));
                        if !notes.is_empty() {
                            output.push_str(&format!(" (notes: {})", notes));
                        }
                        output.push('\n');
                    }
                    Ok(ToolResult {
                        content: output,
                        is_error: false,
                    })
                }
            },
            _ => Ok(ToolResult {
                content: format!("Unknown action: '{}'. Use 'create', 'update', or 'list'.", action),
                is_error: true,
            }),
        }
    }
}

// ============================================
// MAGMA Graph Tools — Model Agency Over the Knowledge Graph
// ============================================
//
// These tools give the model direct control over the MAGMA multi-graph:
//   - graph_query:      Traverse the entity/memory graph, explore connections
//   - entity_track:     Create/update entities the model considers important
//   - procedure_learn:  Record learned tool chains for reuse
//
// Together with the passive auto-tracking (mod.rs) and graph-augmented search
// (memory.rs), these form the active side of graph curation.

// ============================================
// graph_query — traverse and explore the MAGMA graph
// ============================================

pub struct GraphQueryTool;

#[async_trait::async_trait]
impl HiveTool for GraphQueryTool {
    fn name(&self) -> &str { "graph_query" }

    fn description(&self) -> &str {
        "Explore the MAGMA knowledge graph. Use this to discover connections between \
         memories, entities, and procedures. You can:\n\
         - 'stats': Get an overview of the graph (counts of nodes and edges)\n\
         - 'traverse': Start from a node and follow edges to find connected nodes\n\
         - 'neighbors': Find all entities/memories directly connected to a given entity\n\
         - 'find_entity': Look up a specific entity by type and name\n\
         - 'list_entities': List entities of a given type\n\n\
         This helps you understand what you know, discover associations, and find \
         relevant context that keyword search might miss."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["stats", "traverse", "neighbors", "find_entity", "list_entities"],
                    "description": "What to do: stats (overview), traverse (follow edges), neighbors (direct connections), find_entity (lookup), list_entities (browse)"
                },
                "node_type": {
                    "type": "string",
                    "description": "Type of node to start from: 'memory', 'entity', 'procedure', 'event', 'skill'. Required for traverse/neighbors/find_entity."
                },
                "node_id": {
                    "type": "string",
                    "description": "ID of the node to start from. Required for traverse/neighbors."
                },
                "name": {
                    "type": "string",
                    "description": "Entity name for find_entity action."
                },
                "entity_type": {
                    "type": "string",
                    "description": "Entity type filter for list_entities (e.g. 'file', 'model', 'task', 'url', 'command')."
                },
                "max_depth": {
                    "type": "integer",
                    "description": "Max traversal depth for traverse action (default: 2, max: 4)"
                },
                "edge_types": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Filter edges by type: 'related_to', 'caused_by', 'led_to', 'references', 'learned_from', 'used_in', 'modified', 'produced'"
                },
                "limit": {
                    "type": "integer",
                    "description": "Max results to return (default: 20)"
                }
            },
            "required": ["action"]
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::Low }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let action = params.get("action")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: action")?;

        let conn = open_memory_db()?;

        match action {
            "stats" => {
                let events: i64 = conn.query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0)).unwrap_or(0);
                let entities: i64 = conn.query_row("SELECT COUNT(*) FROM entities", [], |r| r.get(0)).unwrap_or(0);
                let procedures: i64 = conn.query_row("SELECT COUNT(*) FROM procedures", [], |r| r.get(0)).unwrap_or(0);
                let edges: i64 = conn.query_row("SELECT COUNT(*) FROM edges", [], |r| r.get(0)).unwrap_or(0);
                let memories: i64 = conn.query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0)).unwrap_or(0);

                // Top entity types
                let mut type_counts = Vec::new();
                if let Ok(mut stmt) = conn.prepare(
                    "SELECT entity_type, COUNT(*) as cnt FROM entities GROUP BY entity_type ORDER BY cnt DESC LIMIT 10"
                ) {
                    if let Ok(rows) = stmt.query_map([], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                    }) {
                        type_counts = rows.filter_map(|r| r.ok()).collect();
                    }
                }

                // Top edge types
                let mut edge_type_counts = Vec::new();
                if let Ok(mut stmt) = conn.prepare(
                    "SELECT edge_type, COUNT(*) as cnt FROM edges GROUP BY edge_type ORDER BY cnt DESC LIMIT 10"
                ) {
                    if let Ok(rows) = stmt.query_map([], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                    }) {
                        edge_type_counts = rows.filter_map(|r| r.ok()).collect();
                    }
                }

                let mut output = format!(
                    "MAGMA Graph Stats:\n\
                     - Memories: {}\n\
                     - Entities: {}\n\
                     - Procedures: {}\n\
                     - Events: {}\n\
                     - Edges: {}\n",
                    memories, entities, procedures, events, edges
                );

                if !type_counts.is_empty() {
                    output.push_str("\nEntity types:\n");
                    for (etype, cnt) in &type_counts {
                        output.push_str(&format!("  {} ({})\n", etype, cnt));
                    }
                }
                if !edge_type_counts.is_empty() {
                    output.push_str("\nEdge types:\n");
                    for (etype, cnt) in &edge_type_counts {
                        output.push_str(&format!("  {} ({})\n", etype, cnt));
                    }
                }

                Ok(ToolResult { content: output, is_error: false })
            },

            "traverse" => {
                let node_type = params.get("node_type").and_then(|v| v.as_str())
                    .ok_or("traverse requires node_type")?;
                let node_id = params.get("node_id").and_then(|v| v.as_str())
                    .ok_or("traverse requires node_id")?;
                let max_depth = params.get("max_depth").and_then(|v| v.as_u64())
                    .map(|v| (v as usize).min(4)).unwrap_or(2);
                let edge_types: Option<Vec<String>> = params.get("edge_types")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect());

                let edges = traverse_graph(&conn, node_type, node_id, max_depth, edge_types.as_deref())?;

                if edges.is_empty() {
                    return Ok(ToolResult {
                        content: format!("No edges found from {} '{}'.", node_type, node_id),
                        is_error: false,
                    });
                }

                let mut output = format!("Traversal from {} '{}' (depth {}):\n\n", node_type, node_id, max_depth);
                for edge in &edges {
                    output.push_str(&format!(
                        "  {} '{}' --[{} w={:.2}]--> {} '{}'\n",
                        edge.source_type, edge.source_id,
                        edge.edge_type, edge.weight,
                        edge.target_type, edge.target_id,
                    ));
                }
                output.push_str(&format!("\n{} edges found.", edges.len()));

                Ok(ToolResult { content: output, is_error: false })
            },

            "neighbors" => {
                let node_type = params.get("node_type").and_then(|v| v.as_str())
                    .ok_or("neighbors requires node_type")?;
                let node_id = params.get("node_id").and_then(|v| v.as_str())
                    .ok_or("neighbors requires node_id")?;
                let limit = params.get("limit").and_then(|v| v.as_u64())
                    .map(|v| (v as usize).min(50)).unwrap_or(20);

                // Direct edges from/to this node
                let edges = traverse_graph(&conn, node_type, node_id, 1, None)?;

                if edges.is_empty() {
                    return Ok(ToolResult {
                        content: format!("No neighbors for {} '{}'.", node_type, node_id),
                        is_error: false,
                    });
                }

                let mut output = format!("Neighbors of {} '{}' ({} connections):\n\n", node_type, node_id, edges.len());
                for (i, edge) in edges.iter().take(limit).enumerate() {
                    // Determine which end is the neighbor
                    let (neighbor_type, neighbor_id) = if edge.source_id == node_id {
                        (&edge.target_type, &edge.target_id)
                    } else {
                        (&edge.source_type, &edge.source_id)
                    };

                    // Try to resolve entity name if it's an entity
                    let display_name = if neighbor_type == "entity" {
                        conn.query_row(
                            "SELECT name FROM entities WHERE id = ?1",
                            rusqlite::params![neighbor_id],
                            |row| row.get::<_, String>(0),
                        ).unwrap_or_else(|_| neighbor_id.clone())
                    } else {
                        neighbor_id.clone()
                    };

                    output.push_str(&format!(
                        "{}. {} '{}' (edge: {}, weight: {:.2})\n",
                        i + 1, neighbor_type, display_name, edge.edge_type, edge.weight,
                    ));
                }

                Ok(ToolResult { content: output, is_error: false })
            },

            "find_entity" => {
                let entity_type = params.get("node_type")
                    .or(params.get("entity_type"))
                    .and_then(|v| v.as_str())
                    .ok_or("find_entity requires node_type or entity_type")?;
                let name = params.get("name").and_then(|v| v.as_str())
                    .ok_or("find_entity requires name")?;

                let result = conn.query_row(
                    "SELECT id, entity_type, name, state, metadata, created_at, updated_at
                     FROM entities WHERE entity_type = ?1 AND name = ?2",
                    rusqlite::params![entity_type, name],
                    |row| {
                        let id: String = row.get(0)?;
                        let etype: String = row.get(1)?;
                        let ename: String = row.get(2)?;
                        let state: String = row.get(3)?;
                        let meta: String = row.get(4)?;
                        let created: String = row.get(5)?;
                        let updated: String = row.get(6)?;
                        Ok(format!(
                            "Entity found:\n\
                             - ID: {}\n\
                             - Type: {}\n\
                             - Name: {}\n\
                             - State: {}\n\
                             - Metadata: {}\n\
                             - Created: {}\n\
                             - Updated: {}",
                            id, etype, ename, state, meta, created, updated
                        ))
                    },
                );

                match result {
                    Ok(output) => Ok(ToolResult { content: output, is_error: false }),
                    Err(_) => Ok(ToolResult {
                        content: format!("No entity found: {} '{}'", entity_type, name),
                        is_error: false,
                    }),
                }
            },

            "list_entities" => {
                let entity_type = params.get("entity_type").and_then(|v| v.as_str());
                let limit = params.get("limit").and_then(|v| v.as_u64())
                    .map(|v| (v as usize).min(50)).unwrap_or(20);

                let entities: Vec<(String, String, String, String)> = if let Some(et) = entity_type {
                    let mut stmt = conn.prepare(
                        "SELECT id, name, state, updated_at FROM entities
                         WHERE entity_type = ?1 ORDER BY updated_at DESC LIMIT ?2"
                    ).map_err(|e| format!("Query error: {}", e))?;
                    let rows: Vec<_> = stmt.query_map(rusqlite::params![et, limit as i64], |row| {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                    }).map_err(|e| format!("Query error: {}", e))?
                    .filter_map(|r| r.ok()).collect();
                    rows
                } else {
                    let mut stmt = conn.prepare(
                        "SELECT id, name, entity_type || ':' || name, updated_at FROM entities
                         ORDER BY updated_at DESC LIMIT ?1"
                    ).map_err(|e| format!("Query error: {}", e))?;
                    let rows: Vec<_> = stmt.query_map(rusqlite::params![limit as i64], |row| {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                    }).map_err(|e| format!("Query error: {}", e))?
                    .filter_map(|r| r.ok()).collect();
                    rows
                };

                if entities.is_empty() {
                    let filter = entity_type.unwrap_or("all");
                    return Ok(ToolResult {
                        content: format!("No entities found (type: {})", filter),
                        is_error: false,
                    });
                }

                let mut output = format!("Entities ({}):\n", entity_type.unwrap_or("all types"));
                for (id, name, state, updated) in &entities {
                    let updated_short: String = updated.chars().take(10).collect();
                    output.push_str(&format!("- {} (id: {}, updated: {}) {}\n", name, id, updated_short, state));
                }

                Ok(ToolResult { content: output, is_error: false })
            },

            _ => Ok(ToolResult {
                content: format!("Unknown action: '{}'. Use 'stats', 'traverse', 'neighbors', 'find_entity', or 'list_entities'.", action),
                is_error: true,
            }),
        }
    }
}

/// Internal graph traversal (shared by GraphQueryTool and used by graph-augmented search).
fn traverse_graph(
    conn: &rusqlite::Connection,
    start_type: &str,
    start_id: &str,
    max_depth: usize,
    edge_type_filter: Option<&[String]>,
) -> Result<Vec<TraversalEdge>, String> {
    let mut result_edges = Vec::new();
    let mut visited: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    let mut frontier: Vec<(String, String)> = vec![(start_type.to_string(), start_id.to_string())];

    for _ in 0..max_depth {
        let mut next_frontier = Vec::new();
        for (ntype, nid) in &frontier {
            if !visited.insert((ntype.clone(), nid.clone())) {
                continue;
            }

            // Outgoing edges
            if let Ok(mut stmt) = conn.prepare(
                "SELECT source_type, source_id, target_type, target_id, edge_type, weight
                 FROM edges WHERE source_type = ?1 AND source_id = ?2
                 ORDER BY weight DESC LIMIT 20"
            ) {
                if let Ok(rows) = stmt.query_map(rusqlite::params![ntype, nid], |row| {
                    Ok(TraversalEdge {
                        source_type: row.get(0)?,
                        source_id: row.get(1)?,
                        target_type: row.get(2)?,
                        target_id: row.get(3)?,
                        edge_type: row.get(4)?,
                        weight: row.get(5)?,
                    })
                }) {
                    for row in rows.filter_map(|r| r.ok()) {
                        if let Some(filter) = edge_type_filter {
                            if !filter.iter().any(|f| f == &row.edge_type) {
                                continue;
                            }
                        }
                        next_frontier.push((row.target_type.clone(), row.target_id.clone()));
                        result_edges.push(row);
                    }
                }
            }

            // Incoming edges (bidirectional)
            if let Ok(mut stmt) = conn.prepare(
                "SELECT source_type, source_id, target_type, target_id, edge_type, weight
                 FROM edges WHERE target_type = ?1 AND target_id = ?2
                 ORDER BY weight DESC LIMIT 20"
            ) {
                if let Ok(rows) = stmt.query_map(rusqlite::params![ntype, nid], |row| {
                    Ok(TraversalEdge {
                        source_type: row.get(0)?,
                        source_id: row.get(1)?,
                        target_type: row.get(2)?,
                        target_id: row.get(3)?,
                        edge_type: row.get(4)?,
                        weight: row.get(5)?,
                    })
                }) {
                    for row in rows.filter_map(|r| r.ok()) {
                        if let Some(filter) = edge_type_filter {
                            if !filter.iter().any(|f| f == &row.edge_type) {
                                continue;
                            }
                        }
                        next_frontier.push((row.source_type.clone(), row.source_id.clone()));
                        result_edges.push(row);
                    }
                }
            }
        }
        frontier = next_frontier;
    }

    Ok(result_edges)
}

struct TraversalEdge {
    source_type: String,
    source_id: String,
    target_type: String,
    target_id: String,
    edge_type: String,
    weight: f64,
}

// ============================================
// entity_track — model curates entities in the graph
// ============================================

pub struct EntityTrackTool;

#[async_trait::async_trait]
impl HiveTool for EntityTrackTool {
    fn name(&self) -> &str { "entity_track" }

    fn description(&self) -> &str {
        "Track an entity in the MAGMA knowledge graph. Use this to register or update \
         important things you encounter: people, projects, preferences, concepts, files, \
         or anything worth remembering as a distinct entity.\n\n\
         Entities are different from memories: a memory is a piece of text, an entity is \
         a named object with structured state. Entities can be connected to memories and \
         other entities via edges.\n\n\
         Actions:\n\
         - 'upsert': Create or update an entity (same type+name updates in place)\n\
         - 'connect': Create an edge between two entities or between an entity and a memory\n\
         - 'delete': Remove an entity"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["upsert", "connect", "delete"],
                    "description": "What to do: upsert (create/update), connect (add edge), delete (remove)"
                },
                "entity_type": {
                    "type": "string",
                    "description": "Type of entity: 'person', 'project', 'preference', 'concept', 'file', 'model', 'tool', or any custom type"
                },
                "name": {
                    "type": "string",
                    "description": "Name of the entity (unique within its type)"
                },
                "state": {
                    "type": "object",
                    "description": "Structured state data for the entity (JSON object with any fields)"
                },
                "metadata": {
                    "type": "object",
                    "description": "Optional metadata (tags, notes, etc.)"
                },
                "target_type": {
                    "type": "string",
                    "description": "For 'connect': type of the target node ('memory', 'entity', 'procedure')"
                },
                "target_id": {
                    "type": "string",
                    "description": "For 'connect': ID of the target node"
                },
                "edge_type": {
                    "type": "string",
                    "description": "For 'connect': relationship type (e.g. 'related_to', 'references', 'used_in', 'produced', 'modified')"
                },
                "edge_weight": {
                    "type": "number",
                    "description": "For 'connect': edge weight 0.0-1.0 (default: 0.8)"
                }
            },
            "required": ["action"]
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::Low }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let action = params.get("action")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: action")?;

        let conn = open_memory_db()?;
        let now = chrono::Utc::now().to_rfc3339();

        match action {
            "upsert" => {
                let entity_type = params.get("entity_type").and_then(|v| v.as_str())
                    .ok_or("upsert requires entity_type")?;
                let name = params.get("name").and_then(|v| v.as_str())
                    .ok_or("upsert requires name")?;
                let state = params.get("state").cloned().unwrap_or(json!({}));
                let metadata = params.get("metadata").cloned().unwrap_or(json!({}));

                // Check if exists
                let existing: Option<String> = conn.query_row(
                    "SELECT id FROM entities WHERE entity_type = ?1 AND name = ?2",
                    rusqlite::params![entity_type, name],
                    |row| row.get(0),
                ).ok();

                if let Some(existing_id) = existing {
                    conn.execute(
                        "UPDATE entities SET state = ?1, metadata = ?2, updated_at = ?3 WHERE id = ?4",
                        rusqlite::params![state.to_string(), metadata.to_string(), now, existing_id],
                    ).map_err(|e| format!("Failed to update entity: {}", e))?;
                    Ok(ToolResult {
                        content: format!("Entity updated: {} '{}' (id: {})", entity_type, name, existing_id),
                        is_error: false,
                    })
                } else {
                    let id = format!("{}_{}", entity_type, name.replace(['/', '\\', ' ', ':'], "_"));
                    let id: String = id.chars().take(128).collect();
                    conn.execute(
                        "INSERT INTO entities (id, entity_type, name, state, metadata, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                        rusqlite::params![id, entity_type, name, state.to_string(), metadata.to_string(), now, now],
                    ).map_err(|e| format!("Failed to create entity: {}", e))?;
                    Ok(ToolResult {
                        content: format!("Entity created: {} '{}' (id: {})", entity_type, name, id),
                        is_error: false,
                    })
                }
            },

            "connect" => {
                let entity_type = params.get("entity_type").and_then(|v| v.as_str())
                    .ok_or("connect requires entity_type (source)")?;
                let name = params.get("name").and_then(|v| v.as_str())
                    .ok_or("connect requires name (source entity)")?;
                let target_type = params.get("target_type").and_then(|v| v.as_str())
                    .ok_or("connect requires target_type")?;
                let target_id = params.get("target_id").and_then(|v| v.as_str())
                    .ok_or("connect requires target_id")?;
                let edge_type = params.get("edge_type").and_then(|v| v.as_str())
                    .unwrap_or("related_to");
                let weight = params.get("edge_weight").and_then(|v| v.as_f64())
                    .unwrap_or(0.8);

                // Resolve source entity ID
                let source_id: String = conn.query_row(
                    "SELECT id FROM entities WHERE entity_type = ?1 AND name = ?2",
                    rusqlite::params![entity_type, name],
                    |row| row.get(0),
                ).map_err(|_| format!("Source entity not found: {} '{}'", entity_type, name))?;

                let edge_id = format!("edge_{}_{}", source_id, target_id);
                let edge_id: String = edge_id.chars().take(128).collect();

                conn.execute(
                    "INSERT OR REPLACE INTO edges (id, source_type, source_id, target_type, target_id, edge_type, weight, metadata, created_at)
                     VALUES (?1, 'entity', ?2, ?3, ?4, ?5, ?6, '{}', ?7)",
                    rusqlite::params![edge_id, source_id, target_type, target_id, edge_type, weight, now],
                ).map_err(|e| format!("Failed to create edge: {}", e))?;

                Ok(ToolResult {
                    content: format!(
                        "Edge created: entity '{}' --[{} w={:.1}]--> {} '{}'",
                        name, edge_type, weight, target_type, target_id,
                    ),
                    is_error: false,
                })
            },

            "delete" => {
                let entity_type = params.get("entity_type").and_then(|v| v.as_str())
                    .ok_or("delete requires entity_type")?;
                let name = params.get("name").and_then(|v| v.as_str())
                    .ok_or("delete requires name")?;

                // Find entity
                let entity_id: Option<String> = conn.query_row(
                    "SELECT id FROM entities WHERE entity_type = ?1 AND name = ?2",
                    rusqlite::params![entity_type, name],
                    |row| row.get(0),
                ).ok();

                if let Some(id) = entity_id {
                    // Delete edges involving this entity
                    if let Err(e) = conn.execute(
                        "DELETE FROM edges WHERE (source_type = 'entity' AND source_id = ?1) OR (target_type = 'entity' AND target_id = ?1)",
                        rusqlite::params![id],
                    ) {
                        eprintln!("[HIVE] WARN: Failed to clean edges for entity {}: {}", id, e);
                    }
                    // Delete the entity
                    conn.execute(
                        "DELETE FROM entities WHERE id = ?1",
                        rusqlite::params![id],
                    ).map_err(|e| format!("Failed to delete entity: {}", e))?;

                    Ok(ToolResult {
                        content: format!("Entity deleted: {} '{}' (and its edges)", entity_type, name),
                        is_error: false,
                    })
                } else {
                    Ok(ToolResult {
                        content: format!("Entity not found: {} '{}'", entity_type, name),
                        is_error: true,
                    })
                }
            },

            _ => Ok(ToolResult {
                content: format!("Unknown action: '{}'. Use 'upsert', 'connect', or 'delete'.", action),
                is_error: true,
            }),
        }
    }
}

// ============================================
// procedure_learn — model records and reuses tool chains
// ============================================

pub struct ProcedureLearnTool;

#[async_trait::async_trait]
impl HiveTool for ProcedureLearnTool {
    fn name(&self) -> &str { "procedure_learn" }

    fn description(&self) -> &str {
        "Record or recall learned procedures (tool chain patterns). Use this when you \
         discover a successful sequence of tool calls that could be reused.\n\n\
         Actions:\n\
         - 'save': Record a new procedure with its steps and trigger pattern\n\
         - 'recall': Find procedures that match a query/trigger pattern\n\
         - 'outcome': Report whether a procedure succeeded or failed (reinforcement)\n\
         - 'list': List all procedures ordered by success rate\n\n\
         Procedures are like muscle memory — they record what works so you can reuse it. \
         The system tracks success/fail counts so reliable procedures rank higher."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["save", "recall", "outcome", "list"],
                    "description": "What to do: save (new procedure), recall (find matching), outcome (report result), list (browse all)"
                },
                "name": {
                    "type": "string",
                    "description": "Procedure name (for save). Should be descriptive: 'research-and-summarize', 'check-discord-and-notify'"
                },
                "description": {
                    "type": "string",
                    "description": "What this procedure does (for save)"
                },
                "steps": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "tool": { "type": "string" },
                            "args_template": { "type": "object" },
                            "notes": { "type": "string" }
                        }
                    },
                    "description": "Ordered tool call steps (for save). Each step has a tool name, arg template, and optional notes."
                },
                "trigger_pattern": {
                    "type": "string",
                    "description": "When to use this procedure — keywords or patterns that indicate this procedure is relevant (for save)"
                },
                "query": {
                    "type": "string",
                    "description": "Search for procedures matching this query (for recall)"
                },
                "procedure_id": {
                    "type": "string",
                    "description": "Procedure ID (for outcome)"
                },
                "success": {
                    "type": "boolean",
                    "description": "Whether the procedure succeeded (for outcome)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Max results (default: 10)"
                }
            },
            "required": ["action"]
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::Low }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let action = params.get("action")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: action")?;

        let conn = open_memory_db()?;
        let now = chrono::Utc::now().to_rfc3339();

        match action {
            "save" => {
                let name = params.get("name").and_then(|v| v.as_str())
                    .ok_or("save requires name")?;
                let description = params.get("description").and_then(|v| v.as_str())
                    .ok_or("save requires description")?;
                let steps = params.get("steps").and_then(|v| v.as_array())
                    .ok_or("save requires steps array")?;
                let trigger = params.get("trigger_pattern").and_then(|v| v.as_str())
                    .unwrap_or("");

                if steps.is_empty() {
                    return Ok(ToolResult {
                        content: "Procedure must have at least one step.".to_string(),
                        is_error: true,
                    });
                }

                let id = format!("proc_{}", name.replace([' ', '/', '\\'], "_").to_lowercase());
                let id: String = id.chars().take(128).collect();
                let steps_json = serde_json::to_string(steps).unwrap_or_else(|_| "[]".to_string());

                // Upsert: if same name exists, update it
                let existing: Option<String> = conn.query_row(
                    "SELECT id FROM procedures WHERE name = ?1",
                    rusqlite::params![name],
                    |row| row.get(0),
                ).ok();

                if let Some(existing_id) = existing {
                    conn.execute(
                        "UPDATE procedures SET description = ?1, steps = ?2, trigger_pattern = ?3, updated_at = ?4 WHERE id = ?5",
                        rusqlite::params![description, steps_json, trigger, now, existing_id],
                    ).map_err(|e| format!("Failed to update procedure: {}", e))?;
                    Ok(ToolResult {
                        content: format!("Procedure updated: '{}' ({} steps, trigger: '{}')", name, steps.len(), trigger),
                        is_error: false,
                    })
                } else {
                    conn.execute(
                        "INSERT INTO procedures (id, name, description, steps, trigger_pattern, success_count, fail_count, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, 0, 0, ?6, ?7)",
                        rusqlite::params![id, name, description, steps_json, trigger, now, now],
                    ).map_err(|e| format!("Failed to save procedure: {}", e))?;
                    Ok(ToolResult {
                        content: format!("Procedure saved: '{}' ({} steps, trigger: '{}')", name, steps.len(), trigger),
                        is_error: false,
                    })
                }
            },

            "recall" => {
                let query = params.get("query").and_then(|v| v.as_str())
                    .ok_or("recall requires query")?;
                let limit = params.get("limit").and_then(|v| v.as_u64())
                    .map(|v| (v as usize).min(20)).unwrap_or(10);

                let pattern = format!("%{}%", query.to_lowercase());

                let mut stmt = conn.prepare(
                    "SELECT id, name, description, steps, trigger_pattern, success_count, fail_count, last_used
                     FROM procedures
                     WHERE LOWER(name) LIKE ?1 OR LOWER(description) LIKE ?1 OR LOWER(trigger_pattern) LIKE ?1
                     ORDER BY (success_count - fail_count) DESC, updated_at DESC
                     LIMIT ?2"
                ).map_err(|e| format!("Query error: {}", e))?;

                let procs: Vec<(String, String, String, String, String, i64, i64, Option<String>)> = stmt.query_map(
                    rusqlite::params![pattern, limit as i64],
                    |row| Ok((
                        row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?,
                        row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?,
                    )),
                ).map_err(|e| format!("Query error: {}", e))?
                .filter_map(|r| r.ok()).collect();

                if procs.is_empty() {
                    return Ok(ToolResult {
                        content: format!("No procedures match '{}'", query),
                        is_error: false,
                    });
                }

                let mut output = format!("Procedures matching '{}':\n\n", query);
                for (id, name, desc, steps_json, trigger, success, fail, last_used) in &procs {
                    let steps: Vec<serde_json::Value> = serde_json::from_str(steps_json).unwrap_or_default();
                    let reliability = if *success + *fail > 0 {
                        format!("{:.0}%", (*success as f64 / (*success + *fail) as f64) * 100.0)
                    } else {
                        "untested".to_string()
                    };
                    let last = last_used.as_deref().map(|s| s.chars().take(10).collect::<String>()).unwrap_or_else(|| "never".to_string());

                    output.push_str(&format!(
                        "- {} (id: {}) — {}\n  Steps: {} | Reliability: {} ({}/{})\n  Trigger: '{}' | Last used: {}\n\n",
                        name, id, desc, steps.len(), reliability, success, success + fail, trigger, last,
                    ));
                }

                Ok(ToolResult { content: output, is_error: false })
            },

            "outcome" => {
                let procedure_id = params.get("procedure_id").and_then(|v| v.as_str())
                    .ok_or("outcome requires procedure_id")?;
                let success = params.get("success").and_then(|v| v.as_bool())
                    .ok_or("outcome requires success (boolean)")?;

                if success {
                    conn.execute(
                        "UPDATE procedures SET success_count = success_count + 1, last_used = ?1, updated_at = ?2 WHERE id = ?3",
                        rusqlite::params![now, now, procedure_id],
                    ).map_err(|e| format!("Failed to update procedure: {}", e))?;
                } else {
                    conn.execute(
                        "UPDATE procedures SET fail_count = fail_count + 1, last_used = ?1, updated_at = ?2 WHERE id = ?3",
                        rusqlite::params![now, now, procedure_id],
                    ).map_err(|e| format!("Failed to update procedure: {}", e))?;
                }

                Ok(ToolResult {
                    content: format!(
                        "Procedure {} recorded as {}.",
                        procedure_id,
                        if success { "success" } else { "failure" },
                    ),
                    is_error: false,
                })
            },

            "list" => {
                let limit = params.get("limit").and_then(|v| v.as_u64())
                    .map(|v| (v as usize).min(50)).unwrap_or(10);

                let mut stmt = conn.prepare(
                    "SELECT id, name, description, success_count, fail_count, trigger_pattern, last_used
                     FROM procedures
                     ORDER BY (success_count - fail_count) DESC, updated_at DESC
                     LIMIT ?1"
                ).map_err(|e| format!("Query error: {}", e))?;

                let procs: Vec<(String, String, String, i64, i64, String, Option<String>)> = stmt.query_map(
                    rusqlite::params![limit as i64],
                    |row| Ok((
                        row.get(0)?, row.get(1)?, row.get(2)?,
                        row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?,
                    )),
                ).map_err(|e| format!("Query error: {}", e))?
                .filter_map(|r| r.ok()).collect();

                if procs.is_empty() {
                    return Ok(ToolResult {
                        content: "No procedures recorded yet.".to_string(),
                        is_error: false,
                    });
                }

                let mut output = format!("All procedures ({}):\n\n", procs.len());
                for (id, name, desc, success, fail, trigger, last_used) in &procs {
                    let reliability = if *success + *fail > 0 {
                        format!("{:.0}%", (*success as f64 / (*success + *fail) as f64) * 100.0)
                    } else {
                        "untested".to_string()
                    };
                    let last = last_used.as_deref().map(|s| s.chars().take(10).collect::<String>()).unwrap_or_else(|| "never".to_string());
                    output.push_str(&format!(
                        "- {} (id: {}) — {}\n  Reliability: {} | Trigger: '{}' | Last used: {}\n\n",
                        name, id, desc, reliability, trigger, last,
                    ));
                }

                Ok(ToolResult { content: output, is_error: false })
            },

            _ => Ok(ToolResult {
                content: format!("Unknown action: '{}'. Use 'save', 'recall', 'outcome', or 'list'.", action),
                is_error: true,
            }),
        }
    }
}

// ============================================
// memory_import_file — RAG file ingestion as a HiveTool (Phase 9.3)
// ============================================

pub struct MemoryImportFileTool;

#[async_trait::async_trait]
impl HiveTool for MemoryImportFileTool {
    fn name(&self) -> &str { "memory_import_file" }

    fn description(&self) -> &str {
        "Import a text file into memory as chunked records for RAG retrieval. \
         The file is split into ~1600-character sections and each is indexed as a searchable memory. \
         Supports: .txt, .md, .rs, .ts, .py, .js, .json, .toml, .yaml, .csv, .html, .css, .go, .java, and more. \
         Use this to ingest documentation, codebases, or reference materials into your knowledge base."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Absolute path to the file to import"
                },
                "tags": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional tags to apply to all imported memories (e.g. ['docs', 'api-reference'])"
                }
            },
            "required": ["path"]
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::High }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let file_path = params.get("path")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: path")?;

        let tags: Option<Vec<String>> = params.get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect());

        let path = std::path::Path::new(file_path);
        if !path.exists() {
            return Ok(ToolResult {
                content: format!("File not found: {}", file_path),
                is_error: true,
            });
        }

        // P6: Sandbox check — only allow imports from home directory or current working directory
        if let Ok(canonical) = std::fs::canonicalize(path) {
            let allowed = dirs::home_dir()
                .and_then(|h| std::fs::canonicalize(&h).ok())
                .map(|h| canonical.starts_with(&h))
                .unwrap_or(false)
                || std::env::current_dir()
                    .ok()
                    .and_then(|c| std::fs::canonicalize(&c).ok())
                    .map(|c| canonical.starts_with(&c))
                    .unwrap_or(false);

            if !allowed {
                return Ok(ToolResult {
                    content: format!(
                        "Blocked: '{}' is outside the home directory and current working directory (P6). \
                         Move the file to your home directory or open HIVE from the file's parent directory.",
                        file_path
                    ),
                    is_error: true,
                });
            }
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
            return Ok(ToolResult {
                content: format!(
                    "Unsupported file type '.{}'. Supported: {}",
                    ext, text_extensions.join(", ")
                ),
                is_error: true,
            });
        }

        // 10MB file size limit
        let metadata = std::fs::metadata(path)
            .map_err(|e| format!("Failed to read file metadata: {}", e))?;
        if metadata.len() > 10 * 1024 * 1024 {
            return Ok(ToolResult {
                content: format!(
                    "File too large ({:.1} MB). Maximum supported size is 10 MB. \
                     Split the file into smaller parts before importing.",
                    metadata.len() as f64 / (1024.0 * 1024.0)
                ),
                is_error: true,
            });
        }

        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read file: {}", e))?;

        if content.trim().is_empty() {
            return Ok(ToolResult {
                content: "File is empty — nothing to import.".to_string(),
                is_error: true,
            });
        }

        let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();

        // Use memory.rs heading-aware splitting (not raw byte chunking)
        let sections = if ext == "md" || ext == "markdown" {
            crate::memory::split_markdown_by_headings(&content, 1600)
        } else {
            crate::memory::split_file_into_sections(&content, 1600)
                .into_iter()
                .map(|s| crate::memory::ImportSection {
                    content: s, heading: None, level: 0, path: vec![],
                })
                .collect()
        };
        let total = sections.len();
        let source = format!("file:{}", filename);

        // Pre-compute embeddings (async)
        let mut section_embeddings: Vec<Vec<f64>> = Vec::with_capacity(total);
        for sec in &sections {
            if sec.content.trim().len() < 20 {
                section_embeddings.push(vec![]);
            } else {
                section_embeddings.push(crate::memory::try_get_embedding(&sec.content).await);
            }
        }

        // DB writes (sync)
        let conn = open_memory_db()?;
        let mut imported = 0;
        let mut all_tags = vec!["imported".to_string(), format!("file:{}", filename)];
        if let Some(custom_tags) = tags {
            all_tags.extend(custom_tags);
        }

        for (i, sec) in sections.iter().enumerate() {
            if sec.content.trim().len() < 20 { continue; }

            let mut sec_tags = all_tags.clone();
            sec_tags.push(format!("section:{}/{}", i + 1, total));
            if let Some(ref heading) = sec.heading {
                sec_tags.push(format!("heading:{}", heading));
            }

            let embeddings = if section_embeddings[i].is_empty() {
                vec![]
            } else {
                vec![section_embeddings[i].clone()]
            };

            match crate::memory::write_memory_internal(
                &conn, &sec.content, &source, None, None, &sec_tags, &embeddings
            ) {
                Ok(record) => {
                    // Set source_file for RAG attribution
                    if let Err(e) = conn.execute(
                        "UPDATE memories SET source_file = ?1 WHERE id = ?2",
                        rusqlite::params![file_path, record.id],
                    ) {
                        eprintln!("[HIVE] WARN: Failed to set source_file for memory {}: {}", record.id, e);
                    }
                    imported += 1;
                }
                Err(e) => eprintln!("[HIVE] Warning: import section {}/{}: {}", i + 1, total, e),
            }
        }

        crate::tools::log_tools::append_to_app_log(&format!(
            "MEMORY | import_file | path={} | sections={} | imported={}", file_path, total, imported
        ));

        Ok(ToolResult {
            content: format!(
                "Imported '{}': {} sections indexed as searchable memories (tags: {}).",
                filename, imported, all_tags.join(", ")
            ),
            is_error: false,
        })
    }
}

// Memory API — memory, MAGMA, working memory, session notes, tasks, skills
// Extracted from api.ts for modularity (P1)

import { invoke } from '@tauri-apps/api/core';
import type { ChatMessage } from './api';
import type { MagmaEvent, MagmaEntity, MagmaProcedure, MagmaEdge, MagmaStats, ToolSchema } from '../types';
export type { MagmaEvent, MagmaEntity, MagmaProcedure, MagmaEdge, MagmaStats };

// ============================================
// Memory System (SQLite + FTS5 + Markdown)
// Adapted from OpenClaw memory architecture (MIT)
// ============================================

export interface MemoryRecord {
  id: string;
  content: string;
  source: string;           // "conversation", "user", "system"
  conversation_id: string | null;
  model_id: string | null;
  tags: string[];
  created_at: string;
  updated_at: string;
}

export interface MemorySearchResult {
  id: string;
  content: string;
  source: string;
  tags: string[];
  score: number;
  snippet: string;
  created_at: string;
}

export interface MemoryStats {
  total_memories: number;
  total_chunks: number;
  total_conversations: number;
  oldest_memory: string | null;
  newest_memory: string | null;
  db_size_bytes: number;
  has_embeddings: boolean;
}

/** Initialize the memory system. Call once on app startup. */
export async function memoryInit(): Promise<string> {
  return invoke('memory_init');
}

/** Save a memory record. */
export async function memorySave(
  content: string,
  source: string,
  conversationId?: string,
  modelId?: string,
  tags: string[] = [],
): Promise<MemoryRecord> {
  return invoke('memory_save', {
    content,
    source,
    conversationId: conversationId || null,
    modelId: modelId || null,
    tags,
  });
}

/** Search memories by query. Returns ranked results. */
export async function memorySearch(
  query: string,
  maxResults?: number,
): Promise<MemorySearchResult[]> {
  return invoke('memory_search', { query, maxResults: maxResults || null });
}

/** List memories, optionally filtered by source. */
export async function memoryList(
  source?: string,
  limit?: number,
): Promise<MemoryRecord[]> {
  return invoke('memory_list', { source: source || null, limit: limit || null });
}

/** Delete a memory by ID. */
export async function memoryDelete(id: string): Promise<boolean> {
  return invoke('memory_delete', { id });
}

/** Delete ALL memories. Returns the number of memories deleted. */
export async function memoryClearAll(): Promise<number> {
  return invoke('memory_clear_all');
}

/** Phase 4C: Promote short_term memories with access_count > 3 to long_term.
 *  Returns number of promoted memories. Call at session start and after flush. */
export async function memoryPromote(): Promise<number> {
  return invoke('memory_promote');
}

/** Phase 4C: Get tier distribution counts (working/short_term/long_term). */
export async function memoryTierCounts(): Promise<Record<string, number>> {
  return invoke('memory_tier_counts');
}

/** Get memory system stats. */
export async function memoryStats(): Promise<MemoryStats> {
  return invoke('memory_stats');
}

/** Check if any embedding provider is available (P2: provider-agnostic).
 *  Returns true if OpenAI, DashScope, OpenRouter, or Ollama can generate embeddings. */
export async function memoryHasEmbeddingsProvider(): Promise<boolean> {
  return invoke('memory_has_embeddings_provider');
}

/** Extract and save key facts from a conversation (pre-compaction flush). */
export async function memoryExtractAndSave(
  conversationId: string,
  modelId: string | undefined,
  messages: ChatMessage[],
): Promise<MemoryRecord[]> {
  return invoke('memory_extract_and_save', {
    conversationId,
    modelId: modelId || null,
    messages,
  });
}

/** Save a user-created memory note ("remember this"). */
export async function memoryRemember(
  content: string,
  tags: string[] = [],
): Promise<MemoryRecord> {
  return invoke('memory_remember', { content, tags });
}

/** Get relevant memories formatted as context for session injection.
 *  contextTokens = model's context window size; memory budget scales proportionally (P2). */
export async function memoryRecall(
  query: string,
  maxResults?: number,
  contextTokens?: number,
): Promise<string> {
  return invoke('memory_recall', { query, maxResults: maxResults || null, contextTokens: contextTokens || null });
}

// ============================================
// Working Memory (Phase 3.5 — Per-Session Scratchpad)
// ============================================

/** Read current working memory contents. Returns empty string if none. */
export async function workingMemoryRead(): Promise<string> {
  return invoke('working_memory_read');
}

/** Write/overwrite working memory contents. */
export async function workingMemoryWrite(content: string): Promise<void> {
  return invoke('working_memory_write', { content });
}

/** Append a timestamped section to working memory. */
export async function workingMemoryAppend(content: string): Promise<void> {
  return invoke('working_memory_append', { content });
}

/** Clear working memory (session end). */
export async function workingMemoryClear(): Promise<void> {
  return invoke('working_memory_clear');
}

/** Flush working memory to short-term memory (saves as memory record, then clears). */
export async function workingMemoryFlush(): Promise<MemoryRecord | null> {
  return invoke('working_memory_flush');
}

// ============================================
// Session Handoff Notes (Phase 3.5.6 — AI Continuity)
// ============================================

/** Read session handoff notes from previous session. Returns empty string if none. */
export async function sessionNotesRead(): Promise<string> {
  return invoke('session_notes_read');
}

/** Write session handoff notes (AI writes continuity notes for next session). */
export async function sessionNotesWrite(content: string): Promise<void> {
  return invoke('session_notes_write', { content });
}

// ============================================
// Cross-Session Task Tracking (Phase 3.5.6)
// ============================================

/** Create or update a tracked task. */
export async function memoryTaskUpsert(
  name: string, description: string, status?: string, notes?: string
): Promise<MagmaEntity> {
  return invoke('memory_task_upsert', {
    name, description, status: status || null, notes: notes || null
  });
}

/** List tasks, optionally filtered by status. */
export async function memoryTaskList(statusFilter?: string): Promise<MagmaEntity[]> {
  return invoke('memory_task_list', { statusFilter: statusFilter || null });
}

// ============================================
// Skills as Graph Nodes (Phase 3.5.5)
// ============================================

/** Sync tool schemas into the MAGMA graph as entities. Called once when tools load. */
export async function memorySyncSkills(tools: ToolSchema[]): Promise<number> {
  return invoke('memory_sync_skills', { tools });
}

/** Discover relevant skills for a query via MAGMA graph traversal. */
export async function memoryDiscoverSkills(query: string): Promise<string[]> {
  return invoke('memory_discover_skills', { query });
}

// ============================================
// Document Ingestion / RAG (Phase 9)
// ============================================

/** Import a file into memory as chunked records. Returns count of sections imported. */
export async function memoryImportFile(filePath: string, customTags?: string[]): Promise<number> {
  return invoke('memory_import_file', { filePath, customTags });
}

// Markdown ↔ DB Sync (Phase 3.5.5)
// ============================================

/** Reimport all markdown memory files into the DB. Returns count of new entries. */
export async function memoryReimportMarkdown(): Promise<number> {
  return invoke('memory_reimport_markdown');
}

/** Get the memory directory path (for showing to user / opening in file manager). */
export async function memoryGetDirectory(): Promise<string> {
  return invoke('memory_get_directory');
}

// ============================================
// --- MAGMA graph operations ---

export async function magmaAddEvent(
  eventType: string, agent: string, content: string,
  metadata?: Record<string, unknown>, sessionId?: string,
): Promise<MagmaEvent> {
  return invoke('magma_add_event', { eventType, agent, content, metadata: metadata || null, sessionId: sessionId || null });
}

export async function magmaEventsSince(
  since: string, agent?: string, limit?: number,
): Promise<MagmaEvent[]> {
  return invoke('magma_events_since', { since, agent: agent || null, limit: limit || null });
}

export async function magmaUpsertEntity(
  entityType: string, name: string,
  entityState?: Record<string, unknown>, metadata?: Record<string, unknown>,
): Promise<MagmaEntity> {
  return invoke('magma_upsert_entity', { entityType, name, entityState: entityState || null, metadata: metadata || null });
}

export async function magmaGetEntity(entityType: string, name: string): Promise<MagmaEntity | null> {
  return invoke('magma_get_entity', { entityType, name });
}

export async function magmaListEntities(entityType?: string, limit?: number): Promise<MagmaEntity[]> {
  return invoke('magma_list_entities', { entityType: entityType || null, limit: limit || null });
}

export async function magmaSaveProcedure(
  name: string, description: string, steps: Record<string, unknown>[], triggerPattern?: string,
): Promise<MagmaProcedure> {
  return invoke('magma_save_procedure', { name, description, steps, triggerPattern: triggerPattern || null });
}

export async function magmaRecordProcedureOutcome(procedureId: string, success: boolean): Promise<void> {
  return invoke('magma_record_procedure_outcome', { procedureId, success });
}

export async function magmaAddEdge(
  sourceType: string, sourceId: string, targetType: string, targetId: string,
  edgeType: string, weight?: number, metadata?: Record<string, unknown>,
): Promise<MagmaEdge> {
  return invoke('magma_add_edge', { sourceType, sourceId, targetType, targetId, edgeType, weight: weight || null, metadata: metadata || null });
}

export async function magmaTraverse(
  nodeType: string, nodeId: string, maxDepth?: number, edgeTypes?: string[],
): Promise<MagmaEdge[]> {
  return invoke('magma_traverse', { nodeType, nodeId, maxDepth: maxDepth || null, edgeTypes: edgeTypes || null });
}

export async function magmaGetStats(): Promise<MagmaStats> {
  return invoke('magma_stats');
}

// ============================================

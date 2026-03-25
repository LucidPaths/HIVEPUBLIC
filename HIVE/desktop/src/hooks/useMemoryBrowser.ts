import { useState, useEffect, useRef } from 'react';
import * as api from '../lib/api';

// Shared memory browser logic used by both MemoryTab (full-page) and MemoryPanel (slide-out).
// Deduplicates ~120 lines of identical state + handlers (Q1 audit fix).

interface UseMemoryBrowserOptions {
  /** When to load data. MemoryPanel passes `isOpen`, MemoryTab passes `true`. */
  active: boolean;
  /** Max records to list. Default 200. */
  listLimit?: number;
  /** Max search results. Default 30. */
  searchLimit?: number;
}

export function useMemoryBrowser({ active, listLimit = 200, searchLimit = 30 }: UseMemoryBrowserOptions) {
  const [memories, setMemories] = useState<api.MemoryRecord[]>([]);
  const [searchQuery, setSearchQuery] = useState('');
  const [searchResults, setSearchResults] = useState<api.MemorySearchResult[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editContent, setEditContent] = useState('');
  const [addingNote, setAddingNote] = useState(false);
  const [newNote, setNewNote] = useState('');
  const [newTags, setNewTags] = useState('');
  const [stats, setStats] = useState<api.MemoryStats | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [deleteConfirm, setDeleteConfirm] = useState<string | null>(null);
  const [clearAllConfirm, setClearAllConfirm] = useState(false);
  const searchTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (active) {
      loadMemories();
      loadStats();
    }
    return () => {
      if (searchTimeoutRef.current) clearTimeout(searchTimeoutRef.current);
    };
  }, [active]);

  async function loadMemories() {
    setLoading(true);
    try {
      setMemories(await api.memoryList(undefined, listLimit));
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  async function loadStats() {
    try {
      setStats(await api.memoryStats());
    } catch { /* non-fatal */ }
  }

  async function handleSearch(query: string) {
    setSearchQuery(query);
    if (!query.trim()) {
      setSearchResults(null);
      return;
    }
    if (searchTimeoutRef.current) clearTimeout(searchTimeoutRef.current);
    searchTimeoutRef.current = setTimeout(async () => {
      try {
        setSearchResults(await api.memorySearch(query, searchLimit));
      } catch (e) {
        setError(String(e));
      }
    }, 300);
  }

  async function handleDelete(id: string) {
    try {
      await api.memoryDelete(id);
      setMemories(prev => prev.filter(m => m.id !== id));
      setSearchResults(prev => prev?.filter(m => m.id !== id) ?? null);
      setDeleteConfirm(null);
      loadStats();
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleClearAll() {
    try {
      await api.memoryClearAll();
      setMemories([]);
      setSearchResults(null);
      setClearAllConfirm(false);
      loadStats();
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleSaveEdit(memory: api.MemoryRecord) {
    if (!editContent.trim()) return;
    try {
      await api.memoryDelete(memory.id);
      await api.memorySave(
        editContent.trim(),
        memory.source,
        memory.conversation_id || undefined,
        memory.model_id || undefined,
        memory.tags,
      );
      setEditingId(null);
      setEditContent('');
      loadMemories();
      loadStats();
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleAddNote() {
    if (!newNote.trim()) return;
    try {
      const tags = newTags.split(',').map(t => t.trim()).filter(Boolean);
      await api.memoryRemember(newNote.trim(), tags);
      setNewNote('');
      setNewTags('');
      setAddingNote(false);
      loadMemories();
      loadStats();
    } catch (e) {
      setError(String(e));
    }
  }

  // Unified display items — merges search results and full list into common shape
  const displayItems = searchResults
    ? searchResults.map(r => ({
        id: r.id, content: r.content, source: r.source,
        tags: r.tags, created_at: r.created_at,
        score: r.score, snippet: r.snippet,
      }))
    : memories.map(m => ({
        id: m.id, content: m.content, source: m.source,
        tags: m.tags, created_at: m.created_at,
        score: undefined as number | undefined,
        snippet: undefined as string | undefined,
      }));

  return {
    // State
    memories, searchQuery, searchResults, loading, stats, error,
    editingId, editContent, addingNote, newNote, newTags,
    deleteConfirm, clearAllConfirm, displayItems,
    // Setters
    setEditingId, setEditContent, setAddingNote, setNewNote, setNewTags,
    setDeleteConfirm, setClearAllConfirm, setError,
    // Actions
    handleSearch, handleDelete, handleClearAll, handleSaveEdit, handleAddNote,
    loadMemories, loadStats,
  };
}

// Shared helpers
export function getSourceBadge(source: string) {
  switch (source) {
    case 'conversation':
      return { label: 'Auto', color: 'bg-blue-500/20 text-blue-400 border-blue-500/30' };
    case 'user':
    case 'user-note':
      return { label: 'Note', color: 'bg-purple-500/20 text-purple-400 border-purple-500/30' };
    case 'system':
      return { label: 'System', color: 'bg-zinc-500/20 text-zinc-400 border-zinc-500/30' };
    default:
      return { label: source, color: 'bg-zinc-500/20 text-zinc-400 border-zinc-500/30' };
  }
}

export function formatMemoryDate(dateStr: string) {
  try {
    const d = new Date(dateStr);
    const now = new Date();
    const diffMs = now.getTime() - d.getTime();
    const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));
    if (diffDays === 0) return 'Today';
    if (diffDays === 1) return 'Yesterday';
    if (diffDays < 7) return `${diffDays}d ago`;
    return d.toLocaleDateString();
  } catch {
    return dateStr;
  }
}

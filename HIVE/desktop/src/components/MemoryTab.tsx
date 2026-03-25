import { useState, useEffect, useMemo } from 'react';
import {
  Brain, Search, Plus, Trash2, Edit3, Save, Tag, Clock,
  AlertCircle, X, RefreshCw, Zap, Database, GitBranch,
  Activity, Box, List, ChevronDown, ChevronRight, Upload,
} from 'lucide-react';
import { open as openFileDialog } from '@tauri-apps/plugin-dialog';
import * as api from '../lib/api';
import { useMemoryBrowser, getSourceBadge, formatMemoryDate } from '../hooks/useMemoryBrowser';

// ============================================================
// MemoryTab — Full-page tab combining:
//   Left panel:  Memory browser (search, list, add, edit, delete)
//   Right panel: MAGMA graph viewer (stats, events, entities)
// Self-contained: calls api.* directly, no props threading.
// ============================================================

export default function MemoryTab() {
  return (
    <div className="h-full flex overflow-hidden">
      {/* Left: Memory Browser */}
      <div className="w-1/2 border-r border-zinc-700 flex flex-col overflow-hidden">
        <MemoryBrowser />
      </div>

      {/* Right: MAGMA Graph Viewer */}
      <div className="w-1/2 flex flex-col overflow-hidden">
        <MagmaViewer />
      </div>
    </div>
  );
}

// ============================================================
// Memory Browser
// ============================================================

function MemoryBrowser() {
  const mb = useMemoryBrowser({ active: true, listLimit: 200, searchLimit: 30 });
  const memoryById = useMemo(() => new Map(mb.memories.map(m => [m.id, m])), [mb.memories]);
  const [importProgress, setImportProgress] = useState<{ done: number; total: number; memories: number } | null>(null);

  async function handleBatchImport() {
    try {
      const files = await openFileDialog({
        multiple: true,
        filters: [
          // Sync with text_extensions in memory_tools.rs
          { name: 'Supported Files', extensions: [
            'txt', 'md', 'rs', 'ts', 'tsx', 'py', 'js', 'jsx', 'json', 'toml',
            'yaml', 'yml', 'csv', 'html', 'css', 'go', 'java', 'c', 'cpp', 'h',
            'hpp', 'sh', 'bat', 'ps1', 'sql', 'xml', 'ini', 'cfg', 'conf', 'log',
          ]},
          { name: 'All Files', extensions: ['*'] },
        ],
      });
      if (!files) return;
      const paths = Array.isArray(files) ? files : [files];
      if (paths.length === 0) return;
      setImportProgress({ done: 0, total: paths.length, memories: 0 });
      let totalMemories = 0;
      for (let i = 0; i < paths.length; i++) {
        try {
          const count = await api.memoryImportFile(paths[i], []);
          totalMemories += count;
        } catch (e) {
          console.warn(`[HIVE] Failed to import ${paths[i]}:`, e);
        }
        setImportProgress({ done: i + 1, total: paths.length, memories: totalMemories });
      }
      // Clear progress after 3s, refresh
      setTimeout(() => setImportProgress(null), 3000);
      mb.loadMemories();
      mb.loadStats();
    } catch (e) {
      mb.setError(String(e));
      setImportProgress(null);
    }
  }

  return (
    <>
      {/* Header */}
      <div className="p-4 border-b border-zinc-700 flex items-center justify-between flex-shrink-0">
        <div className="flex items-center gap-2">
          <Brain className="w-5 h-5 text-purple-400" />
          <h2 className="text-white font-medium">Memory</h2>
          {mb.stats && (
            <span className="text-xs text-zinc-500">
              {mb.stats.total_memories} memories · {(mb.stats.db_size_bytes / 1024).toFixed(0)} KB
            </span>
          )}
        </div>
        <div className="flex items-center gap-1">
          {mb.clearAllConfirm ? (
            <div className="flex items-center gap-1 mr-1">
              <span className="text-xs text-red-400">Clear all?</span>
              <button
                onClick={mb.handleClearAll}
                className="px-2 py-1 text-xs text-red-400 hover:bg-red-500/20 rounded"
              >
                Yes
              </button>
              <button
                onClick={() => mb.setClearAllConfirm(false)}
                className="px-2 py-1 text-xs text-zinc-400 hover:text-white rounded"
              >
                No
              </button>
            </div>
          ) : (
            mb.memories.length > 0 && (
              <button
                onClick={() => mb.setClearAllConfirm(true)}
                className="flex items-center gap-1 px-2 py-1 text-xs text-zinc-500 hover:text-red-400 hover:bg-zinc-800 rounded-lg"
                title="Clear all memories"
              >
                <Trash2 className="w-3.5 h-3.5" />
                Clear All
              </button>
            )
          )}
          <button
            onClick={() => { mb.loadMemories(); mb.loadStats(); }}
            className="p-1.5 hover:bg-zinc-800 rounded-lg text-zinc-500 hover:text-zinc-300 transition-colors"
            title="Refresh"
          >
            <RefreshCw className="w-4 h-4" />
          </button>
        </div>
      </div>

      {/* Search */}
      <div className="px-4 py-3 border-b border-zinc-800 flex-shrink-0">
        <div className="relative">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-zinc-500" />
          <input
            type="text"
            value={mb.searchQuery}
            onChange={e => mb.handleSearch(e.target.value)}
            placeholder="Search memories..."
            className="w-full bg-zinc-800 text-white pl-9 pr-4 py-2 rounded-lg border border-zinc-700 focus:border-purple-500 outline-none text-sm"
          />
          {mb.searchQuery && (
            <button
              onClick={() => mb.handleSearch('')}
              className="absolute right-3 top-1/2 -translate-y-1/2 text-zinc-500 hover:text-white"
            >
              <X className="w-3.5 h-3.5" />
            </button>
          )}
        </div>
      </div>

      {/* Add Note */}
      <div className="px-4 pt-3 flex-shrink-0">
        {mb.addingNote ? (
          <div className="bg-zinc-800 rounded-lg p-3 space-y-2">
            <textarea
              value={mb.newNote}
              onChange={e => mb.setNewNote(e.target.value)}
              placeholder="Write a note to remember..."
              rows={3}
              autoFocus
              className="w-full bg-zinc-900 text-white px-3 py-2 rounded-lg border border-zinc-600 focus:border-purple-500 outline-none text-sm resize-none"
            />
            <input
              type="text"
              value={mb.newTags}
              onChange={e => mb.setNewTags(e.target.value)}
              placeholder="Tags (comma-separated, optional)"
              className="w-full bg-zinc-900 text-white px-3 py-1.5 rounded-lg border border-zinc-600 focus:border-purple-500 outline-none text-xs"
            />
            <div className="flex gap-2 justify-end">
              <button
                onClick={() => { mb.setAddingNote(false); mb.setNewNote(''); mb.setNewTags(''); }}
                className="px-3 py-1 text-xs text-zinc-400 hover:text-white"
              >
                Cancel
              </button>
              <button
                onClick={mb.handleAddNote}
                disabled={!mb.newNote.trim()}
                className="px-3 py-1 text-xs bg-purple-500 hover:bg-purple-600 disabled:opacity-50 text-white rounded-lg"
              >
                Save Note
              </button>
            </div>
          </div>
        ) : (
          <div className="flex gap-2 mb-3">
            <button
              onClick={() => mb.setAddingNote(true)}
              className="flex-1 flex items-center justify-center gap-2 px-3 py-2 bg-zinc-800 hover:bg-zinc-700 text-zinc-300 rounded-lg text-sm border border-zinc-700 border-dashed"
            >
              <Plus className="w-4 h-4" />
              Add Note
            </button>
            <button
              onClick={handleBatchImport}
              disabled={!!importProgress}
              className="flex-1 flex items-center justify-center gap-2 px-3 py-2 bg-zinc-800 hover:bg-zinc-700 text-zinc-300 rounded-lg text-sm border border-zinc-700 border-dashed disabled:opacity-50"
            >
              <Upload className="w-4 h-4" />
              Import Files
            </button>
          </div>
        )}
        {importProgress && (
          <div className="mb-3 p-2.5 rounded-lg bg-purple-500/10 border border-purple-500/30 text-sm">
            <div className="flex items-center justify-between text-purple-400">
              <span>
                {importProgress.done < importProgress.total
                  ? `Importing ${importProgress.done + 1}/${importProgress.total}...`
                  : `Done — ${importProgress.memories} memories imported from ${importProgress.total} files`}
              </span>
            </div>
            <div className="mt-1.5 h-1 bg-zinc-700 rounded-full overflow-hidden">
              <div
                className="h-full bg-purple-500 transition-all"
                style={{ width: `${(importProgress.done / importProgress.total) * 100}%` }}
              />
            </div>
          </div>
        )}
      </div>

      {/* Memory List */}
      <div className="flex-1 overflow-y-auto px-4 py-2 space-y-2">
        {mb.loading ? (
          <div className="text-center text-zinc-500 py-8">Loading memories...</div>
        ) : mb.displayItems.length === 0 ? (
          <div className="text-center text-zinc-500 py-8 text-sm">
            {mb.searchQuery
              ? 'No matching memories found'
              : 'No memories yet. Conversations will be remembered automatically.'}
          </div>
        ) : (
          mb.displayItems.map(item => {
            const badge = getSourceBadge(item.source);
            const isEditing = mb.editingId === item.id;
            const fullMem = memoryById.get(item.id);

            return (
              <div key={item.id} className="bg-zinc-800 rounded-lg p-3 group">
                {isEditing && fullMem ? (
                  <div className="space-y-2">
                    <textarea
                      value={mb.editContent}
                      onChange={e => mb.setEditContent(e.target.value)}
                      rows={4}
                      autoFocus
                      className="w-full bg-zinc-900 text-white px-3 py-2 rounded-lg border border-purple-500 outline-none text-sm resize-y"
                    />
                    <div className="flex gap-2 justify-end">
                      <button
                        onClick={() => { mb.setEditingId(null); mb.setEditContent(''); }}
                        className="px-3 py-1 text-xs text-zinc-400 hover:text-white"
                      >
                        Cancel
                      </button>
                      <button
                        onClick={() => mb.handleSaveEdit(fullMem)}
                        className="flex items-center gap-1 px-3 py-1 text-xs bg-purple-500 hover:bg-purple-600 text-white rounded-lg"
                      >
                        <Save className="w-3 h-3" />
                        Save
                      </button>
                    </div>
                  </div>
                ) : (
                  <>
                    <p className="text-zinc-200 text-sm whitespace-pre-wrap break-words">
                      {item.content.length > 350
                        ? item.content.substring(0, 350) + '...'
                        : item.content}
                    </p>
                    {item.score !== undefined && (
                      <div className="mt-1 text-xs text-purple-400">
                        Relevance: {(item.score * 100).toFixed(0)}%
                      </div>
                    )}
                    <div className="flex items-center gap-2 mt-2 flex-wrap">
                      <span className={`text-xs px-1.5 py-0.5 rounded border ${badge.color}`}>
                        {badge.label}
                      </span>
                      {item.tags.map(tag => (
                        <span key={tag} className="flex items-center gap-0.5 text-xs text-zinc-500">
                          <Tag className="w-2.5 h-2.5" />
                          {tag}
                        </span>
                      ))}
                      <span className="flex items-center gap-1 text-xs text-zinc-600 ml-auto">
                        <Clock className="w-3 h-3" />
                        {formatMemoryDate(item.created_at)}
                      </span>
                    </div>
                    <div className="flex gap-1 mt-2 opacity-0 group-hover:opacity-100 transition-opacity">
                      {fullMem && (
                        <button
                          onClick={() => { mb.setEditingId(item.id); mb.setEditContent(item.content); }}
                          className="flex items-center gap-1 px-2 py-1 text-xs text-zinc-400 hover:text-white hover:bg-zinc-700 rounded"
                        >
                          <Edit3 className="w-3 h-3" />
                          Edit
                        </button>
                      )}
                      {mb.deleteConfirm === item.id ? (
                        <div className="flex items-center gap-1">
                          <span className="text-xs text-red-400">Delete?</span>
                          <button
                            onClick={() => mb.handleDelete(item.id)}
                            className="px-2 py-1 text-xs text-red-400 hover:bg-red-500/20 rounded"
                          >
                            Yes
                          </button>
                          <button
                            onClick={() => mb.setDeleteConfirm(null)}
                            className="px-2 py-1 text-xs text-zinc-400 hover:text-white rounded"
                          >
                            No
                          </button>
                        </div>
                      ) : (
                        <button
                          onClick={() => mb.setDeleteConfirm(item.id)}
                          className="flex items-center gap-1 px-2 py-1 text-xs text-zinc-400 hover:text-red-400 hover:bg-zinc-700 rounded"
                        >
                          <Trash2 className="w-3 h-3" />
                          Delete
                        </button>
                      )}
                    </div>
                  </>
                )}
              </div>
            );
          })
        )}
      </div>

      {/* Error */}
      {mb.error && (
        <div className="px-4 pb-3 flex-shrink-0">
          <div className="flex items-center gap-2 text-red-400 text-sm bg-red-500/10 px-3 py-2 rounded-lg">
            <AlertCircle className="w-4 h-4 shrink-0" />
            <span className="flex-1">{mb.error}</span>
            <button onClick={() => mb.setError(null)} className="text-red-300 hover:text-white">
              <X className="w-3 h-3" />
            </button>
          </div>
        </div>
      )}
    </>
  );
}

// ============================================================
// MAGMA Graph Viewer
// ============================================================

function MagmaViewer() {
  const [stats, setStats] = useState<api.MagmaStats | null>(null);
  const [events, setEvents] = useState<api.MagmaEvent[]>([]);
  const [entities, setEntities] = useState<api.MagmaEntity[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [activeSection, setActiveSection] = useState<'events' | 'entities'>('events');
  const [expandedEvent, setExpandedEvent] = useState<string | null>(null);
  const [expandedEntity, setExpandedEntity] = useState<string | null>(null);

  useEffect(() => {
    loadMagma();
  }, []);

  async function loadMagma() {
    setLoading(true);
    try {
      const [magmaStats, recentEvents, allEntities] = await Promise.all([
        api.magmaGetStats(),
        // Fetch events from last 30 days, up to 100
        api.magmaEventsSince(
          new Date(Date.now() - 30 * 24 * 60 * 60 * 1000).toISOString(),
          undefined,
          100,
        ),
        api.magmaListEntities(undefined, 100),
      ]);
      setStats(magmaStats);
      setEvents(recentEvents);
      setEntities(allEntities);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  function formatEventType(t: string) {
    return t.replace(/_/g, ' ');
  }

  function getEventColor(eventType: string) {
    if (eventType.includes('wake')) return 'text-green-400';
    if (eventType.includes('sleep')) return 'text-zinc-500';
    if (eventType.includes('fail') || eventType.includes('error')) return 'text-red-400';
    if (eventType.includes('task')) return 'text-blue-400';
    if (eventType.includes('specialist')) return 'text-amber-400';
    return 'text-zinc-400';
  }

  function getEntityIcon(entityType: string) {
    if (entityType === 'model') return '🤖';
    if (entityType === 'user') return '👤';
    if (entityType === 'specialist') return '⚡';
    if (entityType === 'session') return '💬';
    return '📦';
  }

  function formatDate(dateStr: string) {
    try {
      const d = new Date(dateStr);
      const diffMs = Date.now() - d.getTime();
      const diffMins = Math.floor(diffMs / 60000);
      const diffHours = Math.floor(diffMs / 3600000);
      const diffDays = Math.floor(diffMs / 86400000);
      if (diffMins < 1) return 'just now';
      if (diffMins < 60) return `${diffMins}m ago`;
      if (diffHours < 24) return `${diffHours}h ago`;
      if (diffDays < 7) return `${diffDays}d ago`;
      return d.toLocaleDateString();
    } catch {
      return dateStr;
    }
  }

  return (
    <>
      {/* Header */}
      <div className="p-4 border-b border-zinc-700 flex items-center justify-between flex-shrink-0">
        <div className="flex items-center gap-2">
          <GitBranch className="w-5 h-5 text-amber-400" />
          <h2 className="text-white font-medium">MAGMA Graph</h2>
          <span className="text-xs text-zinc-500">Multi-graph memory architecture</span>
        </div>
        <button
          onClick={loadMagma}
          disabled={loading}
          className="p-1.5 hover:bg-zinc-800 rounded-lg text-zinc-500 hover:text-zinc-300 transition-colors"
          title="Refresh"
        >
          <RefreshCw className={`w-4 h-4 ${loading ? 'animate-spin' : ''}`} />
        </button>
      </div>

      {error && (
        <div className="px-4 pt-3 flex-shrink-0">
          <div className="flex items-center gap-2 text-red-400 text-xs bg-red-500/10 px-3 py-2 rounded-lg">
            <AlertCircle className="w-3.5 h-3.5 shrink-0" />
            <span className="flex-1">{error}</span>
            <button onClick={() => setError(null)}><X className="w-3 h-3" /></button>
          </div>
        </div>
      )}

      {/* Stats Row */}
      {stats && (
        <div className="px-4 py-3 grid grid-cols-4 gap-2 flex-shrink-0">
          <StatCard icon={<Activity className="w-4 h-4 text-blue-400" />} label="Events" value={stats.events} color="text-blue-400" />
          <StatCard icon={<Box className="w-4 h-4 text-green-400" />} label="Entities" value={stats.entities} color="text-green-400" />
          <StatCard icon={<List className="w-4 h-4 text-amber-400" />} label="Procedures" value={stats.procedures} color="text-amber-400" />
          <StatCard icon={<Database className="w-4 h-4 text-purple-400" />} label="Edges" value={stats.edges} color="text-purple-400" />
        </div>
      )}

      {/* Section Tabs */}
      <div className="px-4 flex gap-1 border-b border-zinc-800 flex-shrink-0">
        <button
          onClick={() => setActiveSection('events')}
          className={`px-3 py-2 text-xs font-medium transition-colors ${
            activeSection === 'events'
              ? 'text-amber-400 border-b-2 border-amber-400'
              : 'text-zinc-500 hover:text-zinc-300'
          }`}
        >
          Episodic Events ({events.length})
        </button>
        <button
          onClick={() => setActiveSection('entities')}
          className={`px-3 py-2 text-xs font-medium transition-colors ${
            activeSection === 'entities'
              ? 'text-amber-400 border-b-2 border-amber-400'
              : 'text-zinc-500 hover:text-zinc-300'
          }`}
        >
          Entities ({entities.length})
        </button>
      </div>

      {/* Section Content */}
      <div className="flex-1 overflow-y-auto px-4 py-3 space-y-1.5">
        {loading && (
          <div className="text-center text-zinc-500 py-8 text-sm">Loading MAGMA graph...</div>
        )}

        {!loading && activeSection === 'events' && (
          events.length === 0 ? (
            <div className="text-center text-zinc-500 py-8 text-sm">
              No events in the last 30 days.
              <p className="text-xs mt-1 text-zinc-600">Events are logged automatically as HIVE routes tasks to specialists.</p>
            </div>
          ) : (
            events.map(event => {
              const isExpanded = expandedEvent === event.id;
              const hasContent = event.content && event.content.length > 0;
              return (
                <div
                  key={event.id}
                  className="bg-zinc-800 rounded-lg overflow-hidden"
                >
                  <button
                    className="w-full flex items-center gap-2 px-3 py-2 text-left hover:bg-zinc-700/50 transition-colors"
                    onClick={() => setExpandedEvent(isExpanded ? null : event.id)}
                  >
                    {hasContent
                      ? isExpanded
                        ? <ChevronDown className="w-3.5 h-3.5 text-zinc-500 flex-shrink-0" />
                        : <ChevronRight className="w-3.5 h-3.5 text-zinc-500 flex-shrink-0" />
                      : <Zap className="w-3.5 h-3.5 text-zinc-600 flex-shrink-0" />
                    }
                    <span className={`text-xs font-medium capitalize flex-shrink-0 w-28 ${getEventColor(event.event_type)}`}>
                      {formatEventType(event.event_type)}
                    </span>
                    <span className="text-zinc-400 text-xs truncate flex-1">
                      {event.agent}
                    </span>
                    <span className="text-zinc-600 text-xs flex-shrink-0">
                      {formatDate(event.created_at)}
                    </span>
                  </button>
                  {isExpanded && hasContent && (
                    <div className="px-3 pb-2 pt-0">
                      <p className="text-zinc-300 text-xs whitespace-pre-wrap break-words bg-zinc-900 rounded px-2 py-1.5">
                        {event.content}
                      </p>
                      {event.session_id && (
                        <p className="text-zinc-600 text-xs mt-1">
                          Session: {event.session_id.substring(0, 16)}…
                        </p>
                      )}
                    </div>
                  )}
                </div>
              );
            })
          )
        )}

        {!loading && activeSection === 'entities' && (
          entities.length === 0 ? (
            <div className="text-center text-zinc-500 py-8 text-sm">
              No entities tracked yet.
              <p className="text-xs mt-1 text-zinc-600">Entities are created when HIVE tracks models, users, and specialists.</p>
            </div>
          ) : (
            entities.map(entity => {
              const isExpanded = expandedEntity === entity.id;
              const stateKeys = Object.keys(entity.state || {});
              return (
                <div key={entity.id} className="bg-zinc-800 rounded-lg overflow-hidden">
                  <button
                    className="w-full flex items-center gap-2 px-3 py-2 text-left hover:bg-zinc-700/50 transition-colors"
                    onClick={() => setExpandedEntity(isExpanded ? null : entity.id)}
                  >
                    {stateKeys.length > 0
                      ? isExpanded
                        ? <ChevronDown className="w-3.5 h-3.5 text-zinc-500 flex-shrink-0" />
                        : <ChevronRight className="w-3.5 h-3.5 text-zinc-500 flex-shrink-0" />
                      : <span className="w-3.5 h-3.5 flex-shrink-0 text-center text-xs">{getEntityIcon(entity.entity_type)}</span>
                    }
                    <span className="text-xs text-zinc-500 flex-shrink-0 w-20 capitalize">
                      {entity.entity_type}
                    </span>
                    <span className="text-zinc-200 text-xs truncate flex-1 font-medium">
                      {entity.name}
                    </span>
                    <span className="text-zinc-600 text-xs flex-shrink-0">
                      {formatDate(entity.updated_at)}
                    </span>
                  </button>
                  {isExpanded && stateKeys.length > 0 && (
                    <div className="px-3 pb-2 pt-0">
                      <div className="bg-zinc-900 rounded px-2 py-1.5 space-y-0.5">
                        {stateKeys.map(k => (
                          <div key={k} className="flex gap-2 text-xs">
                            <span className="text-zinc-500 flex-shrink-0">{k}:</span>
                            <span className="text-zinc-300 break-all">
                              {String(entity.state[k])}
                            </span>
                          </div>
                        ))}
                      </div>
                    </div>
                  )}
                </div>
              );
            })
          )
        )}
      </div>

      {/* Footer hint */}
      <div className="px-4 py-2 border-t border-zinc-800 flex-shrink-0">
        <p className="text-zinc-600 text-xs">
          MAGMA: episodic events + entity state + procedural memory — logged automatically by the orchestrator.
        </p>
      </div>
    </>
  );
}

// ============================================================
// Stat Card (for MAGMA stats row)
// ============================================================

interface StatCardProps {
  icon: React.ReactNode;
  label: string;
  value: number;
  color: string;
}

function StatCard({ icon, label, value, color }: StatCardProps) {
  return (
    <div className="bg-zinc-800 rounded-lg p-2 flex flex-col items-center gap-1">
      {icon}
      <span className={`text-lg font-bold ${color}`}>{value}</span>
      <span className="text-zinc-500 text-xs">{label}</span>
    </div>
  );
}

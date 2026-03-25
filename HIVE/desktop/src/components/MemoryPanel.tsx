import { X, Search, Plus, Trash2, Edit3, Save, Tag, Clock, Brain, AlertCircle } from 'lucide-react';
import { useMemoryBrowser, getSourceBadge, formatMemoryDate } from '../hooks/useMemoryBrowser';

interface Props {
  isOpen: boolean;
  onClose: () => void;
}

export default function MemoryPanel({ isOpen, onClose }: Props) {
  const mb = useMemoryBrowser({ active: isOpen, listLimit: 100, searchLimit: 20 });

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex justify-end">
      {/* Backdrop */}
      <div className="absolute inset-0 bg-black/50" onClick={onClose} />

      {/* Panel */}
      <div className="relative w-full max-w-lg bg-zinc-900 border-l border-zinc-700 flex flex-col shadow-2xl">
        {/* Header */}
        <div className="p-4 border-b border-zinc-700 flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Brain className="w-5 h-5 text-purple-400" />
            <h2 className="text-white font-medium">Memory</h2>
            {mb.stats && (
              <span className="text-xs text-zinc-500">
                {mb.stats.total_memories} memories, {(mb.stats.db_size_bytes / 1024).toFixed(0)} KB
              </span>
            )}
          </div>
          <div className="flex items-center gap-1">
            {mb.clearAllConfirm ? (
              <div className="flex items-center gap-1 mr-2">
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
            <button onClick={onClose} className="p-1.5 hover:bg-zinc-800 rounded-lg text-zinc-400 hover:text-white">
              <X className="w-5 h-5" />
            </button>
          </div>
        </div>

        {/* Search */}
        <div className="p-3 border-b border-zinc-800">
          <div className="relative">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-zinc-500" />
            <input
              type="text"
              value={mb.searchQuery}
              onChange={(e) => mb.handleSearch(e.target.value)}
              placeholder="Search memories..."
              className="w-full bg-zinc-800 text-white pl-9 pr-4 py-2 rounded-lg border border-zinc-700 focus:border-purple-500 outline-none text-sm"
            />
          </div>
        </div>

        {/* Add Note Button / Form */}
        <div className="px-3 pt-3">
          {mb.addingNote ? (
            <div className="bg-zinc-800 rounded-lg p-3 space-y-2">
              <textarea
                value={mb.newNote}
                onChange={(e) => mb.setNewNote(e.target.value)}
                placeholder="Write a note to remember..."
                rows={3}
                className="w-full bg-zinc-900 text-white px-3 py-2 rounded-lg border border-zinc-600 focus:border-purple-500 outline-none text-sm resize-none"
                autoFocus
              />
              <input
                type="text"
                value={mb.newTags}
                onChange={(e) => mb.setNewTags(e.target.value)}
                placeholder="Tags (comma-separated)"
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
                  Save
                </button>
              </div>
            </div>
          ) : (
            <button
              onClick={() => mb.setAddingNote(true)}
              className="w-full flex items-center justify-center gap-2 px-3 py-2 bg-zinc-800 hover:bg-zinc-700 text-zinc-300 rounded-lg text-sm border border-zinc-700 border-dashed"
            >
              <Plus className="w-4 h-4" />
              Add Note
            </button>
          )}
        </div>

        {/* Memory List */}
        <div className="flex-1 overflow-y-auto p-3 space-y-2">
          {mb.loading ? (
            <div className="text-center text-zinc-500 py-8">Loading memories...</div>
          ) : mb.displayItems.length === 0 ? (
            <div className="text-center text-zinc-500 py-8">
              {mb.searchQuery ? 'No matching memories found' : 'No memories yet. Conversations will be remembered automatically.'}
            </div>
          ) : (
            mb.displayItems.map((item) => {
              const sourceBadge = getSourceBadge(item.source);
              const isEditing = mb.editingId === item.id;

              return (
                <div key={item.id} className="bg-zinc-800 rounded-lg p-3 group">
                  {isEditing ? (
                    <div className="space-y-2">
                      <textarea
                        value={mb.editContent}
                        onChange={(e) => mb.setEditContent(e.target.value)}
                        rows={4}
                        className="w-full bg-zinc-900 text-white px-3 py-2 rounded-lg border border-purple-500 outline-none text-sm resize-y"
                        autoFocus
                      />
                      <div className="flex gap-2 justify-end">
                        <button
                          onClick={() => { mb.setEditingId(null); mb.setEditContent(''); }}
                          className="px-3 py-1 text-xs text-zinc-400 hover:text-white"
                        >
                          Cancel
                        </button>
                        <button
                          onClick={() => {
                            const mem = mb.memories.find(m => m.id === item.id);
                            if (mem) mb.handleSaveEdit(mem);
                          }}
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
                        {item.content.length > 300
                          ? item.content.substring(0, 300) + '...'
                          : item.content}
                      </p>

                      {item.score !== undefined && (
                        <div className="mt-1 text-xs text-purple-400">
                          Relevance: {(item.score * 100).toFixed(0)}%
                        </div>
                      )}

                      <div className="flex items-center gap-2 mt-2 flex-wrap">
                        <span className={`text-xs px-1.5 py-0.5 rounded border ${sourceBadge.color}`}>
                          {sourceBadge.label}
                        </span>
                        {item.tags.length > 0 && item.tags.map(tag => (
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
                        <button
                          onClick={() => { mb.setEditingId(item.id); mb.setEditContent(item.content); }}
                          className="flex items-center gap-1 px-2 py-1 text-xs text-zinc-400 hover:text-white hover:bg-zinc-700 rounded"
                        >
                          <Edit3 className="w-3 h-3" />
                          Edit
                        </button>
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

        {/* Error toast */}
        {mb.error && (
          <div className="p-3 border-t border-zinc-800">
            <div className="flex items-center gap-2 text-red-400 text-sm bg-red-500/10 px-3 py-2 rounded-lg">
              <AlertCircle className="w-4 h-4 shrink-0" />
              <span className="flex-1">{mb.error}</span>
              <button onClick={() => mb.setError(null)} className="text-red-300 hover:text-white">
                <X className="w-3 h-3" />
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

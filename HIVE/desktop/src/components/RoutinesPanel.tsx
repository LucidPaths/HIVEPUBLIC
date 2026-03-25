// Self-contained Routines Panel — manages standing instructions (Phase 6)
//
// Architecture: Self-contained component (P1 — modularity).
// Calls api.* directly, manages its own state.
// Can be embedded in SettingsTab or used standalone.

import { useState, useEffect } from 'react';
import { Plus, Trash2, Play, Pause, Clock, Zap, Save, X, RotateCcw, Activity } from 'lucide-react';
import * as api from '../lib/api';
import type { Routine, RoutineStats, TriggerType } from '../types';

export default function RoutinesPanel() {
  const [routines, setRoutines] = useState<Routine[]>([]);
  const [stats, setStats] = useState<RoutineStats | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showCreate, setShowCreate] = useState(false);
  const [daemonRunning, setDaemonRunning] = useState(false);

  // Create form state
  const [newName, setNewName] = useState('');
  const [newDescription, setNewDescription] = useState('');
  const [newTriggerType, setNewTriggerType] = useState<TriggerType>('event');
  const [newCronExpr, setNewCronExpr] = useState('');
  const [newEventPattern, setNewEventPattern] = useState('channel:*');
  const [newEventKeyword, setNewEventKeyword] = useState('');
  const [newActionPrompt, setNewActionPrompt] = useState('');
  const [newResponseChannel, setNewResponseChannel] = useState('');

  // Delete confirmation
  const [deleteConfirm, setDeleteConfirm] = useState<string | null>(null);

  useEffect(() => {
    loadRoutines();
    loadStats();
    checkDaemon();
  }, []);

  async function loadRoutines() {
    setLoading(true);
    try {
      setRoutines(await api.routineList());
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  async function loadStats() {
    try {
      setStats(await api.routineStats());
    } catch {}
  }

  async function checkDaemon() {
    try {
      setDaemonRunning(await api.routinesDaemonStatus());
    } catch {}
  }

  async function handleCreate() {
    if (!newName.trim() || !newActionPrompt.trim()) {
      setError('Name and action prompt are required');
      return;
    }

    try {
      await api.routineCreate({
        name: newName.trim(),
        description: newDescription.trim(),
        triggerType: newTriggerType,
        cronExpr: (newTriggerType === 'cron' || newTriggerType === 'both') ? newCronExpr.trim() : undefined,
        eventPattern: (newTriggerType === 'event' || newTriggerType === 'both') ? newEventPattern.trim() : undefined,
        eventKeyword: newEventKeyword.trim() || undefined,
        actionPrompt: newActionPrompt.trim(),
        responseChannel: newResponseChannel.trim() || undefined,
      });

      // Reset form
      setNewName('');
      setNewDescription('');
      setNewTriggerType('event');
      setNewCronExpr('');
      setNewEventPattern('channel:*');
      setNewEventKeyword('');
      setNewActionPrompt('');
      setNewResponseChannel('');
      setShowCreate(false);
      setError(null);

      loadRoutines();
      loadStats();
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleToggle(routine: Routine) {
    try {
      await api.routineUpdate(routine.id, { enabled: !routine.enabled });
      setRoutines(prev =>
        prev.map(r => r.id === routine.id ? { ...r, enabled: !r.enabled } : r)
      );
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleDelete(id: string) {
    try {
      await api.routineDelete(id);
      setRoutines(prev => prev.filter(r => r.id !== id));
      setDeleteConfirm(null);
      loadStats();
    } catch (e) {
      setError(String(e));
    }
  }

  function triggerIcon(type: TriggerType) {
    switch (type) {
      case 'cron': return <Clock size={14} className="text-blue-400" />;
      case 'event': return <Zap size={14} className="text-amber-400" />;
      case 'both': return <Activity size={14} className="text-green-400" />;
    }
  }

  function triggerLabel(routine: Routine): string {
    const parts: string[] = [];
    if (routine.cron_expr) parts.push(`cron: ${routine.cron_expr}`);
    if (routine.event_pattern) {
      let label = routine.event_pattern;
      if (routine.event_keyword) label += ` [keyword: ${routine.event_keyword}]`;
      parts.push(label);
    }
    return parts.join(' + ') || 'No trigger configured';
  }

  return (
    <div className="mt-8 p-4 bg-zinc-800/50 rounded-xl border border-zinc-700">
      <div className="flex items-center justify-between mb-4">
        <div className="flex items-center gap-2">
          <RotateCcw size={18} className="text-amber-400" />
          <span className="text-white font-semibold">Routines</span>
          <span className="text-xs text-zinc-500">Standing instructions</span>
        </div>
        <div className="flex items-center gap-2">
          {/* Daemon status indicator */}
          <span className={`w-2 h-2 rounded-full ${daemonRunning ? 'bg-green-400' : 'bg-zinc-600'}`} title={daemonRunning ? 'Cron daemon running' : 'Cron daemon stopped'} />
          <span className="text-xs text-zinc-500">{daemonRunning ? 'Active' : 'Stopped'}</span>

          <button
            onClick={() => setShowCreate(!showCreate)}
            className="p-1.5 text-zinc-400 hover:text-amber-400 transition-colors"
            title="Add routine"
          >
            {showCreate ? <X size={16} /> : <Plus size={16} />}
          </button>
        </div>
      </div>

      {/* Stats bar */}
      {stats && stats.total_routines > 0 && (
        <div className="flex gap-4 mb-3 text-xs text-zinc-500">
          <span>{stats.enabled_routines}/{stats.total_routines} enabled</span>
          <span>{stats.total_runs} runs</span>
          {stats.total_failures > 0 && (
            <span className="text-red-400">{stats.total_failures} failures</span>
          )}
          {stats.queue_pending > 0 && (
            <span className="text-amber-400">{stats.queue_pending} queued</span>
          )}
        </div>
      )}

      {/* Error display */}
      {error && (
        <div className="mb-3 p-2 bg-red-500/10 border border-red-500/30 rounded-lg text-sm text-red-400 flex items-center justify-between">
          <span>{error}</span>
          <button onClick={() => setError(null)} className="text-red-400 hover:text-red-300">
            <X size={14} />
          </button>
        </div>
      )}

      {/* Create form */}
      {showCreate && (
        <div className="mb-4 p-3 bg-zinc-900/50 rounded-lg border border-zinc-600 space-y-3">
          <div>
            <label className="text-xs text-zinc-400 block mb-1">Name</label>
            <input
              value={newName}
              onChange={e => setNewName(e.target.value)}
              placeholder="e.g., Morning Summary"
              className="w-full bg-zinc-800 text-white px-3 py-2 rounded-lg border border-zinc-600 focus:border-amber-500 outline-none text-sm"
            />
          </div>

          <div>
            <label className="text-xs text-zinc-400 block mb-1">Description (optional)</label>
            <input
              value={newDescription}
              onChange={e => setNewDescription(e.target.value)}
              placeholder="What this routine does"
              className="w-full bg-zinc-800 text-white px-3 py-2 rounded-lg border border-zinc-600 focus:border-amber-500 outline-none text-sm"
            />
          </div>

          <div>
            <label className="text-xs text-zinc-400 block mb-1">Trigger Type</label>
            <select
              value={newTriggerType}
              onChange={e => setNewTriggerType(e.target.value as TriggerType)}
              className="w-full bg-zinc-800 text-white px-3 py-2 rounded-lg border border-zinc-600 focus:border-amber-500 outline-none text-sm"
            >
              <option value="event">Event (channel message)</option>
              <option value="cron">Cron (scheduled)</option>
              <option value="both">Both</option>
            </select>
          </div>

          {(newTriggerType === 'cron' || newTriggerType === 'both') && (
            <div>
              <label className="text-xs text-zinc-400 block mb-1">
                Cron Expression <span className="text-zinc-600">(min hour day month weekday)</span>
              </label>
              <input
                value={newCronExpr}
                onChange={e => setNewCronExpr(e.target.value)}
                placeholder="0 9 * * * (every day at 9am)"
                className="w-full bg-zinc-800 text-white px-3 py-2 rounded-lg border border-zinc-600 focus:border-amber-500 outline-none text-sm font-mono"
              />
              <div className="mt-1 text-xs text-zinc-600">
                Examples: <code>*/5 * * * *</code> (every 5 min), <code>0 9,17 * * 1-5</code> (9am+5pm weekdays)
              </div>
            </div>
          )}

          {(newTriggerType === 'event' || newTriggerType === 'both') && (
            <>
              <div>
                <label className="text-xs text-zinc-400 block mb-1">Event Pattern</label>
                <select
                  value={newEventPattern}
                  onChange={e => setNewEventPattern(e.target.value)}
                  className="w-full bg-zinc-800 text-white px-3 py-2 rounded-lg border border-zinc-600 focus:border-amber-500 outline-none text-sm"
                >
                  <option value="channel:*">Any channel</option>
                  <option value="channel:telegram">Telegram only</option>
                  <option value="channel:discord">Discord only</option>
                </select>
              </div>
              <div>
                <label className="text-xs text-zinc-400 block mb-1">
                  Keyword Filter <span className="text-zinc-600">(optional, case-insensitive)</span>
                </label>
                <input
                  value={newEventKeyword}
                  onChange={e => setNewEventKeyword(e.target.value)}
                  placeholder="e.g., urgent, deploy, help"
                  className="w-full bg-zinc-800 text-white px-3 py-2 rounded-lg border border-zinc-600 focus:border-amber-500 outline-none text-sm"
                />
              </div>
            </>
          )}

          <div>
            <label className="text-xs text-zinc-400 block mb-1">Action Prompt</label>
            <textarea
              value={newActionPrompt}
              onChange={e => setNewActionPrompt(e.target.value)}
              placeholder="The instruction to execute when triggered. e.g., 'Summarize the last 24 hours of activity from memory.'"
              rows={3}
              className="w-full bg-zinc-800 text-white px-3 py-2 rounded-lg border border-zinc-600 focus:border-amber-500 outline-none text-sm resize-y"
            />
          </div>

          <div>
            <label className="text-xs text-zinc-400 block mb-1">
              Response Channel <span className="text-zinc-600">(optional)</span>
            </label>
            <input
              value={newResponseChannel}
              onChange={e => setNewResponseChannel(e.target.value)}
              placeholder="telegram:12345 or discord:67890 (leave empty for local)"
              className="w-full bg-zinc-800 text-white px-3 py-2 rounded-lg border border-zinc-600 focus:border-amber-500 outline-none text-sm font-mono"
            />
          </div>

          <div className="flex justify-end gap-2">
            <button
              onClick={() => setShowCreate(false)}
              className="px-3 py-1.5 text-sm text-zinc-400 hover:text-white transition-colors"
            >
              Cancel
            </button>
            <button
              onClick={handleCreate}
              className="px-3 py-1.5 text-sm bg-amber-500 text-black rounded-lg hover:bg-amber-400 transition-colors flex items-center gap-1"
            >
              <Save size={14} />
              Create Routine
            </button>
          </div>
        </div>
      )}

      {/* Routines list */}
      {loading ? (
        <div className="text-center text-zinc-500 py-4 text-sm">Loading...</div>
      ) : routines.length === 0 ? (
        <div className="text-center text-zinc-600 py-6 text-sm">
          No routines yet. Create one to automate standing instructions.
        </div>
      ) : (
        <div className="space-y-2">
          {routines.map(routine => (
            <div
              key={routine.id}
              className={`p-3 rounded-lg border transition-colors ${
                routine.enabled
                  ? 'bg-zinc-900/50 border-zinc-600'
                  : 'bg-zinc-900/20 border-zinc-700/50 opacity-60'
              }`}
            >
              <div className="flex items-start justify-between">
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2">
                    {triggerIcon(routine.trigger_type)}
                    <span className="text-sm text-white font-medium truncate">{routine.name}</span>
                    {!routine.enabled && (
                      <span className="text-xs text-zinc-600 bg-zinc-800 px-1.5 py-0.5 rounded">disabled</span>
                    )}
                  </div>
                  {routine.description && (
                    <p className="text-xs text-zinc-500 mt-0.5 truncate">{routine.description}</p>
                  )}
                  <div className="text-xs text-zinc-600 mt-1 font-mono truncate">
                    {triggerLabel(routine)}
                  </div>
                  <div className="text-xs text-zinc-600 mt-0.5 truncate">
                    Action: {routine.action_prompt.substring(0, 80)}{routine.action_prompt.length > 80 ? '...' : ''}
                  </div>
                  {routine.run_count > 0 && (
                    <div className="flex gap-3 mt-1 text-xs text-zinc-600">
                      <span>{routine.run_count} runs</span>
                      <span className="text-green-500">{routine.success_count} ok</span>
                      {routine.fail_count > 0 && <span className="text-red-400">{routine.fail_count} fail</span>}
                      {routine.last_run && <span>last: {new Date(routine.last_run).toLocaleString()}</span>}
                    </div>
                  )}
                </div>

                <div className="flex items-center gap-1 ml-2 shrink-0">
                  {/* Toggle enabled/disabled */}
                  <button
                    onClick={() => handleToggle(routine)}
                    className={`p-1.5 transition-colors ${
                      routine.enabled
                        ? 'text-green-400 hover:text-green-300'
                        : 'text-zinc-600 hover:text-zinc-400'
                    }`}
                    title={routine.enabled ? 'Disable' : 'Enable'}
                  >
                    {routine.enabled ? <Pause size={14} /> : <Play size={14} />}
                  </button>

                  {/* Delete */}
                  {deleteConfirm === routine.id ? (
                    <button
                      onClick={() => handleDelete(routine.id)}
                      className="p-1.5 text-red-400 hover:text-red-300 transition-colors"
                      title="Confirm delete"
                    >
                      <Trash2 size={14} />
                    </button>
                  ) : (
                    <button
                      onClick={() => setDeleteConfirm(routine.id)}
                      className="p-1.5 text-zinc-600 hover:text-red-400 transition-colors"
                      title="Delete"
                    >
                      <Trash2 size={14} />
                    </button>
                  )}
                </div>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

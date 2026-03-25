import { useState, useEffect } from 'react';
import { Activity, Square, Radio, Users, UserCheck, Plus, X } from 'lucide-react';
import * as api from '../../lib/api';
import type { TelegramDaemonStatus, AccessLists } from '../../types';

export default function TelegramDaemonControls() {
  const [status, setStatus] = useState<TelegramDaemonStatus | null>(null);
  const [loading, setLoading] = useState(false);
  const [msg, setMsg] = useState<{ text: string; ok: boolean } | null>(null);
  const [accessLists, setAccessLists] = useState<AccessLists>({ host_ids: [], user_ids: [] });
  const [showAccess, setShowAccess] = useState(false);
  const [newHostId, setNewHostId] = useState('');
  const [newUserId, setNewUserId] = useState('');

  useEffect(() => {
    refreshStatus();
    refreshAccessLists();
    const interval = setInterval(refreshStatus, 10_000);
    return () => clearInterval(interval);
  }, []);

  async function refreshStatus() {
    try {
      const s = await api.getTelegramDaemonStatus();
      setStatus(s);
    } catch {
      // daemon commands may not exist on older builds
    }
  }

  async function refreshAccessLists() {
    try {
      const lists = await api.getTelegramAccessLists();
      setAccessLists(lists);
    } catch { /* older builds */ }
  }

  async function handleStart() {
    setLoading(true);
    try {
      const result = await api.startTelegramDaemon();
      setMsg({ text: result, ok: true });
      await refreshStatus();
    } catch (e) {
      setMsg({ text: `${e}`, ok: false });
    } finally {
      setLoading(false);
    }
  }

  async function handleStop() {
    setLoading(true);
    try {
      const result = await api.stopTelegramDaemon();
      setMsg({ text: result, ok: true });
      await refreshStatus();
    } catch (e) {
      setMsg({ text: `${e}`, ok: false });
    } finally {
      setLoading(false);
    }
  }

  async function addHostId() {
    const id = newHostId.trim();
    if (!id || accessLists.host_ids.includes(id)) return;
    const updated = [...accessLists.host_ids, id];
    try {
      await api.setTelegramHostIds(updated);
      setAccessLists(prev => ({ ...prev, host_ids: updated }));
      setNewHostId('');
      setMsg({ text: `Host ID ${id} added`, ok: true });
    } catch (e) {
      setMsg({ text: `Failed to add host ID: ${e}`, ok: false });
    }
  }

  async function removeHostId(id: string) {
    const updated = accessLists.host_ids.filter(h => h !== id);
    try {
      await api.setTelegramHostIds(updated);
      setAccessLists(prev => ({ ...prev, host_ids: updated }));
    } catch (e) {
      setMsg({ text: `Failed to remove host ID: ${e}`, ok: false });
    }
  }

  async function addUserId() {
    const id = newUserId.trim();
    if (!id || accessLists.user_ids.includes(id)) return;
    const updated = [...accessLists.user_ids, id];
    try {
      await api.setTelegramUserIds(updated);
      setAccessLists(prev => ({ ...prev, user_ids: updated }));
      setNewUserId('');
      setMsg({ text: `User ID ${id} added`, ok: true });
    } catch (e) {
      setMsg({ text: `Failed to add user ID: ${e}`, ok: false });
    }
  }

  async function removeUserId(id: string) {
    const updated = accessLists.user_ids.filter(u => u !== id);
    try {
      await api.setTelegramUserIds(updated);
      setAccessLists(prev => ({ ...prev, user_ids: updated }));
    } catch (e) {
      setMsg({ text: `Failed to remove user ID: ${e}`, ok: false });
    }
  }

  useEffect(() => {
    if (!msg) return;
    const t = setTimeout(() => setMsg(null), 4000);
    return () => clearTimeout(t);
  }, [msg]);

  const isRunning = status?.running ?? false;
  const totalAccess = accessLists.host_ids.length + accessLists.user_ids.length;

  return (
    <div className="bg-zinc-900 rounded-lg p-4 mb-4">
      <div className="flex items-center justify-between mb-2">
        <div className="flex items-center gap-2">
          <Activity className={`w-4 h-4 ${isRunning ? 'text-green-400 animate-pulse' : 'text-zinc-500'}`} />
          <span className="text-white font-medium text-sm">Telegram Daemon</span>
          {isRunning && status?.connected_bot && (
            <span className="text-xs text-green-400">{status.connected_bot}</span>
          )}
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={() => setShowAccess(!showAccess)}
            className="flex items-center gap-1 px-2 py-1 text-xs rounded-lg text-zinc-400 hover:text-amber-400 transition-colors"
            title="Manage access"
          >
            <Users className="w-3 h-3" />
            {totalAccess > 0 ? `${totalAccess} ID(s)` : 'No access'}
          </button>
          <button
            onClick={isRunning ? handleStop : handleStart}
            disabled={loading}
            className={`flex items-center gap-1.5 px-3 py-1.5 text-xs rounded-lg font-medium transition-colors ${
              isRunning
                ? 'bg-red-500/20 text-red-400 hover:bg-red-500/30'
                : 'bg-green-500/20 text-green-400 hover:bg-green-500/30'
            } ${loading ? 'opacity-50' : ''}`}
          >
            {isRunning ? (
              <><Square className="w-3 h-3" /> Stop</>
            ) : (
              <><Radio className="w-3 h-3" /> Start</>
            )}
          </button>
        </div>
      </div>

      <p className="text-zinc-500 text-xs mb-2">
        {isRunning
          ? 'HIVE is listening for Telegram messages. Messages auto-trigger the agentic loop.'
          : 'Start the daemon to receive and auto-respond to Telegram messages.'}
      </p>

      {totalAccess === 0 && (
        <div className="p-2 bg-amber-500/10 border border-amber-500/30 rounded-lg mb-2">
          <p className="text-amber-400 text-xs">
            No Host or User IDs configured. All messages will be rejected until you add at least one ID.
          </p>
        </div>
      )}

      {isRunning && status && (
        <div className="flex items-center gap-4 text-xs text-zinc-400">
          <span>Messages: {status.messages_processed}</span>
          {status.errors > 0 && <span className="text-red-400">Errors: {status.errors}</span>}
          {status.last_poll && <span>Last poll: {status.last_poll}</span>}
        </div>
      )}

      {status?.last_error && (
        <p className="text-xs text-red-400 mt-1">{status.last_error}</p>
      )}

      {msg && (
        <p className={`text-xs mt-2 ${msg.ok ? 'text-green-400' : 'text-red-400'}`}>{msg.text}</p>
      )}

      {/* Host / User ID Management */}
      {showAccess && (
        <div className="mt-3 space-y-3 border-t border-zinc-700 pt-3">
          {/* Host IDs */}
          <div>
            <div className="flex items-center gap-1.5 mb-1.5">
              <UserCheck className="w-3.5 h-3.5 text-green-400" />
              <span className="text-green-400 text-xs font-medium">Host IDs</span>
              <span className="text-zinc-600 text-xs">— full access (your Telegram chat ID)</span>
            </div>
            <div className="space-y-1">
              {accessLists.host_ids.map(id => (
                <div key={id} className="flex items-center justify-between bg-zinc-800 rounded px-2 py-1">
                  <span className="text-white text-xs font-mono">{id}</span>
                  <button onClick={() => removeHostId(id)} className="text-zinc-500 hover:text-red-400">
                    <X className="w-3 h-3" />
                  </button>
                </div>
              ))}
            </div>
            <div className="flex gap-1.5 mt-1.5">
              <input
                type="text"
                value={newHostId}
                onChange={(e) => setNewHostId(e.target.value)}
                placeholder="Chat ID (e.g. 123456789)"
                className="flex-1 bg-zinc-800 text-white px-2 py-1 rounded border border-zinc-600 focus:border-green-500 outline-none text-xs font-mono"
                onKeyDown={(e) => { if (e.key === 'Enter') addHostId(); }}
              />
              <button
                onClick={addHostId}
                disabled={!newHostId.trim()}
                className="px-2 py-1 text-xs bg-green-500/20 text-green-400 hover:bg-green-500/30 disabled:opacity-30 rounded font-medium"
              >
                <Plus className="w-3 h-3" />
              </button>
            </div>
          </div>

          {/* User IDs */}
          <div>
            <div className="flex items-center gap-1.5 mb-1.5">
              <Users className="w-3.5 h-3.5 text-blue-400" />
              <span className="text-blue-400 text-xs font-medium">User IDs</span>
              <span className="text-zinc-600 text-xs">— restricted (no shell, no file writes)</span>
            </div>
            <div className="space-y-1">
              {accessLists.user_ids.map(id => (
                <div key={id} className="flex items-center justify-between bg-zinc-800 rounded px-2 py-1">
                  <span className="text-white text-xs font-mono">{id}</span>
                  <button onClick={() => removeUserId(id)} className="text-zinc-500 hover:text-red-400">
                    <X className="w-3 h-3" />
                  </button>
                </div>
              ))}
            </div>
            <div className="flex gap-1.5 mt-1.5">
              <input
                type="text"
                value={newUserId}
                onChange={(e) => setNewUserId(e.target.value)}
                placeholder="Chat ID (e.g. 987654321)"
                className="flex-1 bg-zinc-800 text-white px-2 py-1 rounded border border-zinc-600 focus:border-blue-500 outline-none text-xs font-mono"
                onKeyDown={(e) => { if (e.key === 'Enter') addUserId(); }}
              />
              <button
                onClick={addUserId}
                disabled={!newUserId.trim()}
                className="px-2 py-1 text-xs bg-blue-500/20 text-blue-400 hover:bg-blue-500/30 disabled:opacity-30 rounded font-medium"
              >
                <Plus className="w-3 h-3" />
              </button>
            </div>
          </div>

          <p className="text-zinc-600 text-xs">
            Message @userinfobot on Telegram to find your chat ID. Host = full tool access. User = safe tools only.
          </p>
        </div>
      )}
    </div>
  );
}

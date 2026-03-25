import { useState, useEffect, useRef } from 'react';
import { X, Cpu, Clock, Zap, AlertTriangle, CheckCircle, XCircle, Loader2 } from 'lucide-react';
import * as api from '../lib/api';

interface Props {
  isOpen: boolean;
  onClose: () => void;
}

export default function WorkerPanel({ isOpen, onClose }: Props) {
  const [workers, setWorkers] = useState<api.WorkerStatus[]>([]);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Poll worker statuses when panel is open
  useEffect(() => {
    if (isOpen) {
      loadWorkers();
      // Poll every 3 seconds while open
      pollRef.current = setInterval(loadWorkers, 3000);
    }
    return () => {
      if (pollRef.current) {
        clearInterval(pollRef.current);
        pollRef.current = null;
      }
    };
  }, [isOpen]);

  async function loadWorkers() {
    try {
      const statuses = await api.getWorkerStatuses();
      setWorkers(statuses);
    } catch (e) {
      console.error('[HIVE] Failed to poll worker statuses:', e);
    }
  }

  function formatElapsed(seconds: number): string {
    if (seconds < 60) return `${seconds}s`;
    if (seconds < 3600) return `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
    return `${Math.floor(seconds / 3600)}h ${Math.floor((seconds % 3600) / 60)}m`;
  }

  function getStatusIcon(status: string) {
    if (status === 'running') return <Loader2 className="w-3.5 h-3.5 text-blue-400 animate-spin" />;
    if (status === 'completed') return <CheckCircle className="w-3.5 h-3.5 text-green-400" />;
    if (status === 'terminated') return <XCircle className="w-3.5 h-3.5 text-zinc-400" />;
    return <AlertTriangle className="w-3.5 h-3.5 text-red-400" />;
  }

  function getStatusColor(status: string): string {
    if (status === 'running') return 'border-blue-500/40 bg-blue-500/5';
    if (status === 'completed') return 'border-green-500/40 bg-green-500/5';
    if (status === 'terminated') return 'border-zinc-500/40 bg-gray-500/5';
    return 'border-red-500/40 bg-red-500/5';
  }

  function getStallWarning(w: api.WorkerStatus): string | null {
    if (w.status !== 'running') return null;
    if (w.idle_seconds > 300) return 'Stalled (5+ min idle)';
    if (w.idle_seconds > 120) return 'Slow (2+ min idle)';
    return null;
  }

  if (!isOpen) return null;

  const runningCount = workers.filter(w => w.status === 'running').length;

  return (
    <div className="fixed inset-y-0 right-0 w-96 bg-zinc-900 border-l border-zinc-700 shadow-2xl z-50 flex flex-col overflow-hidden">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-zinc-700 bg-zinc-800/50">
        <div className="flex items-center gap-2">
          <Cpu className="w-4 h-4 text-blue-400" />
          <span className="font-medium text-sm text-zinc-200">Workers</span>
          {runningCount > 0 && (
            <span className="px-1.5 py-0.5 text-[10px] font-medium bg-blue-500/20 text-blue-300 rounded-full">
              {runningCount} active
            </span>
          )}
        </div>
        <button
          onClick={onClose}
          className="p-1 hover:bg-zinc-700 rounded transition-colors"
        >
          <X className="w-4 h-4 text-zinc-400" />
        </button>
      </div>

      {/* Worker List */}
      <div className="flex-1 overflow-y-auto p-3 space-y-2">
        {workers.length === 0 ? (
          <div className="text-center text-zinc-500 text-sm py-8">
            <Cpu className="w-8 h-8 mx-auto mb-2 opacity-30" />
            <p>No workers spawned yet.</p>
            <p className="text-xs mt-1 text-zinc-600">
              Workers are created via the worker_spawn tool during chat.
            </p>
          </div>
        ) : (
          workers.map(w => {
            const stall = getStallWarning(w);
            return (
              <div
                key={w.id}
                className={`rounded-lg border p-3 ${getStatusColor(w.status)}`}
              >
                {/* Worker header */}
                <div className="flex items-center justify-between mb-1.5">
                  <div className="flex items-center gap-1.5">
                    {getStatusIcon(w.status)}
                    <span className="text-xs font-mono text-zinc-300">{w.id}</span>
                  </div>
                  <span className="text-[10px] text-zinc-500">{w.provider}/{w.model}</span>
                </div>

                {/* Task description */}
                <p className="text-xs text-zinc-400 mb-2 line-clamp-2">{w.task}</p>

                {/* Progress bar */}
                {w.status === 'running' && (
                  <div className="w-full bg-zinc-800 rounded-full h-1.5 mb-2">
                    <div
                      className="bg-blue-500/60 h-1.5 rounded-full transition-all duration-500"
                      style={{ width: `${Math.min((w.turns_used / w.max_turns) * 100, 100)}%` }}
                    />
                  </div>
                )}

                {/* Stats row */}
                <div className="flex items-center gap-3 text-[10px] text-zinc-500">
                  <span className="flex items-center gap-0.5">
                    <Zap className="w-3 h-3" />
                    {w.turns_used}/{w.max_turns} turns
                  </span>
                  <span className="flex items-center gap-0.5">
                    <Clock className="w-3 h-3" />
                    {formatElapsed(w.elapsed_seconds)}
                  </span>
                  <span className="text-zinc-600">pad: {w.scratchpad_id}</span>
                </div>

                {/* Stall warning */}
                {stall && (
                  <div className="mt-1.5 flex items-center gap-1 text-[10px] text-amber-400">
                    <AlertTriangle className="w-3 h-3" />
                    {stall}
                  </div>
                )}

                {/* Summary (when completed/failed) */}
                {w.summary && w.status !== 'running' && (
                  <p className="mt-1.5 text-[10px] text-zinc-400 border-t border-zinc-700/50 pt-1.5">
                    {w.summary}
                  </p>
                )}
              </div>
            );
          })
        )}
      </div>

      {/* Footer */}
      {workers.length > 0 && (
        <div className="px-4 py-2 border-t border-zinc-700 bg-zinc-800/30 text-[10px] text-zinc-600">
          {workers.length} worker{workers.length !== 1 ? 's' : ''} total
          {runningCount > 0 && ` | polling every 3s`}
        </div>
      )}
    </div>
  );
}

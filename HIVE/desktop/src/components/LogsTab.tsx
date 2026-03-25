import { RefObject, useState, useEffect, useRef } from 'react';
import { Terminal, Server, RefreshCw } from 'lucide-react';
import { LogEntry } from '../types';
import * as api from '../lib/api';

interface Props {
  logs: LogEntry[];
  onClearLogs: () => void;
  logsEndRef: RefObject<HTMLDivElement>;
  serverRunning: boolean;
}

export default function LogsTab({ logs, onClearLogs, logsEndRef, serverRunning }: Props) {
  const [activeSection, setActiveSection] = useState<'app' | 'server'>('app');
  const [serverLog, setServerLog] = useState<string>('');
  const [serverLogLoading, setServerLogLoading] = useState(false);
  const pollIntervalRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const serverLogEndRef = useRef<HTMLDivElement>(null);

  // Fetch server log
  async function fetchServerLog() {
    try {
      const log = await api.readServerLog(200);
      setServerLog(log);
    } catch {
      setServerLog('No server log available.');
    }
  }

  // Poll server log when server is running and server tab is active
  useEffect(() => {
    if (activeSection === 'server') {
      setServerLogLoading(true);
      fetchServerLog().finally(() => setServerLogLoading(false));

      if (serverRunning) {
        pollIntervalRef.current = setInterval(fetchServerLog, 3000);
      }
    }
    return () => {
      if (pollIntervalRef.current) {
        clearInterval(pollIntervalRef.current);
        pollIntervalRef.current = null;
      }
    };
  }, [activeSection, serverRunning]);

  // Auto-scroll server log
  useEffect(() => {
    if (activeSection === 'server') {
      serverLogEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    }
  }, [serverLog]);

  return (
    <div className="h-full flex flex-col">
      <div className="p-4 border-b border-zinc-700 flex items-center justify-between">
        <div className="flex items-center gap-4">
          {/* Section tabs */}
          <div className="flex gap-1 bg-zinc-800 p-0.5 rounded-lg">
            <button
              onClick={() => setActiveSection('app')}
              className={`flex items-center gap-1.5 px-3 py-1.5 text-sm rounded-md transition-colors ${
                activeSection === 'app'
                  ? 'bg-amber-500 text-black font-medium'
                  : 'text-zinc-400 hover:text-white'
              }`}
            >
              <Terminal className="w-4 h-4" />
              App Logs
            </button>
            <button
              onClick={() => setActiveSection('server')}
              className={`flex items-center gap-1.5 px-3 py-1.5 text-sm rounded-md transition-colors ${
                activeSection === 'server'
                  ? 'bg-amber-500 text-black font-medium'
                  : 'text-zinc-400 hover:text-white'
              }`}
            >
              <Server className="w-4 h-4" />
              Server Output
              {serverRunning && (
                <span className="w-1.5 h-1.5 rounded-full bg-green-400 animate-pulse" />
              )}
            </button>
          </div>
          <span className="text-zinc-500 text-sm">
            {activeSection === 'app'
              ? `(${logs.length} entries)`
              : serverRunning ? '(live)' : '(stopped)'}
          </span>
        </div>
        <div className="flex gap-2">
          {activeSection === 'server' && (
            <button
              onClick={() => { setServerLogLoading(true); fetchServerLog().finally(() => setServerLogLoading(false)); }}
              className="px-3 py-1 text-xs bg-zinc-700 hover:bg-zinc-600 text-zinc-300 rounded flex items-center gap-1"
            >
              <RefreshCw className={`w-3 h-3 ${serverLogLoading ? 'animate-spin' : ''}`} />
              Refresh
            </button>
          )}
          {activeSection === 'app' && (
            <button
              onClick={onClearLogs}
              className="px-3 py-1 text-xs bg-zinc-700 hover:bg-zinc-600 text-zinc-300 rounded"
            >
              Clear
            </button>
          )}
        </div>
      </div>

      {/* App Logs */}
      {activeSection === 'app' && (
        <div className="flex-1 overflow-auto p-4 font-mono text-sm bg-zinc-900">
          {logs.length === 0 ? (
            <p className="text-zinc-500">No logs yet. Interact with the app to see debug output.</p>
          ) : (
            logs.map((log, i) => (
              <div key={`${log.time}-${i}`} className="flex gap-2 py-0.5 hover:bg-zinc-800/50">
                <span className="text-zinc-600 shrink-0">{log.time}</span>
                <span className={`shrink-0 w-12 ${
                  log.level === 'error' ? 'text-red-400' :
                  log.level === 'warn' ? 'text-yellow-400' :
                  'text-blue-400'
                }`}>
                  [{log.level.toUpperCase()}]
                </span>
                <span className={
                  log.level === 'error' ? 'text-red-300' :
                  log.level === 'warn' ? 'text-yellow-300' :
                  'text-zinc-300'
                }>{log.msg}</span>
              </div>
            ))
          )}
          <div ref={logsEndRef} />
        </div>
      )}

      {/* Server Output */}
      {activeSection === 'server' && (
        <div className="flex-1 overflow-auto p-4 font-mono text-sm bg-zinc-900">
          {serverLog ? (
            <>
              {serverLog.split('\n').map((line, i) => (
                <div key={`srv-${i}-${line.length}`} className="py-0.5 hover:bg-zinc-800/50">
                  <span className={
                    line.toLowerCase().includes('error') ? 'text-red-300' :
                    line.toLowerCase().includes('warn') ? 'text-yellow-300' :
                    line.includes('model loaded') || line.includes('listening') ? 'text-green-300' :
                    'text-zinc-400'
                  }>{line}</span>
                </div>
              ))}
              <div ref={serverLogEndRef} />
            </>
          ) : (
            <p className="text-zinc-500">
              {serverRunning
                ? 'Loading server output...'
                : 'Start a model to see server output here.'}
            </p>
          )}
        </div>
      )}
    </div>
  );
}

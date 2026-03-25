import { useState, useEffect, useRef } from 'react';
import { LogEntry } from '../types';
import { logToApp } from '../lib/api';

export function useLogs() {
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const logsEndRef = useRef<HTMLDivElement>(null);

  // Helper to add log entries
  const addLog = (level: string, ...args: unknown[]) => {
    const time = new Date().toLocaleTimeString();
    const msg = args.map(a => typeof a === 'object' ? JSON.stringify(a) : String(a)).join(' ');
    setLogs(prev => [...prev.slice(-200), { time, level, msg }]);
  };

  // Persist important frontend logs to hive-app.log so the AI can read them
  // via check_logs tool. This bridges the gap between UI-only logs and the
  // persistent log file — making the steering AI aware of its own operations.
  const persistLog = (level: string, msg: string) => {
    const prefix = level === 'error' ? 'FE_ERROR' : level === 'warn' ? 'FE_WARN' : 'FE';
    logToApp(`${prefix} | ${msg}`);
  };

  // Capture console output for logs tab
  useEffect(() => {
    const origLog = console.log;
    const origError = console.error;
    const origWarn = console.warn;

    console.log = (...args) => {
      origLog.apply(console, args);
      if (args[0]?.toString().includes('[HIVE]')) {
        addLog('info', ...args);
        // Persist to disk so the AI can read via check_logs
        const msg = args.map(a => typeof a === 'object' ? JSON.stringify(a) : String(a)).join(' ');
        persistLog('info', msg);
      }
    };
    console.error = (...args) => {
      origError.apply(console, args);
      addLog('error', ...args);
      // Persist errors to disk — critical for AI self-debugging (P4)
      const msg = args.map(a => typeof a === 'object' ? JSON.stringify(a) : String(a)).join(' ');
      persistLog('error', msg);
    };
    console.warn = (...args) => {
      origWarn.apply(console, args);
      addLog('warn', ...args);
      // Persist warnings to disk — the AI should see these too
      const msg = args.map(a => typeof a === 'object' ? JSON.stringify(a) : String(a)).join(' ');
      persistLog('warn', msg);
    };

    addLog('info', '[HIVE] Logs initialized - debug output will appear here');

    return () => {
      console.log = origLog;
      console.error = origError;
      console.warn = origWarn;
    };
  }, []);

  // Auto-scroll logs
  useEffect(() => {
    logsEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [logs]);

  const clearLogs = () => setLogs([]);

  return { logs, logsEndRef, clearLogs };
}

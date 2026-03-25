/**
 * TerminalPane — Phase 10 NEXUS
 *
 * Self-contained xterm.js terminal component wired to a Rust PTY backend.
 * Each instance owns its own PTY session lifecycle (spawn → IO → cleanup).
 *
 * P1: Self-contained — no Context, no App.tsx state threading.
 * P2: Any CLI agent is { command, args }. Same pipe for Shell, Claude Code, Codex.
 * P3: xterm.js (VS Code's terminal) + portable-pty (Wezterm). We write glue.
 */

import { useEffect, useMemo, useRef, useState } from 'react';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import { WebLinksAddon } from '@xterm/addon-web-links';
import '@xterm/xterm/css/xterm.css';
import * as api from '../lib/api';
import type { ChatPaneConfig } from '../types';
import { BUILTIN_AGENTS } from '../types';
import PaneHeader from './PaneHeader';

// ============================================
// HIVE Terminal Theme (zinc/amber palette)
// ============================================

const HIVE_THEME = {
  background: '#18181b',       // zinc-900
  foreground: '#fafafa',       // zinc-50
  cursor: '#f59e0b',           // amber-500
  cursorAccent: '#18181b',
  selectionBackground: '#f59e0b33',
  selectionForeground: '#fafafa',
  black: '#27272a',
  red: '#ef4444',
  green: '#22c55e',
  yellow: '#f59e0b',
  blue: '#3b82f6',
  magenta: '#a855f7',
  cyan: '#06b6d4',
  white: '#e4e4e7',
  brightBlack: '#52525b',
  brightRed: '#f87171',
  brightGreen: '#4ade80',
  brightYellow: '#fbbf24',
  brightBlue: '#60a5fa',
  brightMagenta: '#c084fc',
  brightCyan: '#22d3ee',
  brightWhite: '#fafafa',
};

// ============================================
// Props
// ============================================

interface TerminalPaneProps {
  pane: ChatPaneConfig;
  isActive: boolean;
  isOnly: boolean;
  canAdd: boolean;
  onActivate: () => void;
  onRemove: () => void;
  onAdd: () => void;
  onAddTerminal: (agentId: string) => void;
  onModelSelect: (paneId: string) => void;
  onPtySessionId: (sessionId: string) => void;
  onExit: (exitCode: number | null) => void;
}

// ============================================
// Component
// ============================================

export default function TerminalPane({
  pane, isActive, isOnly, canAdd,
  onActivate, onRemove, onAdd, onAddTerminal, onModelSelect,
  onPtySessionId, onExit,
}: TerminalPaneProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const sessionIdRef = useRef<string | null>(null);
  const [status, setStatus] = useState<'connecting' | 'running' | 'exited'>('connecting');
  const [exitCode, setExitCode] = useState<number | null>(null);

  // Resolve agent config from pane (builtin + custom)
  const allAgents = useMemo(() => [...BUILTIN_AGENTS, ...api.getCustomAgents()], [pane.agentId]);
  const agent = allAgents.find(a => a.id === pane.agentId) || BUILTIN_AGENTS[0];

  // ---- Spawn PTY + wire xterm ----
  useEffect(() => {
    if (!containerRef.current) return;

    // Create xterm.js terminal
    const term = new Terminal({
      theme: HIVE_THEME,
      fontFamily: '"Cascadia Code", "Fira Code", "JetBrains Mono", monospace',
      fontSize: 13,
      lineHeight: 1.2,
      cursorBlink: true,
      cursorStyle: 'bar',
      scrollback: 10000,
      allowProposedApi: true,
    });

    const fitAddon = new FitAddon();
    const webLinksAddon = new WebLinksAddon();
    term.loadAddon(fitAddon);
    term.loadAddon(webLinksAddon);

    term.open(containerRef.current);
    fitAddon.fit();

    terminalRef.current = term;
    fitAddonRef.current = fitAddon;

    // Track cleanup functions
    let outputUnlisten: (() => void) | null = null;
    let exitUnlisten: (() => void) | null = null;
    let disposed = false;

    // Spawn PTY session
    const cols = term.cols;
    const rows = term.rows;

    api.ptySpawn(agent.command, agent.args, cols, rows, agent.bridgeToChat)
      .then(async (sid) => {
        if (disposed) {
          api.ptyKill(sid).catch(() => {});
          return;
        }

        sessionIdRef.current = sid;
        onPtySessionId(sid);
        setStatus('running');

        // Wire keystrokes: xterm → PTY
        term.onData((data) => {
          api.ptyWrite(sid, data).catch((e) => console.warn('[PTY] write failed:', e));
        });

        // Wire output: PTY → xterm
        outputUnlisten = await api.onPtyOutput((sessionId, data) => {
          if (sessionId === sid && !disposed) {
            term.write(data);
          }
        }) as unknown as () => void;

        // Wire exit: PTY process terminated
        exitUnlisten = await api.onPtyExit((sessionId, code) => {
          if (sessionId === sid && !disposed) {
            setStatus('exited');
            setExitCode(code);
            term.write(`\r\n\x1b[38;2;245;158;11m[Process exited${code !== null ? ` with code ${code}` : ''}]\x1b[0m\r\n`);
            onExit(code);
          }
        }) as unknown as () => void;
      })
      .catch((err) => {
        if (!disposed) {
          term.write(`\x1b[31mFailed to spawn ${agent.command}: ${err}\x1b[0m\r\n`);
          setStatus('exited');
        }
      });

    // Cleanup on unmount
    return () => {
      disposed = true;
      if (outputUnlisten) outputUnlisten();
      if (exitUnlisten) exitUnlisten();
      if (sessionIdRef.current) {
        api.ptyKill(sessionIdRef.current).catch(() => {});
      }
      term.dispose();
    };
  }, []); // Mount once — pane identity is stable

  // ---- Resize handling ----
  useEffect(() => {
    if (!containerRef.current || !fitAddonRef.current) return;

    const observer = new ResizeObserver(() => {
      if (fitAddonRef.current && terminalRef.current) {
        try {
          fitAddonRef.current.fit();
          const sid = sessionIdRef.current;
          if (sid) {
            api.ptyResize(sid, terminalRef.current.cols, terminalRef.current.rows).catch(() => {});
          }
        } catch {
          // Ignore fit errors during rapid resizes
        }
      }
    });

    observer.observe(containerRef.current);
    return () => observer.disconnect();
  }, []);

  // ---- Focus when pane becomes active ----
  useEffect(() => {
    if (isActive && terminalRef.current) {
      terminalRef.current.focus();
    }
  }, [isActive]);

  return (
    <div className="flex flex-col h-full bg-[#18181b]" onClick={onActivate}>
      <PaneHeader
        pane={pane}
        isActive={isActive}
        isOnly={isOnly}
        canAdd={canAdd}
        onActivate={onActivate}
        onRemove={onRemove}
        onAdd={onAdd}
        onAddTerminal={onAddTerminal}
        onModelSelect={onModelSelect}
      />

      {/* Status overlay */}
      {status === 'connecting' && (
        <div className="px-3 py-1 text-xs text-zinc-500 border-b border-zinc-800">
          Starting {agent.name}...
        </div>
      )}
      {status === 'exited' && (
        <div className="px-3 py-1 text-xs text-amber-500/70 border-b border-zinc-800">
          {agent.name} exited{exitCode !== null ? ` (code ${exitCode})` : ''}
        </div>
      )}

      {/* Terminal container */}
      <div
        ref={containerRef}
        className="flex-1 min-h-0"
        style={{ padding: '4px 0 0 4px' }}
      />
    </div>
  );
}

import { useMemo, useState } from 'react';
import { X, Plus, GripVertical, Cpu, Cloud, ChevronDown, Terminal, Square, Link } from 'lucide-react';
import type { ChatPaneConfig } from '../types';
import { BUILTIN_AGENTS } from '../types';
import * as api from '../lib/api';

/** Agents that support MCP bridge (HIVE injects its config into their MCP settings). */
const MCP_BRIDGE_AGENTS = ['claude-code'];

interface Props {
  pane: ChatPaneConfig;
  isActive: boolean;
  isOnly: boolean;
  canAdd: boolean;
  onActivate: () => void;
  onRemove: () => void;
  onAdd: () => void;
  onAddTerminal: (agentId: string) => void;
  onModelSelect: (paneId: string) => void;
}

function getProviderLabel(provider?: string): string {
  if (!provider) return '';
  const labels: Record<string, string> = {
    local: 'Local',
    ollama: 'Ollama',
    openai: 'OpenAI',
    anthropic: 'Anthropic',
    openrouter: 'OpenRouter',
    dashscope: 'DashScope',
  };
  return labels[provider] || provider;
}

function getProviderColor(provider?: string): string {
  if (!provider) return 'text-zinc-500';
  const colors: Record<string, string> = {
    local: 'text-amber-400',
    ollama: 'text-blue-400',
    openai: 'text-green-400',
    anthropic: 'text-orange-400',
    openrouter: 'text-purple-400',
    dashscope: 'text-cyan-400',
  };
  return colors[provider] || 'text-zinc-400';
}

function getAgentColor(agentId?: string): string {
  const agent = BUILTIN_AGENTS.find(a => a.id === agentId);
  return agent?.color || 'text-cyan-400';
}

export default function PaneHeader({
  pane, isActive, isOnly, canAdd,
  onActivate, onRemove, onAdd, onAddTerminal, onModelSelect,
}: Props) {
  const isLocal = pane.modelType === 'local';
  const isTerminal = pane.paneType === 'terminal';
  const [showAddMenu, setShowAddMenu] = useState(false);
  const [mcpStatus, setMcpStatus] = useState<'idle' | 'working' | 'done' | 'error'>('idle');
  const supportsMcp = isTerminal && MCP_BRIDGE_AGENTS.includes(pane.agentId || '');
  const terminalAgents = useMemo(() => [...BUILTIN_AGENTS, ...api.getCustomAgents()], []);

  return (
    <div
      className={`flex items-center gap-2 px-3 py-1.5 border-b cursor-pointer select-none relative ${
        isActive
          ? 'bg-zinc-800 border-amber-500/40'
          : 'bg-zinc-900 border-zinc-700 hover:bg-zinc-800/50'
      }`}
      onClick={onActivate}
    >
      {/* Drag handle hint */}
      <GripVertical className="w-3 h-3 text-zinc-600 flex-shrink-0" />

      {/* Pane type icon */}
      {isTerminal ? (
        <Terminal className={`w-3.5 h-3.5 ${getAgentColor(pane.agentId)} flex-shrink-0`} />
      ) : isLocal ? (
        <Cpu className="w-3.5 h-3.5 text-amber-400 flex-shrink-0" />
      ) : (
        <Cloud className={`w-3.5 h-3.5 ${getProviderColor(pane.provider)} flex-shrink-0`} />
      )}

      {/* Name + provider label */}
      {isTerminal ? (
        <span className="text-xs font-medium text-zinc-300 truncate">
          {pane.modelDisplayName || 'Terminal'}
        </span>
      ) : (
        <button
          className="flex items-center gap-1 text-xs font-medium text-zinc-300 hover:text-white transition-colors truncate min-w-0"
          onClick={(e) => {
            e.stopPropagation();
            onModelSelect(pane.id);
          }}
          title="Click to change model"
        >
          <span className="truncate">{pane.modelDisplayName || 'No model'}</span>
          {pane.provider && pane.modelType === 'cloud' && (
            <span className={`text-[10px] ${getProviderColor(pane.provider)}`}>
              {getProviderLabel(pane.provider)}
            </span>
          )}
          <ChevronDown className="w-3 h-3 text-zinc-500 flex-shrink-0" />
        </button>
      )}

      {/* Spacer */}
      <div className="flex-1" />

      {/* MCP bridge button — Claude Code terminal panes only */}
      {supportsMcp && (
        <button
          onClick={(e) => {
            e.stopPropagation();
            if (mcpStatus === 'working' || mcpStatus === 'done') return;
            setMcpStatus('working');
            api.setupMcpBridge(pane.agentId || 'claude-code')
              .then(() => setMcpStatus('done'))
              .catch(() => setMcpStatus('error'));
          }}
          className={`p-0.5 hover:bg-zinc-700 rounded transition-colors ${
            mcpStatus === 'done' ? 'text-green-400' : mcpStatus === 'error' ? 'text-red-400' : ''
          }`}
          title={
            mcpStatus === 'done' ? 'MCP bridge configured'
            : mcpStatus === 'error' ? 'MCP bridge failed'
            : 'Connect HIVE tools via MCP'
          }
        >
          <Link className={`w-3 h-3 ${
            mcpStatus === 'done' ? 'text-green-400'
            : mcpStatus === 'error' ? 'text-red-400'
            : 'text-zinc-500 hover:text-amber-400'
          }`} />
        </button>
      )}

      {/* Kill button — terminal panes only */}
      {isTerminal && pane.ptySessionId && (
        <button
          onClick={(e) => {
            e.stopPropagation();
            api.ptyKill(pane.ptySessionId!).catch(() => {});
          }}
          className="p-0.5 hover:bg-zinc-700 rounded transition-colors"
          title="Kill process"
        >
          <Square className="w-3 h-3 text-zinc-500 hover:text-red-400 fill-current" />
        </button>
      )}

      {/* Add pane button with dropdown */}
      {canAdd && (
        <div className="relative">
          <button
            onClick={(e) => {
              e.stopPropagation();
              setShowAddMenu(prev => !prev);
            }}
            className="p-0.5 hover:bg-zinc-700 rounded transition-colors"
            title="Add pane"
          >
            <Plus className="w-3 h-3 text-zinc-500 hover:text-zinc-300" />
          </button>

          {/* Dropdown menu */}
          {showAddMenu && (
            <>
              {/* Backdrop to close */}
              <div
                className="fixed inset-0 z-40"
                onClick={(e) => {
                  e.stopPropagation();
                  setShowAddMenu(false);
                }}
              />
              <div className="absolute right-0 top-full mt-1 z-50 bg-zinc-800 border border-zinc-700 rounded-md shadow-lg py-1 min-w-[160px]">
                {/* Chat pane */}
                <button
                  className="w-full px-3 py-1.5 text-xs text-left text-zinc-300 hover:bg-zinc-700 flex items-center gap-2"
                  onClick={(e) => {
                    e.stopPropagation();
                    setShowAddMenu(false);
                    onAdd();
                  }}
                >
                  <Cloud className="w-3 h-3 text-zinc-400" />
                  Add Chat
                </button>

                <div className="border-t border-zinc-700 my-1" />

                {/* Terminal pane options — builtin + custom */}
                {terminalAgents.map((agent) => (
                  <button
                    key={agent.id}
                    className="w-full px-3 py-1.5 text-xs text-left text-zinc-300 hover:bg-zinc-700 flex items-center gap-2"
                    onClick={(e) => {
                      e.stopPropagation();
                      setShowAddMenu(false);
                      onAddTerminal(agent.id);
                    }}
                  >
                    <Terminal className={`w-3 h-3 ${agent.color}`} />
                    {agent.name}
                  </button>
                ))}
              </div>
            </>
          )}
        </div>
      )}

      {/* Remove pane button (hidden if only one pane) */}
      {!isOnly && (
        <button
          onClick={(e) => {
            e.stopPropagation();
            onRemove();
          }}
          className="p-0.5 hover:bg-zinc-700 rounded transition-colors"
          title="Remove pane"
        >
          <X className="w-3 h-3 text-zinc-500 hover:text-red-400" />
        </button>
      )}
    </div>
  );
}

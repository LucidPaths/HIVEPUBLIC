import { useState, useEffect } from 'react';
import { Terminal, Plus, Trash2 } from 'lucide-react';
import * as api from '../../lib/api';
import type { AgentConfig } from '../../types';
import { BUILTIN_AGENTS } from '../../types';

export default function AgentRegistrySection() {
  const [customAgents, setCustomAgents] = useState<AgentConfig[]>(() => api.getCustomAgents());
  const [availability, setAvailability] = useState<Record<string, string>>({});
  const [checking, setChecking] = useState(false);
  const [showAddForm, setShowAddForm] = useState(false);
  const [newName, setNewName] = useState('');
  const [newCommand, setNewCommand] = useState('');
  const [newArgs, setNewArgs] = useState('');

  // All agents = builtin + custom
  const allAgents = [...BUILTIN_AGENTS, ...customAgents];

  // Check availability on mount
  useEffect(() => {
    checkAll();
  }, []);

  async function checkAll() {
    setChecking(true);
    const results: Record<string, string> = {};
    for (const agent of [...BUILTIN_AGENTS, ...customAgents]) {
      try {
        results[agent.id] = await api.checkAgentAvailable(agent.command);
      } catch {
        results[agent.id] = '';
      }
    }
    setAvailability(results);
    setChecking(false);
  }

  function addCustomAgent() {
    if (!newName.trim() || !newCommand.trim()) return;
    const agent: AgentConfig = {
      id: `custom-${Date.now().toString(36)}`,
      name: newName.trim(),
      command: newCommand.trim(),
      args: newArgs.trim() ? newArgs.trim().split(/\s+/) : [],
      color: 'text-pink-400',
    };
    const updated = [...customAgents, agent];
    setCustomAgents(updated);
    api.saveCustomAgents(updated);
    setNewName('');
    setNewCommand('');
    setNewArgs('');
    setShowAddForm(false);
    // Check availability for new agent
    api.checkAgentAvailable(agent.command).then(path => {
      setAvailability(prev => ({ ...prev, [agent.id]: path }));
    }).catch(() => {});
  }

  function removeCustomAgent(id: string) {
    const updated = customAgents.filter(a => a.id !== id);
    setCustomAgents(updated);
    api.saveCustomAgents(updated);
  }

  const isBuiltin = (id: string) => BUILTIN_AGENTS.some(a => a.id === id);

  return (
    <div className="bg-zinc-800 rounded-xl p-6 mt-4">
      <div className="flex items-center justify-between mb-2">
        <h3 className="text-white font-medium flex items-center gap-2">
          <Terminal className="w-5 h-5" />
          CLI Agents (NEXUS)
        </h3>
        <button
          onClick={checkAll}
          disabled={checking}
          className="text-xs text-zinc-400 hover:text-white transition-colors disabled:opacity-50"
        >
          {checking ? 'Checking...' : 'Refresh'}
        </button>
      </div>
      <p className="text-zinc-400 text-sm mb-4">
        CLI coding agents available in terminal panes. HIVE doesn't touch their auth — each agent handles its own login.
      </p>

      <div className="space-y-2">
        {allAgents.map(agent => {
          const path = availability[agent.id];
          const isAvailable = path && path.length > 0;
          const builtin = isBuiltin(agent.id);

          return (
            <div key={agent.id} className="bg-zinc-900 rounded-lg p-3 flex items-center gap-3">
              <Terminal className={`w-4 h-4 ${agent.color} flex-shrink-0`} />
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2">
                  <span className="text-sm text-white font-medium">{agent.name}</span>
                  <code className="text-xs text-zinc-500 bg-zinc-800 px-1.5 py-0.5 rounded">{agent.command}</code>
                  {agent.args.length > 0 && (
                    <code className="text-xs text-zinc-600">{agent.args.join(' ')}</code>
                  )}
                </div>
                <div className="flex items-center gap-1.5 mt-0.5">
                  {isAvailable ? (
                    <>
                      <span className="w-1.5 h-1.5 rounded-full bg-green-400" />
                      <span className="text-xs text-green-400">Ready</span>
                      <span className="text-xs text-zinc-600 truncate">{path}</span>
                    </>
                  ) : (
                    <>
                      <span className="w-1.5 h-1.5 rounded-full bg-zinc-600" />
                      <span className="text-xs text-zinc-500">Not found</span>
                    </>
                  )}
                </div>
              </div>
              {!builtin && (
                <button
                  onClick={() => removeCustomAgent(agent.id)}
                  className="p-1 hover:bg-zinc-700 rounded transition-colors"
                  title="Remove agent"
                >
                  <Trash2 className="w-3.5 h-3.5 text-zinc-500 hover:text-red-400" />
                </button>
              )}
            </div>
          );
        })}
      </div>

      {/* Add custom agent */}
      {showAddForm ? (
        <div className="mt-3 bg-zinc-900 rounded-lg p-3 space-y-2">
          <div className="flex gap-2">
            <input
              type="text"
              value={newName}
              onChange={e => setNewName(e.target.value)}
              placeholder="Name (e.g., Continue)"
              className="flex-1 bg-zinc-800 text-white text-sm px-2.5 py-1.5 rounded border border-zinc-700 focus:border-amber-500/50 outline-none"
            />
            <input
              type="text"
              value={newCommand}
              onChange={e => setNewCommand(e.target.value)}
              placeholder="Command (e.g., continue)"
              className="flex-1 bg-zinc-800 text-white text-sm px-2.5 py-1.5 rounded border border-zinc-700 focus:border-amber-500/50 outline-none"
            />
          </div>
          <div className="flex gap-2">
            <input
              type="text"
              value={newArgs}
              onChange={e => setNewArgs(e.target.value)}
              placeholder="Arguments (optional, space-separated)"
              className="flex-1 bg-zinc-800 text-white text-sm px-2.5 py-1.5 rounded border border-zinc-700 focus:border-amber-500/50 outline-none"
            />
            <button
              onClick={addCustomAgent}
              disabled={!newName.trim() || !newCommand.trim()}
              className="px-3 py-1.5 text-sm bg-amber-500 hover:bg-amber-600 disabled:opacity-30 text-black rounded font-medium"
            >
              Add
            </button>
            <button
              onClick={() => setShowAddForm(false)}
              className="px-3 py-1.5 text-sm bg-zinc-700 hover:bg-zinc-600 text-zinc-300 rounded"
            >
              Cancel
            </button>
          </div>
        </div>
      ) : (
        <button
          onClick={() => setShowAddForm(true)}
          className="mt-3 flex items-center gap-1.5 text-sm text-zinc-400 hover:text-white transition-colors"
        >
          <Plus className="w-3.5 h-3.5" />
          Add Custom Agent
        </button>
      )}
    </div>
  );
}

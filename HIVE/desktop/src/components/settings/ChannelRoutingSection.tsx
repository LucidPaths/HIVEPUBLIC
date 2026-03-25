import { useState } from 'react';
import { Radio } from 'lucide-react';
import * as api from '../../lib/api';
import { BUILTIN_AGENTS } from '../../types';

export default function ChannelRoutingSection() {
  const [routing, setRouting] = useState<api.ChannelRoutingConfig>(() => api.getChannelRouting());

  // All available routing targets: chat pane + all agents
  const allAgents = [...BUILTIN_AGENTS, ...api.getCustomAgents()];
  const routingOptions = [
    { value: 'chat', label: 'Active Chat Pane' },
    ...allAgents.map(a => ({ value: a.command, label: `Terminal: ${a.name}` })),
  ];

  function updateRouting(channel: 'telegram' | 'discord', value: string) {
    const updated = { ...routing, [channel]: value };
    setRouting(updated);
    api.saveChannelRouting(updated);
  }

  return (
    <div className="bg-zinc-800 rounded-xl p-6 mt-4">
      <h3 className="text-white font-medium flex items-center gap-2 mb-2">
        <Radio className="w-5 h-5" />
        Channel Routing
      </h3>
      <p className="text-zinc-400 text-sm mb-4">
        Route incoming Telegram/Discord messages to a chat pane or directly to a running terminal agent.
        If the configured agent has no running session, messages fall back to the active chat pane.
      </p>

      <div className="space-y-3">
        {/* Telegram routing */}
        <div className="flex items-center gap-3">
          <span className="text-sm text-zinc-300 w-24">Telegram</span>
          <select
            value={routing.telegram}
            onChange={(e) => updateRouting('telegram', e.target.value)}
            className="flex-1 bg-zinc-900 text-white text-sm px-3 py-1.5 rounded border border-zinc-700 focus:border-amber-500/50 outline-none"
          >
            {routingOptions.map(opt => (
              <option key={`tg-${opt.value}`} value={opt.value}>{opt.label}</option>
            ))}
          </select>
        </div>

        {/* Discord routing */}
        <div className="flex items-center gap-3">
          <span className="text-sm text-zinc-300 w-24">Discord</span>
          <select
            value={routing.discord}
            onChange={(e) => updateRouting('discord', e.target.value)}
            className="flex-1 bg-zinc-900 text-white text-sm px-3 py-1.5 rounded border border-zinc-700 focus:border-amber-500/50 outline-none"
          >
            {routingOptions.map(opt => (
              <option key={`dc-${opt.value}`} value={opt.value}>{opt.label}</option>
            ))}
          </select>
        </div>
      </div>
    </div>
  );
}

import { Cpu } from 'lucide-react';
import * as api from '../../lib/api';

interface Props {
  selectedModel: api.LocalModel | null;
  serverRunning: boolean;
  providerStatuses: Record<string, api.ProviderStatus>;
}

const CATEGORY_META: Record<api.TaskCategory, { label: string; icon: string; desc: string }> = {
  general:      { label: 'General',      icon: '\u{1F4AC}', desc: 'Conversation & Q&A' },
  coding:       { label: 'Coding',       icon: '\u{1F4BB}', desc: 'Code generation & debugging' },
  reasoning:    { label: 'Reasoning',    icon: '\u{1F9E0}', desc: 'Logic, math & analysis' },
  writing:      { label: 'Writing',      icon: '\u270D\uFE0F', desc: 'Text composition & editing' },
  tool_calling: { label: 'Tool Calling', icon: '\u{1F527}', desc: 'Function calls & automation' },
  web:          { label: 'Web',          icon: '\u{1F310}', desc: 'Search & research' },
  creative:     { label: 'Creative',     icon: '\u{1F3A8}', desc: 'Roleplay & creative writing' },
};

export default function SmartRouterSection({ selectedModel, serverRunning, providerStatuses }: Props) {
  const models = api.getRoutableModels(providerStatuses, selectedModel, serverRunning);
  const routingTable = api.getRoutingTable(models);

  return (
    <div className="bg-zinc-800 rounded-xl p-6 mt-6">
      <h3 className="text-white font-medium mb-2 flex items-center gap-2">
        <Cpu className="w-5 h-5" />
        Smart Router
      </h3>
      <p className="text-zinc-400 text-sm mb-4">
        HIVE automatically picks the best available model for each task type based on benchmarks and availability.
      </p>

      {models.length === 0 ? (
        <div className="p-4 bg-zinc-900 rounded-lg text-center">
          <p className="text-zinc-500 text-sm">
            No models available. Load a local model or configure a cloud provider in the Models tab.
          </p>
        </div>
      ) : (
        <div className="space-y-2">
          {(Object.entries(routingTable) as [api.TaskCategory, api.RouteChoice | null][]).map(([cat, choice]) => {
            const meta = CATEGORY_META[cat];
            if (!meta) return null;
            return (
              <div key={cat} className="flex items-center gap-3 p-3 bg-zinc-900 rounded-lg">
                <span className="text-lg w-7 text-center">{meta.icon}</span>
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="text-white text-sm font-medium">{meta.label}</span>
                    <span className="text-zinc-600 text-xs">{meta.desc}</span>
                  </div>
                  {choice ? (
                    <div className="flex items-center gap-2 mt-0.5">
                      <span className="text-xs">{api.getProviderInfo(choice.model.provider).icon}</span>
                      <span className={`text-xs ${api.getProviderInfo(choice.model.provider).color}`}>
                        {choice.model.name}
                      </span>
                      <span className="text-xs text-zinc-600">score: {choice.score}</span>
                    </div>
                  ) : (
                    <span className="text-xs text-zinc-500 mt-0.5 block">No model available</span>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      )}

      {models.length > 0 && (
        <p className="text-zinc-500 text-xs mt-3">
          {models.length} model{models.length !== 1 ? 's' : ''} available across{' '}
          {new Set(models.map(m => m.provider)).size} provider{new Set(models.map(m => m.provider)).size !== 1 ? 's' : ''}.
          Router picks the highest-scoring model per task category.
        </p>
      )}
    </div>
  );
}

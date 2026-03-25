/**
 * SlotConfigSection — Phase 4 specialist slot configuration
 *
 * Self-contained component (P1: Modularity) that lets users assign ANY model
 * (local or cloud) to specialist roles. Provider-agnostic (P2).
 * Calls api.* directly — no state threading through App.tsx.
 *
 * Slots: coder (8081), terminal (8082), webcrawl (8083), toolcall (8084)
 * Consciousness (8080) is the main model — configured elsewhere.
 */

import { useState, useEffect, useMemo } from 'react';
import * as api from '../lib/api';
import type { SlotRole, SlotConfig } from '../types';

interface Props {
  localModels: api.LocalModel[];
  wslModels: api.LocalModel[];
  enabled: boolean;
}

/** Unified model option for the dropdown — local or cloud */
interface ModelOption {
  id: string;
  label: string;
  provider: string;
  model: string;
  vramGb: number;  // 0 for cloud
  contextLength: number;
}

const SPECIALIST_ROLES: { role: SlotRole; label: string; description: string }[] = [
  { role: 'coder',    label: 'Coder',      description: 'Code generation and review' },
  { role: 'terminal', label: 'Terminal',    description: 'Shell commands and system tasks' },
  { role: 'webcrawl', label: 'Web Crawl',   description: 'Web search and content extraction' },
  { role: 'toolcall', label: 'Tool Call',    description: 'General tool execution' },
];

const CLOUD_PROVIDERS: api.ProviderType[] = ['openai', 'anthropic', 'ollama', 'openrouter', 'dashscope'];

export default function SlotConfigSection({ localModels, wslModels, enabled }: Props) {
  const [configs, setConfigs] = useState<SlotConfig[]>([]);
  const [loading, setLoading] = useState(true);
  const [status, setStatus] = useState<{ msg: string; ok: boolean } | null>(null);
  const [cloudModels, setCloudModels] = useState<ModelOption[]>([]);

  // Build local model options
  const localOptions: ModelOption[] = useMemo(() => [...localModels, ...wslModels].map(m => ({
    id: `local:${m.filename}`,
    label: `${m.filename} (${m.size_gb.toFixed(1)} GB)`,
    provider: 'local',
    model: m.filename,
    vramGb: m.size_gb,
    contextLength: m.context_length || 4096,
  })), [localModels, wslModels]);

  useEffect(() => {
    loadConfigs();
    loadCloudModels();
  }, []);

  // Clear status after 4s
  useEffect(() => {
    if (!status) return;
    const t = setTimeout(() => setStatus(null), 4000);
    return () => clearTimeout(t);
  }, [status]);

  async function loadConfigs() {
    try {
      const c = await api.getSlotConfigs();
      setConfigs(c);
    } catch (e) {
      console.warn('[HIVE] SlotConfig: Failed to load configs:', e);
    } finally {
      setLoading(false);
    }
  }

  async function loadCloudModels() {
    const options: ModelOption[] = [];
    for (const provider of CLOUD_PROVIDERS) {
      try {
        const status = await api.checkProviderStatus(provider);
        if (status.configured && status.models.length > 0) {
          const info = api.getProviderInfo(provider);
          for (const m of status.models) {
            options.push({
              id: `${provider}:${m.id}`,
              label: `${info.icon} ${m.name}`,
              provider,
              model: m.id,
              vramGb: 0,  // Cloud = zero VRAM
              contextLength: m.context_length || 4096,
            });
          }
        }
      } catch {
        // Provider not configured — that's fine
      }
    }
    setCloudModels(options);
  }

  function getConfigForRole(role: SlotRole): SlotConfig | undefined {
    return configs.find(c => c.role === role);
  }

  async function handleModelChange(role: SlotRole, optionId: string) {
    if (!optionId) return;

    const allOptions = [...localOptions, ...cloudModels];
    const option = allOptions.find(o => o.id === optionId);
    if (!option) return;

    try {
      const updated = await api.configureSlot(
        role,
        option.provider,
        option.model,
        option.vramGb,
        option.contextLength,
      );
      setConfigs(prev => {
        const idx = prev.findIndex(c => c.role === role);
        if (idx >= 0) {
          const next = [...prev];
          next[idx] = updated;
          return next;
        }
        return [...prev, updated];
      });
      setStatus({ msg: `${role} assigned to ${option.model}`, ok: true });
    } catch (e) {
      setStatus({ msg: `Failed to configure ${role}: ${e}`, ok: false });
    }
  }

  async function handleToggle(role: SlotRole) {
    const existing = getConfigForRole(role);
    if (!existing) return;

    try {
      // Re-configure the slot — backend always returns the authoritative state (B12 fix)
      const updated = await api.configureSlot(
        role,
        existing.primary?.provider || 'local',
        existing.primary?.model || '',
        existing.primary?.vram_gb || 0,
        existing.primary?.context_length || 4096,
      );
      // Use the backend-returned config as source of truth instead of optimistic flip
      setConfigs(prev => prev.map(c =>
        c.role === role ? (updated as SlotConfig) : c,
      ));
    } catch (e) {
      setStatus({ msg: `Failed to toggle ${role}: ${e}`, ok: false });
    }
  }

  /** Build the option ID that matches the current config */
  function getSelectedOptionId(config?: SlotConfig): string {
    if (!config?.primary) return '';
    return `${config.primary.provider}:${config.primary.model}`;
  }

  if (!enabled) return null;

  const hasAnyModels = localOptions.length > 0 || cloudModels.length > 0;

  return (
    <div className="bg-zinc-800 rounded-xl p-6 mt-6">
      <h3 className="text-white font-medium mb-2">Specialist Slots</h3>
      <p className="text-zinc-400 text-sm mb-4">
        Assign models to specialist roles — local or cloud. The consciousness model
        routes tasks to these specialists automatically.
      </p>

      {/* Status message */}
      {status && (
        <div className={`mb-3 p-2.5 rounded-lg text-sm ${
          status.ok
            ? 'bg-green-500/10 border border-green-500/30 text-green-400'
            : 'bg-red-500/10 border border-red-500/30 text-red-400'
        }`}>
          {status.msg}
        </div>
      )}

      {loading ? (
        <p className="text-zinc-500 text-sm py-4 text-center">Loading slot configs...</p>
      ) : !hasAnyModels ? (
        <div className="p-3 bg-zinc-700/30 rounded-lg">
          <p className="text-zinc-500 text-sm">
            No models available. Download local models or configure a cloud provider API key
            to assign specialists.
          </p>
        </div>
      ) : (
        <div className="space-y-3">
          {SPECIALIST_ROLES.map(({ role, label, description }) => {
            const config = getConfigForRole(role);
            const selectedId = getSelectedOptionId(config);
            const isEnabled = config?.enabled ?? false;

            return (
              <div key={role} className="p-3 bg-zinc-700/50 rounded-lg">
                <div className="flex items-center justify-between mb-2">
                  <div>
                    <p className="text-white text-sm font-medium">{label}</p>
                    <p className="text-zinc-500 text-xs">{description}</p>
                  </div>
                  {selectedId && (
                    <button
                      onClick={() => handleToggle(role)}
                      className={`w-10 h-5 rounded-full transition-colors ${
                        isEnabled ? 'bg-amber-500' : 'bg-zinc-600'
                      }`}
                    >
                      <div className={`w-4 h-4 rounded-full bg-white shadow transition-transform ${
                        isEnabled ? 'translate-x-5' : 'translate-x-0.5'
                      }`} />
                    </button>
                  )}
                </div>
                <select
                  value={selectedId}
                  onChange={(e) => handleModelChange(role, e.target.value)}
                  className="w-full bg-zinc-900 text-white px-3 py-2 rounded-lg border border-zinc-600 focus:border-amber-500 outline-none text-sm"
                >
                  <option value="">— No model assigned —</option>
                  {localOptions.length > 0 && (
                    <optgroup label="Local Models">
                      {localOptions.map(o => (
                        <option key={o.id} value={o.id}>{o.label}</option>
                      ))}
                    </optgroup>
                  )}
                  {cloudModels.length > 0 && (
                    <optgroup label="Cloud Providers">
                      {cloudModels.map(o => (
                        <option key={o.id} value={o.id}>{o.label}</option>
                      ))}
                    </optgroup>
                  )}
                </select>
              </div>
            );
          })}
        </div>
      )}

      <p className="text-zinc-500 text-xs mt-3">
        Local specialists run on dedicated ports. Cloud specialists use zero VRAM.
        Mix freely — any provider fills any slot.
      </p>
    </div>
  );
}

import { useState, useEffect } from 'react';
import { Settings, Check, Wrench, Shield, ShieldCheck, ShieldOff } from 'lucide-react';
import * as api from '../../lib/api';

interface Props {
  appSettings: api.AppSettings;
  onAppSettingsChange: (settings: api.AppSettings) => void;
}

export default function ToolApprovalSection({ appSettings, onAppSettingsChange }: Props) {
  const [tools, setTools] = useState<api.ToolSchema[]>([]);
  const [showOverrides, setShowOverrides] = useState(false);

  useEffect(() => {
    api.getAvailableTools().then(setTools).catch(() => {});
  }, []);

  const mode = appSettings.toolApprovalMode ?? 'ask';

  const modes: { id: api.ToolApprovalMode; label: string; icon: React.ReactNode; desc: string }[] = [
    { id: 'ask',     label: 'Always Ask',     icon: <Shield className="w-4 h-4" />,      desc: 'Prompt for high-risk tools every time (safest)' },
    { id: 'session', label: 'Session Trust',   icon: <ShieldCheck className="w-4 h-4" />, desc: 'Prompt once per tool — remember for this session' },
    { id: 'auto',    label: 'Auto-Approve',    icon: <ShieldOff className="w-4 h-4" />,   desc: 'Never prompt — execute all tools automatically' },
  ];

  function setOverride(toolName: string, riskLevel: string) {
    const overrides = { ...(appSettings.toolOverrides ?? {}) };
    if (riskLevel === 'native') {
      delete overrides[toolName];
    } else {
      overrides[toolName] = riskLevel;
    }
    onAppSettingsChange({ ...appSettings, toolOverrides: overrides });
  }

  return (
    <div className="bg-zinc-800 rounded-xl p-6 mt-6">
      <h3 className="text-white font-medium mb-2 flex items-center gap-2">
        <Wrench className="w-5 h-5" />
        Tool Approval
      </h3>
      <p className="text-zinc-400 text-sm mb-4">
        Control how HIVE handles tool execution requests during conversations.
      </p>

      {/* Approval Mode Selector */}
      <div className="space-y-2 mb-4">
        {modes.map(m => (
          <button
            key={m.id}
            onClick={() => onAppSettingsChange({ ...appSettings, toolApprovalMode: m.id })}
            className={`w-full flex items-center gap-3 p-3 rounded-lg text-left transition-colors ${
              mode === m.id
                ? 'bg-amber-500/15 border border-amber-500/50 text-amber-400'
                : 'bg-zinc-700/50 border border-transparent text-zinc-300 hover:bg-zinc-700'
            }`}
          >
            {m.icon}
            <div className="flex-1">
              <span className="font-medium text-sm">{m.label}</span>
              <p className="text-xs text-zinc-500">{m.desc}</p>
            </div>
            {mode === m.id && <Check className="w-4 h-4" />}
          </button>
        ))}
      </div>

      {/* Auto-approve warning */}
      {mode === 'auto' && (
        <div className="p-3 bg-red-500/10 border border-red-500/30 rounded-lg mb-4">
          <p className="text-red-400 text-sm">
            Auto-approve lets HIVE execute all tools without asking — including file writes,
            shell commands, and web requests. Only use this if you trust the model and understand the risks.
          </p>
          <p className="text-red-400/70 text-xs mt-1">
            Remote channel safety: dangerous tools are still blocked for remote Users and
            desktop-only tools (shell, file write) are blocked for all remote senders regardless of this setting.
          </p>
        </div>
      )}

      {/* Per-tool overrides */}
      {tools.length > 0 && (
        <div>
          <button
            onClick={() => setShowOverrides(!showOverrides)}
            className="text-sm text-zinc-400 hover:text-amber-400 flex items-center gap-1.5"
          >
            <Settings className="w-3.5 h-3.5" />
            {showOverrides ? 'Hide' : 'Show'} per-tool overrides ({tools.length} tools)
          </button>

          {showOverrides && (
            <div className="mt-3 space-y-1.5 max-h-64 overflow-y-auto">
              {tools.map(tool => {
                const override = appSettings.toolOverrides?.[tool.name];
                const effectiveRisk = override || tool.risk_level;
                const riskColors: Record<string, string> = {
                  low: 'text-green-400',
                  medium: 'text-yellow-400',
                  high: 'text-orange-400',
                  critical: 'text-red-400',
                  disabled: 'text-zinc-500 line-through',
                };
                return (
                  <div key={tool.name} className="flex items-center justify-between p-2 bg-zinc-900 rounded-lg">
                    <div className="flex-1 min-w-0">
                      <span className="text-white text-sm font-mono truncate block">{tool.name}</span>
                      <span className={`text-xs ${riskColors[effectiveRisk] ?? 'text-zinc-400'}`}>
                        {effectiveRisk}{override ? ' (override)' : ''}
                      </span>
                    </div>
                    <select
                      value={override || 'native'}
                      onChange={(e) => setOverride(tool.name, e.target.value)}
                      className="bg-zinc-800 text-zinc-300 text-xs px-2 py-1 rounded border border-zinc-600 outline-none"
                    >
                      <option value="native">Native ({tool.risk_level})</option>
                      <option value="low">Low (auto)</option>
                      <option value="medium">Medium</option>
                      <option value="high">High (ask)</option>
                      <option value="critical">Critical (ask)</option>
                      <option value="disabled">Disabled</option>
                    </select>
                  </div>
                );
              })}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

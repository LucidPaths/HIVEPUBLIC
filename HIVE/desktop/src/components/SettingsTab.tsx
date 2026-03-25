import { useState, useEffect } from 'react';
import { Settings, Check, ArrowLeft, Cpu, RotateCcw, Save, ExternalLink } from 'lucide-react';
import * as api from '../lib/api';
import { Tab } from '../types';
import VramPreview from './VramPreview';
import RoutinesPanel from './RoutinesPanel';
import ToolApprovalSection from './settings/ToolApprovalSection';
import SkillsSection from './settings/SkillsSection';
import SmartRouterSection from './settings/SmartRouterSection';
import IntegrationsSection from './settings/IntegrationsSection';
import AgentRegistrySection from './settings/AgentRegistrySection';
import ChannelRoutingSection from './settings/ChannelRoutingSection';

interface Props {
  selectedModel: api.LocalModel | null;
  selectedCloudModel: { provider: api.ProviderType; model: api.ProviderModel } | null;
  activeModelType: 'local' | 'cloud';
  serverRunning: boolean;
  modelSettings: api.ModelSettings;
  onModelSettingsChange: (settings: api.ModelSettings) => void;
  onSaveSettings: (filename: string, settings: api.ModelSettings) => void;
  settingsSaved: boolean;
  systemInfo: api.SystemInfo | null;
  appSettings: api.AppSettings;
  onAppSettingsChange: (settings: api.AppSettings) => void;
  onSetTab: (tab: Tab) => void;
  providerStatuses: Record<string, api.ProviderStatus>;
}

export default function SettingsTab({
  selectedModel, selectedCloudModel, activeModelType, serverRunning,
  modelSettings, onModelSettingsChange, onSaveSettings, settingsSaved,
  systemInfo, appSettings, onAppSettingsChange, onSetTab,
  providerStatuses,
}: Props) {

  // === Identity editor state (self-contained — calls api.* directly) ===
  const [identityContent, setIdentityContent] = useState('');
  const [identityLoading, setIdentityLoading] = useState(false);
  const [identityStatus, setIdentityStatus] = useState<{ msg: string; ok: boolean } | null>(null);
  const [identityPath, setIdentityPath] = useState<string | null>(null);
  const [showIdentityEditor, setShowIdentityEditor] = useState(false);

  useEffect(() => {
    loadIdentity();
  }, []);

  async function loadIdentity() {
    try {
      setIdentityLoading(true);
      const [content, path] = await Promise.all([
        api.harnessGetIdentity(),
        api.harnessGetIdentityPath(),
      ]);
      setIdentityContent(content);
      setIdentityPath(path);
    } catch (e) {
      setIdentityStatus({ msg: `Failed to load identity: ${e}`, ok: false });
    } finally {
      setIdentityLoading(false);
    }
  }

  async function handleSaveIdentity() {
    try {
      const msg = await api.harnessSaveIdentity(identityContent);
      setIdentityStatus({ msg, ok: true });
      setShowIdentityEditor(false);
    } catch (e) {
      setIdentityStatus({ msg: `${e}`, ok: false });
    }
  }

  async function handleResetIdentity() {
    try {
      const msg = await api.harnessResetIdentity();
      setIdentityStatus({ msg, ok: true });
      await loadIdentity();
    } catch (e) {
      setIdentityStatus({ msg: `${e}`, ok: false });
    }
  }

  function updateSetting(patch: Partial<api.ModelSettings>) {
    const newSettings = { ...modelSettings, ...patch };
    onModelSettingsChange(newSettings);
    const filename = selectedModel?.filename || selectedCloudModel?.model.id;
    if (filename) {
      onSaveSettings(filename, newSettings);
    }
  }

  // Clear status after 4 seconds
  useEffect(() => {
    if (!identityStatus) return;
    const t = setTimeout(() => setIdentityStatus(null), 4000);
    return () => clearTimeout(t);
  }, [identityStatus]);

  return (
    <div className="h-full p-6 overflow-auto">
      <div className="max-w-2xl mx-auto">
        <h2 className="text-xl font-semibold text-white mb-6">Model Settings</h2>

        {/* Current Model Info */}
        <div className="bg-zinc-800 rounded-xl p-6 mb-4">
          <h3 className="text-white font-medium mb-4 flex items-center gap-2">
            <Settings className="w-5 h-5" />
            {selectedModel || selectedCloudModel ? 'Current Model' : 'No Model Selected'}
          </h3>

          {/* Cloud Model Selected */}
          {activeModelType === 'cloud' && selectedCloudModel ? (
            <div className="space-y-3">
              <div className="p-3 bg-zinc-700/50 rounded-lg flex items-center gap-3">
                <span className="text-xl">{api.getProviderInfo(selectedCloudModel.provider).icon}</span>
                <div>
                  <p className={`font-medium ${api.getProviderInfo(selectedCloudModel.provider).color}`}>
                    {selectedCloudModel.model.name}
                  </p>
                  <p className="text-zinc-400 text-sm">
                    {api.getProviderInfo(selectedCloudModel.provider).name}
                    {selectedCloudModel.model.context_length && (
                      <span> &bull; {(selectedCloudModel.model.context_length / 1000).toFixed(0)}K context</span>
                    )}
                  </p>
                </div>
              </div>
              <div className="p-3 bg-blue-500/10 border border-blue-500/30 rounded-lg">
                <p className="text-blue-400 text-sm">
                  Cloud models are managed by the provider. Local settings (VRAM, GPU layers, KV offload) don't apply.
                </p>
              </div>
            </div>
          ) : selectedModel ? (
            /* Local Model Selected */
            <div className="space-y-3">
              <div className="p-3 bg-zinc-700/50 rounded-lg">
                <p className="text-white font-medium">{selectedModel.filename}</p>
                <p className="text-zinc-400 text-sm">{selectedModel.size_gb.toFixed(2)} GB</p>
              </div>
              {serverRunning && (
                <div className="flex items-center gap-2 text-green-400 text-sm">
                  <div className="w-2 h-2 rounded-full bg-green-400 animate-pulse" />
                  Model is running
                </div>
              )}
            </div>
          ) : (
            <div className="flex items-center gap-3">
              <p className="text-zinc-500">No model selected.</p>
              <button
                onClick={() => onSetTab('models')}
                className="flex items-center gap-1.5 px-3 py-1.5 text-sm bg-amber-500 hover:bg-amber-600 text-black rounded-lg"
              >
                <ArrowLeft className="w-4 h-4" />
                Go to Models
              </button>
            </div>
          )}
        </div>

        {/* VRAM Calculation Settings - Local Models Only */}
        <div className={`bg-zinc-800 rounded-xl p-6 mb-4 ${activeModelType === 'cloud' ? 'opacity-50' : ''}`}>
          <h3 className="text-white font-medium mb-4 flex items-center gap-2">
            VRAM Calculation
            {activeModelType === 'cloud' && (
              <span className="text-xs bg-zinc-700 px-2 py-0.5 rounded text-zinc-400">Local only</span>
            )}
          </h3>

          {/* Restart Note */}
          {serverRunning && activeModelType === 'local' && (
            <div className="mb-4 p-3 bg-yellow-500/10 border border-yellow-500/30 rounded-lg">
              <p className="text-yellow-400 text-sm">
                Changes to these settings require stopping and restarting the model to take effect.
              </p>
            </div>
          )}

          <div className="space-y-4">
            {/* KV Cache Offload Toggle */}
            <div className="flex items-center justify-between p-3 bg-zinc-700/50 rounded-lg">
              <div>
                <p className="text-white">KV Cache Offload</p>
                <p className="text-zinc-400 text-sm">
                  Offload KV cache to system RAM (slower but saves VRAM)
                </p>
              </div>
              <button
                onClick={() => updateSetting({ kvOffload: !modelSettings.kvOffload })}
                className={`w-12 h-6 rounded-full transition-colors ${
                  modelSettings.kvOffload ? 'bg-amber-500' : 'bg-zinc-600'
                }`}
              >
                <div className={`w-5 h-5 rounded-full bg-white shadow transition-transform ${
                  modelSettings.kvOffload ? 'translate-x-6' : 'translate-x-0.5'
                }`} />
              </button>
            </div>

            {/* Context Length Selector */}
            {(() => {
              const modelMax = selectedModel?.context_length || null;
              const sliderMax = modelMax ? Math.min(modelMax, 131072) : 131072;
              const effectiveContext = Math.min(modelSettings.contextLength, sliderMax);
              const isAtMax = effectiveContext >= sliderMax;
              const formatCtx = (n: number) => n >= 1000 ? `${(n / 1024).toFixed(n >= 10240 ? 0 : 1)}K` : n.toString();
              return (
                <div className="p-3 bg-zinc-700/50 rounded-lg">
                  <div className="flex items-center justify-between mb-2">
                    <div>
                      <p className="text-white">Context Length</p>
                      <p className="text-zinc-400 text-sm">
                        Token context window (affects KV cache size)
                        {modelMax && <span className="text-zinc-500"> — model max: {formatCtx(modelMax)}</span>}
                      </p>
                    </div>
                    <span className="text-amber-400 font-mono">
                      {isAtMax ? (
                        <span>Max ({formatCtx(sliderMax)})</span>
                      ) : (
                        effectiveContext.toLocaleString()
                      )}
                    </span>
                  </div>
                  <input
                    type="range"
                    min={512}
                    max={sliderMax}
                    step={512}
                    value={effectiveContext}
                    onChange={(e) => updateSetting({ contextLength: parseInt(e.target.value) })}
                    className="w-full accent-amber-500"
                  />
                  <div className="flex justify-between text-xs text-zinc-500 mt-1">
                    <span>512</span>
                    {sliderMax >= 4096 && <span>4K</span>}
                    {sliderMax >= 8192 && <span>8K</span>}
                    {sliderMax >= 32768 && <span>32K</span>}
                    <span>{formatCtx(sliderMax)}</span>
                  </div>
                </div>
              );
            })()}

            {/* GPU Layers */}
            <div className="p-3 bg-zinc-700/50 rounded-lg">
              <div className="flex items-center justify-between mb-2">
                <div>
                  <p className="text-white">GPU Layers</p>
                  <p className="text-zinc-400 text-sm">
                    Layers to offload to GPU (99 = all)
                  </p>
                </div>
                <span className="text-amber-400 font-mono">{modelSettings.gpuLayers}</span>
              </div>
              <input
                type="range"
                min={0}
                max={99}
                step={1}
                value={modelSettings.gpuLayers}
                onChange={(e) => updateSetting({ gpuLayers: parseInt(e.target.value) })}
                className="w-full accent-amber-500"
              />
              <div className="flex justify-between text-xs text-zinc-500 mt-1">
                <span>CPU only</span>
                <span>All GPU</span>
              </div>
            </div>
          </div>
        </div>

        {/* System Prompt Section */}
        <div className="bg-zinc-800 rounded-xl p-6 mb-4">
          <div className="flex items-center justify-between mb-4">
            <h3 className="text-white font-medium">System Prompt</h3>
            {/* Saved indicator */}
            {settingsSaved && (
              <div className="flex items-center gap-1.5 text-green-400 text-sm animate-pulse">
                <Check className="w-4 h-4" />
                <span>Saved</span>
              </div>
            )}
          </div>

          {selectedModel || selectedCloudModel ? (
            <div className="space-y-3">
              <p className="text-zinc-400 text-sm">
                Custom instructions for how the model should behave.
                {appSettings.harnessEnabled
                  ? ' When the harness is active, this is appended to the identity as "Additional Instructions".'
                  : ' This is sent at the start of every conversation.'
                }
              </p>
              <textarea
                value={modelSettings.systemPrompt}
                onChange={(e) => updateSetting({ systemPrompt: e.target.value })}
                placeholder="You are a helpful assistant..."
                rows={5}
                className="w-full bg-zinc-900 text-white px-4 py-3 rounded-lg border border-zinc-600 focus:border-amber-500 outline-none resize-y text-sm font-mono"
              />
              <p className="text-zinc-500 text-xs">
                Tip: Be specific about tone, format, and behavior you want from the model.
              </p>
            </div>
          ) : (
            <p className="text-zinc-500">Select a model to configure its system prompt.</p>
          )}
        </div>

        {/* VRAM Estimate Preview */}
        {selectedModel && systemInfo?.gpus?.[0] && (
          <div className="bg-zinc-800 rounded-xl p-6">
            <h3 className="text-white font-medium mb-4">VRAM Estimate</h3>
            <VramPreview
              model={selectedModel}
              gpu={systemInfo.gpus[0]}
              settings={modelSettings}
            />
          </div>
        )}

        {/* KV Offload Info */}
        <div className="mt-4 p-4 bg-zinc-800/50 rounded-xl border border-zinc-700">
          <p className="text-zinc-400 text-sm">
            <strong className="text-white">About KV Cache Offload:</strong> When enabled, the KV cache is stored in system RAM instead of VRAM.
            This allows running models that would otherwise not fit, but with a performance penalty (5-20x slower token generation).
            llama.cpp enables this automatically via the <code className="text-amber-400">--no-kv-offload</code> flag.
          </p>
        </div>

        {/* App Settings */}
        <div className="bg-zinc-800 rounded-xl p-6 mt-6">
          <h3 className="text-white font-medium mb-4">App Settings</h3>
          <div className="flex items-center justify-between p-3 bg-zinc-700/50 rounded-lg">
            <div>
              <p className="text-white">Chat History</p>
              <p className="text-zinc-400 text-sm">
                Save conversations across sessions (stored locally)
              </p>
            </div>
            <button
              onClick={() => {
                const newSettings = { ...appSettings, chatPersistence: !appSettings.chatPersistence };
                onAppSettingsChange(newSettings);
                api.saveAppSettings(newSettings);
              }}
              className={`w-12 h-6 rounded-full transition-colors ${
                appSettings.chatPersistence ? 'bg-amber-500' : 'bg-zinc-600'
              }`}
            >
              <div className={`w-5 h-5 rounded-full bg-white shadow transition-transform ${
                appSettings.chatPersistence ? 'translate-x-6' : 'translate-x-0.5'
              }`} />
            </button>
          </div>

          <div className="flex items-center justify-between p-3 bg-zinc-700/50 rounded-lg mt-3">
            <div>
              <p className="text-white">Memory</p>
              <p className="text-zinc-400 text-sm">
                Remember context across conversations (SQLite + FTS5)
              </p>
            </div>
            <button
              onClick={() => {
                const newSettings = { ...appSettings, memoryEnabled: !appSettings.memoryEnabled };
                onAppSettingsChange(newSettings);
                api.saveAppSettings(newSettings);
              }}
              className={`w-12 h-6 rounded-full transition-colors ${
                appSettings.memoryEnabled ? 'bg-amber-500' : 'bg-zinc-600'
              }`}
            >
              <div className={`w-5 h-5 rounded-full bg-white shadow transition-transform ${
                appSettings.memoryEnabled ? 'translate-x-6' : 'translate-x-0.5'
              }`} />
            </button>
          </div>

          <div className="flex items-center justify-between p-3 bg-zinc-700/50 rounded-lg mt-3">
            <div>
              <p className="text-white flex items-center gap-2">
                <Cpu className="w-4 h-4" />
                Cognitive Harness
              </p>
              <p className="text-zinc-400 text-sm">
                Identity + capability awareness — the model knows it's HIVE and what it can do
              </p>
            </div>
            <button
              onClick={() => {
                const newSettings = { ...appSettings, harnessEnabled: !appSettings.harnessEnabled };
                onAppSettingsChange(newSettings);
                api.saveAppSettings(newSettings);
              }}
              className={`w-12 h-6 rounded-full transition-colors ${
                appSettings.harnessEnabled ? 'bg-amber-500' : 'bg-zinc-600'
              }`}
            >
              <div className={`w-5 h-5 rounded-full bg-white shadow transition-transform ${
                appSettings.harnessEnabled ? 'translate-x-6' : 'translate-x-0.5'
              }`} />
            </button>
          </div>
        </div>

        {/* ============================================ */}
        {/* Tool Approval Settings                       */}
        {/* ============================================ */}
        <ToolApprovalSection
          appSettings={appSettings}
          onAppSettingsChange={(s) => { onAppSettingsChange(s); api.saveAppSettings(s); }}
        />

        {/* ============================================ */}
        {/* Thinking Depth Control (P1)                 */}
        {/* ============================================ */}
        <div className="bg-zinc-800 rounded-xl p-6 mt-6">
          <h3 className="text-white font-medium mb-1">Thinking Depth</h3>
          <p className="text-zinc-400 text-sm mb-4">
            Controls reasoning depth for thinking-capable models (Claude, o-series, Kimi, DeepSeek).
            Maps to each provider's native parameter — gracefully ignored by providers that don't support it.
          </p>
          <div className="grid grid-cols-2 gap-3">
            {(['anthropic', 'openai', 'dashscope', 'openrouter'] as const).map(prov => (
              <div key={prov} className="flex items-center justify-between bg-zinc-700/50 rounded-lg px-3 py-2">
                <span className="text-zinc-300 text-sm capitalize">{prov}</span>
                <select
                  value={appSettings.thinkingDepth?.[prov] || 'off'}
                  onChange={(e) => {
                    const val = e.target.value as import('../types').ThinkingDepth;
                    const newDepth = { ...appSettings.thinkingDepth, [prov]: val };
                    // Remove 'off' entries to keep storage clean
                    if (val === 'off') delete newDepth[prov];
                    const newSettings = { ...appSettings, thinkingDepth: newDepth };
                    onAppSettingsChange(newSettings);
                    api.saveAppSettings(newSettings);
                  }}
                  className="bg-zinc-800 text-white text-sm px-2 py-1 rounded border border-zinc-600"
                >
                  <option value="off">Off</option>
                  <option value="low">Low</option>
                  <option value="medium">Medium</option>
                  <option value="high">High</option>
                </select>
              </div>
            ))}
          </div>
        </div>

        {/* ============================================ */}
        {/* Identity Editor — HIVE.md                   */}
        {/* ============================================ */}
        <div className="bg-zinc-800 rounded-xl p-6 mt-6">
          <div className="flex items-center justify-between mb-4">
            <h3 className="text-white font-medium flex items-center gap-2">
              <Cpu className="w-5 h-5" />
              HIVE Identity
            </h3>
            <div className="flex items-center gap-2">
              <button
                onClick={handleResetIdentity}
                className="flex items-center gap-1.5 px-3 py-1.5 text-sm bg-zinc-700 hover:bg-zinc-600 text-zinc-300 rounded-lg"
                title="Reset to factory default"
              >
                <RotateCcw className="w-3.5 h-3.5" />
                Reset
              </button>
              <button
                onClick={() => { loadIdentity(); setShowIdentityEditor(!showIdentityEditor); }}
                className="flex items-center gap-1.5 px-3 py-1.5 text-sm bg-amber-500 hover:bg-amber-600 text-black rounded-lg"
              >
                {showIdentityEditor ? 'Close' : 'Edit'}
              </button>
            </div>
          </div>

          {/* Status message */}
          {identityStatus && (
            <div className={`mb-3 p-2.5 rounded-lg text-sm ${
              identityStatus.ok
                ? 'bg-green-500/10 border border-green-500/30 text-green-400'
                : 'bg-red-500/10 border border-red-500/30 text-red-400'
            }`}>
              {identityStatus.msg}
            </div>
          )}

          <p className="text-zinc-400 text-sm mb-3">
            The identity file defines who HIVE is — personality, principles, and behavioral preferences.
            This is a markdown file you can freely edit. The model reads it at the start of every conversation.
          </p>

          {identityPath && (
            <p className="text-zinc-500 text-xs mb-3 flex items-center gap-1">
              <ExternalLink className="w-3 h-3" />
              {identityPath}
            </p>
          )}

          {showIdentityEditor && (
            <div className="space-y-3">
              {identityLoading ? (
                <p className="text-zinc-500 text-sm py-4 text-center">Loading...</p>
              ) : (
                <>
                  <textarea
                    value={identityContent}
                    onChange={(e) => setIdentityContent(e.target.value)}
                    rows={16}
                    className="w-full bg-zinc-900 text-white px-4 py-3 rounded-lg border border-zinc-600 focus:border-amber-500 outline-none resize-y text-sm font-mono"
                  />
                  <div className="flex items-center justify-end gap-2">
                    <button
                      onClick={() => setShowIdentityEditor(false)}
                      className="px-4 py-2 text-sm text-zinc-400 hover:text-white"
                    >
                      Cancel
                    </button>
                    <button
                      onClick={handleSaveIdentity}
                      className="flex items-center gap-2 px-4 py-2 text-sm bg-amber-500 hover:bg-amber-600 text-black rounded-lg font-medium"
                    >
                      <Save className="w-4 h-4" />
                      Save Identity
                    </button>
                  </div>
                </>
              )}
            </div>
          )}
        </div>

        {/* Phase 4.5.5: Skills System */}
        <SkillsSection />

        {/* Phase 4: Smart Model Router */}
        <SmartRouterSection
          selectedModel={selectedModel}
          serverRunning={serverRunning}
          providerStatuses={providerStatuses}
        />

        {/* Phase 4.5: Integration Keys ("Doors and Keys") */}
        <IntegrationsSection />

        {/* Phase 10: CLI Agent Registry (NEXUS) */}
        <AgentRegistrySection />

        {/* Phase 10.5.4: Channel → Agent Routing (NEXUS) */}
        <ChannelRoutingSection />

        {/* ============================================ */}
        {/* P6: System Tray — Minimize to Tray          */}
        {/* ============================================ */}
        <div className="bg-zinc-800 rounded-xl p-6 mt-6">
          <h3 className="text-white font-medium mb-1">System Tray</h3>
          <p className="text-zinc-400 text-sm mb-4">
            When enabled, closing the window minimizes HIVE to the system tray instead of exiting.
            Servers, daemons, and routines keep running in the background.
          </p>
          <label className="flex items-center gap-3 cursor-pointer">
            <input
              type="checkbox"
              checked={appSettings.minimizeToTray ?? false}
              onChange={async (e) => {
                const enabled = e.target.checked;
                const newSettings = { ...appSettings, minimizeToTray: enabled };
                onAppSettingsChange(newSettings);
                api.saveAppSettings(newSettings);
                try { await api.setMinimizeToTray(enabled); } catch (_) {}
              }}
              className="w-4 h-4 rounded bg-zinc-700 border-zinc-600 text-amber-500 focus:ring-amber-500/50"
            />
            <span className="text-zinc-300 text-sm">Minimize to tray on close</span>
          </label>
        </div>

        {/* Phase 6: Routines (Standing Instructions) */}
        <RoutinesPanel />

        {/* ============================================ */}
        {/* About HIVE — Version + Updater Status        */}
        {/* ============================================ */}
        <div className="bg-zinc-800 rounded-xl p-6 mt-6">
          <h3 className="text-white font-medium mb-1">About HIVE</h3>
          <div className="space-y-2 text-sm">
            <div className="flex items-center justify-between">
              <span className="text-zinc-400">Version</span>
              <span className="text-zinc-200 font-mono">1.0.0</span>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-zinc-400">Auto-updates</span>
              <span className="text-amber-400/80 text-xs">Not yet configured</span>
            </div>
            <p className="text-[11px] text-zinc-500 leading-relaxed pt-1">
              Auto-update infrastructure is scaffolded but requires a signing key and CI/CD pipeline
              before it can deliver updates. For now, check for new releases manually.
            </p>
            <a
              href="https://github.com/LucidPaths/HiveMind/releases"
              target="_blank"
              rel="noopener noreferrer"
              className="inline-flex items-center gap-1.5 text-xs text-amber-500/80 hover:text-amber-400 transition-colors mt-1"
            >
              <ExternalLink size={12} />
              Check for updates on GitHub
            </a>
          </div>
        </div>
      </div>
    </div>
  );
}

// HIVE Desktop - Standalone LLM Runtime with WSL/ROCm Support

import { useState, useEffect, useRef } from 'react';
import * as api from './lib/api';
import { Tab, Backend, MessageOrigin } from './types';
import { Loader2, X } from 'lucide-react';
import { useLogs } from './hooks/useLogs';
import { useHuggingFace } from './hooks/useHuggingFace';
import { useRemoteChannels } from './hooks/useRemoteChannels';

// Tab components
import SetupTab from './components/SetupTab';
import ModelsTab from './components/ModelsTab';
import BrowseTab from './components/BrowseTab';
import MultiPaneChat from './components/MultiPaneChat';
import SettingsTab from './components/SettingsTab';
import LogsTab from './components/LogsTab';
import MemoryTab from './components/MemoryTab';
import McpTab from './components/McpTab';
import ModelInfoPopup from './components/ModelInfoPopup';

export default function App() {
  // System state
  const [systemInfo, setSystemInfo] = useState<api.SystemInfo | null>(null);
  const [wslStatus, setWslStatus] = useState<api.WslStatus | null>(null);
  const [depStatus, setDepStatus] = useState<api.DependencyStatus | null>(null);
  const [backend, setBackend] = useState<Backend>('windows');
  const [loading, setLoading] = useState(true);
  const [setupComplete, setSetupComplete] = useState(false);
  const [installingLlamaServer, setInstallingLlamaServer] = useState(false);
  const [installProgress, setInstallProgress] = useState(0);

  // Live hardware metrics — event-driven, NOT polled per chat turn.
  // Updated on: startup, model start, model stop.
  const [liveMetrics, setLiveMetrics] = useState<api.LiveResourceMetrics | null>(null);

  // Tab state
  const [tab, setTab] = useState<Tab>('setup');
  const tabRef = useRef<Tab>(tab);
  tabRef.current = tab;

  // Unread chat indicator — set when remote messages inject into chat while user is on another tab
  const [chatHasUnread, setChatHasUnread] = useState(false);

  // Model state
  const [localModels, setLocalModels] = useState<api.LocalModel[]>([]);
  const [wslModels, setWslModels] = useState<api.LocalModel[]>([]);
  const [selectedModel, setSelectedModel] = useState<api.LocalModel | null>(null);
  const [serverRunning, setServerRunning] = useState(false);
  const [serverLoading, setServerLoading] = useState(false);

  // Cloud model selection
  const [selectedCloudModel, setSelectedCloudModel] = useState<{provider: api.ProviderType; model: api.ProviderModel} | null>(null);
  const [activeModelType, setActiveModelType] = useState<'local' | 'cloud'>('local');

  // Model info popup state
  const [showModelInfo, setShowModelInfo] = useState(false);

  // App settings
  const [appSettings, setAppSettings] = useState<api.AppSettings>(api.getAppSettings());

  // Model settings state
  const [modelSettings, setModelSettings] = useState<api.ModelSettings>(api.getDefaultModelSettings());

  // Provider state
  const [providers, setProviders] = useState<api.ProviderConfig[]>([]);
  const [providerStatuses, setProviderStatuses] = useState<Record<string, api.ProviderStatus>>({});
  const [showProviders, setShowProviders] = useState(false);
  const [apiKeyInput, setApiKeyInput] = useState<Record<string, string>>({});
  const [showApiKey, setShowApiKey] = useState<Record<string, boolean>>({});
  const [savingKey, setSavingKey] = useState<string | null>(null);

  const [error, setError] = useState<string | null>(null);

  // HuggingFace browsing + recommendations (extracted hook)
  const {
    hfSearch, setHfSearch, hfModels, hfLoading,
    selectedHfModel, hfFiles, downloading, downloadProgress,
    vramCompatibility, recommendedModels, recsLoading,
    searchHuggingFace, selectHfModel, downloadFile,
  } = useHuggingFace({ systemInfo, onError: setError, onModelsLoaded: () => loadModels() });

  // Remote channel routing refs — these point to the ACTIVE pane's sendMessage.
  // useRemoteChannels writes to these; the active ChatPane registers its sendMessage here.
  // This ensures Telegram/Discord/Worker/Routine messages appear in the visible pane.
  const remoteSendRef = useRef<((text?: string) => Promise<void>) | undefined>();
  const remoteOriginRef = useRef<MessageOrigin>('desktop');

  // Settings saved indicator
  const [settingsSaved, setSettingsSaved] = useState(false);
  const savedTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Logs (extracted hook — console capture + auto-scroll)
  const { logs, logsEndRef, clearLogs } = useLogs();


  // Initialize - detect hardware
  useEffect(() => {
    detectSystem();
  }, []);


  // Cognitive Harness — no startup load needed (assembled dynamically per turn)

  // Initialize memory system
  useEffect(() => {
    api.memoryInit().then(msg => {
      console.log(`[HIVE] ${msg}`);
      api.memoryStats().then(stats => {
        console.log(`[HIVE] Memory: ${stats.total_memories} memories, ${stats.total_chunks} chunks, ${stats.total_conversations} conversations`);
      });
      // Phase 4C: Promote short_term memories that have been recalled enough times.
      // Runs at session start so promotion isn't trapped inside search_hybrid.
      api.memoryPromote().then(count => {
        if (count > 0) console.log(`[HIVE] Memory: promoted ${count} short_term → long_term`);
      }).catch(e => console.warn('[HIVE] Memory promotion failed (non-fatal):', e));
    }).catch(err => {
      console.error('[HIVE] Memory init failed:', err);
    });
  }, []);

  // P6: Sync minimize-to-tray setting to Rust on startup
  useEffect(() => {
    if (appSettings.minimizeToTray) {
      api.setMinimizeToTray(true).catch(() => {});
    }
  }, []);

  // Remote channels: Telegram, Discord, Workers, Routines (extracted hook)
  // remoteSendRef/remoteOriginRef point to the active pane's chat — updated by MultiPaneChat.
  useRemoteChannels({
    sendMessageRef: remoteSendRef,
    messageOriginRef: remoteOriginRef,
    onChatInjection: () => {
      if (tabRef.current !== 'chat') setChatHasUnread(true);
    },
  });

  // Clear chat unread indicator when switching to chat (covers onSetTab from child components)
  useEffect(() => {
    if (tab === 'chat') setChatHasUnread(false);
  }, [tab]);

  // Auto-populate browse tab on first visit
  useEffect(() => {
    if (tab === 'browse' && hfModels.length === 0 && !hfLoading) {
      searchHuggingFace();
    }
  }, [tab]);

  // Load model settings when model is selected.
  // Local models: use saved slider value. Cloud models: ALWAYS use provider-reported context.
  // Cloud context is authoritative — sendMessage reads it directly from the provider,
  // so this just ensures the UI shows the right number. No localStorage "auto-upgrade" games.
  useEffect(() => {
    if (selectedModel) {
      setModelSettings(api.getModelSettings(selectedModel.filename));
    } else if (selectedCloudModel) {
      const saved = api.getModelSettings(selectedCloudModel.model.id);
      const providerContext = selectedCloudModel.model.context_length;
      if (providerContext && providerContext !== saved.contextLength) {
        saved.contextLength = providerContext;
        api.saveModelSettings(selectedCloudModel.model.id, { contextLength: providerContext });
      }
      setModelSettings(saved);
    } else {
      setModelSettings(api.getDefaultModelSettings());
    }
  }, [selectedModel, selectedCloudModel]);

  // ==========================================
  // Business Logic Functions
  // ==========================================

  async function detectSystem() {
    console.log('[HIVE] detectSystem: Starting hardware detection...');
    setLoading(true);
    try {
      const [sysInfo, wsl, deps, initialMetrics] = await Promise.all([
        api.getSystemInfo(),
        api.checkWsl(),
        api.checkDependencies(),
        api.getLiveResourceUsage().catch(() => null),
      ]);
      console.log('[HIVE] detectSystem: System info:', {
        gpuCount: sysInfo.gpus?.length || 0,
        gpus: sysInfo.gpus?.map(g => `${g.name} (${(g.vram_mb/1024).toFixed(1)}GB)`),
        recommendedBackend: sysInfo.recommended_backend
      });
      console.log('[HIVE] detectSystem: WSL status:', { installed: wsl?.installed, distro: wsl?.distro });
      console.log('[HIVE] detectSystem: Dependencies:', deps);
      setSystemInfo(sysInfo);
      setWslStatus(wsl);
      setDepStatus(deps);
      if (initialMetrics) setLiveMetrics(initialMetrics);

      const selectedBackend = (deps.recommended_backend === 'wsl' && wsl.installed) ? 'wsl' : 'windows';
      setBackend(selectedBackend);
      await loadModels(selectedBackend, wsl);
      loadProviders();

      if (deps.ready_to_run) {
        setSetupComplete(true);
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  async function installLlamaServer() {
    setInstallingLlamaServer(true);
    setInstallProgress(0);
    setError(null);
    try {
      await api.downloadLlamaServer((downloaded, total) => {
        setInstallProgress(Math.round((downloaded / total) * 100));
      });
      const deps = await api.checkDependencies();
      setDepStatus(deps);
    } catch (e) {
      setError(String(e));
    } finally {
      setInstallingLlamaServer(false);
      setInstallProgress(0);
    }
  }

  async function loadProviders() {
    console.log('[HIVE] loadProviders: Starting...');
    try {
      const providerList = await api.getProviders();
      console.log('[HIVE] loadProviders: Got providers:', providerList.map(p => ({
        type: p.provider_type,
        has_api_key: p.has_api_key
      })));
      setProviders(providerList);

      const statuses: Record<string, api.ProviderStatus> = {};
      for (const p of providerList) {
        if (p.provider_type !== 'local') {
          try {
            console.log(`[HIVE] loadProviders: Checking status for ${p.provider_type}...`);
            statuses[p.provider_type] = await api.checkProviderStatus(p.provider_type);
            console.log(`[HIVE] loadProviders: ${p.provider_type} status:`, {
              connected: statuses[p.provider_type].connected,
              error: statuses[p.provider_type].error,
              modelCount: statuses[p.provider_type].models.length
            });
          } catch (statusErr) {
            console.error(`[HIVE] loadProviders: ${p.provider_type} status check failed:`, statusErr);
            statuses[p.provider_type] = {
              provider_type: p.provider_type,
              configured: p.has_api_key,
              connected: false,
              error: 'Failed to check status',
              models: [],
            };
          }
        }
      }
      setProviderStatuses(statuses);
      console.log('[HIVE] loadProviders: Complete');
    } catch (e) {
      console.error('[HIVE] loadProviders FAILED:', e);
    }
  }

  async function saveApiKey(provider: api.ProviderType) {
    const key = apiKeyInput[provider];
    if (!key?.trim()) return;

    console.log(`[HIVE] saveApiKey: Starting for provider "${provider}"`);
    setSavingKey(provider);
    setError(null);
    try {
      console.log(`[HIVE] saveApiKey: Calling api.storeApiKey...`);
      await api.storeApiKey(provider, key.trim());
      console.log(`[HIVE] saveApiKey: storeApiKey succeeded`);
      setApiKeyInput(prev => ({ ...prev, [provider]: '' }));

      console.log(`[HIVE] saveApiKey: Refreshing providers...`);
      await loadProviders();
      console.log(`[HIVE] saveApiKey: Providers refreshed successfully`);
    } catch (e) {
      const errMsg = String(e);
      console.error('[HIVE] saveApiKey FAILED:', errMsg);
      setError(`Failed to save API key: ${errMsg}. Your system keyring may not be accessible.`);
    } finally {
      setSavingKey(null);
      console.log(`[HIVE] saveApiKey: Complete for provider "${provider}"`);
    }
  }

  async function removeApiKey(provider: api.ProviderType) {
    console.log('[HIVE] removeApiKey: Removing key for', provider);
    setSavingKey(provider);
    try {
      await api.deleteApiKey(provider);

      if (selectedCloudModel?.provider === provider) {
        console.log('[HIVE] removeApiKey: Clearing selected cloud model (was from removed provider)');
        setSelectedCloudModel(null);
        setActiveModelType('local');
      }

      await loadProviders();
    } catch (e) {
      setError(String(e));
    } finally {
      setSavingKey(null);
    }
  }

  async function loadModels(selectedBackend?: Backend, wslStatusOverride?: api.WslStatus | null) {
    const b = selectedBackend || backend;
    const wsl = wslStatusOverride !== undefined ? wslStatusOverride : wslStatus;
    try {
      const local = await api.listLocalModels();
      setLocalModels(local);

      if (b === 'wsl' && wsl?.installed) {
        const wslModelsList = await api.listWslModels([
          '$HOME/models',
          '$HOME/Models',
          '$HOME/llama.cpp/models',
          '$HOME/.cache/huggingface',
          '$HOME/Downloads',
          '$HOME',
        ]);
        setWslModels(wslModelsList);
      }
    } catch (e) {
      setError(String(e));
    }
  }

  function getAllModels(): api.LocalModel[] {
    if (backend === 'wsl') {
      return [...wslModels, ...localModels];
    }
    return localModels;
  }

  // VRAM pre-launch warning state
  const [vramWarning, setVramWarning] = useState<{message: string; onConfirm: () => void; onCancel: () => void} | null>(null);

  async function startModel() {
    if (!selectedModel) return;
    setServerLoading(true);
    setError(null);
    try {
      const gpuLayers = modelSettings.gpuLayers;
      const modelMax = selectedModel.context_length || null;
      const contextLength = modelMax
        ? Math.min(modelSettings.contextLength, modelMax)
        : modelSettings.contextLength;
      const kvOffload = modelSettings.kvOffload;

      // VRAM pre-launch check
      const primaryGpu = systemInfo?.gpus?.[0];
      if (primaryGpu && primaryGpu.vram_mb > 0) {
        try {
          const compat = await api.checkVramCompatibility(
            selectedModel.size_bytes,
            selectedModel.filename,
            primaryGpu.vram_mb,
            contextLength,
            kvOffload,
          );
          const ramGb = systemInfo?.ram?.total_gb ?? 0;
          const gpuVendor = primaryGpu.vendor?.toLowerCase();
          const cpuCores = systemInfo?.cpu?.cores ?? 4;
          const tier = api.getSpeedTier(compat, ramGb, gpuVendor, cpuCores);

          if (tier.tier === 'too_large') {
            setServerLoading(false);
            // Build actionable warning with quant walk-down suggestion
            const vramGb = (primaryGpu.vram_mb / 1024);
            let message = `This model requires ~${compat.estimate.total_gb.toFixed(1)} GB but your GPU has ${vramGb.toFixed(1)} GB VRAM and ${ramGb.toFixed(0)} GB RAM. It likely won't run.`;

            // If MoE, mention expert offload possibility
            if (compat.estimate.is_moe && compat.estimate.moe_active_gb != null) {
              message += `\n\nThis is a Mixture-of-Experts model. With expert offloading, it needs ~${compat.estimate.moe_active_gb.toFixed(1)} GB VRAM (active experts only).`;
            }

            // Suggest a quant that would fit
            const paramsB = api.estimateParamsB(compat.estimate.model_weights_gb, compat.estimate.quantization);
            const suggestion = api.bestQuantForBudget(paramsB, vramGb, contextLength);
            if (suggestion) {
              message += `\n\nA ${suggestion.quant} quantization (~${suggestion.estimatedGb} GB) would fit your GPU at ${suggestion.contextLength} context.`;
            }

            const confirmed = await new Promise<boolean>((resolve) => {
              setVramWarning({
                message,
                onConfirm: () => { setVramWarning(null); resolve(true); },
                onCancel: () => { setVramWarning(null); resolve(false); },
              });
            });
            if (!confirmed) return;
            setServerLoading(true);
          } else if (tier.tier === 'slow') {
            console.log(`[HIVE] startModel: VRAM warning — model will use RAM offload (${tier.detail})`);
          }
        } catch (e) {
          // Non-fatal — proceed without VRAM check
          console.warn('[HIVE] startModel: VRAM check failed (non-fatal):', e);
        }
      }

      console.log('[HIVE] startModel: Starting model', JSON.stringify({
        model: selectedModel.filename,
        backend,
        gpuLayers,
        contextLength,
        kvOffload,
        modelMaxContext: modelMax,
      }));
      if (backend === 'wsl') {
        console.log('[HIVE] startModel: Using WSL backend');
        await api.startServerWsl(
          selectedModel.path,
          8080,
          gpuLayers,
          contextLength,
          kvOffload,
          wslStatus?.llama_server_path || undefined
        );
      } else {
        console.log('[HIVE] startModel: Using native Windows backend');
        await api.startServerNative(selectedModel.path, 8080, gpuLayers, contextLength, kvOffload);
      }

      console.log('[HIVE] startModel: Waiting for server health check...');
      let ready = false;
      for (let i = 0; i < 60; i++) {
        await new Promise(r => setTimeout(r, 1000));
        ready = await api.checkServerHealth();
        if (ready) break;
        if (i % 10 === 9) console.log(`[HIVE] startModel: Still waiting... (${i+1}s)`);
      }
      setServerRunning(ready);
      if (ready) {
        console.log('[HIVE] startModel: Server ready! Switching to chat tab');
        // Refresh metrics now that model loaded into VRAM
        api.getLiveResourceUsage().then(setLiveMetrics).catch(() => {});
        setTab('chat');
      } else {
        console.error('[HIVE] startModel: Server failed to respond after 60s');
        setError('Server started but not responding after 60s. The model may be too large or llama-server may not be working.');
      }
    } catch (e) {
      console.error('[HIVE] startModel: Failed:', e);
      setError(String(e));
    } finally {
      setServerLoading(false);
    }
  }

  async function stopModel() {
    console.log('[HIVE] stopModel: Stopping server...');
    try {
      await api.stopServer();
      setServerRunning(false);
      console.log('[HIVE] stopModel: Server stopped');
      // Refresh metrics now that VRAM is freed
      api.getLiveResourceUsage().then(setLiveMetrics).catch(() => {});

      // Phase 4: Stop any running specialist servers and record sleep
      const specialistRoles = ['coder', 'terminal', 'webcrawl', 'toolcall'] as const;
      for (const role of specialistRoles) {
        api.stopSpecialistServer(role)
          .then(() => {
            api.recordSlotSleep(role).catch(() => {});
            api.magmaAddEvent('specialist_sleep', role, 'Stopped with consciousness').catch(() => {});
            console.log(`[HIVE] stopModel: Stopped specialist ${role}`);
          })
          .catch(() => {}); // Non-fatal — role wasn't running
      }
    } catch (e) {
      console.error('[HIVE] stopModel: Failed:', e);
      setError(String(e));
    }
  }


  // Conversation persistence: each ChatPane manages its own useConversationManager.
  // Remote channel messages route to the active pane (via remoteSendRef bridge).

  function saveSettingsWithIndicator(filename: string, newSettings: api.ModelSettings) {
    api.saveModelSettings(filename, newSettings);
    setSettingsSaved(true);
    if (savedTimeoutRef.current) {
      clearTimeout(savedTimeoutRef.current);
    }
    savedTimeoutRef.current = setTimeout(() => {
      setSettingsSaved(false);
    }, 2000);
  }

  function proceedToModels() {
    setSetupComplete(true);
    setTab('models');
  }

  // ==========================================
  // Render
  // ==========================================

  if (loading) {
    return (
      <div className="h-screen bg-zinc-900 flex items-center justify-center">
        <div className="text-center">
          <Loader2 className="w-12 h-12 mx-auto text-amber-500 animate-spin mb-4" />
          <p className="text-zinc-400">Detecting hardware...</p>
        </div>
      </div>
    );
  }

  return (
    <div className="h-screen bg-zinc-900 flex flex-col">
      {/* Header */}
      <header className="bg-zinc-800 border-b border-zinc-700 px-4 py-3 flex items-center justify-between">
        <div className="flex items-center gap-4">
          <h1 className="text-xl font-bold text-white">HIVE</h1>
          <div className="flex gap-1">
            {(['setup', 'models', 'browse', 'chat', 'memory', 'mcp', 'settings', 'logs'] as Tab[]).map((t) => (
              <button
                key={t}
                onClick={() => {
                  console.log('[HIVE] UI: Tab changed to:', t);
                  setTab(t);
                  if (t === 'chat') setChatHasUnread(false);
                }}
                disabled={!setupComplete && t !== 'setup'}
                className={`px-3 py-1.5 rounded-lg text-sm font-medium transition-colors ${
                  tab === t
                    ? 'bg-amber-500 text-black'
                    : 'text-zinc-400 hover:text-white hover:bg-zinc-700 disabled:opacity-50 disabled:cursor-not-allowed'
                }`}
              >
                <span className="flex items-center gap-1.5">
                  {t === 'mcp' ? 'MCP' : t.charAt(0).toUpperCase() + t.slice(1)}
                  {t === 'chat' && chatHasUnread && tab !== 'chat' && (
                    <span className="w-2 h-2 rounded-full bg-amber-400 animate-pulse" />
                  )}
                </span>
              </button>
            ))}
          </div>
        </div>
        <div className="flex items-center gap-3">
          {serverRunning ? (
            <div className="flex items-center gap-2 text-green-400 text-sm">
              <div className="w-2 h-2 rounded-full bg-green-400 animate-pulse" />
              Running ({backend.toUpperCase()})
            </div>
          ) : (
            <div className="text-zinc-500 text-sm">
              Backend: {backend.toUpperCase()}
            </div>
          )}
        </div>
      </header>

      {/* Tab Content */}
      <main className="flex-1 flex flex-col overflow-hidden">
        {tab === 'setup' && (
          <SetupTab
            systemInfo={systemInfo}
            wslStatus={wslStatus}
            depStatus={depStatus}
            backend={backend}
            setBackend={setBackend}
            onDetectSystem={detectSystem}
            onInstallLlamaServer={installLlamaServer}
            installingLlamaServer={installingLlamaServer}
            installProgress={installProgress}
            onProceedToModels={proceedToModels}
          />
        )}

        {tab === 'models' && (
          <ModelsTab
            models={getAllModels()}
            selectedModel={selectedModel}
            onSelectModel={(model) => {
              setSelectedModel(model);
              setActiveModelType('local');
              setSelectedCloudModel(null);
            }}
            serverRunning={serverRunning}
            serverLoading={serverLoading}
            onStartModel={startModel}
            onStopModel={stopModel}
            onLoadModels={() => loadModels()}
            backend={backend}
            providers={providers}
            providerStatuses={providerStatuses}
            showProviders={showProviders}
            onToggleProviders={() => setShowProviders(!showProviders)}
            apiKeyInput={apiKeyInput}
            onApiKeyInputChange={(provider, value) => setApiKeyInput(prev => ({ ...prev, [provider]: value }))}
            showApiKey={showApiKey}
            onToggleShowApiKey={(provider) => setShowApiKey(prev => ({ ...prev, [provider]: !prev[provider] }))}
            savingKey={savingKey}
            onSaveApiKey={saveApiKey}
            onRemoveApiKey={removeApiKey}
            selectedCloudModel={selectedCloudModel}
            onSelectCloudModel={(provider, model) => {
              setSelectedCloudModel({ provider, model });
              setActiveModelType('cloud');
              setSelectedModel(null);
            }}
            activeModelType={activeModelType}
            onSetTab={setTab}
            onShowModelInfo={() => setShowModelInfo(true)}
          />
        )}

        {tab === 'browse' && (
          <BrowseTab
            hfSearch={hfSearch}
            setHfSearch={setHfSearch}
            hfModels={hfModels}
            hfLoading={hfLoading}
            selectedHfModel={selectedHfModel}
            hfFiles={hfFiles}
            downloading={downloading}
            downloadProgress={downloadProgress}
            recommendedModels={recommendedModels}
            recsLoading={recsLoading}
            vramCompatibility={vramCompatibility}
            systemInfo={systemInfo}
            onSearchHuggingFace={searchHuggingFace}
            onSelectHfModel={selectHfModel}
            onDownloadFile={downloadFile}
          />
        )}

        {/* Chat stays mounted (CSS hidden) so in-flight responses, streaming,
            and conversation history survive tab switches. Other tabs are cheap to
            remount so they keep conditional rendering. */}
        <div className={tab === 'chat' ? 'flex-1 flex flex-col overflow-hidden' : 'hidden'}>
          <MultiPaneChat
            serverRunning={serverRunning}
            selectedModel={selectedModel}
            selectedCloudModel={selectedCloudModel}
            activeModelType={activeModelType}
            appSettings={appSettings}
            localModels={localModels}
            wslModels={wslModels}
            backend={backend}
            wslStatus={wslStatus}
            providerStatuses={providerStatuses}
            systemInfo={systemInfo}
            liveMetrics={liveMetrics}
            vramCompatibility={vramCompatibility}
            modelSettings={modelSettings}
            onSetTab={setTab}
            remoteSendRef={remoteSendRef}
            remoteOriginRef={remoteOriginRef}
          />
        </div>

        {tab === 'memory' && (
          <MemoryTab />
        )}

        {tab === 'mcp' && (
          <McpTab />
        )}

        {tab === 'settings' && (
          <SettingsTab
            selectedModel={selectedModel}
            selectedCloudModel={selectedCloudModel}
            activeModelType={activeModelType}
            serverRunning={serverRunning}
            modelSettings={modelSettings}
            onModelSettingsChange={setModelSettings}
            onSaveSettings={saveSettingsWithIndicator}
            settingsSaved={settingsSaved}
            systemInfo={systemInfo}
            appSettings={appSettings}
            onAppSettingsChange={setAppSettings}
            onSetTab={setTab}
            providerStatuses={providerStatuses}
          />
        )}

        {tab === 'logs' && (
          <LogsTab
            logs={logs}
            onClearLogs={clearLogs}
            logsEndRef={logsEndRef}
            serverRunning={serverRunning}
          />
        )}
      </main>

      {/* Model Info Popup */}
      {showModelInfo && selectedModel && (
        <ModelInfoPopup
          selectedModel={selectedModel}
          serverRunning={serverRunning}
          serverLoading={serverLoading}
          systemInfo={systemInfo}
          modelSettings={modelSettings}
          onClose={() => setShowModelInfo(false)}
          onStartModel={startModel}
          onStopModel={stopModel}
          onSetTab={setTab}
        />
      )}

      {/* VRAM Warning Dialog */}
      {vramWarning && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60">
          <div className="bg-zinc-800 border border-amber-500/40 rounded-xl p-6 max-w-md mx-4 shadow-2xl">
            <h3 className="text-amber-400 font-medium mb-3 flex items-center gap-2">
              <span className="text-xl">&#x26A0;</span>
              VRAM Warning
            </h3>
            <p className="text-zinc-300 text-sm mb-4">{vramWarning.message}</p>
            <div className="flex gap-3 justify-end">
              <button
                onClick={() => { vramWarning.onCancel(); setServerLoading(false); }}
                className="px-4 py-2 bg-zinc-700 hover:bg-zinc-600 text-white text-sm rounded-lg"
              >
                Cancel
              </button>
              <button
                onClick={vramWarning.onConfirm}
                className="px-4 py-2 bg-amber-500 hover:bg-amber-600 text-black text-sm rounded-lg font-medium"
              >
                Try Anyway
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Error toast */}
      {error && (
        <div className="fixed bottom-4 right-4 max-w-md bg-red-500/90 text-white p-4 rounded-xl flex items-center gap-3">
          <span className="text-sm flex-1">{error}</span>
          <button onClick={() => setError(null)} className="hover:bg-red-600 p-1 rounded">
            <X className="w-4 h-4" />
          </button>
        </div>
      )}
    </div>
  );
}

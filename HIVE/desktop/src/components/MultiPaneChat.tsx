/**
 * MultiPaneChat — Unified Multi-Pane Adaptive Chat
 *
 * Manages N independent chat panes in a resizable panel layout.
 * Each pane gets its own model, conversation, and streaming state.
 *
 * Default: single pane (backwards compatible with existing behavior).
 * Power user: split into 2, 3, 4+ panes with different models.
 *
 * Lattice compliance:
 * - P1 (Modularity): Each pane is a self-contained ChatPane component
 * - P2 (Provider Agnosticism): Any pane can use any provider
 * - P3 (Simplicity Wins): Single new dependency (react-resizable-panels)
 * - P8 (Low Floor, High Ceiling): Starts as single pane, splits on demand
 */

import { useState, useCallback, useRef } from 'react';
import { Group, Panel, Separator } from 'react-resizable-panels';
import * as api from '../lib/api';
import type { ChatPaneConfig, Backend, Tab, MessageOrigin } from '../types';
import { BUILTIN_AGENTS } from '../types';
import ChatPane from './ChatPane';
import TerminalPane from './TerminalPane';

const MAX_PANES = 4;
const LAYOUT_STORAGE_KEY = 'hive-pane-layout';

interface MultiPaneChatProps {
  // Shared global state from App.tsx
  serverRunning: boolean;
  selectedModel: api.LocalModel | null;
  selectedCloudModel: { provider: api.ProviderType; model: api.ProviderModel } | null;
  activeModelType: 'local' | 'cloud';
  appSettings: api.AppSettings;
  localModels: api.LocalModel[];
  wslModels: api.LocalModel[];
  backend: Backend;
  wslStatus: api.WslStatus | null;
  providerStatuses: Record<string, api.ProviderStatus>;
  systemInfo: api.SystemInfo | null;
  liveMetrics: api.LiveResourceMetrics | null;
  vramCompatibility: Record<string, api.VramCompatibility>;
  modelSettings: api.ModelSettings;
  onSetTab: (tab: Tab) => void;
  // Remote channel routing — the active pane registers its sendMessage here
  remoteSendRef: React.MutableRefObject<((text?: string) => Promise<void>) | undefined>;
  remoteOriginRef: React.MutableRefObject<MessageOrigin>;
}

function createDefaultPane(id: string, model: api.LocalModel | null, cloudModel: { provider: api.ProviderType; model: api.ProviderModel } | null, modelType: 'local' | 'cloud'): ChatPaneConfig {
  if (modelType === 'cloud' && cloudModel) {
    return {
      id,
      modelType: 'cloud',
      provider: cloudModel.provider,
      modelId: cloudModel.model.id,
      modelDisplayName: cloudModel.model.name || cloudModel.model.id,
    };
  }
  return {
    id,
    modelType: 'local',
    modelId: model?.filename,
    modelDisplayName: model?.filename?.replace(/\.gguf$/i, '') || 'No model',
  };
}

function loadSavedPanes(): ChatPaneConfig[] | null {
  try {
    const saved = localStorage.getItem(LAYOUT_STORAGE_KEY);
    if (saved) {
      const parsed = JSON.parse(saved);
      if (Array.isArray(parsed) && parsed.length > 0) {
        return parsed;
      }
    }
  } catch {
    // Ignore parse errors
  }
  return null;
}

function savePanes(panes: ChatPaneConfig[]) {
  try {
    localStorage.setItem(LAYOUT_STORAGE_KEY, JSON.stringify(panes));
  } catch {
    // Non-fatal
  }
}

export default function MultiPaneChat({
  serverRunning, selectedModel, selectedCloudModel, activeModelType,
  appSettings, localModels, wslModels, backend, wslStatus,
  providerStatuses, systemInfo, liveMetrics, vramCompatibility,
  modelSettings, onSetTab, remoteSendRef, remoteOriginRef,
}: MultiPaneChatProps) {
  // Ref-based counter to avoid module-level state that breaks with StrictMode/HMR (M22)
  const paneCounterRef = useRef(0);
  function nextPaneId(): string {
    return `pane-${++paneCounterRef.current}-${Date.now().toString(36)}`;
  }

  // Initialize panes — restore from localStorage or create default single pane
  const [panes, setPanes] = useState<ChatPaneConfig[]>(() => {
    const saved = loadSavedPanes();
    if (saved) return saved;
    return [createDefaultPane(nextPaneId(), selectedModel, selectedCloudModel, activeModelType)];
  });

  const [activePaneId, setActivePaneId] = useState<string>(panes[0]?.id || '');

  const addPane = useCallback(() => {
    if (panes.length >= MAX_PANES) return;
    const newPane = createDefaultPane(nextPaneId(), selectedModel, selectedCloudModel, activeModelType);
    const newPanes = [...panes, newPane];
    setPanes(newPanes);
    setActivePaneId(newPane.id);
    savePanes(newPanes);
  }, [panes, selectedModel, selectedCloudModel, activeModelType]);

  const addTerminalPane = useCallback((agentId: string) => {
    if (panes.length >= MAX_PANES) return;
    const allAgents = [...BUILTIN_AGENTS, ...api.getCustomAgents()];
    const agent = allAgents.find(a => a.id === agentId) || BUILTIN_AGENTS[0];
    const newPane: ChatPaneConfig = {
      id: nextPaneId(),
      paneType: 'terminal',
      modelType: 'local', // Unused for terminal panes but required by type
      modelDisplayName: agent.name,
      agentId: agent.id,
    };
    const newPanes = [...panes, newPane];
    setPanes(newPanes);
    setActivePaneId(newPane.id);
    savePanes(newPanes);
  }, [panes]);

  const updatePanePtySession = useCallback((paneId: string, ptySessionId: string) => {
    setPanes(prev => {
      const updated = prev.map(p => p.id === paneId ? { ...p, ptySessionId } : p);
      savePanes(updated);
      return updated;
    });
  }, []);

  const handleTerminalExit = useCallback((_paneId: string, _exitCode: number | null) => {
    // Terminal pane stays visible with "[Exited]" status — user can close manually.
    // No auto-remove: the user might want to scroll back through output.
  }, []);

  const removePane = useCallback((paneId: string) => {
    if (panes.length <= 1) return;
    const newPanes = panes.filter(p => p.id !== paneId);
    setPanes(newPanes);
    if (activePaneId === paneId) {
      setActivePaneId(newPanes[0]?.id || '');
    }
    savePanes(newPanes);
  }, [panes, activePaneId]);

  const openModelSelector = useCallback((_paneId: string) => {
    // Future: open a model selector dropdown/modal for this specific pane
    // For now, the pane inherits the global model selection from App.tsx
  }, []);

  return (
    <div className="h-full flex flex-col overflow-hidden">
      <Group
        orientation="horizontal"
        id="hive-chat-panels"
        className="flex-1"
      >
        {panes.map((pane, index) => (
          <PanelGroupItem
            key={pane.id}
            pane={pane}
            index={index}
            totalPanes={panes.length}
            isActive={activePaneId === pane.id}
            onActivate={() => setActivePaneId(pane.id)}
            onRemove={() => removePane(pane.id)}
            onAdd={addPane}
            onAddTerminal={addTerminalPane}
            onModelSelect={openModelSelector}
            onPtySessionId={(sid) => updatePanePtySession(pane.id, sid)}
            onTerminalExit={(code) => handleTerminalExit(pane.id, code)}
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
            onSetTab={onSetTab}
            isRemoteTarget={activePaneId === pane.id}
            remoteSendRef={remoteSendRef}
            remoteOriginRef={remoteOriginRef}
          />
        ))}
      </Group>
    </div>
  );
}

/** Wrapper that renders a Panel + optional ResizeHandle for each pane */
function PanelGroupItem({
  pane, index, totalPanes, isActive,
  onActivate, onRemove, onAdd, onAddTerminal, onModelSelect,
  onPtySessionId, onTerminalExit,
  isRemoteTarget, remoteSendRef, remoteOriginRef,
  ...sharedProps
}: {
  pane: ChatPaneConfig;
  index: number;
  totalPanes: number;
  isActive: boolean;
  onActivate: () => void;
  onRemove: () => void;
  onAdd: () => void;
  onAddTerminal: (agentId: string) => void;
  onModelSelect: (paneId: string) => void;
  onPtySessionId: (sessionId: string) => void;
  onTerminalExit: (exitCode: number | null) => void;
  isRemoteTarget: boolean;
  remoteSendRef: React.MutableRefObject<((text?: string) => Promise<void>) | undefined>;
  remoteOriginRef: React.MutableRefObject<MessageOrigin>;
} & Omit<MultiPaneChatProps, 'onSetTab' | 'remoteSendRef' | 'remoteOriginRef'> & { onSetTab: (tab: Tab) => void }) {
  const isTerminal = pane.paneType === 'terminal';

  return (
    <>
      <Panel
        id={pane.id}
        defaultSize={100 / totalPanes}
        minSize={20}
        className="flex flex-col"
      >
        {isTerminal ? (
          <TerminalPane
            pane={pane}
            isActive={isActive}
            isOnly={totalPanes === 1}
            canAdd={totalPanes < MAX_PANES}
            onActivate={onActivate}
            onRemove={onRemove}
            onAdd={onAdd}
            onAddTerminal={onAddTerminal}
            onModelSelect={onModelSelect}
            onPtySessionId={onPtySessionId}
            onExit={onTerminalExit}
          />
        ) : (
          <ChatPane
            pane={pane}
            isActive={isActive}
            isOnly={totalPanes === 1}
            canAdd={totalPanes < MAX_PANES}
            onActivate={onActivate}
            onRemove={onRemove}
            onAdd={onAdd}
            onAddTerminal={onAddTerminal}
            onModelSelect={onModelSelect}
            isRemoteTarget={isRemoteTarget}
            remoteSendRef={remoteSendRef}
            remoteOriginRef={remoteOriginRef}
            {...sharedProps}
          />
        )}
      </Panel>
      {index < totalPanes - 1 && (
        <Separator className="w-1.5 bg-zinc-800 hover:bg-amber-500/30 active:bg-amber-500/50 transition-colors cursor-col-resize" />
      )}
    </>
  );
}

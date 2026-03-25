/**
 * ChatPane — A self-contained chat pane that owns its own useChat() instance.
 *
 * Each pane gets independent:
 * - Message history
 * - Streaming state
 * - Tool approval
 * - Conversation persistence
 *
 * This component wraps ChatTab with a PaneHeader and manages the per-pane
 * model configuration. Hook rules are satisfied because each pane is a
 * separate component instance (hooks called at top level, not in a loop).
 */

import { useState, useEffect } from 'react';
import * as api from '../lib/api';
import { useChat } from '../useChat';
import { useConversationManager } from '../hooks/useConversationManager';
import ChatTab from './ChatTab';
import PaneHeader from './PaneHeader';
import type { ChatPaneConfig, Backend, MessageOrigin } from '../types';

interface ChatPaneProps {
  pane: ChatPaneConfig;
  isActive: boolean;
  isOnly: boolean;
  canAdd: boolean;
  onActivate: () => void;
  onRemove: () => void;
  onAdd: () => void;
  onAddTerminal: (agentId: string) => void;
  onModelSelect: (paneId: string) => void;

  // Remote channel routing — the active pane bridges its sendMessage to these refs
  isRemoteTarget: boolean;
  remoteSendRef: React.MutableRefObject<((text?: string) => Promise<void>) | undefined>;
  remoteOriginRef: React.MutableRefObject<MessageOrigin>;

  // Shared global state (read-only — comes from App.tsx)
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
  onSetTab: (tab: import('../types').Tab) => void;
}

export default function ChatPane({
  pane, isActive, isOnly, canAdd,
  onActivate, onRemove, onAdd, onAddTerminal, onModelSelect,
  isRemoteTarget, remoteSendRef, remoteOriginRef,
  serverRunning, selectedModel, selectedCloudModel, activeModelType,
  appSettings, localModels, wslModels, backend, wslStatus,
  providerStatuses, systemInfo, liveMetrics, vramCompatibility,
  modelSettings, onSetTab,
}: ChatPaneProps) {
  const [, setError] = useState<string | null>(null);
  const [attachments, setAttachments] = useState<api.FileAttachment[]>([]);

  // Each pane gets its own useChat instance — this is the core of multi-pane
  const chatInstance = useChat({
    activeModelType,
    serverRunning,
    selectedModel,
    selectedCloudModel,
    appSettings,
    attachments,
    localModels,
    wslModels,
    backend,
    wslStatus,
    providerStatuses,
    systemInfo,
    liveMetrics,
    vramCompatibility,
    modelSettings,
    setError,
  });

  // Remote channel bridge — when this pane is the active remote target,
  // register its sendMessage so Telegram/Discord/Worker/Routine messages appear here.
  // Uses refs (not closures) to avoid stale function references.
  useEffect(() => {
    if (isRemoteTarget) {
      remoteSendRef.current = async (text?: string) => {
        // Copy the external origin (set by useRemoteChannels) into this pane's useChat
        chatInstance.messageOriginRef.current = remoteOriginRef.current;
        // Delegate to the pane's sendMessage via ref (always current)
        return chatInstance.sendMessageRef.current?.(text);
      };
    }
    return () => {
      // Clean up when this pane stops being the remote target
      if (isRemoteTarget) {
        remoteSendRef.current = undefined;
      }
    };
  }, [isRemoteTarget, remoteSendRef, remoteOriginRef, chatInstance.messageOriginRef, chatInstance.sendMessageRef]);

  // Per-pane conversation manager
  const {
    conversations, currentConversationId,
    startNewConversation, loadConversation, deleteConversation,
  } = useConversationManager({
    messages: chatInstance.messages,
    setMessages: chatInstance.setMessages,
    isGenerating: chatInstance.isGenerating,
    resetChat: chatInstance.resetChat,
    chatPersistence: appSettings.chatPersistence,
    memoryEnabled: appSettings.memoryEnabled,
    selectedModel,
    selectedCloudModel,
  });

  return (
    <div className="h-full flex flex-col overflow-hidden">
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
      <div className="flex-1 overflow-hidden">
        <ChatTab
          messages={chatInstance.messages}
          input={chatInstance.input}
          setInput={chatInstance.setInput}
          isGenerating={chatInstance.isGenerating}
          streamingContent={chatInstance.streamingContent}
          streamingThinking={chatInstance.streamingThinking}
          routingSpecialist={chatInstance.routingSpecialist}
          messagesEndRef={chatInstance.messagesEndRef}
          serverRunning={serverRunning}
          selectedModel={selectedModel}
          selectedCloudModel={selectedCloudModel}
          activeModelType={activeModelType}
          chatPersistence={appSettings.chatPersistence}
          conversations={conversations}
          currentConversationId={currentConversationId}
          memoryEnabled={appSettings.memoryEnabled}
          onSendMessage={chatInstance.sendMessage}
          onStopGeneration={chatInstance.stopGeneration}
          onStartNewConversation={startNewConversation}
          onLoadConversation={loadConversation}
          onDeleteConversation={deleteConversation}
          onClearMessages={chatInstance.resetChat}
          onSetTab={onSetTab}
          toolsEnabled={chatInstance.toolsEnabled}
          onToggleTools={chatInstance.setToolsEnabled}
          availableTools={chatInstance.availableTools}
          pendingToolCalls={chatInstance.pendingToolCalls}
          onToolApproval={chatInstance.toolApprovalCallback}
          attachments={attachments}
          onAttachmentsChange={setAttachments}
          harnessEnabled={appSettings.harnessEnabled}
          lastHarnessContext={chatInstance.lastHarnessContext}
        />
      </div>
    </div>
  );
}

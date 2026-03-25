import { RefObject, useState, useEffect, useRef } from 'react';
import { X, Send, Square, Trash2, Monitor, Wrench, ChevronDown, ChevronRight, Shield, Brain, Download, Upload, Zap, Paperclip, File, Cpu, ArrowUpRight } from 'lucide-react';
import * as api from '../lib/api';
import { Tab, Message, ToolCall, ToolSchema, HarnessContext } from '../types';
import MemoryPanel from './MemoryPanel';
import WorkerPanel from './WorkerPanel';

interface Props {
  // Chat state
  messages: Message[];
  input: string;
  setInput: (s: string) => void;
  isGenerating: boolean;
  streamingContent: string;
  streamingThinking: string;
  routingSpecialist: string | null;
  messagesEndRef: RefObject<HTMLDivElement>;

  // Model state
  serverRunning: boolean;
  selectedModel: api.LocalModel | null;
  selectedCloudModel: { provider: api.ProviderType; model: api.ProviderModel } | null;
  activeModelType: 'local' | 'cloud';

  // Conversation persistence & memory
  chatPersistence: boolean;
  conversations: api.Conversation[];
  currentConversationId: string | null;
  memoryEnabled: boolean;

  // Tool framework
  toolsEnabled: boolean;
  onToggleTools: (enabled: boolean) => void;
  availableTools: ToolSchema[];
  pendingToolCalls: ToolCall[];
  onToolApproval: ((approved: boolean) => void) | null;

  // Actions
  onSendMessage: () => void;
  onStopGeneration: () => void;
  onStartNewConversation: () => void;
  onLoadConversation: (id: string) => void;
  onDeleteConversation: (id: string) => void;
  onClearMessages: () => void;
  onSetTab: (tab: Tab) => void;

  // File attachments
  attachments: api.FileAttachment[];
  onAttachmentsChange: (attachments: api.FileAttachment[]) => void;

  // Cognitive Harness
  harnessEnabled: boolean;
  lastHarnessContext: HarnessContext | null;
}

/** Render a collapsible thinking/reasoning block (P8: hidden by default, expandable for power users) */
function ThinkingBlock({ thinking }: { thinking: string }) {
  const [expanded, setExpanded] = useState(false);
  const lines = thinking.split('\n').length;
  const preview = thinking.substring(0, 80).replace(/\n/g, ' ');

  return (
    <div className="mb-2">
      <button
        onClick={() => setExpanded(!expanded)}
        className="flex items-center gap-1.5 text-xs text-zinc-500 hover:text-zinc-300 transition-colors"
      >
        {expanded ? <ChevronDown className="w-3 h-3" /> : <ChevronRight className="w-3 h-3" />}
        <Brain className="w-3 h-3 text-purple-400" />
        <span>Thinking ({lines} lines)</span>
        {!expanded && <span className="text-zinc-600 truncate max-w-[200px]">— {preview}...</span>}
      </button>
      {expanded && (
        <pre className="mt-1 px-3 py-2 bg-zinc-900/60 border border-zinc-700/50 rounded-lg text-xs text-zinc-400 whitespace-pre-wrap max-h-[300px] overflow-y-auto">
          {thinking}
        </pre>
      )}
    </div>
  );
}

/** Render a tool call block (collapsible) */
function ToolCallBlock({ tc, tools }: { tc: ToolCall; tools: ToolSchema[] }) {
  const [expanded, setExpanded] = useState(false);
  const schema = tools.find(t => t.name === tc.name);
  const riskColor = schema?.risk_level === 'high' || schema?.risk_level === 'critical'
    ? 'text-red-400 border-red-500/30'
    : schema?.risk_level === 'medium'
    ? 'text-amber-400 border-amber-500/30'
    : 'text-emerald-400 border-emerald-500/30';

  return (
    <div className={`mt-2 border rounded-lg ${riskColor} bg-zinc-900/50`}>
      <button
        onClick={() => setExpanded(!expanded)}
        className="w-full flex items-center gap-2 px-3 py-1.5 text-sm"
      >
        {expanded ? <ChevronDown className="w-3 h-3" /> : <ChevronRight className="w-3 h-3" />}
        <Wrench className="w-3 h-3" />
        <span className="font-mono">{tc.name}</span>
        {schema && (
          <span className="text-xs opacity-60">({schema.risk_level})</span>
        )}
      </button>
      {expanded && (
        <div className="px-3 pb-2 text-xs">
          <pre className="text-zinc-400 whitespace-pre-wrap overflow-x-auto">
            {JSON.stringify(tc.arguments, null, 2)}
          </pre>
        </div>
      )}
    </div>
  );
}

/** Render a tool result block */
function ToolResultBlock({ msg }: { msg: Message }) {
  const [expanded, setExpanded] = useState(false);
  const preview = msg.content.length > 120
    ? msg.content.substring(0, 120) + '...'
    : msg.content;

  return (
    <div className="flex justify-start">
      <div className="max-w-[80%] rounded-xl px-4 py-2 bg-zinc-800/60 border border-zinc-700 text-zinc-300 text-sm">
        <div className="flex items-center gap-2 text-xs text-zinc-500 mb-1">
          <Wrench className="w-3 h-3" />
          <span className="font-mono">{msg.toolName || 'tool'}</span>
          <span>result</span>
          {msg.content.length > 120 && (
            <button
              onClick={() => setExpanded(!expanded)}
              className="text-amber-400 hover:text-amber-300 ml-auto"
            >
              {expanded ? 'collapse' : 'expand'}
            </button>
          )}
        </div>
        <pre className="whitespace-pre-wrap font-mono text-xs">
          {expanded ? msg.content : preview}
        </pre>
      </div>
    </div>
  );
}

export default function ChatTab({
  messages, input, setInput, isGenerating, streamingContent, streamingThinking, routingSpecialist, messagesEndRef,
  serverRunning, selectedModel, selectedCloudModel, activeModelType,
  chatPersistence, conversations, currentConversationId, memoryEnabled,
  toolsEnabled, onToggleTools, availableTools, pendingToolCalls, onToolApproval,
  onSendMessage, onStopGeneration, onStartNewConversation,
  onLoadConversation, onDeleteConversation, onClearMessages, onSetTab,
  attachments, onAttachmentsChange,
  harnessEnabled, lastHarnessContext,
}: Props) {
  // Active model display name — used for streaming name tag and fallback for untagged messages
  const activeModelName = activeModelType === 'cloud' && selectedCloudModel
    ? selectedCloudModel.model.name || selectedCloudModel.model.id
    : selectedModel?.filename?.replace(/\.gguf$/i, '') || 'Local Model';

  const [memoryStats, setMemoryStats] = useState<api.MemoryStats | null>(null);
  const [showMemoryPanel, setShowMemoryPanel] = useState(false);
  const [showWorkerPanel, setShowWorkerPanel] = useState(false);
  const streamingStartRef = useRef<number>(0);
  const streamingContentRef = useRef<string>('');  // B5 fix: ref for interval closure
  const [tokensPerSec, setTokensPerSec] = useState<string | null>(null);
  const speedIntervalRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const speedClearTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const importInputRef = useRef<HTMLInputElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // Auto-resize textarea
  function autoResize() {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = 'auto';
    el.style.height = Math.min(el.scrollHeight, 200) + 'px';
  }

  // Drag-and-drop file upload state
  const [isDragOver, setIsDragOver] = useState(false);

  async function handleFilesUpload(files: FileList | File[]) {
    const MAX_FILE_SIZE = 50 * 1024 * 1024; // 50 MB per file
    const fileArray = Array.from(files);
    const newAttachments: api.FileAttachment[] = [];
    for (const file of fileArray) {
      if (file.size > MAX_FILE_SIZE) {
        console.warn(`[HIVE] Skipping "${file.name}" — ${(file.size / 1048576).toFixed(0)} MB exceeds 50 MB limit`);
        continue;
      }
      try {
        const buffer = await file.arrayBuffer();
        const data = Array.from(new Uint8Array(buffer));
        const savedPath = await api.saveAttachment(file.name, data);
        newAttachments.push({
          name: file.name,
          path: savedPath,
          size: file.size,
          type: file.type || 'application/octet-stream',
        });
      } catch (err) {
        console.error(`[HIVE] Failed to save attachment "${file.name}":`, err);
      }
    }
    if (newAttachments.length > 0) {
      onAttachmentsChange([...attachments, ...newAttachments]);
    }
  }

  function handleDragOver(e: React.DragEvent) {
    e.preventDefault();
    e.stopPropagation();
    setIsDragOver(true);
  }

  function handleDragLeave(e: React.DragEvent) {
    e.preventDefault();
    e.stopPropagation();
    setIsDragOver(false);
  }

  function handleDrop(e: React.DragEvent) {
    e.preventDefault();
    e.stopPropagation();
    setIsDragOver(false);
    if (e.dataTransfer.files.length > 0) {
      handleFilesUpload(e.dataTransfer.files);
    }
  }

  function removeAttachment(index: number) {
    onAttachmentsChange(attachments.filter((_, i) => i !== index));
  }

  // Load memory stats when enabled
  useEffect(() => {
    if (memoryEnabled) {
      api.memoryStats().then(setMemoryStats).catch(() => {});
    }
  }, [memoryEnabled, messages.length]);

  // Track streaming speed (tokens/sec)
  // Keep ref in sync so interval callback reads current value (B5 fix — stale closure)
  streamingContentRef.current = streamingContent;
  useEffect(() => {
    if (isGenerating && streamingContent) {
      if (!streamingStartRef.current) {
        streamingStartRef.current = Date.now();
      }
      // Update speed every 500ms during generation
      if (!speedIntervalRef.current) {
        speedIntervalRef.current = setInterval(() => {
          const elapsed = (Date.now() - streamingStartRef.current) / 1000;
          const currentContent = streamingContentRef.current;
          if (elapsed > 0.5 && currentContent.length > 0) {
            const tokens = api.estimateTokens(currentContent);
            setTokensPerSec((tokens / elapsed).toFixed(1));
          }
        }, 500);
      }
    } else {
      // Clean up on generation end
      if (speedIntervalRef.current) {
        clearInterval(speedIntervalRef.current);
        speedIntervalRef.current = null;
      }
      if (!isGenerating) {
        streamingStartRef.current = 0;
        // Keep the last speed reading for a moment, then clear
        if (tokensPerSec) {
          speedClearTimeoutRef.current = setTimeout(() => setTokensPerSec(null), 3000);
        }
      }
    }
    return () => {
      if (speedIntervalRef.current) {
        clearInterval(speedIntervalRef.current);
        speedIntervalRef.current = null;
      }
      if (speedClearTimeoutRef.current) {
        clearTimeout(speedClearTimeoutRef.current);
        speedClearTimeoutRef.current = null;
      }
    };
  }, [isGenerating, streamingContent]);

  // Estimated tokens for current input
  const inputTokens = input.trim() ? api.estimateTokens(input) : 0;



  function handleExport() {
    const json = api.exportConversationsToJson();
    const blob = new Blob([json], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `hive-conversations-${new Date().toISOString().slice(0, 10)}.json`;
    a.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }

  function handleImport(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => {
      try {
        const count = api.importConversationsFromJson(reader.result as string);
        console.log(`[HIVE] Imported ${count} conversations`);
        // Trigger conversation list refresh via a small state nudge
        onStartNewConversation();
      } catch (err) {
        console.error('[HIVE] Import failed:', err);
      }
    };
    reader.readAsText(file);
    // Reset input so same file can be re-imported
    e.target.value = '';
  }

  return (
    <div className="h-full flex">
      {/* Conversation Sidebar (when persistence is enabled) */}
      {chatPersistence && (
        <div className="w-56 border-r border-zinc-700 flex flex-col bg-zinc-900/50">
          <div className="p-2">
            <button
              onClick={onStartNewConversation}
              className="w-full px-3 py-2 bg-amber-500 hover:bg-amber-600 text-black text-sm rounded-lg font-medium"
            >
              + New Chat
            </button>
          </div>
          <div className="flex-1 overflow-y-auto">
            {conversations.map(conv => (
              <div
                key={conv.id}
                className={`group flex items-center gap-1 px-3 py-2 cursor-pointer text-sm border-b border-zinc-800 ${
                  currentConversationId === conv.id
                    ? 'bg-zinc-700/50 text-white'
                    : 'text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200'
                }`}
                onClick={() => onLoadConversation(conv.id)}
              >
                <span className="flex-1 truncate">{conv.title}</span>
                <button
                  onClick={(e) => { e.stopPropagation(); onDeleteConversation(conv.id); }}
                  className="opacity-0 group-hover:opacity-100 text-zinc-500 hover:text-red-400 p-0.5"
                  title="Delete conversation"
                >
                  <X className="w-3 h-3" />
                </button>
              </div>
            ))}
            {conversations.length === 0 && (
              <p className="text-zinc-600 text-xs p-3">No saved conversations</p>
            )}
          </div>
          {/* Export / Import */}
          <div className="p-2 border-t border-zinc-800 flex gap-1">
            <button
              onClick={handleExport}
              disabled={conversations.length === 0}
              className="flex-1 flex items-center justify-center gap-1.5 px-2 py-1.5 text-xs text-zinc-400 hover:text-white hover:bg-zinc-800 disabled:opacity-30 rounded"
              title="Export conversations"
            >
              <Download className="w-3 h-3" />
              Export
            </button>
            <button
              onClick={() => importInputRef.current?.click()}
              className="flex-1 flex items-center justify-center gap-1.5 px-2 py-1.5 text-xs text-zinc-400 hover:text-white hover:bg-zinc-800 rounded"
              title="Import conversations"
            >
              <Upload className="w-3 h-3" />
              Import
            </button>
            <input
              ref={importInputRef}
              type="file"
              accept=".json"
              onChange={handleImport}
              className="hidden"
            />
          </div>
        </div>
      )}

      {/* Chat Main Area */}
      <div
        className={`flex-1 flex flex-col relative ${isDragOver ? 'ring-2 ring-amber-500/50' : ''}`}
        onDragOver={handleDragOver}
        onDragLeave={handleDragLeave}
        onDrop={handleDrop}
      >
        {/* Drag overlay */}
        {isDragOver && (
          <div className="absolute inset-0 z-50 bg-zinc-900/80 flex items-center justify-center pointer-events-none">
            <div className="text-amber-400 text-lg font-medium flex items-center gap-2">
              <Upload className="w-6 h-6" />
              Drop files to attach
            </div>
          </div>
        )}
        {/* Check if any model is ready */}
        {!serverRunning && !selectedCloudModel ? (
          <div className="flex-1 flex items-center justify-center">
            <div className="text-center text-zinc-500">
              <p className="mb-4">No model selected.</p>
              <button
                onClick={() => onSetTab('models')}
                className="text-amber-400 hover:underline"
              >
                Go to Models tab to select one
              </button>
            </div>
          </div>
        ) : (
          <>
            {/* Active Model Header */}
            <div className="px-4 py-2 bg-zinc-800/50 border-b border-zinc-700 flex items-center justify-between">
              {activeModelType === 'cloud' && selectedCloudModel ? (
                <div className="flex items-center gap-2">
                  <span>{api.getProviderInfo(selectedCloudModel.provider).icon}</span>
                  <span className={api.getProviderInfo(selectedCloudModel.provider).color + ' font-medium'}>
                    {selectedCloudModel.model.name}
                  </span>
                  <span className="text-xs bg-zinc-700 px-2 py-0.5 rounded text-zinc-400">Cloud</span>
                </div>
              ) : selectedModel ? (
                <div className="flex items-center gap-2">
                  <Monitor className="w-4 h-4 text-green-400" />
                  <span className="text-white font-medium">{selectedModel.filename}</span>
                  <span className="text-xs bg-zinc-700 px-2 py-0.5 rounded text-zinc-400">Local</span>
                </div>
              ) : (
                <span className="text-zinc-500">No model</span>
              )}
              <div className="flex items-center gap-3">
                {/* Harness indicator */}
                {harnessEnabled && (
                  <div
                    className="flex items-center gap-1.5 text-xs px-2.5 py-1 rounded-lg border bg-cyan-500/15 text-cyan-400 border-cyan-500/30"
                    title={lastHarnessContext
                      ? `Harness: ${lastHarnessContext.identity_source} | ${lastHarnessContext.tool_count} tools | memory: ${lastHarnessContext.memory_status}`
                      : 'Cognitive harness active'
                    }
                  >
                    <Cpu className="w-3 h-3" />
                    <span>HIVE</span>
                  </div>
                )}
                {/* Memory indicator (clickable to open panel) */}
                {memoryEnabled && (
                  <button
                    onClick={() => setShowMemoryPanel(true)}
                    className="flex items-center gap-1.5 text-xs px-2.5 py-1 rounded-lg border bg-purple-500/15 text-purple-400 border-purple-500/30 hover:bg-purple-500/25 transition-colors"
                    title={memoryStats
                      ? `Memory: ${memoryStats.total_memories} memories — click to manage`
                      : 'Memory active — click to manage'
                    }
                  >
                    <Brain className="w-3 h-3" />
                    <span>Memory{memoryStats ? ` (${memoryStats.total_memories})` : ''}</span>
                  </button>
                )}
                {/* Workers indicator */}
                {toolsEnabled && (
                  <button
                    onClick={() => setShowWorkerPanel(true)}
                    className="flex items-center gap-1.5 text-xs px-2.5 py-1 rounded-lg border bg-blue-500/10 text-blue-400 border-blue-500/25 hover:bg-blue-500/20 transition-colors"
                    title="View worker sub-agents"
                  >
                    <Cpu className="w-3 h-3" />
                    <span>Workers</span>
                  </button>
                )}
                {/* Tools toggle */}
                <button
                  onClick={() => onToggleTools(!toolsEnabled)}
                  className={`flex items-center gap-1.5 text-xs px-2.5 py-1 rounded-lg border transition-colors ${
                    toolsEnabled
                      ? 'bg-amber-500/15 text-amber-400 border-amber-500/30'
                      : 'bg-zinc-800 text-zinc-500 border-zinc-700 hover:text-zinc-300'
                  }`}
                  title={toolsEnabled
                    ? `Tools ON (${availableTools.length} available)`
                    : 'Enable tool use (file ops, commands, web)'
                  }
                >
                  <Wrench className="w-3 h-3" />
                  <span>Tools {toolsEnabled ? 'ON' : 'OFF'}</span>
                </button>
                <button
                  onClick={() => onSetTab('models')}
                  className="text-xs text-zinc-400 hover:text-white"
                >
                  Change
                </button>
              </div>
            </div>

            {/* Messages */}
            <div className="flex-1 overflow-y-auto p-4 space-y-4">
              {messages.length === 0 && !streamingContent && (
                <div className="h-full flex items-center justify-center text-zinc-500">
                  Start a conversation{toolsEnabled ? ' (tools enabled)' : ''}
                </div>
              )}
              {messages.map((msg, i) => {
                const msgKey = msg.id || `msg-${i}`;
                // Tool result messages get special rendering
                if (msg.role === 'tool') {
                  return <ToolResultBlock key={msgKey} msg={msg} />;
                }

                // Sender identity tag — name + channel indicator (P1: metadata, not content parsing)
                const isUser = msg.role === 'user';
                const senderLabel = msg.senderName || (isUser ? 'You' : 'Assistant');
                const channelBadge = msg.senderChannel && msg.senderChannel !== 'hive'
                  ? msg.senderChannel.charAt(0).toUpperCase() + msg.senderChannel.slice(1)
                  : null;

                return (
                  <div
                    key={msgKey}
                    className={`flex flex-col ${isUser ? 'items-end' : 'items-start'}`}
                  >
                    {/* Sender name tag */}
                    <div className={`flex items-center gap-1.5 mb-0.5 px-1 ${isUser ? 'flex-row-reverse' : ''}`}>
                      <span className={`text-xs font-medium ${isUser ? 'text-amber-400/70' : 'text-zinc-500'}`}>
                        {senderLabel}
                      </span>
                      {channelBadge && (
                        <span className="text-[10px] px-1.5 py-0.5 rounded bg-zinc-700/50 text-zinc-400">
                          {channelBadge}
                        </span>
                      )}
                    </div>
                    <div
                      className={`max-w-[80%] rounded-xl px-4 py-3 ${
                        isUser
                          ? 'bg-amber-500 text-black'
                          : 'bg-zinc-800 text-zinc-100'
                      }`}
                    >
                      {/* Show collapsible thinking block if present (P8: hidden by default) */}
                      {msg.thinking && <ThinkingBlock thinking={msg.thinking} />}
                      {/* Hide empty text when tool calls are present — tool blocks show what's happening */}
                      {(msg.content.trim() || !msg.toolCalls?.length) && (
                        <pre className="whitespace-pre-wrap font-sans">{msg.content}</pre>
                      )}
                      {/* Show tool call blocks if present */}
                      {msg.toolCalls && msg.toolCalls.length > 0 && (
                        <div className="mt-2">
                          {msg.toolCalls.map((tc) => (
                            <ToolCallBlock key={tc.id} tc={tc} tools={availableTools} />
                          ))}
                        </div>
                      )}
                    </div>
                  </div>
                );
              })}
              {/* Live streaming thinking indicator */}
              {streamingThinking && !streamingContent && (
                <div className="flex flex-col items-start">
                  <span className="text-xs font-medium text-zinc-500 mb-0.5 px-1">{activeModelName}</span>
                  <div className="max-w-[80%] rounded-xl px-3 py-2 bg-zinc-800/60 border border-purple-500/20">
                    <div className="flex items-center gap-1.5 text-xs text-purple-400">
                      <Brain className="w-3 h-3 animate-pulse" />
                      <span>Reasoning...</span>
                    </div>
                  </div>
                </div>
              )}
              {streamingContent && (
                <div className="flex flex-col items-start">
                  <span className="text-xs font-medium text-zinc-500 mb-0.5 px-1">{activeModelName}</span>
                  <div className="max-w-[80%] rounded-xl px-4 py-3 bg-zinc-800 text-zinc-100">
                    {/* Show live thinking as collapsible if present */}
                    {streamingThinking && <ThinkingBlock thinking={streamingThinking} />}
                    {/* Phase 4 C5: Routing indicator for specialist delegation */}
                    {routingSpecialist && (
                      <div className="flex items-center gap-2 mb-2 px-2 py-1 bg-amber-500/10 border border-amber-500/30 rounded-lg text-amber-400 text-xs">
                        <ArrowUpRight className="w-3.5 h-3.5 animate-pulse" />
                        <span className="font-medium">{streamingContent || `Routing to ${routingSpecialist}...`}</span>
                      </div>
                    )}
                    {!routingSpecialist && (
                      <pre className="whitespace-pre-wrap font-sans">{streamingContent}</pre>
                    )}
                    <span className="inline-block w-2 h-4 bg-amber-500 animate-pulse ml-1" />
                    {/* Tokens/sec display */}
                    {tokensPerSec && (
                      <div className="flex items-center gap-1 mt-2 text-xs text-zinc-500">
                        <Zap className="w-3 h-3 text-amber-400" />
                        <span>{tokensPerSec} tok/s</span>
                      </div>
                    )}
                  </div>
                </div>
              )}
              <div ref={messagesEndRef} />
            </div>

            {/* Tool Approval Modal */}
            {pendingToolCalls.length > 0 && onToolApproval && (
              <div className="mx-4 mb-2 p-4 bg-zinc-800 border border-amber-500/40 rounded-xl">
                <div className="flex items-center gap-2 text-amber-400 mb-3">
                  <Shield className="w-4 h-4" />
                  <span className="font-medium text-sm">Tool Approval Required</span>
                </div>
                <div className="space-y-2 mb-3">
                  {pendingToolCalls.map((tc) => {
                    const schema = availableTools.find(t => t.name === tc.name);
                    return (
                      <div key={tc.id} className="text-sm">
                        <span className="font-mono text-amber-300">{tc.name}</span>
                        <span className="text-zinc-500 text-xs ml-2">
                          ({schema?.risk_level || 'unknown'} risk)
                        </span>
                        <pre className="text-xs text-zinc-400 mt-1 whitespace-pre-wrap">
                          {JSON.stringify(tc.arguments, null, 2)}
                        </pre>
                      </div>
                    );
                  })}
                </div>
                <div className="flex gap-2">
                  <button
                    onClick={() => onToolApproval(true)}
                    className="px-4 py-1.5 bg-emerald-600 hover:bg-emerald-500 text-white text-sm rounded-lg"
                  >
                    Approve
                  </button>
                  <button
                    onClick={() => onToolApproval(false)}
                    className="px-4 py-1.5 bg-zinc-700 hover:bg-zinc-600 text-white text-sm rounded-lg"
                  >
                    Deny
                  </button>
                </div>
              </div>
            )}

            {/* Input */}
            <div className="p-4 border-t border-zinc-800">
              {/* Attachment bar */}
              {attachments.length > 0 && (
                <div className="flex flex-wrap gap-2 mb-2">
                  {attachments.map((att, i) => (
                    <div key={att.path} className="flex items-center gap-1.5 bg-zinc-800 border border-zinc-700 rounded-lg px-2.5 py-1 text-xs text-zinc-300">
                      <File className="w-3 h-3 text-amber-400" />
                      <span className="max-w-[120px] truncate">{att.name}</span>
                      <span className="text-zinc-500">({(att.size / 1024).toFixed(0)}KB)</span>
                      <button
                        onClick={() => removeAttachment(i)}
                        className="text-zinc-500 hover:text-red-400 ml-0.5"
                        title="Remove attachment"
                      >
                        <X className="w-3 h-3" />
                      </button>
                    </div>
                  ))}
                </div>
              )}
              <div className="flex gap-2">
                {/* Paperclip button */}
                <button
                  onClick={() => fileInputRef.current?.click()}
                  className="px-3 py-3 bg-zinc-800 hover:bg-zinc-700 text-zinc-400 hover:text-amber-400 rounded-xl border border-zinc-700 transition-colors"
                  title="Attach files"
                >
                  <Paperclip className="w-5 h-5" />
                </button>
                <input
                  ref={fileInputRef}
                  type="file"
                  multiple
                  className="hidden"
                  onChange={(e) => {
                    if (e.target.files && e.target.files.length > 0) {
                      handleFilesUpload(e.target.files);
                      e.target.value = '';
                    }
                  }}
                />
                <div className="flex-1 relative">
                  <textarea
                    ref={textareaRef}
                    value={input}
                    onChange={(e) => { setInput(e.target.value); autoResize(); }}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter' && !e.shiftKey) {
                        e.preventDefault();
                        onSendMessage();
                      }
                      // Shift+Enter naturally inserts a newline in textarea
                    }}
                    placeholder={toolsEnabled ? "Type a message (tools enabled)..." : "Type a message... (Shift+Enter for new line)"}
                    disabled={isGenerating}
                    rows={1}
                    className="w-full bg-zinc-800 text-white px-4 py-3 pr-20 rounded-xl border border-zinc-700 focus:border-amber-500 outline-none disabled:opacity-50 resize-none overflow-y-auto"
                    style={{ maxHeight: '200px' }}
                  />
                  {/* Token counter */}
                  {inputTokens > 0 && (
                    <span className="absolute right-3 bottom-3 text-xs text-zinc-500">
                      ~{inputTokens} tok
                    </span>
                  )}
                </div>
                {isGenerating ? (
                  <button
                    onClick={onStopGeneration}
                    className="px-4 py-3 bg-red-500 hover:bg-red-600 text-white rounded-xl"
                    title="Stop generation"
                  >
                    <Square className="w-5 h-5" />
                  </button>
                ) : (
                  <button
                    onClick={onSendMessage}
                    disabled={!input.trim()}
                    className="px-4 py-3 bg-amber-500 hover:bg-amber-600 disabled:opacity-50 text-black rounded-xl"
                  >
                    <Send className="w-5 h-5" />
                  </button>
                )}
                <button
                  onClick={() => {
                    if (chatPersistence) {
                      onStartNewConversation();
                    } else {
                      onClearMessages();
                    }
                  }}
                  className="px-4 py-3 bg-zinc-700 hover:bg-zinc-600 text-white rounded-xl"
                >
                  <Trash2 className="w-5 h-5" />
                </button>
              </div>
            </div>
          </>
        )}
      </div>

      {/* Memory Management Panel */}
      <MemoryPanel
        isOpen={showMemoryPanel}
        onClose={() => {
          setShowMemoryPanel(false);
          // Refresh stats after closing
          if (memoryEnabled) {
            api.memoryStats().then(setMemoryStats).catch(() => {});
          }
        }}
      />

      {/* Worker Status Panel */}
      <WorkerPanel
        isOpen={showWorkerPanel}
        onClose={() => setShowWorkerPanel(false)}
      />
    </div>
  );
}

import { useState, useEffect, useRef } from 'react';
import * as api from '../lib/api';
import type { Message } from '../types';

interface UseConversationManagerProps {
  messages: Message[];
  setMessages: (msgs: Message[]) => void;
  isGenerating: boolean;
  resetChat: () => void;
  chatPersistence: boolean;
  memoryEnabled: boolean;
  selectedModel: api.LocalModel | null;
  selectedCloudModel: { provider: api.ProviderType; model: api.ProviderModel } | null;
}

export function useConversationManager({
  messages, setMessages, isGenerating, resetChat,
  chatPersistence, memoryEnabled,
  selectedModel, selectedCloudModel,
}: UseConversationManagerProps) {
  const [conversations, setConversations] = useState<api.Conversation[]>([]);
  const [currentConversationId, setCurrentConversationId] = useState<string | null>(null);

  // Memory flush on app close — refs avoid stale closure in beforeunload handler
  const memoryFlushRef = useRef<{
    enabled: boolean;
    messages: Message[];
    conversationId: string | null;
    modelId: string;
  }>({ enabled: false, messages: [], conversationId: null, modelId: 'unknown' });

  // Load conversations list when persistence setting changes
  useEffect(() => {
    if (chatPersistence) {
      setConversations(api.getConversations());
      const lastId = api.getCurrentConversationId();
      if (lastId) {
        const conv = api.getConversation(lastId);
        if (conv) {
          setCurrentConversationId(lastId);
          setMessages(conv.messages.map(m => ({
            role: m.role as 'user' | 'assistant' | 'tool',
            content: m.content,
            ...(m.thinking && { thinking: m.thinking }),
            ...(m.tool_call_id && { toolCallId: m.tool_call_id }),
            ...(m.tool_calls && { toolCalls: m.tool_calls.map(tc => ({
              id: tc.id,
              name: tc.function.name,
              arguments: typeof tc.function.arguments === 'string'
                ? JSON.parse(tc.function.arguments || '{}')
                : tc.function.arguments,
            })) }),
          })));
        }
      }
    }
  }, [chatPersistence]);

  // Auto-save conversation when messages change
  useEffect(() => {
    if (!chatPersistence || messages.length === 0 || isGenerating) return;

    const modelId = selectedModel?.filename || selectedCloudModel?.model.id || 'unknown';
    const now = new Date().toISOString();

    let convId = currentConversationId;
    if (!convId) {
      convId = api.generateConversationId();
      setCurrentConversationId(convId);
    }

    const conversation: api.Conversation = {
      id: convId,
      title: api.generateConversationTitle(messages.map(m => ({ ...m, role: m.role as 'user' | 'assistant' | 'system' }))),
      messages: messages.map(m => ({
        role: m.role as 'user' | 'assistant' | 'system' | 'tool',
        content: m.content,
        ...(m.thinking && { thinking: m.thinking }),
        ...(m.toolCallId && { tool_call_id: m.toolCallId }),
        ...(m.toolCalls && { tool_calls: m.toolCalls.map(tc => ({
          id: tc.id,
          type: 'function' as const,
          function: {
            name: tc.name,
            arguments: typeof tc.arguments === 'string' ? tc.arguments : JSON.stringify(tc.arguments),
          },
        })) }),
      })),
      modelId,
      createdAt: api.getConversation(convId)?.createdAt || now,
      updatedAt: now,
    };

    api.saveConversation(conversation);
    api.setCurrentConversationId(convId);
    setConversations(api.getConversations());
  }, [messages, isGenerating, chatPersistence, selectedModel, selectedCloudModel]);

  // Keep memory flush ref in sync so beforeunload handler has fresh data
  useEffect(() => {
    memoryFlushRef.current = {
      enabled: memoryEnabled,
      messages,
      conversationId: currentConversationId,
      modelId: selectedModel?.filename || selectedCloudModel?.model.id || 'unknown',
    };
  }, [messages, currentConversationId, memoryEnabled, selectedModel, selectedCloudModel]);

  // Flush conversation to memory when app/tab closes
  useEffect(() => {
    const handleBeforeUnload = () => {
      const { enabled, messages: msgs, conversationId, modelId } = memoryFlushRef.current;
      if (enabled && msgs.length > 2 && conversationId) {
        const chatMsgs: api.ChatMessage[] = msgs.map(m => ({
          role: m.role as 'user' | 'assistant',
          content: m.content,
        }));
        // Fire-and-forget — we can't await in beforeunload but the Tauri
        // command will execute synchronously on the Rust side before shutdown.
        api.memoryExtractAndSave(conversationId, modelId, chatMsgs).catch(() => {});
      }
    };
    window.addEventListener('beforeunload', handleBeforeUnload);
    return () => window.removeEventListener('beforeunload', handleBeforeUnload);
  }, []);

  function startNewConversation() {
    // Flush current conversation to memory before clearing (OpenClaw pre-compaction pattern)
    if (memoryEnabled && messages.length > 2 && currentConversationId) {
      const modelId = selectedModel?.filename || selectedCloudModel?.model.id || 'unknown';
      const chatMsgs: api.ChatMessage[] = messages.map(m => ({ role: m.role as 'user' | 'assistant', content: m.content }));
      api.memoryExtractAndSave(currentConversationId, modelId, chatMsgs)
        .then(saved => {
          if (saved.length > 0) {
            console.log(`[HIVE] Memory: Saved ${saved.length} memories from conversation`);
          }
        })
        .catch(err => console.warn('[HIVE] Memory flush failed (non-fatal):', err));
    }

    // Phase 3.5: Flush working memory to short-term before starting fresh session
    api.workingMemoryFlush()
      .then(record => {
        if (record) console.log('[HIVE] Working memory flushed to short-term memory');
      })
      .catch(err => console.warn('[HIVE] Working memory flush failed (non-fatal):', err));

    resetChat();
    setCurrentConversationId(null);
    api.setCurrentConversationId(null);
  }

  function loadConversation(id: string) {
    const conv = api.getConversation(id);
    if (conv) {
      setCurrentConversationId(id);
      setMessages(conv.messages.map(m => ({
        role: m.role as 'user' | 'assistant' | 'tool',
        content: m.content,
        ...(m.thinking && { thinking: m.thinking }),
        ...(m.tool_call_id && { toolCallId: m.tool_call_id }),
        ...(m.tool_calls && { toolCalls: m.tool_calls.map(tc => ({
          id: tc.id,
          name: tc.function.name,
          arguments: typeof tc.function.arguments === 'string'
            ? JSON.parse(tc.function.arguments || '{}')
            : tc.function.arguments,
        })) }),
      })));
      api.setCurrentConversationId(id);
    }
  }

  function deleteConversation(id: string) {
    api.deleteConversation(id);
    setConversations(api.getConversations());
    if (currentConversationId === id) {
      startNewConversation();
    }
  }

  return {
    conversations,
    currentConversationId,
    startNewConversation,
    loadConversation,
    deleteConversation,
  };
}

// useChat.ts — Chat logic extracted from App.tsx
// Contains: sendMessage, stopGeneration, tool loop, harness build, streaming

import { useState, useEffect, useRef } from 'react';
import * as api from './lib/api';
import { SPECIALIST_PORTS } from './types';
import type { Backend, Message, ToolSchema, ToolCall, HarnessContext, SlotConfig, MessageOrigin } from './types';

// ============================================
// Pure functions extracted to chainPolicies.ts (Phase 4A — P1: Modularity)
// Re-exported here for backward compatibility with existing imports.
// ============================================
import {
  substitutePlanVariables,
  evaluatePlanCondition,
  detectRepetition,
  classifyToolCalls,
  isChainComplete,
  detectExternalChannel,
  TERMINAL_TOOLS,
  REPETITION_INITIAL,
  buildVolatileContext,
  computeToolResultMaxChars,
  formatToolResult,
  shouldSaveProcedure,
  buildProcedureData,
  classifyError,
  parseModelFilename,
} from './lib/chainPolicies';
import type { PlanStep, RepetitionState, ChainHistoryEntry } from './lib/chainPolicies';

// Stable message ID generator — avoids array-index React keys (M21)
let _msgIdSeq = 0;
function nextMsgId(): string {
  return `msg-${++_msgIdSeq}-${Date.now().toString(36)}`;
}

// Re-export for backward compatibility (tests + other consumers import from useChat)
export {
  substitutePlanVariables,
  evaluatePlanCondition,
  detectRepetition,
  classifyToolCalls,
  isChainComplete,
  detectExternalChannel,
  TERMINAL_TOOLS,
  REPETITION_INITIAL,
  buildVolatileContext,
  computeToolResultMaxChars,
  formatToolResult,
  shouldSaveProcedure,
  buildProcedureData,
  classifyError,
  parseModelFilename,
};
export type { PlanStep, RepetitionState, ChainHistoryEntry };

/** Context passed to plan execution — avoids parameter explosion (P3) */
interface PlanExecContext {
  availableTools: ToolSchema[];
  appSettings: api.AppSettings;
  sessionApprovedTools: Set<string>;
  maxResultChars: number;
  requestApproval: (calls: ToolCall[]) => Promise<boolean>;
  setStreamingContent: (s: string) => void;
}

/** Execute a declared plan (plan_execute tool) — sequential steps with variable substitution.
 *  Returns summary string and any tools that were session-approved. */
async function executePlanSteps(
  goal: string,
  steps: PlanStep[],
  ctx: PlanExecContext,
): Promise<{ summary: string; approvedTools: string[] }> {
  // Pre-approve high/critical tools before starting
  const uniqueToolNames = [...new Set(steps.map(s => s.tool))];
  const toolsNeedingApproval = uniqueToolNames.filter(toolName => {
    if (toolName === 'plan_execute') return false; // no nested plans
    if (ctx.appSettings.toolApprovalMode === 'session' && ctx.sessionApprovedTools.has(toolName)) return false;
    const schema = ctx.availableTools.find(t => t.name === toolName);
    return schema ? api.needsApproval(schema.risk_level, toolName, ctx.appSettings) : true;
  });

  if (toolsNeedingApproval.length > 0) {
    ctx.setStreamingContent(`Plan "${goal}" — approving tools...`);
    const approvalCalls: ToolCall[] = toolsNeedingApproval.map(name => ({
      id: `plan-${name}`, name, arguments: {},
    }));
    const approved = await ctx.requestApproval(approvalCalls);
    if (!approved) {
      return {
        summary: `PLAN_DENIED: User denied approval for: ${toolsNeedingApproval.join(', ')}`,
        approvedTools: [],
      };
    }
  }

  // Execute steps sequentially
  const variables = new Map<string, string>();
  const stepResults: string[] = [];
  let completedSteps = 0;
  let skippedSteps = 0;
  let failedSteps = 0;

  for (let si = 0; si < steps.length; si++) {
    const step = steps[si];
    const stepLabel = `Step ${si + 1}/${steps.length}: ${step.tool}`;
    ctx.setStreamingContent(`Plan: ${goal} — ${stepLabel}...`);

    // Block nested plans (v1 limitation)
    if (step.tool === 'plan_execute') {
      stepResults.push(`${stepLabel}: SKIPPED (nested plans not supported)`);
      skippedSteps++;
      continue;
    }

    // Check condition
    if (step.condition) {
      if (!evaluatePlanCondition(step.condition, variables)) {
        stepResults.push(`${stepLabel}: SKIPPED (condition not met)`);
        skippedSteps++;
        console.log(`[HIVE PLAN] ${stepLabel}: condition "${step.condition}" → false, skipping`);
        continue;
      }
    }

    // Check if tool is disabled
    if (ctx.appSettings.toolOverrides?.[step.tool] === 'disabled') {
      stepResults.push(`${stepLabel}: SKIPPED (tool disabled)`);
      skippedSteps++;
      continue;
    }

    // Substitute variables in args
    const substitutedArgs = substitutePlanVariables(step.args, variables) as Record<string, unknown>;

    try {
      console.log(`[HIVE PLAN] ${stepLabel}`, JSON.stringify(substitutedArgs).substring(0, 200));
      const result = await api.executeTool(step.tool, substitutedArgs);

      let resultContent = result.content;
      if (resultContent.length > ctx.maxResultChars) {
        resultContent = resultContent.substring(0, ctx.maxResultChars) + '\n[...truncated]';
      }

      if (step.save_as) {
        variables.set(step.save_as, result.is_error ? `TOOL_ERROR: ${resultContent}` : resultContent);
        // Extract .id for dot-notation access ($variable.id) from known tool result patterns.
        // Scratchpad/Worker creation tools return "Scratchpad 'X' created..." / "Worker 'X' spawned..."
        if (!result.is_error) {
          const idMatch = resultContent.match(/(?:Scratchpad|Worker) '([^']+)'/);
          if (idMatch) {
            variables.set(`${step.save_as}.id`, idMatch[1]);
          }
        }
      }

      const status = result.is_error ? 'ERROR' : 'OK';
      const preview = resultContent.substring(0, 300).replace(/\n/g, ' ');
      stepResults.push(`${stepLabel}: ${status} — ${preview}${resultContent.length > 300 ? '...' : ''}`);

      if (result.is_error) failedSteps++;
      else completedSteps++;

      console.log(`[HIVE PLAN] ${stepLabel}: ${status}, ${resultContent.length} chars`);
    } catch (e) {
      const errMsg = e instanceof Error ? e.message : String(e);
      stepResults.push(`${stepLabel}: EXCEPTION — ${errMsg}`);
      failedSteps++;
      if (step.save_as) {
        variables.set(step.save_as, `TOOL_EXCEPTION: ${errMsg}`);
      }
      console.error(`[HIVE PLAN] ${stepLabel}: exception`, e);
    }
  }

  const summary = [
    `PLAN COMPLETE: "${goal}"`,
    `Results: ${completedSteps} succeeded, ${failedSteps} failed, ${skippedSteps} skipped out of ${steps.length} steps`,
    '',
    ...stepResults,
  ].join('\n');

  console.log(`[HIVE PLAN] Complete: "${goal}" — ${completedSteps}/${steps.length} succeeded`);
  return { summary, approvedTools: uniqueToolNames };
}

// ============================================
// Persistent Tool Logging (P4: Errors Are Answers)
// ============================================

/** Log a tool lifecycle event to both console and persistent app log.
 *  Format: TOOL_CHAIN | phase | tool_name | detail
 *  These are readable by the model via check_logs tool. */
function toolLog(phase: string, toolName: string, detail: string): void {
  const line = `TOOL_CHAIN | ${phase} | ${toolName} | ${detail}`;
  console.log(`[HIVE] ${line}`);
  api.logToApp(line);
}

// ============================================
// Tool Execution Helper
// ============================================

/** Execute a tool via the Rust backend and format the result as a Message.
 *  Handles: result truncation, status prefixes, read_file hint, MAGMA logging.
 */
async function executeAndFormatTool(tc: ToolCall, maxResultChars: number): Promise<Message> {
  try {
    const result = await api.executeTool(tc.name, tc.arguments as Record<string, unknown>);

    const { message: toolMsg, wasTruncated } = formatToolResult(tc, result, maxResultChars);

    if (wasTruncated) {
      toolLog('TRUNCATED', tc.name, `${result.content.length} → ${maxResultChars} chars`);
    }

    const preview = result.content.substring(0, 150).replace(/\n/g, ' ');
    toolLog(result.is_error ? 'RESULT_ERROR' : 'RESULT_OK', tc.name, `${result.content.length} chars | ${preview}`);

    // Log specialist task completion to MAGMA episodic graph
    if (tc.name === 'route_to_specialist') {
      const specialist = (tc.arguments as Record<string, unknown>)?.specialist as string;
      api.magmaAddEvent(
        'specialist_task', specialist || 'unknown',
        `${result.is_error ? 'FAILED' : 'OK'}: ${toolMsg.content.substring(0, 200)}`,
      ).catch(() => {});
    }

    return toolMsg;
  } catch (e) {
    const errContent = e instanceof Error ? e.message : String(e);
    toolLog('EXCEPTION', tc.name, errContent);
    return {
      role: 'tool',
      content: `TOOL_EXCEPTION [${tc.name}]: ${errContent}`,
      toolCallId: tc.id,
      toolName: tc.name,
    };
  }
}

// ============================================
// Hook Interface
// ============================================

export interface UseChatParams {
  activeModelType: 'local' | 'cloud';
  serverRunning: boolean;
  selectedModel: api.LocalModel | null;
  selectedCloudModel: { provider: api.ProviderType; model: api.ProviderModel } | null;
  appSettings: api.AppSettings;
  attachments: api.FileAttachment[];
  localModels: api.LocalModel[];
  wslModels: api.LocalModel[];
  backend: Backend;
  wslStatus: api.WslStatus | null;
  providerStatuses: Record<string, api.ProviderStatus>;
  systemInfo: api.SystemInfo | null;
  liveMetrics: api.LiveResourceMetrics | null;
  vramCompatibility: Record<string, api.VramCompatibility>;
  modelSettings: api.ModelSettings;
  setError: (error: string | null) => void;
}

export function useChat(params: UseChatParams) {
  const {
    activeModelType, serverRunning, selectedModel, selectedCloudModel,
    appSettings, attachments, localModels, wslModels, backend, wslStatus,
    providerStatuses, systemInfo, liveMetrics, vramCompatibility,
    modelSettings, setError,
  } = params;

  // Chat state
  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState('');
  const [isGenerating, _setIsGenerating] = useState(false);
  const isGeneratingRef = useRef(false);
  // Keep ref in sync with state — ref is the source of truth for stale-closure-safe reads (M23)
  const setIsGenerating = (val: boolean) => { isGeneratingRef.current = val; _setIsGenerating(val); };
  const [streamingContent, setStreamingContent] = useState('');
  const [streamingThinking, setStreamingThinking] = useState('');
  const [routingSpecialist, setRoutingSpecialist] = useState<string | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const abortControllerRef = useRef<AbortController | null>(null);

  // Tool framework state
  const [availableTools, setAvailableTools] = useState<ToolSchema[]>([]);
  const [toolsEnabled, setToolsEnabled] = useState(true);
  const [pendingToolCalls, setPendingToolCalls] = useState<ToolCall[]>([]);
  const [toolApprovalCallback, setToolApprovalCallback] = useState<((approved: boolean) => void) | null>(null);

  // Message origin tracking — where the current conversation trigger came from.
  // 'desktop' = typed in UI (full access), 'remote-host' or 'remote-user' = from Telegram/Discord.
  // Reset to 'desktop' at start of each sendMessage; overwritten by App.tsx event listeners.
  const messageOriginRef = useRef<MessageOrigin>('desktop');

  // Cognitive Harness state
  const [lastHarnessContext, setLastHarnessContext] = useState<HarnessContext | null>(null);
  // Track truncation across turns — model needs to know about context pressure
  const lastTruncatedCountRef = useRef(0);
  // Progressive context summarization — tracks which compression tiers have fired.
  // Tier 1 (65%): structured summary of oldest 30% → replaces them in apiMessages.
  // Tier 2 (80%): aggressive — keep last 10 raw + comprehensive summary.
  // Tier 3 (95%): emergency — clear tool results, minimal context.
  // Each tier fires ONCE per conversation. Reset on clear.
  const contextSummarizedRef = useRef(false);  // Tier 1 (kept as `contextSummarizedRef` for compat)
  const contextSummarized80Ref = useRef(false); // Tier 2
  // Cached summary from tier 1/2 — reusable across tool loop iterations
  const contextSummaryRef = useRef<string | null>(null);
  // Track working memory state in-memory (avoids file read per turn)
  const hasWorkingMemoryRef = useRef(false);
  // Cached stable harness prompt — only rebuild when meaningful inputs change.
  // Key = hash of (model, provider, tools, memory count, user system prompt).
  // This is the KV-cache-friendly prefix that llama.cpp can reuse across turns.
  const cachedHarnessRef = useRef<{ key: string; prompt: string } | null>(null);
  // Queue for Telegram/Discord messages that arrive while model is generating.
  // Without this, messages are silently dropped (sendMessage returns early when isGenerating).
  const pendingExternalRef = useRef<{ text: string; origin: MessageOrigin }[]>([]);
  // Ref to always hold the latest sendMessage — fixes stale closure in setTimeout queue drain
  // and in App.tsx event listeners (Telegram/Discord/Routines).
  const sendMessageRef = useRef<(text?: string) => Promise<void>>();

  // Session-level tool approval tracking (for 'session' mode — resets on reload)
  const sessionApprovedToolsRef = useRef<Set<string>>(new Set());

  // Seed working memory state once at startup (avoids per-turn file reads)
  useEffect(() => {
    api.workingMemoryRead()
      .then(wm => { hasWorkingMemoryRef.current = wm.trim().length > 0; })
      .catch(() => { hasWorkingMemoryRef.current = false; });
  }, []);

  // Load available tools
  useEffect(() => {
    api.getAvailableTools().then(tools => {
      setAvailableTools(tools);
      console.log(`[HIVE] Loaded ${tools.length} tools: ${tools.map(t => t.name).join(', ')}`);
      // Phase 3.5.5: Sync tools as MAGMA graph nodes (fire-and-forget, non-blocking)
      api.memorySyncSkills(tools)
        .then(n => { if (n > 0) console.log(`[HIVE] Synced ${n} skills to memory graph`); })
        .catch(() => {}); // Non-fatal — memory might not be initialized yet
    }).catch(err => {
      console.error('[HIVE] Failed to load tools:', err);
    });
  }, []);

  // Auto-scroll chat
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages, streamingContent]);

  // ==========================================
  // Chat Helper Functions
  // ==========================================

  /**
   * Phase 4: Ensure a specialist is available before route_to_specialist executes.
   *
   * Provider-agnostic (P2): cloud specialists need no server startup.
   * Returns the slot config if ready, or null if unavailable.
   */
  async function ensureSpecialistRunning(specialist: string): Promise<SlotConfig | null> {
    // Look up slot config for model assignment
    try {
      const configs = await api.getSlotConfigs();
      const config = configs.find(c => c.role === specialist);
      if (!config?.primary) {
        console.warn(`[HIVE] No model configured for specialist: ${specialist}`);
        return null;
      }

      const { provider, model, context_length } = config.primary;

      // Cloud providers don't need a local server — they're always "running"
      if (provider !== 'local') {
        console.log(`[HIVE] Specialist ${specialist} uses cloud provider ${provider}:${model}`);
        api.recordSlotWake(specialist as api.SlotRole, provider, model, null, 0).catch(() => {});
        api.magmaAddEvent('specialist_wake', specialist, `Cloud: ${provider}/${model}`).catch(() => {});
        return config;
      }

      // Local provider — need a running llama-server
      const port = SPECIALIST_PORTS[specialist];
      if (!port) return null;

      // Already running?
      const healthy = await api.checkServerHealth(port);
      if (healthy) return config;

      // Find the model in local/WSL model lists
      const allModels = [...localModels, ...wslModels];
      const modelInfo = allModels.find(m => m.filename === model);
      if (!modelInfo) {
        console.warn(`[HIVE] Model ${model} not found locally for specialist ${specialist}`);
        return null;
      }

      // Phase 4 C2: VRAM budget enforcement — evict idle specialists if needed
      try {
        const budget = await api.getVramBudget();
        const availableGb = budget.total_gb - budget.used_gb - budget.safety_buffer_gb;
        const neededGb = config.primary?.vram_gb ?? modelInfo.size_gb;
        if (availableGb < neededGb && budget.total_gb > 0) {
          const vramMsg = `VRAM budget tight: need ${neededGb}GB, available ${availableGb.toFixed(1)}GB — evicting idle specialists`;
          console.log(`[HIVE] ${vramMsg}`);
          api.logToApp(`SLOTS | vram_pressure | specialist=${specialist} | ${vramMsg}`);
          // Evict running specialists that aren't the one we're starting
          const otherRoles = ['coder', 'terminal', 'webcrawl', 'toolcall'].filter(r => r !== specialist);
          for (const role of otherRoles) {
            const rolePort = SPECIALIST_PORTS[role];
            if (!rolePort) continue;
            const roleHealthy = await api.checkServerHealth(rolePort).catch(() => false);
            if (roleHealthy) {
              console.log(`[HIVE] Evicting idle specialist: ${role} (port ${rolePort})`);
              api.logToApp(`SLOTS | vram_evict | role=${role} | port=${rolePort} | reason=make room for ${specialist}`);
              await api.stopSpecialistServer(role);
              api.recordSlotSleep(role as api.SlotRole).catch(() => {});
              api.magmaAddEvent('specialist_sleep', role, `Evicted for ${specialist} VRAM`).catch(() => {});
              // Recheck budget after eviction
              const newBudget = await api.getVramBudget();
              const newAvailable = newBudget.total_gb - newBudget.used_gb - newBudget.safety_buffer_gb;
              if (newAvailable >= neededGb) break;
            }
          }
        }
      } catch {
        // VRAM check is best-effort — proceed with startup regardless
      }

      console.log(`[HIVE] Auto-starting specialist: ${specialist} with model ${model}`);
      setRoutingSpecialist(specialist);
      setStreamingContent(`Starting ${specialist} specialist...`);

      if (backend === 'wsl') {
        await api.startSpecialistServerWsl(
          specialist, modelInfo.path, undefined, context_length, undefined,
          wslStatus?.llama_server_path || undefined,
        );
      } else {
        await api.startSpecialistServer(specialist, modelInfo.path, undefined, context_length);
      }

      // Wait for health (up to 45s — model loading takes time)
      for (let i = 0; i < 45; i++) {
        await new Promise(r => setTimeout(r, 1000));
        const ready = await api.checkServerHealth(port);
        if (ready) {
          console.log(`[HIVE] Specialist ${specialist} ready after ${i + 1}s`);
          api.recordSlotWake(specialist as api.SlotRole, provider, model, port, modelInfo.size_gb).catch(() => {});
          api.magmaAddEvent('specialist_wake', specialist, `Started ${model} on port ${port}`).catch(() => {});
          return config;
        }
        if (i % 10 === 9) {
          console.log(`[HIVE] Specialist ${specialist} still loading... (${i + 1}s)`);
          setStreamingContent(`Starting ${specialist} specialist... (${i + 1}s)`);
        }
      }
      console.warn(`[HIVE] Specialist ${specialist} failed to start within 45s`);
      return null;
    } catch (e) {
      console.warn(`[HIVE] Failed to ensure specialist ${specialist}:`, e);
      return null;
    }
  }

  /** Request user approval for high-risk tool calls via a promise */
  function requestToolApproval(calls: ToolCall[]): Promise<boolean> {
    return new Promise((resolve) => {
      setPendingToolCalls(calls);
      setToolApprovalCallback(() => (approved: boolean) => {
        setPendingToolCalls([]);
        setToolApprovalCallback(null);
        resolve(approved);
      });
    });
  }

  // classifyError and parseModelFilename are imported from chainPolicies.ts (Phase 4A)

  // ==========================================
  // Core Chat Function
  // ==========================================

  async function sendMessage(externalText?: string) {
    const textToSend = externalText ?? input.trim();
    if (!textToSend) return;

    // Reset message origin to 'desktop' for UI-typed messages.
    // Remote origins are set by App.tsx event listeners BEFORE calling sendMessageRef.
    if (!externalText) {
      messageOriginRef.current = 'desktop';
    }

    // If model is busy, queue external messages (Telegram/Discord) instead of dropping them.
    // User-typed messages are blocked by the UI (send button disabled), so only external hits this.
    // Use ref to avoid stale closure reads (M23)
    if (isGeneratingRef.current) {
      if (externalText) {
        pendingExternalRef.current.push({ text: externalText, origin: messageOriginRef.current });
        console.log(`[HIVE] sendMessage: Queued external message (model busy), queue size: ${pendingExternalRef.current.length}`);
      }
      return;
    }

    const canChat = (activeModelType === 'local' && serverRunning) ||
                    (activeModelType === 'cloud' && selectedCloudModel);
    if (!canChat) return;

    const msgPreview = textToSend.substring(0, 50) + (textToSend.length > 50 ? '...' : '');
    console.log('[HIVE] sendMessage: User message:', msgPreview);
    console.log('[HIVE] sendMessage: Using', activeModelType === 'cloud'
      ? `cloud provider: ${selectedCloudModel?.provider}/${selectedCloudModel?.model.id}`
      : 'local llama.cpp server');

    // Detect if this message came from an external channel (Telegram/Discord).
    // Used by the auto-reroute chain policy below to ensure replies go back to
    // the originating channel even if the model forgets to call the send tool.
    const externalRoute = detectExternalChannel(textToSend);

    // Sender identity tags — who sent this message and from where (P1: metadata, not content parsing).
    // User side: external channel sender name or "You" for HIVE UI.
    // Model side: cloud model name or local filename — future multi-model will differentiate by this.
    const userSenderName = externalRoute?.senderName || 'You';
    const userChannel: Message['senderChannel'] = externalRoute?.channel || 'hive';
    const modelSenderName = activeModelType === 'cloud' && selectedCloudModel
      ? selectedCloudModel.model.name || selectedCloudModel.model.id
      : selectedModel?.filename?.replace(/\.gguf$/i, '') || 'Local Model';

    const userMsg: Message = { id: nextMsgId(), role: 'user', content: textToSend, senderName: userSenderName, senderChannel: userChannel };
    const newMessages = [...messages, userMsg];
    setMessages(newMessages);
    if (!externalText) setInput(''); // only clear input if user typed it
    setIsGenerating(true);
    setStreamingContent('');
    setStreamingThinking('');

    abortControllerRef.current = new AbortController();

    // Declare outside try so finally block and tool loop can access them
    const sendStartTime = Date.now();
    let currentHarnessPrompt: string | null = null; // Local var — avoids stale React closure
    let currentVolatileContext: string | null = null; // Per-turn live metrics (separate message)
    const msgProvider = activeModelType === 'cloud' && selectedCloudModel
      ? selectedCloudModel.provider
      : 'local' as api.ProviderType;
    const msgModelId = activeModelType === 'cloud' && selectedCloudModel
      ? selectedCloudModel.model.id
      : '';

    // Cloud providers: use their reported context length. Always. No localStorage override.
    // Local models: use user's slider setting (modelSettings.contextLength).
    // This is the SINGLE source of truth for "how much context can this model handle?"
    const maxContextTokens = (activeModelType === 'cloud' && selectedCloudModel?.model?.context_length)
      ? selectedCloudModel.model.context_length
      : (modelSettings.contextLength || 4096);

    try {
      // Determine provider info FIRST — needed by message conversion below
      const provider = msgProvider;
      const modelId = msgModelId;
      const isLocal = provider === 'local';

      // Set session context so workers/tools know the current API model ID.
      // The framework provides truth — LLMs never need to guess their own model ID (P2, P7).
      if (modelId && provider !== 'local') {
        api.setSessionModelContext(provider, modelId).catch(() => {});
      }

      let apiMessages: api.ChatMessage[] = [];

      // Cognitive Harness — builds identity + capability-aware system prompt.
      // Falls back to raw user system prompt if harness is disabled.
      //
      // CACHING STRATEGY (for llama.cpp KV prefix match):
      //   apiMessages[0] = STABLE system prompt — cached, only rebuilt when meaningful inputs change
      //   apiMessages[1] = VOLATILE context — tiny (~30 tokens), rebuilt every turn (turn count, VRAM)
      //   apiMessages[2] = Memory recall — query-dependent, changes per turn
      //   apiMessages[3+] = Conversation history — grows incrementally (prefix cached by llama.cpp)
      //
      // LATENCY OPTIMIZATION: All async pre-chat work fires in parallel via Promise.all().
      // Before: sequential awaits = 350-500ms. After: parallel = max(200ms, 100ms, 100ms) ≈ 200ms.
      //
      // System prompt is NEVER mutated after this point (Principle 2: provider-agnostic).

      // === PARALLEL PRE-CHAT WORK ===
      // Fire all independent async work simultaneously — don't block on one to start another.
      const effectiveContext = maxContextTokens;
      const preChatStart = Date.now();

      // Memory work runs in parallel — metrics come from React state (event-driven, not polled)
      const [memoryCount, memoryContext, hasEmbeddingProvider] = await Promise.all([
        // 1. Memory stats (SQLite query — ~50-100ms)
        appSettings.memoryEnabled
          ? api.memoryStats().then(s => s.total_memories).catch(e => {
              console.warn('[HIVE] Memory stats unavailable (non-fatal):', e);
              return 0;
            })
          : Promise.resolve(0),
        // 2. Memory recall (FTS5 + optional embeddings — ~30-1000ms)
        appSettings.memoryEnabled
          ? api.memoryRecall(userMsg.content, 5, effectiveContext).catch(e => {
              console.warn('[HIVE] Memory recall failed (non-fatal):', e);
              return null as string | null;
            })
          : Promise.resolve(null as string | null),
        // 3. Embedding provider availability (P2: check ALL providers, not just OpenAI)
        appSettings.memoryEnabled
          ? api.memoryHasEmbeddingsProvider().catch(() => false)
          : Promise.resolve(false),
      ]);

      // === HARNESS BUILD (uses parallel results) ===
      if (appSettings.harnessEnabled) {
        try {
          const availableModelNames: string[] = [];
          for (const [providerType, status] of Object.entries(providerStatuses)) {
            if (status.connected && status.models) {
              for (const m of status.models) {
                const isActive = activeModelType === 'cloud' && selectedCloudModel
                  && selectedCloudModel.provider === providerType
                  && selectedCloudModel.model.id === m.id;
                if (!isActive) {
                  availableModelNames.push(`${providerType}:${m.name}`);
                }
              }
            }
          }
          if (localModels.length > 0) {
            const activeLocal = selectedModel?.filename;
            for (const lm of localModels) {
              if (lm.filename !== activeLocal) {
                availableModelNames.push(`local:${lm.filename}`);
              }
            }
          }

          const localFilename = selectedModel?.filename || '';
          const fileMeta = localFilename ? parseModelFilename(localFilename) : {};
          const hasEmbeddingApi = hasEmbeddingProvider;
          const userTurns = messages.filter(m => m.role === 'user').length + 1;

          const activeModelVram = selectedModel?.filename
            ? vramCompatibility[selectedModel.filename]?.estimate?.total_gb
            : undefined;

          // Estimate tokens used in conversation for context pressure tracking (Phase 3.5)
          const conversationTokens = api.estimateMessagesTokens(
            messages.map(m => ({ role: m.role, content: m.content }))
          );
          // Working memory state tracked in-memory (no per-turn file read)
          const hasWorkingMemory = hasWorkingMemoryRef.current;

          // === STABLE CACHE KEY ===
          const activeModelName = activeModelType === 'cloud' && selectedCloudModel
            ? selectedCloudModel.model.name
            : selectedModel?.filename || '';
          const toolKey = availableTools.map(t => `${t.name}:${t.risk_level}`).join(',');
          const userPromptKey = modelSettings.systemPrompt?.trim() || '';
          const stableCacheKey = [
            activeModelName, provider, toolKey, memoryCount,
            effectiveContext, hasEmbeddingApi ? 'hybrid' : 'keyword',
            availableModelNames.join(','), userPromptKey,
          ].join('|');

          // Rebuild stable prompt only if cache key changed
          if (!cachedHarnessRef.current || cachedHarnessRef.current.key !== stableCacheKey) {
            const capabilities: api.CapabilitySnapshot = {
              tools: availableTools.map(t => t.name),
              active_model: activeModelName || null,
              provider: provider,
              available_models: availableModelNames,
              memory_enabled: appSettings.memoryEnabled,
              memory_count: memoryCount,
              gpu: systemInfo?.gpus?.[0]?.name || null,
              vram_gb: systemInfo?.gpus?.[0]?.vram_mb ? systemInfo.gpus[0].vram_mb / 1024 : null,
              ram_gb: systemInfo?.ram?.total_gb || null,
              context_length: effectiveContext,
              quantization: isLocal ? fileMeta.quant : undefined,
              model_parameters: isLocal ? fileMeta.params : undefined,
              architecture: isLocal ? fileMeta.arch : undefined,
              backend: isLocal ? backend : undefined,
              cpu: systemInfo?.cpu?.name || undefined,
              tool_risks: availableTools.map(t => `${t.name}:${t.risk_level}`),
              memory_search_mode: appSettings.memoryEnabled
                ? (hasEmbeddingApi ? 'hybrid' : 'keyword')
                : undefined,
              conversation_turns: userTurns,
              messages_truncated: lastTruncatedCountRef.current,
              os_platform: 'Windows',
              vram_used_mb: liveMetrics?.vram_used_mb ?? undefined,
              vram_free_mb: liveMetrics?.vram_free_mb ?? undefined,
              ram_available_mb: liveMetrics?.ram_available_mb ?? undefined,
              gpu_utilization: liveMetrics?.gpu_utilization ?? undefined,
              active_model_vram_gb: activeModelVram,
              tokens_used: conversationTokens,
              has_working_memory: hasWorkingMemory,
            };

            const harnessCtx = await api.harnessBuild(
              capabilities,
              modelSettings.systemPrompt?.trim() || undefined,
            );
            setLastHarnessContext(harnessCtx);
            cachedHarnessRef.current = { key: stableCacheKey, prompt: harnessCtx.system_prompt };
            currentHarnessPrompt = harnessCtx.system_prompt;
            currentVolatileContext = harnessCtx.volatile_context;

            console.log(`[HIVE] Harness: REBUILT (cache miss) | ${harnessCtx.identity_source} | ${harnessCtx.tool_count} tools | memory: ${harnessCtx.memory_status}`);
          } else {
            // Cache HIT — reuse stable prompt, build volatile context in TypeScript.
            // The volatile context is pure number formatting (~30 tokens) — no file I/O,
            // no DB queries. Building it here saves a full Rust IPC + harness rebuild per turn.
            currentHarnessPrompt = cachedHarnessRef.current.prompt;
            currentVolatileContext = buildVolatileContext({
              conversationTurns: userTurns,
              messagesTruncated: lastTruncatedCountRef.current,
              vramUsedMb: liveMetrics?.vram_used_mb,
              vramFreeMb: liveMetrics?.vram_free_mb,
              vramGb: systemInfo?.gpus?.[0]?.vram_mb ? systemInfo.gpus[0].vram_mb / 1024 : undefined,
              gpuUtilization: liveMetrics?.gpu_utilization,
              activeModelVramGb: activeModelVram,
              contextLength: effectiveContext,
              tokensUsed: conversationTokens,
              hasWorkingMemory,
              ramAvailableMb: liveMetrics?.ram_available_mb,
            });

            console.log(`[HIVE] Harness: cache hit (stable prompt reused, volatile built in TS) | turn ${userTurns}`);
          }

          // Inject: [0] stable system prompt (KV-cacheable prefix)
          apiMessages.push({ role: 'system', content: currentHarnessPrompt });
          // Inject: [1] volatile context as separate message (doesn't bust prefix cache)
          // Append channel source indicator so the model knows WHERE this message came from.
          // Without this, the model infers channel from absence of a prefix — weak signal
          // in multi-turn conversations mixing Telegram and HIVE UI messages.
          // Phase 5C: Also includes context bus summary (agent activity feed).
          // Start both optional fetches in parallel to reduce per-turn latency (L33)
          const busSummaryPromise = currentVolatileContext
            ? api.contextBusSummary().catch(() => '')
            : Promise.resolve('');
          const skillsPromise = appSettings.harnessEnabled
            ? api.harnessGetRelevantSkills(textToSend).catch(() => '')
            : Promise.resolve('');

          if (currentVolatileContext) {
            const channelTag = externalRoute
              ? `Channel: ${externalRoute.channel} (reply will be auto-delivered — just generate content)`
              : 'Channel: HIVE Desktop (reply as normal text)';
            let volatileParts = `${currentVolatileContext} | ${channelTag}`;
            // Phase 5C: Append context bus summary (best-effort, non-blocking)
            const busSummary = await busSummaryPromise;
            if (busSummary.trim()) volatileParts += ` | ${busSummary}`;
            apiMessages.push({ role: 'system', content: volatileParts });
          }
          // Inject: [1.5] relevant skills based on user message (Phase 4.5.5)
          // Skills are independent of memory — they're just .md files in ~/.hive/skills/.
          // Best-effort: failure is silent. Injected as a separate message so the
          // stable prompt KV cache is preserved.
          if (appSettings.harnessEnabled) {
            const skillsContext = await skillsPromise;
            if (skillsContext.trim()) {
              apiMessages.push({ role: 'system', content: skillsContext });
              console.log('[HIVE] Skills context injected');
            }
          }
          // Inject: [2] session handoff notes — FIRST TURN ONLY.
          // One-shot injection, not repeated. Notes from previous session
          // so the model picks up where it left off. Cleared after injection.
          if (userTurns <= 1 && appSettings.memoryEnabled) {
            try {
              const sessionNotes = await api.sessionNotesRead();
              if (sessionNotes.trim()) {
                apiMessages.push({
                  role: 'system',
                  content: `[Session continuity — notes from previous session]\n${sessionNotes.trim()}`,
                });
                console.log('[HIVE] Session notes injected (first turn only)');
              }
            } catch {
              // Non-fatal — session notes are optional
            }
          }
        } catch (err) {
          console.warn('[HIVE] Harness: Build failed (non-fatal):', err);
          const systemPromptContent = modelSettings.systemPrompt?.trim() || '';
          if (systemPromptContent) {
            apiMessages.push({ role: 'system', content: systemPromptContent });
          }
        }
      } else {
        const systemPromptContent = modelSettings.systemPrompt?.trim() || '';
        if (systemPromptContent) {
          apiMessages.push({ role: 'system', content: systemPromptContent });
        }
      }

      // File attachments as SESSION INJECTION
      if (attachments.length > 0) {
        const fileList = attachments.map(f => {
          const sizeStr = f.size >= 1048576 ? `${(f.size / 1048576).toFixed(1)} MB` : `${(f.size / 1024).toFixed(0)} KB`;
          const isPdf = f.name.toLowerCase().endsWith('.pdf');
          return `- ${f.name} (${sizeStr}) → ${isPdf ? 'read_pdf' : 'read_file'} path: "${f.path}"`;
        }).join('\n');
        apiMessages.push({
          role: 'system',
          content: `[P3 — SESSION CONTEXT] Project files attached by user (accessible via tool calls):\n${fileList}\n\nUse read_file or read_pdf tools with the paths above to access file contents when needed.`,
        });
      }

      // Memory recall — already resolved from parallel work above
      if (memoryContext) {
        apiMessages.push({ role: 'system', content: `[P3 — SESSION CONTEXT] ${memoryContext}` });
        console.log('[HIVE] Memory: Session-injected relevant memories');
      }

      // Convert tool messages in conversation history.
      // Two formats: text-based (local) or native API (OpenAI, Anthropic, DashScope, etc).
      // Text-based: <tool_call>/<tool_response> tags — model sees tools as conversation text.
      // Native API: role:"tool" + tool_call_id — provider maps to model's native format.
      // CRITICAL: if tools[] isn't being sent (tools off), role:"tool" MUST be
      // converted to text — otherwise the API gets an unknown role and the model breaks.
      // DashScope/Kimi K2.5 supports native OpenAI-compatible tool calling (tools[] param).
      // This is the ONE place all chat paths build messages — P5.
      const needsTextToolFormat = isLocal;
      const useTools = toolsEnabled && availableTools.length > 0;
      apiMessages.push(...newMessages.map(m => {
        // Text-based providers: always convert tool history to tags
        if (needsTextToolFormat) {
          if (m.role === 'assistant' && m.toolCalls?.length) {
            const toolCallBlocks = m.toolCalls.map(tc =>
              `<tool_call>\n${JSON.stringify({ name: tc.name, arguments: tc.arguments })}\n</tool_call>`
            ).join('\n');
            return { role: 'assistant' as const, content: toolCallBlocks };
          }
          if (m.role === 'tool') {
            return { role: 'user' as const, content: `<tool_response>\n${m.content}\n</tool_response>` };
          }
        }
        // Native API providers: if tools[] WON'T be sent (tools toggled off mid-conversation),
        // still convert any leftover role:"tool" to avoid unknown-role API errors
        if (!needsTextToolFormat && !useTools && m.role === 'tool') {
          return { role: 'user' as const, content: `[Tool result]: ${m.content}` };
        }
        if (!needsTextToolFormat && !useTools && m.role === 'assistant' && m.toolCalls?.length) {
          const names = m.toolCalls.map(tc => tc.name).join(', ');
          return { role: 'assistant' as const, content: m.content || `Used ${names}` };
        }
        const msg: api.ChatMessage = { role: m.role, content: m.content };
        if (m.toolCallId) msg.tool_call_id = m.toolCallId;
        // Reconstruct OpenAI-format tool_calls on assistant messages.
        // Without this, the API sees orphaned role:"tool" messages — breaks multi-turn tool use.
        if (m.role === 'assistant' && m.toolCalls?.length) {
          msg.tool_calls = m.toolCalls.map(tc => ({
            id: tc.id,
            type: 'function' as const,
            function: { name: tc.name, arguments: JSON.stringify(tc.arguments) },
          }));
        }
        return msg;
      }));

      const maxContext = maxContextTokens;

      // Intelligence Graduation Phase 7: Progressive context summarization.
      // Multi-tier compression replaces single-shot working memory write.
      // Each tier fires ONCE per conversation (ref guards). Structured prompts
      // produce 3.70/5.0 quality vs 3.44 freeform (Factory.ai research).
      const conversationMsgs = apiMessages.filter(m => m.role !== 'system');
      const systemMsgs = apiMessages.filter(m => m.role === 'system');
      const contextUsage = api.estimateMessagesTokens(conversationMsgs) / maxContext;

      // --- Tier 1: Structured summarization at 65% (oldest 30%) ---
      if (contextUsage >= 0.65 && !contextSummarizedRef.current
          && conversationMsgs.length > 6) {
        contextSummarizedRef.current = true;
        const cutPoint = Math.floor(conversationMsgs.length * 0.3);
        const olderMessages = conversationMsgs.slice(0, cutPoint);
        const recentMessages = conversationMsgs.slice(cutPoint);

        if (olderMessages.length > 0) {
          // Build fallback summary (always available — P4)
          const userTopics = olderMessages
            .filter(m => m.role === 'user')
            .map(m => m.content.substring(0, 80).replace(/\n/g, ' ').trim())
            .filter(Boolean);
          const toolMentions = olderMessages
            .filter(m => m.role === 'tool')
            .map(m => {
              const match = m.content.match(/^TOOL_(?:OK|ERROR|EXCEPTION|DENIED|DEFERRED|SKIPPED) \[([^\]]+)\]/);
              return match ? match[1] : null;
            })
            .filter(Boolean);
          const uniqueTools = [...new Set(toolMentions)];
          const fallbackParts: string[] = [`[Context summary — ${olderMessages.length} earlier messages compressed]`];
          if (userTopics.length > 0) fallbackParts.push(`Topics discussed: ${userTopics.join(' | ')}`);
          if (uniqueTools.length > 0) fallbackParts.push(`Tools used: ${uniqueTools.join(', ')}`);
          let summaryText = fallbackParts.join('\n');

          // Try model-based structured summarization (non-blocking, with timeout)
          if (activeModelType === 'cloud' && selectedCloudModel) {
            const sanitizedPrompt = olderMessages
              .map(m => {
                if (m.role === 'user') return `user: ${m.content.substring(0, 100).replace(/\n/g, ' ').trim()}`;
                if (m.role === 'tool') return `tool: ${m.content.substring(0, 150).replace(/\n/g, ' ').trim()}`;
                return `assistant: ${m.content.substring(0, 200).replace(/\n/g, ' ').trim()}`;
              })
              .join('\n');
            const capturedProvider = selectedCloudModel.provider;
            const capturedModelId = selectedCloudModel.model.id;

            try {
              const modelSummary = await api.chatWithProvider(
                capturedProvider,
                capturedModelId,
                [
                  { role: 'system', content: `Summarize this conversation segment for continuity. Use this structure:
## Current State
What task is being worked on, what's accomplished so far.
## Key Decisions
Technical decisions made and their rationale.
## Modified Files
Exact file paths that were changed or discussed.
## Pending Work
Next steps, blockers, open questions.
## Critical Context
User preferences, error patterns, or anything costly to rediscover.

Be specific. Include file paths, function names, variable names, error messages. Limit: 1500 tokens.` },
                  { role: 'user', content: sanitizedPrompt },
                ],
              );
              if (modelSummary?.trim()) {
                summaryText = `[Context summary — ${olderMessages.length} messages compressed by model]\n` +
                  modelSummary.trim().substring(0, 6000).replace(/[\x00-\x08\x0B\x0C\x0E-\x1F]/g, '');
              }
            } catch {
              console.log('[HIVE] Tier 1 model summary failed, using fallback');
            }
          }

          // Replace old messages with summary in apiMessages (not in display messages)
          contextSummaryRef.current = summaryText;
          apiMessages = [...systemMsgs, { role: 'system' as const, content: summaryText }, ...recentMessages];

          // Persist to working memory for session continuity
          if (appSettings.memoryEnabled) {
            api.workingMemoryWrite(summaryText).catch(() => {});
            hasWorkingMemoryRef.current = true;
          }
          console.log(`[HIVE] Context at ${(contextUsage * 100).toFixed(0)}% — Tier 1 summarization: ${olderMessages.length} msgs → ${summaryText.length} chars`);
        }
      }

      // --- Tier 2: Aggressive summarization at 80% (keep last 10 raw) ---
      if (contextUsage >= 0.80 && !contextSummarized80Ref.current
          && conversationMsgs.length > 12) {
        contextSummarized80Ref.current = true;
        const keepCount = 10;
        const recentConv = conversationMsgs.slice(-keepCount);
        const olderConv = conversationMsgs.slice(0, -keepCount);

        if (olderConv.length > 0) {
          let aggressiveSummary = `[Aggressive context summary — ${olderConv.length} messages compressed, ${keepCount} recent preserved]`;

          if (activeModelType === 'cloud' && selectedCloudModel) {
            const sanitized = olderConv
              .map(m => `${m.role}: ${m.content.substring(0, 100).replace(/\n/g, ' ').trim()}`)
              .join('\n');
            try {
              const summary = await api.chatWithProvider(
                selectedCloudModel.provider,
                selectedCloudModel.model.id,
                [
                  { role: 'system', content: 'Produce a comprehensive summary of this conversation. Include ALL technical details: file paths, function names, decisions, error messages, and outcomes. This summary replaces the original messages — anything not included will be lost. Be thorough but concise. Limit: 2000 tokens.' },
                  { role: 'user', content: sanitized },
                ],
              );
              if (summary?.trim()) {
                aggressiveSummary += '\n' + summary.trim().substring(0, 8000).replace(/[\x00-\x08\x0B\x0C\x0E-\x1F]/g, '');
              }
            } catch {
              // Fallback: use tier 1 summary if available
              if (contextSummaryRef.current) {
                aggressiveSummary += '\n' + contextSummaryRef.current;
              }
            }
          } else if (contextSummaryRef.current) {
            aggressiveSummary += '\n' + contextSummaryRef.current;
          }

          contextSummaryRef.current = aggressiveSummary;
          apiMessages = [...systemMsgs, { role: 'system' as const, content: aggressiveSummary }, ...recentConv];

          if (appSettings.memoryEnabled) {
            api.workingMemoryWrite(aggressiveSummary).catch(() => {});
          }
          console.log(`[HIVE] Context at ${(contextUsage * 100).toFixed(0)}% — Tier 2 aggressive: ${olderConv.length} msgs compressed, ${keepCount} kept raw`);
        }
      }

      // --- Tier 3: Emergency at 95% — clear tool results (Anthropic pattern: 84% token reduction) ---
      if (contextUsage >= 0.95) {
        const convMsgs = apiMessages.filter(m => m.role !== 'system');
        const compressedConv = convMsgs.map(m => {
          if (m.role === 'tool' && m.content.length > 200) {
            const prefix = m.content.substring(0, 150);
            return { ...m, content: `${prefix}\n[RESULT COMPRESSED — ${m.content.length} chars → 150]` };
          }
          return m;
        });
        const sysMsgs = apiMessages.filter(m => m.role === 'system');
        apiMessages = [...sysMsgs, ...compressedConv];
        console.log(`[HIVE] Context at ${(contextUsage * 100).toFixed(0)}% — Tier 3 emergency: tool results compressed`);
      }

      const beforeCount = apiMessages.length;
      apiMessages = api.truncateMessagesToFit(apiMessages, maxContext);

      const droppedThisTurn = beforeCount - apiMessages.length;
      if (droppedThisTurn > 0) {
        lastTruncatedCountRef.current += droppedThisTurn;
        console.log(`[HIVE] sendMessage: Truncated ${droppedThisTurn} messages to fit ${maxContext} token context (total dropped: ${lastTruncatedCountRef.current})`);
      }

      // Track whether the originating channel received its reply (set true by tool loop
      // if telegram_send/discord_send succeeds, stays false for streaming paths and when
      // tools are stripped for channel replies). Used by the unified guarantee below.
      let channelDelivered = false;
      // Final assistant content for the channel guarantee — set by whichever code path runs.
      let lastAssistantContent: string | null = null;

      // Use tool-aware path when tools are enabled
      if (useTools) {
        // === Tool-enabled agentic loop ===
        // For external channel replies (Telegram/Discord/PTY agent), strip the send tools from
        // the model's view. The post-loop guarantee handles delivery — the model just
        // generates content. This saves ~500 tokens/turn on tool schemas AND prevents
        // prompt injection from redirecting replies to a different chat_id (the
        // framework hardcodes the destination from externalRoute, not model choice).
        const stripTools = externalRoute
          ? externalRoute.channel === 'pty-agent'
            ? ['send_to_agent']
            : ['telegram_send', 'discord_send']
          : [];
        const loopTools = stripTools.length > 0
          ? availableTools.filter(t => !stripTools.includes(t.name))
          : availableTools;
        const preChatMs = Date.now() - preChatStart;
        console.log(`[HIVE] sendMessage: Tools enabled (${loopTools.length} tools${externalRoute ? `, stripped channel sends for ${externalRoute.channel} reply` : ''}) | pre-chat: ${preChatMs}ms | ${apiMessages.length} messages`);
        setStreamingContent('Thinking...');

        let currentMessages = [...newMessages];
        let loopCount = 0;
        const maxLoops = 10; // prevent infinite tool loops
        let repetitionState = REPETITION_INITIAL;
        // Deferred tool resurfacing: track tools that were deferred and not yet re-called.
        // After 2 turns without re-call, inject a reminder so the model doesn't forget.
        let unresolvedDeferred: string[] = [];
        let deferredTurnCount = 0;
        // Phase 9.2: Track tool chain for procedure extraction (P4, P5)
        const chainHistory: { name: string; argsKeys: string[]; success: boolean }[] = [];

        while (loopCount < maxLoops) {
          loopCount++;

          const llmStart = Date.now();
          const thinkingDepth = appSettings.thinkingDepth?.[provider] || undefined;
          const chatResponse = await api.chatWithTools(
            provider, modelId, apiMessages, loopTools, maxContext, thinkingDepth,
          );
          const llmMs = Date.now() - llmStart;
          console.log(`[HIVE] sendMessage: chatWithTools loop ${loopCount} took ${llmMs}ms | type: ${chatResponse.type}`);

          if (chatResponse.type === 'text') {
            // Model replied with text — done. Thinking separated at provider boundary (P1).
            const textContent = chatResponse.content?.trim() || '';
            const thinking = chatResponse.thinking || undefined;
            if (textContent) {
              currentMessages = [...currentMessages, { id: nextMsgId(), role: 'assistant' as const, content: textContent, thinking, senderName: modelSenderName }];
            } else if (loopCount > 1) {
              // After tool execution, always add an assistant message so history
              // doesn't end with a tool result (prevents role alternation errors)
              currentMessages = [...currentMessages, { id: nextMsgId(), role: 'assistant' as const, content: 'Done.', thinking, senderName: modelSenderName }];
            }
            setMessages(currentMessages);
            break;
          }

          // Model wants to call tools — thinking may accompany tool calls
          let { tool_calls, thinking: toolThinking } = chatResponse;
          for (const tc of tool_calls) {
            const argsPreview = JSON.stringify(tc.arguments).substring(0, 200);
            toolLog('REQUESTED', tc.name, `id=${tc.id} args=${argsPreview}`);
          }

          // === Chain Policy: Repetition Detection ===
          const repetition = detectRepetition(tool_calls, repetitionState);
          repetitionState = repetition.state;
          if (repetition.stuck) {
            toolLog('LOOP_BREAK', tool_calls[0]?.name || '?', repetition.reason || 'stuck');
            currentMessages = [...currentMessages, { id: nextMsgId(), role: 'assistant' as const, content: 'Done.', senderName: modelSenderName }];
            setMessages(currentMessages);
            break;
          }

          // Add assistant message with tool calls (thinking separated — P1).
          // Content is always empty — any text the model produces alongside tool calls
          // is narration noise ("Let me search...", "I'll send that..."). The model gets
          // to speak AFTER it sees the tool result, which is the useful response.
          // This also eliminates stutter loops entirely — no text = nothing to stutter.
          const assistantMsg: Message = {
            id: nextMsgId(),
            role: 'assistant',
            content: '',
            thinking: toolThinking || undefined,
            toolCalls: tool_calls,
            senderName: modelSenderName,
          };
          currentMessages = [...currentMessages, assistantMsg];
          setMessages(currentMessages);

          // === Chain Policy: Remote Channel Security Gate ===
          // Check tool access based on message origin (desktop vs remote Host vs remote User).
          // Blocked tools get an immediate error result without reaching approval.
          const origin = messageOriginRef.current;
          const blockedByOrigin: ToolCall[] = [];
          const allowedCalls = tool_calls.filter(tc => {
            const reason = api.checkToolOriginAccess(tc.name, origin);
            if (reason) {
              blockedByOrigin.push(tc);
              return false;
            }
            return true;
          });

          // Send blocked tool results back to the model
          if (blockedByOrigin.length > 0) {
            for (const tc of blockedByOrigin) {
              toolLog('BLOCKED', tc.name, `origin=${origin}`);
              const blockMsg: Message = {
                id: nextMsgId(),
                role: 'tool',
                content: api.checkToolOriginAccess(tc.name, origin) || `Tool '${tc.name}' is not available from this channel.`,
                toolCallId: tc.id,
                toolName: tc.name,
              };
              currentMessages = [...currentMessages, blockMsg];
            }
            setMessages(currentMessages);
            // If ALL calls were blocked, skip to next iteration
            if (allowedCalls.length === 0) {
              tool_calls = [];
              continue;
            }
            // Otherwise continue with the allowed subset
            tool_calls = allowedCalls;
          }

          // === Chain Policy: Tool Approval Gate ===
          // For remote Host: dangerous tools ALWAYS require approval (override auto-approve).
          const effectiveSettings = origin === 'remote-host'
            ? { ...appSettings, toolApprovalMode: 'ask' as api.ToolApprovalMode }
            : appSettings;

          const unapprovedCalls = tool_calls.filter(tc => {
            if (effectiveSettings.toolApprovalMode === 'session' && sessionApprovedToolsRef.current.has(tc.name)) {
              return false;
            }
            const schema = availableTools.find(t => t.name === tc.name);
            return schema ? api.needsApproval(schema.risk_level, tc.name, effectiveSettings) : true;
          });

          if (unapprovedCalls.length > 0) {
            for (const tc of unapprovedCalls) toolLog('APPROVAL_PENDING', tc.name, 'waiting for user');
            setStreamingContent('Waiting for approval...');
            const approved = await requestToolApproval(unapprovedCalls);
            if (!approved) {
              const deniedNames = unapprovedCalls.map(tc => tc.name).join(', ');
              for (const tc of unapprovedCalls) toolLog('DENIED', tc.name, 'user denied execution');
              const denyMsg: Message = {
                role: 'tool',
                content: `TOOL_DENIED: User denied execution of: ${deniedNames}. ` +
                  `You may explain what you were trying to do and ask the user if they'd like to proceed differently, ` +
                  `or try an alternative approach that doesn't require these tools.`,
                toolCallId: unapprovedCalls[0]?.id || 'denied',
              };
              currentMessages = [...currentMessages, denyMsg];
              setMessages(currentMessages);
              break;
            }
            if (appSettings.toolApprovalMode === 'session') {
              for (const tc of tool_calls) {
                sessionApprovedToolsRef.current.add(tc.name);
              }
            }
          }

          // === Chain Policy: Classify & Defer ===
          // Terminal tools mixed with research tools get deferred — model composed
          // the message BEFORE seeing results. Deferral forces see-then-send.
          const toolExecStart = Date.now();
          setStreamingContent('Executing tools...');
          const TOOL_RESULT_MAX_CHARS = computeToolResultMaxChars(maxContext);
          const { execute: executeNow, deferred } = classifyToolCalls(tool_calls);
          for (const tc of deferred) toolLog('DEFERRED', tc.name, 'terminal tool deferred — research not yet processed');

          // Deferred resurfacing: check if previously deferred tools were re-called this turn
          const calledThisTurn = new Set(tool_calls.map(tc => tc.name));
          unresolvedDeferred = unresolvedDeferred.filter(name => !calledThisTurn.has(name));
          // Track newly deferred tools
          for (const tc of deferred) {
            if (!unresolvedDeferred.includes(tc.name)) {
              unresolvedDeferred.push(tc.name);
              deferredTurnCount = 0;
            }
          }

          // Add deferred tool responses
          for (const tc of deferred) {
            const deferMsg: Message = {
              role: 'tool',
              content: `TOOL_DEFERRED [${tc.name}]: Research tools in this turn haven't been processed yet. Review the research results above, then call ${tc.name} again with your composed response.`,
              toolCallId: tc.id,
              toolName: tc.name,
            };
            currentMessages = [...currentMessages, deferMsg];
            setMessages(currentMessages);
          }

          // === Chain Policy: Terminal Dedup ===
          // If the model emitted duplicate terminal tool calls in one turn (e.g. 3x telegram_send),
          // only execute the first. Format confusion can cause models to emit the same send
          // multiple times — executing all of them causes triple-texting on Telegram/Discord.
          const terminalSeen = new Set<string>();
          const dedupedExecute = executeNow.filter(tc => {
            if (TERMINAL_TOOLS.has(tc.name)) {
              if (terminalSeen.has(tc.name)) {
                toolLog('DEDUP_SKIP', tc.name, 'duplicate terminal call in same turn');
                return false;
              }
              terminalSeen.add(tc.name);
            }
            return true;
          });
          // Add TOOL_SKIPPED results for deduped calls so the model sees them acknowledged
          for (const tc of executeNow) {
            if (!dedupedExecute.includes(tc)) {
              currentMessages = [...currentMessages, {
                role: 'tool' as const,
                content: `TOOL_SKIPPED [${tc.name}]: Duplicate call — already executed this turn.`,
                toolCallId: tc.id,
                toolName: tc.name,
              }];
            }
          }

          // Execute non-deferred, non-duplicate tool calls
          for (const tc of dedupedExecute) {
            // Check if tool is disabled via per-tool overrides
            if (appSettings.toolOverrides?.[tc.name] === 'disabled') {
              const disabledResult: Message = {
                role: 'tool',
                content: `Tool "${tc.name}" is disabled in settings.`,
                toolCallId: tc.id,
              };
              currentMessages = [...currentMessages, disabledResult];
              setMessages(currentMessages);
              continue;
            }
            toolLog('EXECUTING', tc.name, `args=${JSON.stringify(tc.arguments).substring(0, 300)}`);

            // Phase 4: Provider-agnostic specialist routing (P2)
            // Cloud providers are handled directly here; local providers go through the Rust tool.
            if (tc.name === 'route_to_specialist') {
              const args = tc.arguments as Record<string, unknown>;
              const specialist = args?.specialist as string;
              const task = args?.task as string;
              if (specialist) {
                setRoutingSpecialist(specialist);
                const slotConfig = await ensureSpecialistRunning(specialist);
                setStreamingContent('Executing tools...');

                // Cloud routing: bypass Rust tool, use existing provider chat (P2)
                if (slotConfig?.primary && slotConfig.primary.provider !== 'local') {
                  const { provider, model } = slotConfig.primary;
                  try {
                    toolLog('SPECIALIST_ROUTE', tc.name, `${specialist} → ${provider}/${model}`);
                    setStreamingContent(`Routing to ${specialist} (${provider})...`);
                    // Phase 5A: Inject HIVE identity + specialist role + MAGMA wake context.
                    // Cloud specialists get the same harness as consciousness — they ARE HIVE,
                    // not generic assistants. Uses cached harness (already built this turn).
                    const specialistMessages: api.ChatMessage[] = [];
                    const cachedHarness = cachedHarnessRef.current?.prompt;
                    if (cachedHarness) {
                      const specialistIdentity = `${cachedHarness}\n\n## Specialist Role: ${specialist}\nYou are operating as the ${specialist} specialist in the HIVE orchestration harness. The consciousness model has routed this task to you for your domain expertise. Provide thorough analysis — your response will be relayed back to the orchestrating consciousness layer.`;
                      specialistMessages.push({ role: 'system', content: specialistIdentity });
                      toolLog('SPECIALIST_HARNESS', tc.name, `Injected HIVE identity (${specialistIdentity.length} chars) for ${specialist}`);
                    } else {
                      // Fallback: minimal specialist identity if harness not yet cached
                      specialistMessages.push({ role: 'system', content: `You are the ${specialist} specialist in the HIVE AI orchestration harness. Provide thorough, expert analysis.` });
                      toolLog('SPECIALIST_HARNESS', tc.name, `Fallback identity for ${specialist} (no cached harness)`);
                    }
                    // MAGMA wake context briefing (Phase 4 — specialist continuity)
                    try {
                      const wakeContext = await api.getWakeContext(specialist as api.SlotRole, task || '');
                      if (wakeContext && wakeContext.trim()) {
                        specialistMessages.push({ role: 'system', content: wakeContext });
                      }
                    } catch { /* Wake context is best-effort */ }
                    specialistMessages.push({ role: 'user', content: task || '' });

                    // Intelligence Graduation Phase 1: chatWithTools replaces chatWithProvider.
                    // Cloud specialists can now call HIVE tools (memory, files, web, etc.).
                    // chatWithProvider returned text-only — this was the #1 intelligence gap (P2 violation).
                    const specialistMaxLoops = 8;
                    let specialistResult = '';
                    for (let sLoop = 0; sLoop < specialistMaxLoops; sLoop++) {
                      const sResponse = await api.chatWithTools(
                        provider as api.ProviderType,
                        model,
                        specialistMessages,
                        loopTools,
                      );

                      if (sResponse.type === 'text') {
                        specialistResult = sResponse.content || '';
                        break;
                      }

                      // Specialist wants to call tools — execute them and loop back
                      const { tool_calls: sCalls } = sResponse;
                      toolLog('SPECIALIST_TOOLS', tc.name, `${specialist} calling ${sCalls.length} tool(s): ${sCalls.map(t => t.name).join(', ')}`);
                      setStreamingContent(`${specialist} executing tools (${sLoop + 1})...`);

                      // Add assistant message with tool_calls to specialist conversation
                      specialistMessages.push({
                        role: 'assistant',
                        content: sResponse.content || '',
                        tool_calls: sCalls.map(st => ({
                          id: st.id,
                          type: 'function' as const,
                          function: { name: st.name, arguments: JSON.stringify(st.arguments) },
                        })),
                      });

                      // Execute each tool and add results
                      for (const st of sCalls) {
                        try {
                          const sToolResult = await api.executeTool(st.name, st.arguments as Record<string, unknown>);
                          let resultContent = sToolResult.is_error
                            ? `TOOL_ERROR [${st.name}]: ${sToolResult.content}`
                            : sToolResult.content;
                          if (resultContent.length > TOOL_RESULT_MAX_CHARS) {
                            resultContent = resultContent.substring(0, TOOL_RESULT_MAX_CHARS) + '\n[TRUNCATED]';
                          }
                          specialistMessages.push({ role: 'tool', content: resultContent, tool_call_id: st.id });
                          toolLog('SPECIALIST_TOOL_OK', st.name, `specialist=${specialist} chars=${resultContent.length}`);
                        } catch (toolErr) {
                          specialistMessages.push({
                            role: 'tool',
                            content: `TOOL_EXCEPTION [${st.name}]: ${toolErr instanceof Error ? toolErr.message : String(toolErr)}`,
                            tool_call_id: st.id,
                          });
                          toolLog('SPECIALIST_TOOL_ERR', st.name, `specialist=${specialist} err=${toolErr}`);
                        }
                      }
                    }

                    if (!specialistResult && specialistMaxLoops > 0) {
                      toolLog('SPECIALIST_MAXLOOP', tc.name, `${specialist} hit ${specialistMaxLoops} loop limit`);
                    }

                    const cloudResult = specialistResult || '(specialist returned no content)';
                    const toolMsg: Message = {
                      role: 'tool',
                      content: cloudResult,
                      toolCallId: tc.id,
                      toolName: tc.name,
                    };
                    currentMessages = [...currentMessages, toolMsg];
                    setMessages(currentMessages);
                    api.magmaAddEvent('specialist_task', specialist, `Cloud OK: ${cloudResult.substring(0, 200)}`).catch(() => {});
                    api.contextBusWrite(specialist, `Completed task (${provider}/${model}, ${cloudResult.length} chars)`).catch(() => {});
                    toolLog('RESULT_OK', tc.name, `specialist=${specialist} chars=${cloudResult.length}`);
                    setRoutingSpecialist(null);
                    continue;
                  } catch (cloudErr) {
                    toolLog('RESULT_ERROR', tc.name, `specialist=${specialist} err=${cloudErr}`);
                    const errorMsg: Message = {
                      role: 'tool',
                      content: `Cloud specialist error (${provider}/${model}): ${cloudErr}`,
                      toolCallId: tc.id,
                      toolName: tc.name,
                    };
                    currentMessages = [...currentMessages, errorMsg];
                    setMessages(currentMessages);
                    api.magmaAddEvent('specialist_task', specialist, `Cloud FAILED: ${cloudErr}`).catch(() => {});
                    setRoutingSpecialist(null);
                    continue;
                  }
                }
                // Local specialist — tool will be executed by Rust route_to_specialist
                setRoutingSpecialist(null);
              }
            }

            // Phase 7: Plan execution — delegates to extracted executePlanSteps (P1)
            if (tc.name === 'plan_execute') {
              const planArgs = tc.arguments as Record<string, unknown>;
              const goal = (planArgs.goal as string) || 'Executing plan';
              const steps = (planArgs.steps as PlanStep[]) || [];

              if (!steps.length || steps.length < 2) {
                // Plan has no valid steps — send validation error back to model
                // so it can retry with a proper plan or choose a different tool (P4).
                let errorContent: string;
                try {
                  const result = await api.executeTool(tc.name, tc.arguments as Record<string, unknown>);
                  errorContent = result.content;
                } catch (e) {
                  errorContent = `plan_execute validation failed: ${e instanceof Error ? e.message : String(e)}. ` +
                    'If you want to run a single tool, call it directly. ' +
                    'If you want a background worker for async tasks, use worker_spawn instead.';
                }
                currentMessages = [...currentMessages, {
                  role: 'tool' as const, content: `TOOL_ERROR [plan_execute]: ${errorContent}`,
                  toolCallId: tc.id, toolName: tc.name,
                }];
                setMessages(currentMessages);
                continue;
              }

              toolLog('PLAN_START', tc.name, `goal="${goal}" steps=${steps.length}`);
              const planResult = await executePlanSteps(goal, steps, {
                availableTools: loopTools,
                appSettings,
                sessionApprovedTools: sessionApprovedToolsRef.current,
                maxResultChars: TOOL_RESULT_MAX_CHARS,
                requestApproval: requestToolApproval,
                setStreamingContent,
              });

              // Session-remember any tools approved during plan
              if (appSettings.toolApprovalMode === 'session') {
                for (const name of planResult.approvedTools) sessionApprovedToolsRef.current.add(name);
              }

              // Log plan execution as MAGMA event (Phase 4)
              const failedSteps = (planResult.summary.match(/FAILED/g) || []).length;
              api.magmaAddEvent(
                failedSteps === 0 ? 'plan_success' : 'plan_partial',
                'orchestrator',
                `Plan "${goal}": ${steps.length} steps, ${failedSteps} failed`,
              ).catch(() => {});

              currentMessages = [...currentMessages, {
                role: 'tool' as const,
                content: planResult.summary,
                toolCallId: tc.id, toolName: tc.name,
              }];
              setMessages(currentMessages);
              continue;
            }

            // Default tool execution — handles truncation, status prefixes, MAGMA logging
            const toolMsg = await executeAndFormatTool(tc, TOOL_RESULT_MAX_CHARS);
            currentMessages = [...currentMessages, toolMsg];
            setMessages(currentMessages);

            // Phase 9.2: Record tool execution in chain history for procedure extraction
            const isToolError = toolMsg.content.startsWith('TOOL_ERROR') || toolMsg.content.startsWith('TOOL_EXCEPTION');
            chainHistory.push({
              name: tc.name,
              argsKeys: Object.keys((tc.arguments as Record<string, unknown>) || {}),
              success: !isToolError,
            });
          }

          // === Chain Policy: Terminal Completion ===
          // Deferred tools (TOOL_DEFERRED) don't count — they weren't actually executed.
          const toolExecMs = Date.now() - toolExecStart;
          toolLog('EXEC_DONE', '*', `${dedupedExecute.length} tool(s) in ${toolExecMs}ms`);
          if (isChainComplete(tool_calls, currentMessages)) {
            channelDelivered = true;
            toolLog('CHAIN_COMPLETE', tool_calls.map(t => t.name).join('+'), 'terminal tool succeeded');
            currentMessages = [...currentMessages, { id: nextMsgId(), role: 'assistant' as const, content: 'Message sent.', senderName: modelSenderName }];
            setMessages(currentMessages);
            break;
          }

          // Rebuild apiMessages with tool results for next iteration
          apiMessages = [];
          // Re-inject system prompt (harness-enhanced or raw, matching initial construction)
          // Uses local var, not React state — avoids stale closure from setLastHarnessContext
          // Stable prompt first (KV-cached), then volatile context separately.
          if (currentHarnessPrompt) {
            apiMessages.push({ role: 'system', content: currentHarnessPrompt });
            if (currentVolatileContext) {
              const channelTag = externalRoute
                ? `Channel: ${externalRoute.channel} (reply will be auto-delivered — just generate content)`
                : 'Channel: HIVE Desktop (reply as normal text)';
              apiMessages.push({ role: 'system', content: `${currentVolatileContext} | ${channelTag}` });
            }
          } else if (modelSettings.systemPrompt?.trim()) {
            apiMessages.push({ role: 'system', content: modelSettings.systemPrompt.trim() });
          }

          // Convert tool messages for text-based providers.
          // We're inside the agentic loop so tools[] IS being sent for native providers —
          // role:"tool" is valid here. Only text-based providers (local) need conversion.
          apiMessages.push(...currentMessages.map(m => {
            if (needsTextToolFormat) {
              if (m.role === 'assistant' && m.toolCalls?.length) {
                const toolCallBlocks = m.toolCalls.map(tc =>
                  `<tool_call>\n${JSON.stringify({ name: tc.name, arguments: tc.arguments })}\n</tool_call>`
                ).join('\n');
                return { role: 'assistant' as const, content: toolCallBlocks };
              }
              if (m.role === 'tool') {
                return { role: 'user' as const, content: `<tool_response>\n${m.content}\n</tool_response>` };
              }
            }
            const msg: api.ChatMessage = { role: m.role, content: m.content };
            if (m.toolCallId) msg.tool_call_id = m.toolCallId;
            // Reconstruct OpenAI-format tool_calls on assistant messages (same as initial build)
            if (m.role === 'assistant' && m.toolCalls?.length) {
              msg.tool_calls = m.toolCalls.map(tc => ({
                id: tc.id,
                type: 'function' as const,
                function: { name: tc.name, arguments: JSON.stringify(tc.arguments) },
              }));
            }
            return msg;
          }));

          // Deferred resurfacing: if deferred tools haven't been re-called for 2+ turns, remind the model
          if (unresolvedDeferred.length > 0) {
            deferredTurnCount++;
            if (deferredTurnCount >= 2) {
              const names = unresolvedDeferred.join(', ');
              apiMessages.push({
                role: 'system' as const,
                content: `[REMINDER] You deferred ${names} ${deferredTurnCount} turns ago because research wasn't ready. Research results are now in context. Call ${names} to deliver your response.`,
              });
              toolLog('RESURFACE', names, `deferred ${deferredTurnCount} turns ago, reminding model`);
            }
          }

          // Inject cached context summary if progressive summarization has fired.
          // This ensures the model retains compressed context even in the tool loop.
          if (contextSummaryRef.current) {
            const sysEnd = apiMessages.findIndex(m => m.role !== 'system');
            if (sysEnd > 0) {
              apiMessages.splice(sysEnd, 0, { role: 'system' as const, content: contextSummaryRef.current });
            }
          }

          // Truncate BEFORE re-sending — without this, tool results pile up
          // and blow out context (e.g., 8 tool calls × 4k chars = 32k chars = no room left)
          const toolLoopBefore = apiMessages.length;
          apiMessages = api.truncateMessagesToFit(apiMessages, maxContext);
          const toolLoopDropped = toolLoopBefore - apiMessages.length;
          if (toolLoopDropped > 0) lastTruncatedCountRef.current += toolLoopDropped;

          setStreamingContent('Thinking...');
        }

        if (loopCount >= maxLoops) {
          console.warn('[HIVE] sendMessage: Tool loop hit max iterations');
        }

        // Phase 9.2: Procedure learning from tool chains (P4, P5)
        // Save new successful chains as learned procedures. Failures are logged as
        // MAGMA events for the model's awareness — the model can record outcomes
        // manually via the procedure_learn tool's 'outcome' action.
        if (chainHistory.some(t => !t.success) && chainHistory.length >= 2) {
          // Log failure event to MAGMA so the model knows about it
          const chainName = chainHistory.map(t => t.name).join(' → ');
          const failedTools = chainHistory.filter(t => !t.success).map(t => t.name).join(', ');
          api.magmaAddEvent('procedure_failure', 'orchestrator',
            `Chain "${chainName}" failed at: ${failedTools}. Trigger: "${textToSend.substring(0, 80)}"`,
          ).catch(() => {});
        }

        const procedureData = buildProcedureData(chainHistory, textToSend);
        if (procedureData) {
          api.magmaSaveProcedure(
            procedureData.chainName,
            `Auto-extracted from: "${procedureData.triggerPattern}"`,
            procedureData.steps,
            procedureData.triggerPattern,
          ).catch(() => {}); // fire-and-forget
          toolLog('PROCEDURE_SAVED', procedureData.chainName, `${procedureData.steps.length} steps, trigger="${procedureData.triggerPattern.substring(0, 50)}"`);
        }

        // Phase 5C: Write tool chain summary to context bus (best-effort, P4)
        if (chainHistory.length > 0) {
          const toolNames = chainHistory.map(t => t.name).join(', ');
          const successes = chainHistory.filter(t => t.success).length;
          const busEntry = `Tool chain: ${toolNames} (${successes}/${chainHistory.length} ok)`;
          api.contextBusWrite('consciousness', busEntry).catch(() => {});
        }

        // Capture last assistant content for channel guarantee
        const toolLastAssistant = [...currentMessages].reverse().find(
          m => m.role === 'assistant' && m.content && m.content !== 'Done.' && m.content !== 'Message sent.'
        );
        if (toolLastAssistant?.content) lastAssistantContent = toolLastAssistant.content;

      } else if (activeModelType === 'cloud' && selectedCloudModel) {
        // === Streaming cloud chat (no tools) ===
        const preChatMs = Date.now() - preChatStart;
        console.log(`[HIVE] sendMessage: Streaming from cloud API... | pre-chat: ${preChatMs}ms`);
        let cloudResponse = '';
        let cloudThinking = '';
        let cloudRafPending = false;
        let thinkingRafPending = false;

        // Unique stream ID for this streaming call — prevents token collision
        // when multiple panes stream concurrently (each pane filters by its own ID).
        const streamId = `s-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;

        // Listen for content tokens (filtered by streamId for multi-pane isolation)
        const unlistenContent = await api.onCloudChatToken((token) => {
          cloudResponse += token;
          if (!cloudRafPending) {
            cloudRafPending = true;
            requestAnimationFrame(() => {
              setStreamingContent(cloudResponse);
              cloudRafPending = false;
            });
          }
        }, streamId);

        // Listen for thinking tokens (separate stream — P1: modularity)
        const unlistenThinking = await api.onCloudThinkingToken((token) => {
          cloudThinking += token;
          if (!thinkingRafPending) {
            thinkingRafPending = true;
            requestAnimationFrame(() => {
              setStreamingThinking(cloudThinking);
              thinkingRafPending = false;
            });
          }
        }, streamId);

        try {
          const streamThinkingDepth = appSettings.thinkingDepth?.[selectedCloudModel.provider] || undefined;
          const streamResult = await api.chatWithProviderStream(
            selectedCloudModel.provider,
            selectedCloudModel.model.id,
            apiMessages,
            streamId,
            streamThinkingDepth,
          );
          // Final flush — ensure last tokens are rendered
          const finalContent = streamResult.content || cloudResponse;
          const finalThinking = streamResult.thinking || (cloudThinking.trim() ? cloudThinking : undefined);
          setStreamingContent(finalContent);
          if (finalThinking) setStreamingThinking(finalThinking);
          console.log('[HIVE] sendMessage: Cloud stream complete, content:', finalContent.length, 'thinking:', finalThinking?.length ?? 0);

          if (finalContent && finalContent.trim()) {
            setMessages([...newMessages, { id: nextMsgId(), role: 'assistant', content: finalContent, thinking: finalThinking || undefined, senderName: modelSenderName }]);
            lastAssistantContent = finalContent;
          } else {
            console.warn('[HIVE] sendMessage: Cloud provider returned empty response');
            setMessages([...newMessages, {
              id: nextMsgId(),
              role: 'assistant',
              content: '*Provider returned an empty response. The model may have hit its context limit or the request was rejected.*',
            }]);
          }
        } finally {
          unlistenContent();
          unlistenThinking();
        }
      } else {
        // === Regular local streaming chat (no tools) ===
        const healthy = await api.checkServerHealth();
        if (!healthy) {
          throw new Error('Server not responding. Try restarting the model.');
        }

        console.log('[HIVE] sendMessage: Streaming from local server...');
        let response = '';
        let localRafPending = false;
        await api.chat(
          apiMessages,
          8080,
          (token) => {
            response += token;
            // Batch UI updates to animation frames (~60fps) instead of per-token
            if (!localRafPending) {
              localRafPending = true;
              requestAnimationFrame(() => {
                setStreamingContent(response);
                localRafPending = false;
              });
            }
          },
          abortControllerRef.current!.signal
        );
        // Final flush — strip thinking tags from local model output (P5: same pattern as Rust side)
        const [cleanLocal, localThinking] = api.stripThinking(response);
        setStreamingContent(cleanLocal);
        if (localThinking) setStreamingThinking(localThinking);
        console.log('[HIVE] sendMessage: Local response complete, content:', cleanLocal.length, 'thinking:', localThinking?.length ?? 0);

        if (cleanLocal && cleanLocal.trim()) {
          setMessages([...newMessages, { id: nextMsgId(), role: 'assistant', content: cleanLocal, thinking: localThinking || undefined, senderName: modelSenderName }]);
          lastAssistantContent = cleanLocal;
        } else {
          console.warn('[HIVE] sendMessage: Model returned empty response — context may be full');
          setMessages([...newMessages, {
            id: nextMsgId(),
            role: 'assistant',
            content: '*Model returned an empty response. This usually means the context window is full. Try starting a new conversation or reducing context length in Settings.*',
          }]);
        }
      }

      // === Unified Channel Guarantee: External Channel Delivery ===
      // Covers ALL code paths (tool loop, cloud streaming, local streaming).
      // If the message came from Telegram/Discord/PTY agent and no tool call already delivered,
      // force-send the last assistant content back to the originating channel.
      // The model does NOT get a choice. Channel in → channel out. Always.
      if (externalRoute && !channelDelivered && lastAssistantContent) {
        if (externalRoute.channel === 'pty-agent') {
          // PTY agent: write reply back to the terminal session via send_to_agent
          console.log(`[HIVE] CHANNEL_GUARANTEE: force-delivering to agent session ${externalRoute.chatId}`);
          try {
            const result = await api.executeTool('send_to_agent', {
              session_id: externalRoute.chatId,
              input: lastAssistantContent + '\n',
            });
            if (result.is_error) {
              console.error(`[HIVE] CHANNEL_GUARANTEE_FAIL (agent): ${result.content}`);
            }
          } catch (e) {
            console.error(`[HIVE] CHANNEL_GUARANTEE_FAIL (agent): ${String(e)}`);
          }
        } else {
          const sendTool = externalRoute.channel === 'telegram' ? 'telegram_send' : 'discord_send';
          const chatIdKey = externalRoute.channel === 'telegram' ? 'chat_id' : 'channel_id';
          console.log(`[HIVE] CHANNEL_GUARANTEE: force-delivering to ${externalRoute.channel} ${externalRoute.chatId}`);
          try {
            const result = await api.executeTool(sendTool, {
              [chatIdKey]: externalRoute.chatId,
              text: lastAssistantContent,
            });
            if (result.is_error) {
              console.error(`[HIVE] CHANNEL_GUARANTEE_FAIL: ${result.content}`);
            }
          } catch (e) {
            console.error(`[HIVE] CHANNEL_GUARANTEE_FAIL: ${String(e)}`);
          }
        }
      }
    } catch (e) {
      const isAbort = (e instanceof Error && e.name === 'AbortError') ||
                      (e instanceof DOMException && e.name === 'AbortError');
      if (!isAbort) {
        const rawMsg = e instanceof Error ? e.message
          : typeof e === 'string' ? e
          : JSON.stringify(e) !== '{}' ? JSON.stringify(e)
          : 'Unknown error';
        // Classify error for actionable user guidance
        const errMsg = classifyError(rawMsg);
        console.error('[HIVE] sendMessage: Error:', errMsg);
        setError(errMsg);
      } else {
        console.log('[HIVE] sendMessage: Generation aborted by user');
      }
    } finally {
      setIsGenerating(false);
      setStreamingContent('');
      setStreamingThinking('');
      setRoutingSpecialist(null);
      abortControllerRef.current = null;

      // Harness: execution time logged for debugging (memory system handles learning)
      if (appSettings.harnessEnabled) {
        const executionTimeMs = Date.now() - sendStartTime;
        console.log(`[HIVE] Harness: Turn completed in ${executionTimeMs}ms`);
      }

      // Drain queued external messages (Telegram/Discord that arrived while busy).
      // Process one at a time — each call to sendMessage will pick up the next.
      const nextQueued = pendingExternalRef.current.shift();
      if (nextQueued) {
        console.log(`[HIVE] sendMessage: Processing queued message (${pendingExternalRef.current.length} remaining)`);
        // Restore the original message origin before draining (P6: prevent privilege escalation).
        // Without this, a desktop message processed between queue and drain would reset
        // messageOriginRef to 'desktop', giving the remote message full tool access.
        messageOriginRef.current = nextQueued.origin;
        // Use setTimeout(0) to let React re-render (isGenerating = false) before re-entering.
        setTimeout(() => sendMessageRef.current?.(nextQueued.text), 0);
      }
    }
  }

  function stopGeneration() {
    if (abortControllerRef.current) {
      console.log('[HIVE] stopGeneration: Aborting current request');
      abortControllerRef.current.abort();
    }
  }

  /** Reset chat state — called by startNewConversation and clear messages */
  function resetChat() {
    setMessages([]);
    lastTruncatedCountRef.current = 0;
    contextSummarizedRef.current = false;
    contextSummarized80Ref.current = false;
    contextSummaryRef.current = null;
    hasWorkingMemoryRef.current = false;
  }

  // Keep sendMessageRef in sync — event listeners and queue drain use this
  // to always call the latest version (avoids stale closure bugs).
  sendMessageRef.current = sendMessage;

  // Phase 4 C3: Auto-sleep idle specialists (60s poll interval)
  // Specialists idle for >5 minutes get stopped to free VRAM.
  // Never sleeps consciousness. Best-effort — errors are silently ignored.
  useEffect(() => {
    const IDLE_TIMEOUT_MS = 5 * 60 * 1000; // 5 minutes
    const POLL_INTERVAL_MS = 60 * 1000; // check every 60s
    const timer = setInterval(async () => {
      try {
        const states = await api.getSlotStates();
        const now = Date.now();
        for (const state of states) {
          if (state.role === 'consciousness') continue;
          if (state.status !== 'active') continue;
          if (!state.last_active) continue;
          const lastActive = new Date(state.last_active).getTime();
          if (now - lastActive > IDLE_TIMEOUT_MS) {
            const rolePort = SPECIALIST_PORTS[state.role];
            if (!rolePort) continue;
            const healthy = await api.checkServerHealth(rolePort).catch(() => false);
            if (healthy) {
              const idleSecs = Math.round((now - lastActive) / 1000);
              console.log(`[HIVE] Auto-sleeping idle specialist: ${state.role} (idle ${idleSecs}s)`);
              api.logToApp(`SLOTS | auto_sleep | role=${state.role} | idle_seconds=${idleSecs} | port=${rolePort}`);
              await api.stopSpecialistServer(state.role);
              api.recordSlotSleep(state.role as api.SlotRole).catch(() => {});
              api.magmaAddEvent('specialist_sleep', state.role, `Auto-sleep: idle ${idleSecs}s`).catch(() => {});
            }
          }
        }
      } catch {
        // Auto-sleep is best-effort
      }
    }, POLL_INTERVAL_MS);
    return () => clearInterval(timer);
  }, []); // Run once on mount

  return {
    // Chat state
    messages, setMessages,
    input, setInput,
    isGenerating,
    streamingContent,
    streamingThinking,
    routingSpecialist,
    messagesEndRef,
    // Tool state
    availableTools,
    toolsEnabled, setToolsEnabled,
    pendingToolCalls,
    toolApprovalCallback,
    // Harness
    lastHarnessContext,
    // Refs exposed for App.tsx effects
    lastTruncatedCountRef,
    contextSummarizedRef,
    hasWorkingMemoryRef,
    sendMessageRef,
    messageOriginRef,
    // Functions
    sendMessage,
    stopGeneration,
    resetChat,
  };
}

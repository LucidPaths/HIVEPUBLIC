// chainPolicies.ts — Pure functions for tool chaining, classification, and formatting.
// Zero I/O, zero React dependencies. Every function is deterministic and testable (P1, P3).
//
// Extracted from useChat.ts (Phase 4A) — these functions were already exported and tested.

import { parseChannelPrompt } from './channelPrompt';
import type { ChannelRoute } from './channelPrompt';
import type { ToolCall, Message } from '../types';

// ============================================
// Plan Execution Helpers (Phase 7 — tool chaining)
// ============================================

export interface PlanStep {
  tool: string;
  args: Record<string, unknown>;
  save_as?: string;
  condition?: string;
}

/** Replace $variable_name (and $variable.field) in strings with stored plan results.
 *  Dot-notation: $scratchpad.id looks up "scratchpad.id" in variables first,
 *  then falls back to "$scratchpad.id" (unresolved). Fields are extracted by
 *  executePlanSteps when save_as stores tool results. */
export function substitutePlanVariables(value: unknown, variables: Map<string, string>): unknown {
  if (typeof value === 'string') {
    return value.replace(/\$([a-zA-Z_][a-zA-Z0-9_]*(?:\.[a-zA-Z_][a-zA-Z0-9_]*)*)/g, (_match, name) => {
      return variables.get(name) ?? `$${name}`;
    });
  }
  if (Array.isArray(value)) {
    return value.map(v => substitutePlanVariables(v, variables));
  }
  if (typeof value === 'object' && value !== null) {
    const result: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(value)) {
      result[k] = substitutePlanVariables(v, variables);
    }
    return result;
  }
  return value;
}

/** Evaluate a plan condition — truthy if substituted value is non-empty and not an error */
export function evaluatePlanCondition(condition: string, variables: Map<string, string>): boolean {
  const substituted = condition.replace(/\$([a-zA-Z_][a-zA-Z0-9_]*(?:\.[a-zA-Z_][a-zA-Z0-9_]*)*)/g, (_match, name) => {
    return variables.get(name) ?? '';
  });
  const trimmed = substituted.trim();
  return trimmed.length > 0 && !trimmed.startsWith('TOOL_ERROR') && !trimmed.startsWith('TOOL_EXCEPTION');
}

// ============================================
// Chain Policies (P1 — modular tool chaining)
// ============================================
// Each policy is a pure function operating on tool calls + state.
// The while-loop composes them; adding/removing a policy never touches the loop body.

/** Tools that signal task completion — chain terminates after these succeed.
 *  Module-level so new terminal tools are added in one place. */
export const TERMINAL_TOOLS = new Set(['telegram_send', 'discord_send']);

/** Repetition tracking state across tool loop turns */
export interface RepetitionState {
  lastToolSet: string;
  lastFingerprint: string;
  consecutiveCount: number;
  /** Sliding window of recent tool sets for ping-pong detection (A→B→A→B) */
  recentSets: string[];
}

export const REPETITION_INITIAL: RepetitionState = {
  lastToolSet: '',
  lastFingerprint: '',
  consecutiveCount: 0,
  recentSets: [],
};

/** Detect model stuck in a tool call loop.
 *  Two-tier: exact-same calls (fast break) vs same-tools-different-args (slower break).
 *  Also detects ping-pong patterns (A→B→A→B) that single-previous tracking misses. */
export function detectRepetition(
  toolCalls: ToolCall[],
  prev: RepetitionState,
): { state: RepetitionState; stuck: boolean; reason?: string } {
  const currentToolSet = toolCalls.map(t => t.name).sort().join(',');
  const currentFingerprint = toolCalls
    .map(t => `${t.name}:${JSON.stringify(t.arguments)}`)
    .sort().join('|');

  let consecutiveCount = prev.consecutiveCount;
  if (currentToolSet === prev.lastToolSet) {
    if (currentFingerprint === prev.lastFingerprint) {
      consecutiveCount += 2; // Exact same call — truly stuck, fast-track
    } else {
      consecutiveCount++; // Same tools, different args — might be refining
    }
    if (consecutiveCount >= 3) {
      return {
        state: { lastToolSet: currentToolSet, lastFingerprint: currentFingerprint, consecutiveCount, recentSets: [...prev.recentSets.slice(-5), currentToolSet] },
        stuck: true,
        reason: `"${currentToolSet}" repeated ${consecutiveCount} times`,
      };
    }
  } else {
    consecutiveCount = 0;
  }

  // Ping-pong detection: A→B→A→B pattern over sliding window
  const recentSets = [...prev.recentSets.slice(-5), currentToolSet];
  if (recentSets.length >= 4) {
    const last4 = recentSets.slice(-4);
    if (last4[0] === last4[2] && last4[1] === last4[3] && last4[0] !== last4[1]) {
      return {
        state: { lastToolSet: currentToolSet, lastFingerprint: currentFingerprint, consecutiveCount, recentSets },
        stuck: true,
        reason: `ping-pong: "${last4[0]}" ↔ "${last4[1]}"`,
      };
    }
  }

  return {
    state: { lastToolSet: currentToolSet, lastFingerprint: currentFingerprint, consecutiveCount, recentSets },
    stuck: false,
  };
}

/** Classify tool calls into execute-now vs deferred.
 *  Terminal tools mixed with research/data tools get deferred — the model composed
 *  the message BEFORE seeing research results. Deferral forces see-then-send. */
export function classifyToolCalls(toolCalls: ToolCall[]): { execute: ToolCall[]; deferred: ToolCall[] } {
  const hasTerminal = toolCalls.some(tc => TERMINAL_TOOLS.has(tc.name));
  const hasResearch = toolCalls.some(tc => !TERMINAL_TOOLS.has(tc.name));
  if (hasTerminal && hasResearch) {
    return {
      execute: toolCalls.filter(tc => !TERMINAL_TOOLS.has(tc.name)),
      deferred: toolCalls.filter(tc => TERMINAL_TOOLS.has(tc.name)),
    };
  }
  return { execute: toolCalls, deferred: [] };
}

/** Check if a terminal tool succeeded this turn — chain should terminate. */
export function isChainComplete(toolCalls: ToolCall[], messages: Message[]): boolean {
  return toolCalls.some(tc =>
    TERMINAL_TOOLS.has(tc.name) &&
    messages.some(m => m.toolCallId === tc.id && m.role === 'tool' && m.content.startsWith('TOOL_OK'))
  );
}

/** Detect if the user's message came from an external channel and extract routing info.
 *  Delegates to channelPrompt.ts — the single source of truth for format + parsing (P5). */
export function detectExternalChannel(userContent: string): ChannelRoute | null {
  return parseChannelPrompt(userContent);
}

// ============================================
// Tool Result Helpers (extracted for testability — P1, P3)
// ============================================

/** Compute max chars for tool results based on model's context window.
 *  Formula: 30% of context converted to chars (×4 char/token estimate), clamped to [4000, 40000]. */
export function computeToolResultMaxChars(maxContext: number): number {
  return Math.max(4000, Math.min(40000, Math.floor(maxContext * 0.3 * 4)));
}

/** Chain history entry for procedure extraction */
export interface ChainHistoryEntry {
  name: string;
  argsKeys: string[];
  success: boolean;
}

/** Format a tool execution result into a Message with status prefix and optional truncation.
 *  Pure function — no I/O, no logging. Caller handles logging separately.
 *  Handles three cases: TOOL_OK (success), TOOL_ERROR (tool returned error), TOOL_EXCEPTION (execution threw). */
export function formatToolResult(
  tc: ToolCall,
  result: { content: string; is_error: boolean },
  maxResultChars: number,
): { message: Message; wasTruncated: boolean } {
  let resultContent = result.content;
  let wasTruncated = false;

  if (resultContent.length > maxResultChars) {
    wasTruncated = true;
    resultContent = resultContent.substring(0, maxResultChars);
    if (tc.name === 'read_file') {
      resultContent += `\n\n[... context-truncated from ${result.content.length} to ${maxResultChars} chars. Use read_file with a smaller "limit" (e.g. 200) or use "offset" to read specific sections.]`;
    } else {
      resultContent += `\n\n[... truncated from ${result.content.length} to ${maxResultChars} chars — result too large for context]`;
    }
  }

  const statusPrefix = result.is_error
    ? `TOOL_ERROR [${tc.name}]: `
    : `TOOL_OK [${tc.name}]: `;

  return {
    message: {
      role: 'tool',
      content: statusPrefix + resultContent,
      toolCallId: tc.id,
      toolName: tc.name,
    },
    wasTruncated,
  };
}

/** Determine if a tool chain should be saved as a learned procedure.
 *  Conditions: 2-5 steps, all successful. Too short = trivial, too long = complex/fragile. */
export function shouldSaveProcedure(chainHistory: ChainHistoryEntry[]): boolean {
  const successfulTools = chainHistory.filter(t => t.success);
  return successfulTools.length >= 2 && successfulTools.length <= 5 && chainHistory.every(t => t.success);
}

/** Build procedure metadata from a successful chain history.
 *  Returns null if chain doesn't qualify (use shouldSaveProcedure first). */
export function buildProcedureData(
  chainHistory: ChainHistoryEntry[],
  triggerText: string,
): { chainName: string; triggerPattern: string; steps: { tool: string; arg_pattern: string[] }[] } | null {
  if (!shouldSaveProcedure(chainHistory)) return null;
  const successfulTools = chainHistory.filter(t => t.success);
  const triggerPattern = triggerText.substring(0, 100).toLowerCase().trim();
  const chainName = successfulTools.map(t => t.name).join(' → ');
  const steps = successfulTools.map(t => ({ tool: t.name, arg_pattern: t.argsKeys }));
  return { chainName, triggerPattern, steps };
}

/** Build volatile context string in TypeScript (mirrors Rust build_volatile_context).
 *  Pure number formatting — no I/O. Saves a full Rust IPC + harness rebuild on cache hits. */
export function buildVolatileContext(opts: {
  conversationTurns: number;
  messagesTruncated: number;
  vramUsedMb?: number | null;
  vramFreeMb?: number | null;
  vramGb?: number | null;
  gpuUtilization?: number | null;
  activeModelVramGb?: number | null;
  contextLength?: number | null;
  tokensUsed?: number | null;
  hasWorkingMemory: boolean;
  ramAvailableMb?: number | null;
}): string {
  const parts: string[] = [];

  if (opts.conversationTurns > 0) {
    let info = `Turn ${opts.conversationTurns}`;
    if (opts.messagesTruncated > 0) {
      info += ` — ${opts.messagesTruncated} earlier messages were dropped to fit context`;
    }
    parts.push(info);
  }

  if (opts.vramUsedMb != null && opts.vramFreeMb != null) {
    const used = opts.vramUsedMb / 1024;
    const free = opts.vramFreeMb / 1024;
    const total = opts.vramGb ?? (used + free);
    const util = opts.gpuUtilization != null ? `, ${opts.gpuUtilization}% GPU util` : '';
    let vram = `VRAM: ${used.toFixed(1)}/${total.toFixed(0)} GB used (${free.toFixed(1)} GB free${util})`;
    if (opts.activeModelVramGb != null) vram += `, model uses ~${opts.activeModelVramGb.toFixed(1)} GB`;
    if (free > 10) vram += ' — room for 13B+ alongside';
    else if (free > 5) vram += ' — room for 7-8B Q4 alongside';
    else if (free > 2.5) vram += ' — room for small 3B alongside';
    else vram += ' — VRAM near full';
    parts.push(vram);
  }

  if (opts.tokensUsed != null && opts.contextLength && opts.contextLength > 0) {
    const pct = Math.round(opts.tokensUsed / opts.contextLength * 100);
    const ctxK = (opts.contextLength / 1000).toFixed(0);
    const usedK = (opts.tokensUsed / 1000).toFixed(1);
    let ctx = `Context: ${usedK}K/${ctxK}K tokens (${pct}%)`;
    if (pct >= 80) ctx += ' — CRITICAL: context nearly full, summarize key points to working memory NOW';
    else if (pct >= 70) ctx += ' — HIGH: consider summarizing important context to working memory';
    else if (pct >= 50) ctx += ' — moderate';
    parts.push(ctx);
  }

  if (opts.hasWorkingMemory) {
    parts.push('Working memory: active (session scratchpad has content)');
  }

  if (opts.ramAvailableMb != null) {
    parts.push(`RAM free: ${(opts.ramAvailableMb / 1024).toFixed(0)} GB`);
  }

  return parts.length > 0 ? `[Live Status] ${parts.join(' | ')}` : '';
}

// ============================================
// Error Classification + Model Filename Parsing
// ============================================

/** Classify errors into actionable categories (OpenClaw pattern) */
export function classifyError(msg: string): string {
  const lower = msg.toLowerCase();
  // Context overflow
  if (lower.includes('context') && (lower.includes('length') || lower.includes('exceed') || lower.includes('overflow'))
      || lower.includes('maximum context') || lower.includes('too many tokens') || /token.{0,20}limit/.test(lower)) {
    return `Context overflow: ${msg}\n\nTry: reduce context length in model settings, start a new conversation, or use a model with a larger context window.`;
  }
  // Rate limiting
  if (lower.includes('429') || lower.includes('rate limit') || lower.includes('too many requests') || lower.includes('quota')) {
    return `Rate limited: ${msg}\n\nThe API provider is throttling requests. Wait a moment and try again.`;
  }
  // Auth errors
  if (lower.includes('401') || lower.includes('403') || lower.includes('unauthorized')
      || (lower.includes('invalid') && lower.includes('key')) || lower.includes('authentication')) {
    return `Authentication error: ${msg}\n\nCheck your API key in Settings → Provider Keys.`;
  }
  // Network errors
  if (lower.includes('fetch') || lower.includes('network') || lower.includes('econnrefused')
      || lower.includes('timeout') || lower.includes('502') || lower.includes('503') || lower.includes('enotfound')) {
    return `Network error: ${msg}\n\nCheck your internet connection, or verify the model server is running.`;
  }
  // Local model crash / VRAM
  if (lower.includes('vram') || lower.includes('out of memory') || lower.includes('oom') || lower.includes('server crashed')) {
    return `Model crashed (likely out of memory): ${msg}\n\nTry: reduce GPU layers, lower context length, or use a smaller/more quantized model.`;
  }
  return msg;
}

/** Parse model metadata from GGUF filename (standard naming convention)
 *  e.g. "mistral-7b-instruct-v0.2.Q4_K_M.gguf" → { params: "7B", quant: "Q4_K_M" } */
export function parseModelFilename(filename: string): { params?: string; quant?: string; arch?: string } {
  const result: { params?: string; quant?: string; arch?: string } = {};

  // Extract parameter count: matches "7b", "13b", "70b", "1.5b", etc.
  const paramMatch = filename.match(/[\-_.](\d+(?:\.\d+)?)[bB][\-_.]/);
  if (paramMatch) result.params = paramMatch[1] + 'B';

  // Extract quantization: matches Q4_K_M, Q5_K_S, Q8_0, IQ4_NL, F16, etc.
  const quantMatch = filename.match(/((?:I?Q\d+_[A-Z0-9_]+|Q\d+_\d+|F(?:16|32)|BF16))/i);
  if (quantMatch) result.quant = quantMatch[1].toUpperCase();

  // Infer architecture from common model name prefixes
  const lower = filename.toLowerCase();
  const archPatterns: [RegExp, string][] = [
    [/llama/, 'llama'], [/mistral/, 'mistral'], [/mixtral/, 'mixtral'],
    [/qwen/, 'qwen'], [/phi[-_]/, 'phi'], [/gemma/, 'gemma'],
    [/deepseek/, 'deepseek'], [/command[-_]r/, 'command-r'],
    [/codestral/, 'codestral'], [/starcoder/, 'starcoder'],
  ];
  for (const [pat, arch] of archPatterns) {
    if (pat.test(lower)) { result.arch = arch; break; }
  }

  return result;
}

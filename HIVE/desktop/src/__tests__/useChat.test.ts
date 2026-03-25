// Tests for useChat pure functions (chain policies, plan helpers, channel detection)

import { describe, it, expect } from 'vitest';
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
} from '../useChat';
import { checkToolOriginAccess } from '../lib/api';
import { buildTelegramPrompt, buildDiscordPrompt, parseChannelPrompt } from '../lib/channelPrompt';
import type { RepetitionState, ChainHistoryEntry } from '../useChat';
import type { ToolCall, Message } from '../types';

// ============================================
// substitutePlanVariables
// ============================================

describe('substitutePlanVariables', () => {
  it('replaces simple variable in string', () => {
    const vars = new Map([['name', 'HIVE']]);
    expect(substitutePlanVariables('Hello $name', vars)).toBe('Hello HIVE');
  });

  it('replaces dot-notation variable', () => {
    const vars = new Map([['scratchpad.id', 'abc123']]);
    expect(substitutePlanVariables('Use $scratchpad.id here', vars)).toBe('Use abc123 here');
  });

  it('preserves unresolved variables', () => {
    const vars = new Map<string, string>();
    expect(substitutePlanVariables('$unknown stays', vars)).toBe('$unknown stays');
  });

  it('replaces variables in nested objects', () => {
    const vars = new Map([['file', 'test.txt']]);
    const input = { path: '$file', meta: { name: '$file' } };
    expect(substitutePlanVariables(input, vars)).toEqual({ path: 'test.txt', meta: { name: 'test.txt' } });
  });

  it('replaces variables in arrays', () => {
    const vars = new Map([['x', 'val']]);
    expect(substitutePlanVariables(['$x', 'literal'], vars)).toEqual(['val', 'literal']);
  });

  it('passes through non-string primitives unchanged', () => {
    const vars = new Map<string, string>();
    expect(substitutePlanVariables(42, vars)).toBe(42);
    expect(substitutePlanVariables(true, vars)).toBe(true);
    expect(substitutePlanVariables(null, vars)).toBe(null);
  });

  it('replaces multiple variables in one string', () => {
    const vars = new Map([['a', 'X'], ['b', 'Y']]);
    expect(substitutePlanVariables('$a and $b', vars)).toBe('X and Y');
  });
});

// ============================================
// evaluatePlanCondition
// ============================================

describe('evaluatePlanCondition', () => {
  it('returns true for resolved non-empty variable', () => {
    const vars = new Map([['result', 'some data']]);
    expect(evaluatePlanCondition('$result', vars)).toBe(true);
  });

  it('returns false for unresolved variable (empty string)', () => {
    const vars = new Map<string, string>();
    expect(evaluatePlanCondition('$missing', vars)).toBe(false);
  });

  it('returns false for TOOL_ERROR result', () => {
    const vars = new Map([['result', 'TOOL_ERROR: file not found']]);
    expect(evaluatePlanCondition('$result', vars)).toBe(false);
  });

  it('returns false for TOOL_EXCEPTION result', () => {
    const vars = new Map([['result', 'TOOL_EXCEPTION: timeout']]);
    expect(evaluatePlanCondition('$result', vars)).toBe(false);
  });

  it('returns true for literal non-empty string', () => {
    const vars = new Map<string, string>();
    expect(evaluatePlanCondition('always true', vars)).toBe(true);
  });

  it('returns false for whitespace-only string', () => {
    const vars = new Map<string, string>();
    expect(evaluatePlanCondition('   ', vars)).toBe(false);
  });
});

// ============================================
// detectRepetition
// ============================================

describe('detectRepetition', () => {
  const makeToolCall = (name: string, args: Record<string, unknown> = {}): ToolCall => ({
    id: `call_${Math.random().toString(36).slice(2)}`,
    name,
    arguments: args,
  });

  it('returns not stuck for first call', () => {
    const result = detectRepetition([makeToolCall('web_search')], REPETITION_INITIAL);
    expect(result.stuck).toBe(false);
  });

  it('returns not stuck for different tools', () => {
    const first = detectRepetition([makeToolCall('web_search')], REPETITION_INITIAL);
    const second = detectRepetition([makeToolCall('read_file')], first.state);
    expect(second.stuck).toBe(false);
  });

  it('detects exact same call repeated (fast-track: +2 per exact match, triggers at >=3)', () => {
    const tc = [makeToolCall('web_search', { query: 'test' })];
    const r1 = detectRepetition(tc, REPETITION_INITIAL);
    expect(r1.stuck).toBe(false); // First time: count = 0 (different from initial empty)

    // Second time: same tool+args → consecutiveCount += 2 → 2 (threshold is >= 3, so not stuck yet)
    const r2 = detectRepetition(tc, r1.state);
    expect(r2.stuck).toBe(false);
    expect(r2.state.consecutiveCount).toBe(2);

    // Third time: another +2 → 4 (>= 3 → stuck!)
    const r3 = detectRepetition(tc, r2.state);
    expect(r3.stuck).toBe(true);
    expect(r3.reason).toContain('repeated');
  });

  it('detects same tool with different args after 4 rounds', () => {
    let state: RepetitionState = REPETITION_INITIAL;
    let caught = false;
    // Different args: +1 per match, triggers at >= 3
    for (let i = 0; i < 5; i++) {
      const result = detectRepetition([makeToolCall('web_search', { query: `q${i}` })], state);
      state = result.state;
      if (result.stuck) {
        caught = true;
        break;
      }
    }
    expect(caught).toBe(true);
  });

  it('detects ping-pong pattern A-B-A-B', () => {
    let state: RepetitionState = REPETITION_INITIAL;
    const tools = ['web_search', 'read_file', 'web_search', 'read_file'];
    let caught = false;
    for (const tool of tools) {
      const result = detectRepetition([makeToolCall(tool)], state);
      state = result.state;
      if (result.stuck) {
        expect(result.reason).toContain('ping-pong');
        caught = true;
        break;
      }
    }
    expect(caught).toBe(true);
  });

  it('resets consecutive count when tool changes', () => {
    let state: RepetitionState = REPETITION_INITIAL;
    const r1 = detectRepetition([makeToolCall('web_search', { q: '1' })], state);
    const r2 = detectRepetition([makeToolCall('web_search', { q: '2' })], r1.state);
    // Now switch to a different tool — should reset
    const r3 = detectRepetition([makeToolCall('read_file')], r2.state);
    expect(r3.state.consecutiveCount).toBe(0);
    expect(r3.stuck).toBe(false);
  });
});

// ============================================
// classifyToolCalls
// ============================================

describe('classifyToolCalls', () => {
  const makeTC = (name: string): ToolCall => ({ id: `id_${name}`, name, arguments: {} });

  it('executes all calls when no terminal tools', () => {
    const calls = [makeTC('web_search'), makeTC('read_file')];
    const result = classifyToolCalls(calls);
    expect(result.execute).toEqual(calls);
    expect(result.deferred).toEqual([]);
  });

  it('executes all calls when only terminal tools', () => {
    const calls = [makeTC('telegram_send')];
    const result = classifyToolCalls(calls);
    expect(result.execute).toEqual(calls);
    expect(result.deferred).toEqual([]);
  });

  it('defers terminal tools when mixed with research tools', () => {
    const calls = [makeTC('web_search'), makeTC('telegram_send'), makeTC('read_file')];
    const result = classifyToolCalls(calls);
    expect(result.execute.map(t => t.name)).toEqual(['web_search', 'read_file']);
    expect(result.deferred.map(t => t.name)).toEqual(['telegram_send']);
  });

  it('defers discord_send when mixed', () => {
    const calls = [makeTC('discord_send'), makeTC('memory_search')];
    const result = classifyToolCalls(calls);
    expect(result.execute.map(t => t.name)).toEqual(['memory_search']);
    expect(result.deferred.map(t => t.name)).toEqual(['discord_send']);
  });
});

// ============================================
// isChainComplete
// ============================================

describe('isChainComplete', () => {
  it('returns true when terminal tool succeeded', () => {
    const toolCalls: ToolCall[] = [{ id: 'tc1', name: 'telegram_send', arguments: {} }];
    const messages: Message[] = [
      { role: 'tool', content: 'TOOL_OK: Message sent', toolCallId: 'tc1' },
    ];
    expect(isChainComplete(toolCalls, messages)).toBe(true);
  });

  it('returns false when terminal tool errored', () => {
    const toolCalls: ToolCall[] = [{ id: 'tc1', name: 'telegram_send', arguments: {} }];
    const messages: Message[] = [
      { role: 'tool', content: 'TOOL_ERROR: Failed to send', toolCallId: 'tc1' },
    ];
    expect(isChainComplete(toolCalls, messages)).toBe(false);
  });

  it('returns false for non-terminal tool success', () => {
    const toolCalls: ToolCall[] = [{ id: 'tc1', name: 'web_search', arguments: {} }];
    const messages: Message[] = [
      { role: 'tool', content: 'TOOL_OK: Results found', toolCallId: 'tc1' },
    ];
    expect(isChainComplete(toolCalls, messages)).toBe(false);
  });

  it('returns false when no matching tool result message', () => {
    const toolCalls: ToolCall[] = [{ id: 'tc1', name: 'telegram_send', arguments: {} }];
    const messages: Message[] = [];
    expect(isChainComplete(toolCalls, messages)).toBe(false);
  });

  it('returns true if any terminal tool in batch succeeded', () => {
    const toolCalls: ToolCall[] = [
      { id: 'tc1', name: 'web_search', arguments: {} },
      { id: 'tc2', name: 'discord_send', arguments: {} },
    ];
    const messages: Message[] = [
      { role: 'tool', content: 'TOOL_OK: Results', toolCallId: 'tc1' },
      { role: 'tool', content: 'TOOL_OK: Sent', toolCallId: 'tc2' },
    ];
    expect(isChainComplete(toolCalls, messages)).toBe(true);
  });
});

// ============================================
// detectExternalChannel
// ============================================

describe('detectExternalChannel', () => {
  it('detects Telegram message', () => {
    const result = detectExternalChannel('[Telegram from John (@johndoe) | chat: 12345] Hello');
    expect(result).toEqual({ channel: 'telegram', chatId: '12345', senderName: 'John' });
  });

  it('detects Discord message', () => {
    const result = detectExternalChannel('[Discord from Alice (@alice) | ch: 99887766] Hey there');
    expect(result).toEqual({ channel: 'discord', chatId: '99887766', senderName: 'Alice' });
  });

  it('returns null for regular chat message', () => {
    expect(detectExternalChannel('Hello, how are you?')).toBeNull();
  });

  it('returns null for partial match', () => {
    expect(detectExternalChannel('[Telegram from')).toBeNull();
  });

  it('handles Telegram without username', () => {
    const result = detectExternalChannel('[Telegram from Bob | chat: 555] Hi');
    expect(result).toEqual({ channel: 'telegram', chatId: '555', senderName: 'Bob' });
  });

  it('handles Discord with extra pipe-separated fields', () => {
    const result = detectExternalChannel('[Discord from Eve (@eve) | ch: 123 | guild: 456] test');
    expect(result).toEqual({ channel: 'discord', chatId: '123', senderName: 'Eve' });
  });

  it('handles Telegram with role tag (Host)', () => {
    const result = detectExternalChannel('[Telegram from TestUser (@testuser42) | chat: 99999999 | Host]\nHello');
    expect(result).toEqual({ channel: 'telegram', chatId: '99999999', senderName: 'TestUser' });
  });

  it('handles Telegram with role tag (User)', () => {
    const result = detectExternalChannel('[Telegram from Bob (@bob) | chat: 12345 | User]\nHi');
    expect(result).toEqual({ channel: 'telegram', chatId: '12345', senderName: 'Bob' });
  });

  it('handles Discord with role tag', () => {
    const result = detectExternalChannel('[Discord from Alice (@alice) | ch: 99887766 | Host]\nHey');
    expect(result).toEqual({ channel: 'discord', chatId: '99887766', senderName: 'Alice' });
  });
});

// ============================================
// TERMINAL_TOOLS constant
// ============================================

describe('TERMINAL_TOOLS', () => {
  it('contains telegram_send', () => {
    expect(TERMINAL_TOOLS.has('telegram_send')).toBe(true);
  });

  it('contains discord_send', () => {
    expect(TERMINAL_TOOLS.has('discord_send')).toBe(true);
  });

  it('does not contain non-terminal tools', () => {
    expect(TERMINAL_TOOLS.has('web_search')).toBe(false);
    expect(TERMINAL_TOOLS.has('read_file')).toBe(false);
    expect(TERMINAL_TOOLS.has('memory_save')).toBe(false);
  });
});

// ============================================
// channelPrompt round-trip (P5: format + parser in sync)
// ============================================

describe('channelPrompt round-trip', () => {
  it('buildTelegramPrompt → parseChannelPrompt round-trips correctly', () => {
    const prompt = buildTelegramPrompt('TestUser', 'testuser42', '99999999', 'Host', 'Hello there');
    const parsed = parseChannelPrompt(prompt);
    expect(parsed).toEqual({ channel: 'telegram', chatId: '99999999', senderName: 'TestUser' });
  });

  it('buildDiscordPrompt → parseChannelPrompt round-trips correctly', () => {
    const prompt = buildDiscordPrompt('Alice', '99887766', '12345', 'User', 'Hey');
    const parsed = parseChannelPrompt(prompt);
    expect(parsed).toEqual({ channel: 'discord', chatId: '99887766', senderName: 'Alice' });
  });

  it('buildTelegramPrompt without username round-trips', () => {
    const prompt = buildTelegramPrompt('Bob', undefined, '555', 'User', 'Hi');
    const parsed = parseChannelPrompt(prompt);
    expect(parsed).toEqual({ channel: 'telegram', chatId: '555', senderName: 'Bob' });
  });

  it('buildDiscordPrompt without guild round-trips', () => {
    const prompt = buildDiscordPrompt('Eve', '123', undefined, 'Host', 'test');
    const parsed = parseChannelPrompt(prompt);
    expect(parsed).toEqual({ channel: 'discord', chatId: '123', senderName: 'Eve' });
  });

  it('detectExternalChannel delegates to parseChannelPrompt', () => {
    const prompt = buildTelegramPrompt('Test', 'testuser', '999', 'Host', 'msg');
    expect(detectExternalChannel(prompt)).toEqual(parseChannelPrompt(prompt));
  });
});

// ============================================
// Phase 10: normalizeCommand (agent routing)
// ============================================

import { normalizeCommand } from '../hooks/useRemoteChannels';

describe('normalizeCommand', () => {
  it('returns bare command unchanged (lowercase)', () => {
    expect(normalizeCommand('claude')).toBe('claude');
  });

  it('strips .exe extension', () => {
    expect(normalizeCommand('claude.exe')).toBe('claude');
  });

  it('strips .cmd and .bat extensions', () => {
    expect(normalizeCommand('codex.cmd')).toBe('codex');
    expect(normalizeCommand('aider.bat')).toBe('aider');
  });

  it('extracts basename from Unix path', () => {
    expect(normalizeCommand('/usr/bin/claude')).toBe('claude');
    expect(normalizeCommand('/home/user/.local/bin/aider')).toBe('aider');
  });

  it('extracts basename from Windows path', () => {
    expect(normalizeCommand('C:\\Users\\me\\AppData\\Local\\claude.exe')).toBe('claude');
    expect(normalizeCommand('D:\\tools\\codex.exe')).toBe('codex');
  });

  it('handles mixed separators', () => {
    expect(normalizeCommand('C:\\Program Files/bin\\claude.exe')).toBe('claude');
  });

  it('is case-insensitive', () => {
    expect(normalizeCommand('Claude.EXE')).toBe('claude');
    expect(normalizeCommand('CODEX')).toBe('codex');
  });
});

// ============================================
// Phase 2C: computeToolResultMaxChars
// ============================================

describe('computeToolResultMaxChars', () => {
  it('clamps to minimum 4000 for tiny context windows', () => {
    expect(computeToolResultMaxChars(0)).toBe(4000);
    expect(computeToolResultMaxChars(1000)).toBe(4000);
    expect(computeToolResultMaxChars(2000)).toBe(4000);
  });

  it('scales linearly within range', () => {
    // 8192 tokens → 8192 * 0.3 * 4 = 9830
    expect(computeToolResultMaxChars(8192)).toBe(9830);
    // 16384 → 16384 * 0.3 * 4 = 19660
    expect(computeToolResultMaxChars(16384)).toBe(19660);
  });

  it('clamps to maximum 40000 for large context windows', () => {
    // 128000 → 128000 * 0.3 * 4 = 153600 → clamped to 40000
    expect(computeToolResultMaxChars(128000)).toBe(40000);
    expect(computeToolResultMaxChars(200000)).toBe(40000);
  });

  it('handles 4K context (common small model)', () => {
    // 4096 * 0.3 * 4 = 4915
    expect(computeToolResultMaxChars(4096)).toBe(4915);
  });

  it('handles 32K context (common mid-range)', () => {
    // 32768 * 0.3 * 4 = 39321
    expect(computeToolResultMaxChars(32768)).toBe(39321);
  });

  it('boundary: exact minimum threshold', () => {
    // Need maxContext * 0.3 * 4 = 4000 → maxContext = 3333.33
    // At 3334: 3334 * 0.3 * 4 = 4000.8 → floor = 4000
    expect(computeToolResultMaxChars(3334)).toBe(4000);
    // At 3333: 3333 * 0.3 * 4 = 3999.6 → floor = 3999 → clamped to 4000
    expect(computeToolResultMaxChars(3333)).toBe(4000);
  });
});

// ============================================
// Phase 2C: formatToolResult
// ============================================

describe('formatToolResult', () => {
  const makeTC = (name: string, id = 'tc_1'): ToolCall => ({ id, name, arguments: {} });

  it('prefixes successful result with TOOL_OK', () => {
    const { message } = formatToolResult(
      makeTC('web_search'),
      { content: 'Found 3 results', is_error: false },
      10000,
    );
    expect(message.role).toBe('tool');
    expect(message.content).toBe('TOOL_OK [web_search]: Found 3 results');
    expect(message.toolCallId).toBe('tc_1');
    expect(message.toolName).toBe('web_search');
  });

  it('prefixes error result with TOOL_ERROR', () => {
    const { message } = formatToolResult(
      makeTC('read_file'),
      { content: 'File not found: /missing.txt', is_error: true },
      10000,
    );
    expect(message.content).toBe('TOOL_ERROR [read_file]: File not found: /missing.txt');
  });

  it('does not truncate when content fits', () => {
    const content = 'x'.repeat(5000);
    const { message, wasTruncated } = formatToolResult(
      makeTC('memory_search'),
      { content, is_error: false },
      10000,
    );
    expect(wasTruncated).toBe(false);
    expect(message.content).toBe(`TOOL_OK [memory_search]: ${content}`);
  });

  it('truncates long content with generic message', () => {
    const content = 'y'.repeat(20000);
    const { message, wasTruncated } = formatToolResult(
      makeTC('web_search'),
      { content, is_error: false },
      5000,
    );
    expect(wasTruncated).toBe(true);
    expect(message.content).toContain('TOOL_OK [web_search]:');
    expect(message.content).toContain('[... truncated from 20000 to 5000 chars');
    expect(message.content).toContain('result too large for context');
  });

  it('truncates read_file with specific hint about limit/offset', () => {
    const content = 'z'.repeat(15000);
    const { message, wasTruncated } = formatToolResult(
      makeTC('read_file'),
      { content, is_error: false },
      4000,
    );
    expect(wasTruncated).toBe(true);
    expect(message.content).toContain('context-truncated from 15000 to 4000 chars');
    expect(message.content).toContain('Use read_file with a smaller "limit"');
    expect(message.content).toContain('"offset" to read specific sections');
  });

  it('truncation preserves first maxResultChars of content', () => {
    const content = 'A'.repeat(3000) + 'B'.repeat(3000);
    const { message } = formatToolResult(
      makeTC('web_search'),
      { content, is_error: false },
      3000,
    );
    // Should have the prefix + first 3000 'A's + truncation notice
    const afterPrefix = message.content.replace('TOOL_OK [web_search]: ', '');
    expect(afterPrefix.startsWith('A'.repeat(3000))).toBe(true);
    expect(afterPrefix).not.toContain('B');
  });

  it('preserves toolCallId and toolName in returned message', () => {
    const { message } = formatToolResult(
      { id: 'call_abc123', name: 'memory_save', arguments: { text: 'hello' } },
      { content: 'Saved', is_error: false },
      10000,
    );
    expect(message.toolCallId).toBe('call_abc123');
    expect(message.toolName).toBe('memory_save');
    expect(message.role).toBe('tool');
  });

  it('handles empty content gracefully', () => {
    const { message, wasTruncated } = formatToolResult(
      makeTC('list_directory'),
      { content: '', is_error: false },
      10000,
    );
    expect(wasTruncated).toBe(false);
    expect(message.content).toBe('TOOL_OK [list_directory]: ');
  });

  it('truncation boundary: content exactly at limit is not truncated', () => {
    const content = 'x'.repeat(5000);
    const { wasTruncated } = formatToolResult(
      makeTC('web_search'),
      { content, is_error: false },
      5000,
    );
    expect(wasTruncated).toBe(false);
  });

  it('truncation boundary: content one char over limit is truncated', () => {
    const content = 'x'.repeat(5001);
    const { wasTruncated } = formatToolResult(
      makeTC('web_search'),
      { content, is_error: false },
      5000,
    );
    expect(wasTruncated).toBe(true);
  });
});

// ============================================
// Phase 2C: buildVolatileContext
// ============================================

describe('buildVolatileContext', () => {
  it('returns empty string when no data provided', () => {
    expect(buildVolatileContext({
      conversationTurns: 0,
      messagesTruncated: 0,
      hasWorkingMemory: false,
    })).toBe('');
  });

  it('includes turn count', () => {
    const result = buildVolatileContext({
      conversationTurns: 5,
      messagesTruncated: 0,
      hasWorkingMemory: false,
    });
    expect(result).toContain('[Live Status]');
    expect(result).toContain('Turn 5');
  });

  it('includes truncation warning when messages were dropped', () => {
    const result = buildVolatileContext({
      conversationTurns: 10,
      messagesTruncated: 3,
      hasWorkingMemory: false,
    });
    expect(result).toContain('3 earlier messages were dropped');
  });

  it('includes VRAM info with room-for hints', () => {
    // 12GB free → room for 13B+
    const result = buildVolatileContext({
      conversationTurns: 1,
      messagesTruncated: 0,
      vramUsedMb: 8192, // 8 GB used
      vramFreeMb: 12288, // 12 GB free
      vramGb: 20,
      hasWorkingMemory: false,
    });
    expect(result).toContain('VRAM:');
    expect(result).toContain('room for 13B+ alongside');
  });

  it('VRAM near full hint when free < 2.5 GB', () => {
    const result = buildVolatileContext({
      conversationTurns: 1,
      messagesTruncated: 0,
      vramUsedMb: 22528, // 22 GB
      vramFreeMb: 1536, // 1.5 GB
      hasWorkingMemory: false,
    });
    expect(result).toContain('VRAM near full');
  });

  it('room for 7-8B hint when 5 < free <= 10 GB', () => {
    const result = buildVolatileContext({
      conversationTurns: 1,
      messagesTruncated: 0,
      vramUsedMb: 16384, // 16 GB
      vramFreeMb: 7168, // 7 GB
      hasWorkingMemory: false,
    });
    expect(result).toContain('room for 7-8B Q4 alongside');
  });

  it('room for small 3B hint when 2.5 < free <= 5 GB', () => {
    const result = buildVolatileContext({
      conversationTurns: 1,
      messagesTruncated: 0,
      vramUsedMb: 20480, // 20 GB
      vramFreeMb: 3072, // 3 GB
      hasWorkingMemory: false,
    });
    expect(result).toContain('room for small 3B alongside');
  });

  it('includes GPU utilization when provided', () => {
    const result = buildVolatileContext({
      conversationTurns: 1,
      messagesTruncated: 0,
      vramUsedMb: 8192,
      vramFreeMb: 8192,
      gpuUtilization: 85,
      hasWorkingMemory: false,
    });
    expect(result).toContain('85% GPU util');
  });

  it('includes active model VRAM when provided', () => {
    const result = buildVolatileContext({
      conversationTurns: 1,
      messagesTruncated: 0,
      vramUsedMb: 8192,
      vramFreeMb: 8192,
      activeModelVramGb: 4.2,
      hasWorkingMemory: false,
    });
    expect(result).toContain('model uses ~4.2 GB');
  });

  it('context pressure: moderate at 50%', () => {
    const result = buildVolatileContext({
      conversationTurns: 1,
      messagesTruncated: 0,
      contextLength: 32000,
      tokensUsed: 16000,
      hasWorkingMemory: false,
    });
    expect(result).toContain('50%');
    expect(result).toContain('moderate');
  });

  it('context pressure: HIGH at 70%', () => {
    const result = buildVolatileContext({
      conversationTurns: 1,
      messagesTruncated: 0,
      contextLength: 32000,
      tokensUsed: 22400,
      hasWorkingMemory: false,
    });
    expect(result).toContain('HIGH');
    expect(result).toContain('consider summarizing');
  });

  it('context pressure: CRITICAL at 80%+', () => {
    const result = buildVolatileContext({
      conversationTurns: 1,
      messagesTruncated: 0,
      contextLength: 32000,
      tokensUsed: 28800,
      hasWorkingMemory: false,
    });
    expect(result).toContain('CRITICAL');
    expect(result).toContain('summarize key points to working memory NOW');
  });

  it('includes working memory indicator', () => {
    const result = buildVolatileContext({
      conversationTurns: 1,
      messagesTruncated: 0,
      hasWorkingMemory: true,
    });
    expect(result).toContain('Working memory: active');
  });

  it('includes RAM info', () => {
    const result = buildVolatileContext({
      conversationTurns: 1,
      messagesTruncated: 0,
      hasWorkingMemory: false,
      ramAvailableMb: 16384,
    });
    expect(result).toContain('RAM free: 16 GB');
  });

  it('combines multiple sections with pipe separator', () => {
    const result = buildVolatileContext({
      conversationTurns: 3,
      messagesTruncated: 0,
      hasWorkingMemory: true,
      ramAvailableMb: 8192,
    });
    // Should have: [Live Status] Turn 3 | Working memory: active | RAM free: 8 GB
    expect(result.split(' | ').length).toBe(3);
  });
});

// ============================================
// Phase 2C: shouldSaveProcedure
// ============================================

describe('shouldSaveProcedure', () => {
  const success = (name: string): ChainHistoryEntry => ({
    name, argsKeys: ['query'], success: true,
  });
  const failure = (name: string): ChainHistoryEntry => ({
    name, argsKeys: ['query'], success: false,
  });

  it('returns false for empty chain', () => {
    expect(shouldSaveProcedure([])).toBe(false);
  });

  it('returns false for single-step chain (too trivial)', () => {
    expect(shouldSaveProcedure([success('web_search')])).toBe(false);
  });

  it('returns true for 2-step all-success chain', () => {
    expect(shouldSaveProcedure([
      success('web_search'),
      success('memory_save'),
    ])).toBe(true);
  });

  it('returns true for 5-step all-success chain (max)', () => {
    expect(shouldSaveProcedure([
      success('web_search'),
      success('read_file'),
      success('memory_save'),
      success('scratchpad_write'),
      success('telegram_send'),
    ])).toBe(true);
  });

  it('returns false for 6-step chain (too complex)', () => {
    expect(shouldSaveProcedure([
      success('web_search'),
      success('read_file'),
      success('memory_save'),
      success('scratchpad_write'),
      success('telegram_send'),
      success('discord_send'),
    ])).toBe(false);
  });

  it('returns false when any step failed', () => {
    expect(shouldSaveProcedure([
      success('web_search'),
      failure('read_file'),
      success('memory_save'),
    ])).toBe(false);
  });

  it('returns false for all-failure chain', () => {
    expect(shouldSaveProcedure([
      failure('web_search'),
      failure('read_file'),
    ])).toBe(false);
  });
});

// ============================================
// Phase 2C: buildProcedureData
// ============================================

describe('buildProcedureData', () => {
  const success = (name: string, keys: string[] = ['query']): ChainHistoryEntry => ({
    name, argsKeys: keys, success: true,
  });
  const failure = (name: string): ChainHistoryEntry => ({
    name, argsKeys: ['query'], success: false,
  });

  it('returns null for non-qualifying chain', () => {
    expect(buildProcedureData([success('web_search')], 'test')).toBeNull();
    expect(buildProcedureData([success('a'), failure('b')], 'test')).toBeNull();
  });

  it('builds correct chain name from tool sequence', () => {
    const data = buildProcedureData([
      success('web_search'),
      success('memory_save'),
    ], 'search and save results');
    expect(data).not.toBeNull();
    expect(data!.chainName).toBe('web_search → memory_save');
  });

  it('lowercases and trims trigger pattern', () => {
    const data = buildProcedureData([
      success('web_search'),
      success('memory_save'),
    ], '  Search the Web for HIVE Info  ');
    expect(data!.triggerPattern).toBe('search the web for hive info');
  });

  it('truncates trigger pattern to 100 chars', () => {
    const longTrigger = 'x'.repeat(200);
    const data = buildProcedureData([
      success('web_search'),
      success('memory_save'),
    ], longTrigger);
    expect(data!.triggerPattern.length).toBe(100);
  });

  it('preserves arg keys in steps', () => {
    const data = buildProcedureData([
      success('web_search', ['query', 'max_results']),
      success('memory_save', ['text', 'tags']),
    ], 'test');
    expect(data!.steps).toEqual([
      { tool: 'web_search', arg_pattern: ['query', 'max_results'] },
      { tool: 'memory_save', arg_pattern: ['text', 'tags'] },
    ]);
  });

  it('only includes successful tools in output', () => {
    // shouldSaveProcedure requires all success, but let's verify the filter
    const chain = [
      success('web_search'),
      success('read_file'),
      success('memory_save'),
    ];
    const data = buildProcedureData(chain, 'test');
    expect(data!.steps.length).toBe(3);
    expect(data!.steps.map(s => s.tool)).toEqual(['web_search', 'read_file', 'memory_save']);
  });
});

// ============================================
// T1: checkToolOriginAccess — security-critical origin enforcement
// ============================================

describe('checkToolOriginAccess', () => {
  // Desktop origin: everything allowed
  it('allows all tools for desktop origin', () => {
    expect(checkToolOriginAccess('run_command', 'desktop')).toBeNull();
    expect(checkToolOriginAccess('write_file', 'desktop')).toBeNull();
    expect(checkToolOriginAccess('telegram_send', 'desktop')).toBeNull();
    expect(checkToolOriginAccess('memory_save', 'desktop')).toBeNull();
    expect(checkToolOriginAccess('web_search', 'desktop')).toBeNull();
  });

  // Remote host: desktop-only tools blocked, dangerous non-desktop-only allowed
  it('blocks desktop-only tools for remote-host', () => {
    const result1 = checkToolOriginAccess('run_command', 'remote-host');
    expect(result1).not.toBeNull();
    expect(result1).toContain('desktop-only');

    const result2 = checkToolOriginAccess('write_file', 'remote-host');
    expect(result2).not.toBeNull();
    expect(result2).toContain('desktop-only');
  });

  it('allows dangerous non-desktop-only tools for remote-host', () => {
    // These are dangerous but not desktop-only — hosts can use them (with approval)
    expect(checkToolOriginAccess('telegram_send', 'remote-host')).toBeNull();
    expect(checkToolOriginAccess('discord_send', 'remote-host')).toBeNull();
    expect(checkToolOriginAccess('worker_spawn', 'remote-host')).toBeNull();
  });

  it('allows safe tools for remote-host', () => {
    expect(checkToolOriginAccess('web_search', 'remote-host')).toBeNull();
    expect(checkToolOriginAccess('memory_save', 'remote-host')).toBeNull();
    expect(checkToolOriginAccess('read_file', 'remote-host')).toBeNull();
  });

  // Remote user: ALL dangerous tools blocked
  it('blocks all dangerous tools for remote-user', () => {
    const dangerous = [
      'run_command', 'write_file', 'telegram_send', 'discord_send',
      'github_issues', 'github_prs', 'worker_spawn', 'send_to_agent',
      'plan_execute', 'memory_import_file',
    ];
    for (const tool of dangerous) {
      const result = checkToolOriginAccess(tool, 'remote-user');
      expect(result).not.toBeNull();
      expect(result).toContain('restricted');
    }
  });

  it('allows safe tools for remote-user', () => {
    expect(checkToolOriginAccess('web_search', 'remote-user')).toBeNull();
    expect(checkToolOriginAccess('memory_save', 'remote-user')).toBeNull();
    expect(checkToolOriginAccess('read_file', 'remote-user')).toBeNull();
    expect(checkToolOriginAccess('memory_search', 'remote-user')).toBeNull();
  });

  // Ensure desktop-only is a SUBSET of dangerous
  it('desktop-only tools are also in dangerous set (superset invariant)', () => {
    // run_command and write_file must be blocked for remote-user too
    expect(checkToolOriginAccess('run_command', 'remote-user')).not.toBeNull();
    expect(checkToolOriginAccess('write_file', 'remote-user')).not.toBeNull();
  });
});

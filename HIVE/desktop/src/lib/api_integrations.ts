// Integration API — Telegram, Discord, routines, queue, daemon, MCP
// Extracted from api.ts for modularity (P1)

import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import type {
  TelegramDaemonStatus, TelegramIncoming, DiscordDaemonStatus, DiscordIncoming,
  Routine, RoutineTriggered, QueuedMessage, RoutineStats, ChannelEvent,
  AccessLists,
} from '../types';

// ============================================
// Integration Keys ("Doors and Keys" pattern)
// ============================================

/** Integration providers that accept API keys/tokens */
export type IntegrationProvider = 'telegram' | 'github' | 'discord' | 'discord_channel_id' | 'brave' | 'jina';

/**
 * Store an integration key (Telegram bot token, GitHub PAT, etc.)
 * The key is encrypted with AES-256-GCM and stored in ~/.hive/secrets.enc
 */
export async function storeIntegrationKey(provider: IntegrationProvider, key: string): Promise<string> {
  return invoke('store_api_key', { provider, apiKey: key });
}

/**
 * Check if an integration key is configured
 */
export async function hasIntegrationKey(provider: IntegrationProvider): Promise<boolean> {
  return invoke('has_api_key', { provider });
}

/**
 * Delete an integration key
 */
export async function deleteIntegrationKey(provider: IntegrationProvider): Promise<void> {
  return invoke('delete_api_key', { provider });
}

/**
 * Check all integration statuses at once
 */
export async function getIntegrationStatuses(): Promise<Record<IntegrationProvider, boolean>> {
  const [telegram, github, discord, discord_channel_id, brave, jina] = await Promise.all([
    hasIntegrationKey('telegram').catch(() => false),
    hasIntegrationKey('github').catch(() => false),
    hasIntegrationKey('discord').catch(() => false),
    hasIntegrationKey('discord_channel_id').catch(() => false),
    hasIntegrationKey('brave').catch(() => false),
    hasIntegrationKey('jina').catch(() => false),
  ]);
  return { telegram, github, discord, discord_channel_id, brave, jina };
}

// ============================================
// Telegram Daemon (Phase 4.5)
// ============================================

/**
 * Start the Telegram polling daemon.
 * Spawns a background Tokio task that long-polls getUpdates and emits
 * "telegram-incoming" events when messages arrive.
 */
export async function startTelegramDaemon(): Promise<string> {
  return invoke('start_telegram_daemon');
}

/**
 * Stop the Telegram polling daemon.
 */
export async function stopTelegramDaemon(): Promise<string> {
  return invoke('stop_telegram_daemon');
}

/**
 * Get daemon status (running, messages processed, errors, etc.)
 */
export async function getTelegramDaemonStatus(): Promise<TelegramDaemonStatus> {
  return invoke('get_telegram_daemon_status');
}

/**
 * Set Telegram Host IDs — chat_ids with full desktop-equivalent permissions.
 */
export async function setTelegramHostIds(chatIds: string[]): Promise<string> {
  return invoke('set_telegram_host_ids', { chatIds });
}

/**
 * Set Telegram User IDs — chat_ids with restricted permissions (no dangerous tools).
 */
export async function setTelegramUserIds(chatIds: string[]): Promise<string> {
  return invoke('set_telegram_user_ids', { chatIds });
}

/**
 * Get current Telegram access lists (host_ids + user_ids).
 */
export async function getTelegramAccessLists(): Promise<AccessLists> {
  return invoke('get_telegram_access_lists');
}

/**
 * Listen for incoming Telegram messages from the daemon.
 * Returns an unlisten function.
 */
export async function onTelegramMessage(
  callback: (msg: TelegramIncoming) => void
): Promise<UnlistenFn> {
  return listen<TelegramIncoming>('telegram-incoming', (event) => {
    callback(event.payload);
  });
}

// ============================================
// Discord Daemon (Phase 5)
// ============================================

/**
 * Start the Discord polling daemon.
 * Spawns a background Tokio task that polls watched channels and emits
 * "discord-incoming" events when messages arrive.
 */
export async function startDiscordDaemon(): Promise<string> {
  return invoke('start_discord_daemon');
}

/**
 * Stop the Discord polling daemon.
 */
export async function stopDiscordDaemon(): Promise<string> {
  return invoke('stop_discord_daemon');
}

/**
 * Get Discord daemon status (running, messages processed, errors, etc.)
 */
export async function getDiscordDaemonStatus(): Promise<DiscordDaemonStatus> {
  return invoke('get_discord_daemon_status');
}

/**
 * Set watched Discord channel IDs for the daemon to poll.
 */
export async function setDiscordWatchedChannels(channelIds: string[]): Promise<string> {
  return invoke('set_discord_watched_channels', { channelIds });
}

/**
 * Set Discord Host IDs — user IDs with full desktop-equivalent permissions.
 */
export async function setDiscordHostIds(userIds: string[]): Promise<string> {
  return invoke('set_discord_host_ids', { userIds });
}

/**
 * Set Discord User IDs — user IDs with restricted permissions (no dangerous tools).
 */
export async function setDiscordUserIds(userIds: string[]): Promise<string> {
  return invoke('set_discord_user_ids', { userIds });
}

/**
 * Get current Discord access lists (host_ids + user_ids).
 */
export async function getDiscordAccessLists(): Promise<AccessLists> {
  return invoke('get_discord_access_lists');
}

/**
 * Listen for incoming Discord messages from the daemon.
 * Returns an unlisten function.
 */
export async function onDiscordMessage(
  callback: (msg: DiscordIncoming) => void
): Promise<UnlistenFn> {
  return listen<DiscordIncoming>('discord-incoming', (event) => {
    callback(event.payload);
  });
}

// ============================================
// Routines Engine (Phase 6 — Standing Instructions)
// ============================================

/**
 * Create a new routine (standing instruction).
 */
export async function routineCreate(params: {
  name: string;
  description: string;
  triggerType: string;
  cronExpr?: string;
  eventPattern?: string;
  eventKeyword?: string;
  actionPrompt: string;
  responseChannel?: string;
}): Promise<Routine> {
  return invoke('routine_create', {
    name: params.name,
    description: params.description,
    triggerType: params.triggerType,
    cronExpr: params.cronExpr ?? null,
    eventPattern: params.eventPattern ?? null,
    eventKeyword: params.eventKeyword ?? null,
    actionPrompt: params.actionPrompt,
    responseChannel: params.responseChannel ?? null,
  });
}

/**
 * List all routines.
 */
export async function routineList(): Promise<Routine[]> {
  return invoke('routine_list');
}

/**
 * Update a routine. Only provided fields are updated.
 */
export async function routineUpdate(id: string, updates: {
  name?: string;
  description?: string;
  enabled?: boolean;
  triggerType?: string;
  cronExpr?: string;
  eventPattern?: string;
  eventKeyword?: string;
  actionPrompt?: string;
  responseChannel?: string;
}): Promise<string> {
  return invoke('routine_update', {
    id,
    name: updates.name ?? null,
    description: updates.description ?? null,
    enabled: updates.enabled ?? null,
    triggerType: updates.triggerType ?? null,
    cronExpr: updates.cronExpr ?? null,
    eventPattern: updates.eventPattern ?? null,
    eventKeyword: updates.eventKeyword ?? null,
    actionPrompt: updates.actionPrompt ?? null,
    responseChannel: updates.responseChannel ?? null,
  });
}

/**
 * Delete a routine.
 */
export async function routineDelete(id: string): Promise<string> {
  return invoke('routine_delete', { id });
}

/**
 * Record that a routine ran (update stats).
 */
export async function routineRecordRun(id: string, success: boolean, resultSummary?: string): Promise<string> {
  return invoke('routine_record_run', { id, success, resultSummary: resultSummary ?? null });
}

/**
 * Get routines engine stats.
 */
export async function routineStats(): Promise<RoutineStats> {
  return invoke('routine_stats');
}

// ============================================
// Message Queue (Phase 6)
// ============================================

/**
 * Enqueue a channel message for reliable processing.
 */
export async function queueEnqueue(params: {
  channelType: string;
  channelId: string;
  senderName: string;
  senderId: string;
  text: string;
  wrappedText: string;
  routineId?: string;
}): Promise<string> {
  return invoke('queue_enqueue', {
    channelType: params.channelType,
    channelId: params.channelId,
    senderName: params.senderName,
    senderId: params.senderId,
    text: params.text,
    wrappedText: params.wrappedText,
    routineId: params.routineId ?? null,
  });
}

/**
 * Dequeue the next pending message for processing.
 */
export async function queueDequeue(): Promise<QueuedMessage | null> {
  return invoke('queue_dequeue');
}

/**
 * Mark a queued message as completed.
 */
export async function queueComplete(id: string): Promise<string> {
  return invoke('queue_complete', { id });
}

/**
 * Mark a queued message as failed. Retries automatically if under max attempts.
 */
export async function queueFail(id: string, error: string): Promise<string> {
  return invoke('queue_fail', { id, error });
}

/**
 * Get active queue entries (pending, processing, dead).
 */
export async function queueStatus(): Promise<QueuedMessage[]> {
  return invoke('queue_status');
}

/**
 * Purge completed messages older than 24 hours.
 */
export async function queuePurgeCompleted(): Promise<string> {
  return invoke('queue_purge_completed');
}

// ============================================
// Routines Daemon (Phase 6)
// ============================================

/**
 * Start the routines cron evaluation daemon.
 * Checks cron expressions every 60 seconds and emits "routine-triggered" events.
 */
export async function routinesDaemonStart(): Promise<string> {
  return invoke('routines_daemon_start');
}

/**
 * Stop the routines cron daemon.
 */
export async function routinesDaemonStop(): Promise<string> {
  return invoke('routines_daemon_stop');
}

/**
 * Check if the routines daemon is running.
 */
export async function routinesDaemonStatus(): Promise<boolean> {
  return invoke('routines_daemon_status');
}

/**
 * Listen for routine-triggered events (from cron or channel events).
 * Returns an unlisten function.
 */
export async function onRoutineTriggered(
  callback: (event: RoutineTriggered) => void
): Promise<UnlistenFn> {
  return listen<RoutineTriggered>('routine-triggered', (event) => {
    callback(event.payload);
  });
}

/**
 * Listen for unified channel events (from any channel daemon).
 * Returns an unlisten function.
 */
export async function onChannelEvent(
  callback: (event: ChannelEvent) => void
): Promise<UnlistenFn> {
  return listen<ChannelEvent>('channel-incoming', (event) => {
    callback(event.payload);
  });
}

// ============================================
// Worker Completion Events (Phase 8 — Workers ping back)
// ============================================

export interface WorkerCompletedEvent {
  worker_id: string;
  status: 'completed' | 'failed';
  summary?: string;
  error?: string;
  scratchpad_id: string;
  turns_used: number;
}

/**
 * Listen for worker completion/failure events.
 * Workers emit "worker-completed" when they finish (success or failure).
 * Returns an unlisten function.
 */
export async function onWorkerCompleted(
  callback: (event: WorkerCompletedEvent) => void
): Promise<UnlistenFn> {
  return listen<WorkerCompletedEvent>('worker-completed', (event) => {
    callback(event.payload);
  });
}

export interface WorkerMessageEvent {
  worker_id: string;
  message: string;
  severity: 'info' | 'warning' | 'error' | 'done';
  scratchpad_id: string;
}

/**
 * Listen for mid-task worker messages (report_to_parent tool).
 * Workers use this to send progress updates, errors, and findings to the parent chat.
 * Returns an unlisten function.
 */
export async function onWorkerMessage(
  callback: (event: WorkerMessageEvent) => void
): Promise<UnlistenFn> {
  return listen<WorkerMessageEvent>('worker-message', (event) => {
    callback(event.payload);
  });
}

export interface WorkerStatusUpdateEvent {
  worker_id: string;
  turns_used: number;
  tools_executed: number;
  elapsed_seconds: number;
  max_time_seconds: number;
  max_turns: number;
}

/**
 * Listen for periodic worker status updates (every ~60s while running).
 * Used for live observability — frontend can show real-time worker progress.
 * Returns an unlisten function.
 */
export async function onWorkerStatusUpdate(
  callback: (event: WorkerStatusUpdateEvent) => void
): Promise<UnlistenFn> {
  return listen<WorkerStatusUpdateEvent>('worker-status-update', (event) => {
    callback(event.payload);
  });
}

// ============================================
// Agent Bridge Events (Phase 10.5 — cross-agent response injection)
// ============================================

export interface AgentResponseEvent {
  session_id: string;
  agent_name: string;
  content: string;
}

/**
 * Listen for agent response events from the bridge monitor.
 * Emitted when a bridged PTY session produces output and then goes silent,
 * indicating the agent has finished responding. Used to inject agent output
 * into the orchestrating model's chat.
 */
export async function onAgentResponse(
  callback: (event: AgentResponseEvent) => void
): Promise<UnlistenFn> {
  return listen<AgentResponseEvent>('agent-response', (event) => {
    callback(event.payload);
  });
}

// ============================================
// MCP (Model Context Protocol) — Phase 9
// ============================================

// P5: must match mcp_client.rs::McpServerConfig
export interface McpServerConfig {
  name: string;
  command: string;
  args?: string[];
  env?: Record<string, string>;
  transport?: 'stdio' | 'http';  // default: 'stdio'
  url?: string;                   // required when transport='http'
}

// P5: must match mcp_client.rs::McpConnectionInfo
export interface McpConnectionInfo {
  name: string;
  command: string;
  tools: string[];
  connected: boolean;
  transport: string;
  url?: string;
}

/**
 * Connect to an external MCP server. Spawns the process, discovers tools,
 * and registers them in the tool registry. Returns discovered tool names.
 */
export async function mcpConnect(config: McpServerConfig): Promise<string[]> {
  return invoke('mcp_connect', { config });
}

/**
 * Disconnect from an MCP server. Kills the process and unregisters tools.
 */
export async function mcpDisconnect(name: string): Promise<void> {
  return invoke('mcp_disconnect', { name });
}

/**
 * List all active MCP server connections.
 */
export async function mcpListConnections(): Promise<McpConnectionInfo[]> {
  return invoke('mcp_list_connections');
}

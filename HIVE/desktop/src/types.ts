// Shared types used across App.tsx and tab components

export type Tab = 'setup' | 'models' | 'browse' | 'chat' | 'memory' | 'mcp' | 'settings' | 'logs';
export type Backend = 'windows' | 'wsl';

/** Provider-agnostic thinking depth — Rust mirror: types.rs::ThinkingDepth. Must stay in sync (P5). */
export type ThinkingDepth = 'off' | 'low' | 'medium' | 'high';

export interface Message {
  id?: string;                  // stable unique ID for React keys (M21 — avoids array-index reuse)
  role: 'user' | 'assistant' | 'tool';
  content: string;
  thinking?: string;            // reasoning tokens (separated from content — P1: modularity)
  toolCalls?: ToolCall[];       // present when assistant wants to call tools
  toolCallId?: string;          // present on role:'tool' messages (result of a tool call)
  toolName?: string;            // which tool was called (for display)
  // Sender identity — who produced this message and from where.
  // Foundation for multi-model orchestration: distinguishes Model A from Model B,
  // Telegram user from HIVE UI user, worker from orchestrator.
  senderName?: string;          // display name: "Kimi", "Lucid", model ID, worker name
  senderChannel?: 'hive' | 'telegram' | 'discord' | 'pty-agent';  // originating channel
}

export interface ToolCall {
  id: string;
  name: string;
  arguments: Record<string, unknown>;
}

export interface ToolSchema {
  name: string;
  description: string;
  parameters: Record<string, unknown>;
  risk_level: 'low' | 'medium' | 'high' | 'critical';
}

export interface ToolResult {
  content: string;
  is_error: boolean;
}

export type ChatResponse =
  | { type: 'text'; content: string; thinking: string | null }
  | { type: 'tool_calls'; content: string | null; thinking: string | null; tool_calls: ToolCall[] };

/** Response from streaming chat — content and thinking separated (P2: provider-agnostic) */
export interface StreamResponse {
  content: string;
  thinking: string | null;
}

export interface LogEntry {
  time: string;
  level: string;
  msg: string;
}

// ============================================
// Cognitive Harness Types
// ============================================

/** Result of harness_build — the assembled system prompt + metadata.
 *  system_prompt is STABLE (cacheable across turns by llama.cpp KV prefix match).
 *  volatile_context is tiny per-turn live metrics (injected as separate message). */
export interface HarnessContext {
  system_prompt: string;
  volatile_context: string;
  identity_source: string;
  tool_count: number;
  memory_status: string;
}

/** Capability snapshot passed to harness_build (frontend → Rust) */
export interface CapabilitySnapshot {
  tools: string[];
  active_model: string | null;
  provider: string;
  available_models: string[];
  memory_enabled: boolean;
  memory_count: number;
  gpu: string | null;
  vram_gb: number | null;
  ram_gb: number | null;

  // === Situational Awareness (Phase 4b) ===
  context_length?: number;          // Effective context window in tokens
  quantization?: string;            // e.g. "Q4_K_M", "F16"
  model_parameters?: string;        // e.g. "7B", "13B"
  architecture?: string;            // e.g. "llama", "qwen2"
  backend?: string;                 // "windows" or "wsl"
  cpu?: string;                     // CPU name
  tool_risks?: string[];            // ["read_file:low", "run_command:high"]
  memory_search_mode?: string;      // "hybrid" or "keyword"
  conversation_turns?: number;      // User message count this conversation
  messages_truncated?: number;      // Messages dropped by context truncation
  os_platform?: string;             // "Windows 11", "Linux"

  // === Live Resource Metrics (Phase 4b+) ===
  vram_used_mb?: number;            // GPU VRAM currently in use (MB)
  vram_free_mb?: number;            // GPU VRAM currently free (MB) — key routing number
  ram_available_mb?: number;        // System RAM currently available (MB)
  gpu_utilization?: number;         // GPU util % (0-100)
  active_model_vram_gb?: number;    // Estimated VRAM of running model (from pre-launch calc)

  // === Context Pressure Tracking (Phase 3.5) ===
  tokens_used?: number;             // Estimated tokens used in current conversation
  has_working_memory?: boolean;     // Whether working memory has content
}

// ============================================
// Phase 4: Slot System & Orchestrator Types
// ============================================

export type SlotRole = 'consciousness' | 'coder' | 'terminal' | 'webcrawl' | 'toolcall';
export type SlotStatus = 'idle' | 'loading' | 'active' | 'sleeping';

/** Port assignments for specialist slots — must match server.rs::port_for_slot() (P5).
 *  If you change these, update the Rust source of truth too. */
export const SPECIALIST_PORTS: Record<string, number> = {
  consciousness: 8080, coder: 8081, terminal: 8082, webcrawl: 8083, toolcall: 8084,
};

export interface SlotAssignment {
  provider: string;     // "local", "ollama", "openai", "anthropic"
  model: string;        // model filename or API model ID
  vram_gb: number;      // estimated VRAM cost (0 for cloud)
  context_length: number;
}

export interface SlotConfig {
  role: SlotRole;
  primary?: SlotAssignment;
  fallbacks: SlotAssignment[];
  enabled: boolean;
}

export interface SlotState {
  role: SlotRole;
  status: SlotStatus;
  assignment?: SlotAssignment;
  server_port?: number;
  loaded_at?: string;
  last_active?: string;
  vram_used_gb: number;
}

export interface VramBudget {
  total_gb: number;
  used_gb: number;
  safety_buffer_gb: number;
}

export interface RouteDecision {
  slot: SlotRole;
  reason: string;
  confidence: number;
  needs_wake: boolean;
  needs_evict?: SlotRole;
}

// MAGMA graph types
export interface MagmaEvent {
  id: string;
  event_type: string;
  agent: string;
  content: string;
  metadata: Record<string, unknown>;
  session_id?: string;
  created_at: string;
}

export interface MagmaEntity {
  id: string;
  entity_type: string;
  name: string;
  state: Record<string, unknown>;
  metadata: Record<string, unknown>;
  created_at: string;
  updated_at: string;
}

export interface MagmaProcedure {
  id: string;
  name: string;
  description: string;
  steps: Record<string, unknown>[];
  trigger_pattern: string;
  success_count: number;
  fail_count: number;
  last_used?: string;
  created_at: string;
  updated_at: string;
}

export interface MagmaEdge {
  id: string;
  source_type: string;
  source_id: string;
  target_type: string;
  target_id: string;
  edge_type: string;
  weight: number;
  metadata: Record<string, unknown>;
  created_at: string;
}

export interface MagmaStats {
  events: number;
  entities: number;
  procedures: number;
  edges: number;
}

// ============================================
// Remote Channel Security — Host/User Roles
// ============================================

/** Sender role determines tool access over remote channels */
export type SenderRole = 'host' | 'user';

/**
 * Message origin — where the current conversation trigger came from.
 * 'desktop' = full access, 'remote-host' = trust but verify, 'remote-user' = restricted.
 */
export type MessageOrigin = 'desktop' | 'remote-host' | 'remote-user' | 'pty-agent';

/** Access list configuration for a daemon (Telegram/Discord) */
export interface AccessLists {
  host_ids: string[];
  user_ids: string[];
}

// ============================================
// Telegram Daemon Types (Phase 4.5)
// ============================================

/** Emitted by the Telegram daemon when a message arrives */
export interface TelegramIncoming {
  chat_id: string;
  from_name: string;
  from_username: string;
  text: string;
  update_id: number;
  wrapped_text: string;
  sender_role: SenderRole;
}

/** Daemon status — queryable from the frontend */
export interface TelegramDaemonStatus {
  running: boolean;
  messages_processed: number;
  errors: number;
  last_error: string | null;
  last_poll: string | null;
  connected_bot: string | null;
}

// ============================================
// Discord Daemon Types (Phase 5)
// ============================================

/** Emitted by the Discord daemon when a message arrives */
export interface DiscordIncoming {
  channel_id: string;
  guild_id: string | null;
  author_name: string;
  author_id: string;
  text: string;
  message_id: string;
  wrapped_text: string;
  sender_role: SenderRole;
}

/** Daemon status — queryable from the frontend */
export interface DiscordDaemonStatus {
  running: boolean;
  messages_processed: number;
  errors: number;
  last_error: string | null;
  last_poll: string | null;
  connected_bot: string | null;
  watched_channels: string[];
}

// ============================================
// Unified Channel Event (Phase 6 — P5: one event path)
// ============================================

/** Normalized event from any channel (Telegram, Discord, future channels) */
export interface ChannelEvent {
  channel_type: string;    // "telegram", "discord", "local"
  channel_id: string;      // platform-specific channel/chat ID
  sender_name: string;
  sender_id: string;
  text: string;
  wrapped_text: string;    // security-wrapped text
  raw_event_id: string;    // platform-specific event/message ID
  metadata: Record<string, unknown>;
}

// ============================================
// Routines Engine Types (Phase 6 — Standing Instructions)
// ============================================

export type TriggerType = 'cron' | 'event' | 'both';
export type QueueStatus = 'pending' | 'processing' | 'completed' | 'failed' | 'dead';

/** A persistent standing instruction that HIVE evaluates autonomously */
export interface Routine {
  id: string;
  name: string;
  description: string;
  enabled: boolean;

  // Trigger config
  trigger_type: TriggerType;
  cron_expr: string | null;       // "minute hour day month weekday"
  event_pattern: string | null;   // "channel:telegram", "channel:*"
  event_keyword: string | null;   // keyword filter (case-insensitive)

  // Action config
  action_prompt: string;          // instruction sent to the agentic loop
  response_channel: string | null; // output routing: "telegram:<id>", "discord:<id>", null = local

  // Execution stats
  run_count: number;
  success_count: number;
  fail_count: number;
  last_run: string | null;
  last_result: string | null;

  // Timestamps
  created_at: string;
  updated_at: string;
}

/** Emitted when a routine is triggered (by cron tick or channel event) */
export interface RoutineTriggered {
  routine_id: string;
  routine_name: string;
  action_prompt: string;
  response_channel: string | null;
  trigger_reason: string;
  source_event: ChannelEvent | null;
  queue_id: string | null;
}

/** A queued message from a channel, awaiting processing */
export interface QueuedMessage {
  id: string;
  channel_type: string;
  channel_id: string;
  sender_name: string;
  sender_id: string;
  text: string;
  wrapped_text: string;
  status: QueueStatus;
  attempts: number;
  max_attempts: number;
  error: string | null;
  routine_id: string | null;
  created_at: string;
  processed_at: string | null;
}

/** Routines engine summary stats */
export interface RoutineStats {
  total_routines: number;
  enabled_routines: number;
  total_runs: number;
  total_successes: number;
  total_failures: number;
  queue_pending: number;
  queue_processing: number;
  queue_dead: number;
}

// ============================================
// Multi-Pane Chat Types (Unified Multi-Pane Adaptive Chat)
// ============================================

/** Model type for a chat pane — local GGUF or cloud provider */
export type PaneModelType = 'local' | 'cloud';

/** Pane content type — chat (LLM conversation) or terminal (PTY CLI agent) */
export type PaneType = 'chat' | 'terminal';

/** Agent preset for terminal panes (Phase 10 — NEXUS) */
export interface AgentConfig {
  id: string;
  name: string;       // Display name: "Claude Code", "Shell"
  command: string;     // CLI command: "claude", "bash", "cmd"
  args: string[];      // Default args
  color: string;       // Tailwind color class for pane header icon
  bridgeToChat?: boolean; // Auto-inject output into orchestrator chat (silence-detected)
}

/** Built-in agent presets — any CLI tool that takes stdin/stdout */
export const BUILTIN_AGENTS: AgentConfig[] = [
  { id: 'shell', name: 'Shell', command: 'bash', args: [], color: 'text-zinc-400' },
  { id: 'claude-code', name: 'Claude Code', command: 'claude', args: [], color: 'text-amber-400', bridgeToChat: true },
  { id: 'codex', name: 'Codex', command: 'codex', args: [], color: 'text-green-400', bridgeToChat: true },
  { id: 'aider', name: 'Aider', command: 'aider', args: [], color: 'text-indigo-400', bridgeToChat: true },
];

/** Configuration for a single chat pane */
export interface ChatPaneConfig {
  id: string;                    // Unique pane identifier (e.g., "pane-1", "pane-abc123")
  paneType?: PaneType;           // 'chat' (default) | 'terminal' — optional for backwards compat
  modelType: PaneModelType;      // 'local' or 'cloud'
  provider?: string;             // Provider type for cloud models (e.g., "openai", "anthropic")
  modelId?: string;              // Model identifier (filename for local, model ID for cloud)
  modelDisplayName: string;      // Human-readable model name for the pane header
  conversationId?: string;       // Per-pane conversation ID for persistence
  port?: number;                 // For local models — which llama-server port to target
  // Terminal pane fields (Phase 10 — NEXUS):
  agentId?: string;              // References AgentConfig.id (e.g., "shell", "claude-code")
  ptySessionId?: string;         // Set after PTY spawn, used for write/resize/kill
}

/** Layout of panes — persisted to localStorage */
export interface PaneLayout {
  panes: ChatPaneConfig[];
  direction: 'horizontal' | 'vertical';
}

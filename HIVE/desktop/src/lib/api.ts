// HIVE API - Standalone Runtime
//
// - Hardware detection (GPU, WSL, ROCm/CUDA)
// - Tauri commands for local model/server management
// - HuggingFace API for model discovery
// - llama.cpp server for inference

import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';

// ============================================
// Types - Hardware Detection
// ============================================


// Re-export extracted modules
export * from './api_memory';
export * from './api_integrations';
export * from './api_recommendations';

export interface GpuInfo {
  vendor: string;        // "NVIDIA", "AMD", "Intel", "Unknown"
  name: string;          // "NVIDIA GeForce RTX 4090"
  vram_mb: number;       // VRAM in MB
  driver_version: string;
}

export interface CpuInfo {
  name: string;          // "AMD Ryzen 9 5900X"
  cores: number;         // Physical cores
  threads: number;       // Logical processors
}

export interface RamInfo {
  total_mb: number;      // Total RAM in MB
  total_gb: number;      // Total RAM in GB
}

export interface SystemInfo {
  gpus: GpuInfo[];
  cpu: CpuInfo | null;
  ram: RamInfo | null;
  wsl_available: boolean;
  wsl_distro: string | null;
  recommended_backend: string; // "windows" | "wsl"
}

export interface WslStatus {
  installed: boolean;
  distro: string | null;
  llama_server_path: string | null;
  rocm_version: string | null;
  cuda_version: string | null;
}

export interface ServerStatus {
  running: boolean;
  port: number;
  backend: string;
  model_path: string | null;
}

export interface DependencyStatus {
  // Windows/NVIDIA dependencies
  windows_llama_server: string | null;  // Path if found, null if missing
  cuda_available: boolean;

  // WSL/AMD dependencies
  wsl_installed: boolean;
  wsl_distro: string | null;
  wsl_llama_server: string | null;      // Path if found, null if missing
  rocm_available: boolean;
  rocm_version: string | null;

  // What's needed based on detected hardware
  recommended_backend: string;           // "windows" or "wsl"
  ready_to_run: boolean;                 // True if all deps for recommended backend are met
  missing_deps: string[];                // List of what's missing
}

// ============================================
// Types - VRAM Calculation
// ============================================

/** Metadata extracted from a GGUF file header */
export interface GgufMetadata {
  architecture: string | null;      // e.g., "llama", "qwen", "phi"
  name: string | null;              // Human-readable model name
  parameter_count: number | null;   // Total parameters
  quantization: string | null;      // e.g., "Q4_K_M", "Q5_K_M", "F16"
  file_type: number | null;         // GGUF file_type enum value
  context_length: number | null;    // Maximum context window
  embedding_length: number | null;  // Hidden dimension size
  block_count: number | null;       // Number of transformer layers
  head_count: number | null;        // Number of attention heads
  head_count_kv: number | null;     // Number of KV heads (for GQA)
  expert_count: number | null;      // MoE: total experts (e.g., 8 for Mixtral 8x7B)
  expert_used_count: number | null; // MoE: active experts per token (e.g., 2 for Mixtral)
  file_size_bytes: number;          // Actual file size
}

/** VRAM estimate breakdown */
export interface VramEstimate {
  model_weights_gb: number;         // Memory for model weights
  kv_cache_gb: number;              // Memory for KV cache at given context
  overhead_gb: number;              // CUDA/ROCm overhead + scratch space
  total_gb: number;                 // Total estimated VRAM
  context_length: number;           // Context length used in calculation
  quantization: string;             // Quantization type
  confidence: string;               // "high", "medium", "low"
  kv_offload: boolean;              // Whether KV cache is excluded (offloaded to RAM)
  is_moe: boolean;                  // Whether this is a Mixture-of-Experts model
  moe_active_gb: number | null;     // MoE: VRAM for active experts only (with expert offload)
}

/** VRAM compatibility status for UI badges */
export interface VramCompatibility {
  estimate: VramEstimate;
  available_vram_gb: number;        // User's GPU VRAM
  status: string;                   // "good" (green), "tight" (yellow), "insufficient" (red)
  headroom_gb: number;              // Available VRAM - estimated usage
}

// ============================================
// Types - Models
// ============================================

export interface LocalModel {
  id: string;
  filename: string;
  size_bytes: number;
  size_gb: number;
  path: string;
  context_length: number | null;  // Max context window from GGUF metadata
}

export interface HfModelFile {
  filename: string;
  size: number;
  downloadUrl: string;
}

export interface HfModel {
  id: string;
  author: string;
  name: string;
  downloads: number;
  likes: number;
  files: HfModelFile[];
  baseModel?: string;       // extracted from tags (e.g. "meta-llama/Llama-3.1-8B-Instruct")
  qualityScore?: number;    // Open LLM Leaderboard average (0-100)
  pipelineTag?: string;     // e.g. "text-generation", "image-text-to-text"
  domainTags?: string[];    // filtered domain tags (e.g. ["code", "chat", "reasoning"])
}

// ============================================
// Types - Chat
// ============================================

export interface ChatMessage {
  role: 'user' | 'assistant' | 'system' | 'tool';
  content: string;
  thinking?: string;              // reasoning tokens — preserved across save/load (B4 fix)
  tool_call_id?: string;
  // OpenAI-format tool_calls on assistant messages — required for multi-turn tool use.
  // Without this, the API sees orphaned role:"tool" messages with no matching call.
  tool_calls?: Array<{
    id: string;
    type: 'function';
    function: { name: string; arguments: string };
  }>;
}

// ============================================
// Hardware Detection Commands
// ============================================

export async function detectGpus(): Promise<GpuInfo[]> {
  return invoke('detect_gpus');
}

export async function getSystemInfo(): Promise<SystemInfo> {
  return invoke('get_system_info');
}

// ============================================
// WSL Management Commands
// ============================================

export async function checkWsl(): Promise<WslStatus> {
  return invoke('check_wsl');
}

export async function runWslCommand(command: string): Promise<string> {
  return invoke('run_wsl_command', { command });
}

export async function setWslDistro(distro: string): Promise<void> {
  return invoke('set_wsl_distro', { distro });
}

// ============================================
// Dependency Management Commands
// ============================================

export async function checkDependencies(): Promise<DependencyStatus> {
  return invoke('check_dependencies');
}

export async function downloadLlamaServer(
  onProgress?: (downloaded: number, total: number) => void
): Promise<string> {
  let unlisten: UnlistenFn | null = null;

  // Set up progress listener
  if (onProgress) {
    unlisten = await listen<DownloadProgress>('download-progress', (event) => {
      if (event.payload.filename === 'llama-server.zip') {
        onProgress(event.payload.downloaded, event.payload.total);
      }
    });
  }

  try {
    const path: string = await invoke('download_llama_server');
    return path;
  } finally {
    if (unlisten) {
      unlisten();
    }
  }
}

export async function getLlamaServerInstallPath(): Promise<string> {
  return invoke('get_llama_server_install_path');
}

// ============================================
// Model Management Commands
// ============================================

export async function listLocalModels(): Promise<LocalModel[]> {
  return invoke('list_local_models');
}

export async function listWslModels(searchPaths: string[]): Promise<LocalModel[]> {
  return invoke('list_wsl_models', { searchPaths });
}

export async function getModelsDirectory(): Promise<string> {
  return invoke('get_models_directory');
}

export async function openModelsDirectory(): Promise<void> {
  return invoke('open_models_directory');
}

// ============================================
// Server Management Commands
// ============================================

export async function startServerNative(
  modelPath: string,
  port?: number,
  gpuLayers?: number,
  contextLength?: number,
  kvOffload?: boolean
): Promise<ServerStatus> {
  return invoke('start_server_native', { modelPath, port, gpuLayers, contextLength, kvOffload });
}

export async function startServerWsl(
  modelPath: string,
  port?: number,
  gpuLayers?: number,
  contextLength?: number,
  kvOffload?: boolean,
  llamaServerPath?: string
): Promise<ServerStatus> {
  return invoke('start_server_wsl', { modelPath, port, gpuLayers, contextLength, kvOffload, llamaServerPath });
}

export async function stopServer(): Promise<void> {
  return invoke('stop_server');
}

export async function getServerStatus(): Promise<ServerStatus> {
  return invoke('get_server_status');
}

// ============================================
// App Paths
// ============================================

export async function getAppPaths(): Promise<Record<string, string>> {
  return invoke('get_app_paths');
}

/** Save an uploaded file to the attachments directory. Returns the full path. */
export async function saveAttachment(filename: string, data: number[]): Promise<string> {
  return invoke('save_attachment', { filename, data });
}

/** File attachment metadata (for project files) */
export interface FileAttachment {
  name: string;
  path: string;       // full path on disk (from save_attachment)
  size: number;       // bytes
  type: string;       // MIME type
}

// ============================================
// Live Resource Metrics (for situational awareness)
// ============================================

/** Live VRAM + RAM usage metrics from nvidia-smi/rocm-smi + PowerShell */
export interface LiveResourceMetrics {
  vram_used_mb: number | null;
  vram_free_mb: number | null;
  vram_total_mb: number | null;
  ram_available_mb: number | null;
  ram_used_mb: number | null;
  gpu_utilization: number | null;
  gpu_vendor: string;
}

/** Get live GPU VRAM and system RAM usage. Called once per chat turn. */
export async function getLiveResourceUsage(): Promise<LiveResourceMetrics> {
  return invoke('get_live_resource_usage');
}

// ============================================
// VRAM Calculation Commands
// ============================================

/** Get GGUF metadata from a local file */
export async function getGgufMetadata(path: string): Promise<GgufMetadata> {
  return invoke('get_gguf_metadata', { path });
}

/** Estimate VRAM for a model file */
export async function estimateModelVram(
  fileSizeBytes: number,
  filename: string,
  path?: string,
  contextLength?: number,
  includeKvCache?: boolean
): Promise<VramEstimate> {
  return invoke('estimate_model_vram', {
    path,
    fileSizeBytes,
    filename,
    contextLength,
    includeKvCache,
  });
}

/** Check VRAM compatibility for a model against user's GPU */
export async function checkVramCompatibility(
  fileSizeBytes: number,
  filename: string,
  availableVramMb: number,
  contextLength?: number,
  includeKvCache?: boolean
): Promise<VramCompatibility> {
  return invoke('check_vram_compatibility', {
    fileSizeBytes,
    filename,
    availableVramMb,
    contextLength,
    includeKvCache,
  });
}

/** Get VRAM badge color based on compatibility status */
export function getVramBadgeColor(status: string): { bg: string; text: string; border: string } {
  switch (status) {
    case 'good':
      return { bg: 'bg-green-500/20', text: 'text-green-400', border: 'border-green-500/50' };
    case 'tight':
      return { bg: 'bg-yellow-500/20', text: 'text-yellow-400', border: 'border-yellow-500/50' };
    case 'insufficient':
      return { bg: 'bg-red-500/20', text: 'text-red-400', border: 'border-red-500/50' };
    default:
      return { bg: 'bg-zinc-500/20', text: 'text-zinc-400', border: 'border-zinc-500/50' };
  }
}

/** Get VRAM status icon (for UI) */
export function getVramStatusIcon(status: string): string {
  switch (status) {
    case 'good':
      return '🟢';
    case 'tight':
      return '🟡';
    case 'insufficient':
      return '🔴';
    default:
      return '⚪';
  }
}

/** Format VRAM estimate for display */
export function formatVramEstimate(estimate: VramEstimate): string {
  return `~${estimate.total_gb.toFixed(1)} GB VRAM`;
}

// ============================================
// HuggingFace API (Model Discovery)
// ============================================

const HF_API = 'https://huggingface.co/api';

// Domain tags we care about (subset of HuggingFace's noisy tag list)
const DOMAIN_TAGS = new Set([
  'code', 'chat', 'math', 'agent', 'reasoning', 'roleplay',
  'function-calling', 'tool-use', 'instruct', 'uncensored',
  'multilingual', 'biology', 'chemistry', 'medical', 'legal', 'finance',
]);

// Pipeline tag → human-readable label
const PIPELINE_LABELS: Record<string, string> = {
  'text-generation': 'Text Generation',
  'text2text-generation': 'Text Generation',
  'image-text-to-text': 'Vision + Text',
  'visual-question-answering': 'Visual QA',
  'question-answering': 'Question Answering',
  'summarization': 'Summarization',
  'conversational': 'Conversational',
  'translation': 'Translation',
  'fill-mask': 'Fill Mask',
};

// Name patterns → domain tags (when tags don't cover it)
const NAME_HINTS: [RegExp, string][] = [
  [/coder|codestral|starcoder|deepseek-coder|qwen.*code|code-?llama/i, 'code'],
  [/vision|vl\b|llava|cogvlm|internvl|minicpm-v/i, 'vision'],
  [/math|mathstral|deepseek-math|qwen.*math/i, 'math'],
  [/r1|reasoning|think|cot\b/i, 'reasoning'],
  [/tool|function|hermes/i, 'tool-use'],
  [/uncensored|abliterated|dolphin/i, 'uncensored'],
  [/roleplay|rp\b|mytho|lumimaid/i, 'roleplay'],
  [/instruct|chat\b/i, 'instruct'],
];

/** Get human-readable label for a HuggingFace pipeline_tag */
export function getPipelineLabel(pipelineTag: string): string {
  return PIPELINE_LABELS[pipelineTag] || pipelineTag.replace(/-/g, ' ').replace(/\b\w/g, c => c.toUpperCase());
}

/** Extract domain tags from HuggingFace tags + model name heuristics */
export function extractDomainTags(name: string, tags?: string[]): string[] {
  const domainSet = new Set<string>();

  // 1. Extract domain tags from HF tags
  if (tags) {
    for (const t of tags) {
      const lower = t.toLowerCase();
      if (DOMAIN_TAGS.has(lower)) domainSet.add(lower);
    }
  }

  // 2. Apply name-based heuristics for tags not covered
  for (const [pattern, tag] of NAME_HINTS) {
    if (pattern.test(name)) domainSet.add(tag);
  }

  return [...domainSet];
}

export async function searchHfModels(query: string, limit = 20): Promise<HfModel[]> {
  // Search for GGUF models (with retry for transient network failures)
  const searchQuery = query ? `${query} gguf` : 'gguf';
  const url = `${HF_API}/models?search=${encodeURIComponent(searchQuery)}&filter=gguf&sort=downloads&direction=-1&limit=${limit}`;

  const response = await withRetry(() => fetch(url).then(r => {
    if (!r.ok) throw new Error(`HuggingFace API error: ${r.status}`);
    return r;
  }), 2, 1000);

  const models = await response.json();

  return models.map((m: any) => {
    const tags: string[] = m.tags || [];
    // Extract base_model from tags (not the quantized variant)
    const baseModelTag = tags.find((t: string) =>
      t.startsWith('base_model:') && !t.startsWith('base_model:quantized:')
    );
    const baseModel = baseModelTag?.replace('base_model:', '') || undefined;

    const modelName = m.id?.split('/')[1] || m.id;
    const pipelineTag: string | undefined = m.pipeline_tag || undefined;
    const domainTags = extractDomainTags(modelName, tags);

    return {
      id: m.id || m.modelId,
      author: m.id?.split('/')[0] || 'unknown',
      name: modelName,
      downloads: m.downloads || 0,
      likes: m.likes || 0,
      files: [],
      baseModel,
      pipelineTag,
      domainTags: domainTags.length > 0 ? domainTags : undefined,
    };
  });
}

export async function getHfModelFiles(repoId: string): Promise<HfModelFile[]> {
  // Use /tree/main endpoint — returns actual file sizes in one request
  // (the /models/{id} siblings array never includes size)
  const url = `${HF_API}/models/${repoId}/tree/main`;

  const response = await withRetry(() => fetch(url).then(r => {
    if (!r.ok) throw new Error(`Failed to get model files: ${r.status}`);
    return r;
  }), 2, 1000);

  const files: any[] = await response.json();

  return files
    .filter((f: any) => f.path?.endsWith('.gguf'))
    .map((f: any) => ({
      filename: f.path,
      size: f.lfs?.size || f.size || 0,
      downloadUrl: `https://huggingface.co/${repoId}/resolve/main/${f.path}`,
    }));
}

// ============================================
// Open LLM Leaderboard - Benchmark Scores
// ============================================

const LEADERBOARD_API = 'https://datasets-server.huggingface.co';

/** Fetch benchmark score for a single base model from Open LLM Leaderboard v2 */
async function fetchBenchmarkScore(baseModelId: string): Promise<number | null> {
  try {
    const whereClause = `fullname='${baseModelId.replace(/'/g, "''")}'`;
    const url = `${LEADERBOARD_API}/filter?dataset=open-llm-leaderboard/contents&config=default&split=train&where=${encodeURIComponent(whereClause)}`;
    const response = await withRetry(() => fetch(url), 1, 1000);
    if (!response.ok) return null;
    const data = await response.json();
    const row = data?.rows?.[0]?.row;
    if (!row) return null;
    // Field name has emoji — match robustly by prefix to avoid encoding mismatches
    const avgEntry = Object.entries(row).find(([key]) => key.startsWith('Average'));
    const avg = avgEntry?.[1];
    return typeof avg === 'number' ? avg : null;
  } catch {
    return null;
  }
}

/** Fetch benchmark scores for multiple base models in parallel. Returns map of baseModelId → score */
export async function fetchBenchmarkScores(baseModelIds: string[]): Promise<Record<string, number>> {
  const unique = [...new Set(baseModelIds.filter(Boolean))];
  const results: Record<string, number> = {};

  await Promise.all(
    unique.map(async (id) => {
      const score = await fetchBenchmarkScore(id);
      if (score !== null) results[id] = score;
    })
  );

  return results;
}

// Get file size from URL via Rust (HEAD request)
export async function getRemoteFileSize(url: string): Promise<number> {
  return invoke('get_remote_file_size', { url });
}

// Download progress event type
export interface DownloadProgress {
  downloaded: number;
  total: number;
  percentage: number;
  filename: string;
}

// Download model using Rust (streams directly to disk, no memory issues)
export async function downloadModel(
  url: string,
  filename: string,
  onProgress?: (downloaded: number, total: number) => void
): Promise<string> {
  let unlisten: UnlistenFn | null = null;

  // Set up progress listener
  if (onProgress) {
    unlisten = await listen<DownloadProgress>('download-progress', (event) => {
      if (event.payload.filename === filename) {
        onProgress(event.payload.downloaded, event.payload.total);
      }
    });
  }

  try {
    // Use Rust command for streaming download
    const savedPath: string = await invoke('download_model', { url, filename });
    return savedPath;
  } finally {
    // Clean up listener
    if (unlisten) {
      unlisten();
    }
  }
}

// Download model directly to WSL filesystem (for WSL backend)
export async function downloadModelWsl(
  url: string,
  filename: string,
  onProgress?: (downloaded: number, total: number) => void
): Promise<string> {
  let unlisten: UnlistenFn | null = null;

  // Set up progress listener
  if (onProgress) {
    unlisten = await listen<DownloadProgress>('download-progress', (event) => {
      if (event.payload.filename === filename) {
        onProgress(event.payload.downloaded, event.payload.total);
      }
    });
  }

  try {
    // Use Rust command for WSL download (saves to $HOME/models in WSL)
    const savedPath: string = await invoke('download_model_wsl', { url, filename });
    return savedPath;
  } finally {
    // Clean up listener
    if (unlisten) {
      unlisten();
    }
  }
}

// ============================================
// llama.cpp Server API (Inference)
// ============================================

export async function checkServerHealth(port = 8080): Promise<boolean> {
  try {
    const response = await fetch(`http://127.0.0.1:${port}/health`);
    return response.ok;
  } catch {
    return false;
  }
}

export async function chat(
  messages: ChatMessage[],
  port = 8080,
  onToken?: (token: string) => void,
  signal?: AbortSignal
): Promise<string> {
  const response = await fetch(`http://127.0.0.1:${port}/v1/chat/completions`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      messages,
      stream: true,
      // llama.cpp KV cache: reuse prefix tokens from previous request on this slot.
      // The stable system prompt is identical across turns, so llama-server skips
      // re-evaluating those tokens entirely. Major perf win for local models.
      cache_prompt: true,
      id_slot: 0,
    }),
    signal, // Pass abort signal to fetch
  });

  if (!response.ok) {
    const body = await response.text().catch(() => '');
    throw new Error(`Chat failed (HTTP ${response.status}): ${body || response.statusText}`);
  }
  if (!response.body) return '';

  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let fullResponse = '';
  // Buffer for incomplete SSE lines that arrive split across chunks.
  // Without this, partial JSON at chunk boundaries gets silently dropped.
  let lineBuffer = '';

  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;

      // Prepend any leftover from previous chunk
      const chunk = lineBuffer + decoder.decode(value, { stream: true });
      const lines = chunk.split('\n');

      // Last element may be incomplete — hold it for next iteration
      lineBuffer = lines.pop() || '';

      for (const line of lines) {
        const trimmed = line.trim();
        if (!trimmed.startsWith('data: ')) continue;

        const data = trimmed.slice(6);
        if (data === '[DONE]') continue;

        try {
          const json = JSON.parse(data);
          if (json.error) {
            throw new Error(`Server error: ${json.error.message || JSON.stringify(json.error)}`);
          }
          const content = json.choices?.[0]?.delta?.content;
          if (content) {
            fullResponse += content;
            if (onToken) onToken(content);
          }
        } catch (parseErr) {
          if (parseErr instanceof Error && parseErr.message.startsWith('Server error:')) throw parseErr;
        }
      }
    }

    // Process any remaining buffered data after stream ends
    if (lineBuffer.trim().startsWith('data: ')) {
      const data = lineBuffer.trim().slice(6);
      if (data !== '[DONE]') {
        try {
          const json = JSON.parse(data);
          const content = json.choices?.[0]?.delta?.content;
          if (content) {
            fullResponse += content;
            if (onToken) onToken(content);
          }
        } catch { /* ignore incomplete final chunk */ }
      }
    }
  } catch (e) {
    if (signal?.aborted) {
      return fullResponse;
    }
    throw e;
  }

  return fullResponse;
}


// ============================================
// App Settings (global, not per-model)
// ============================================

/** Tool approval modes — how HIVE handles tool execution permissions */
export type ToolApprovalMode = 'ask' | 'session' | 'auto';

export interface AppSettings {
  chatPersistence: boolean;     // Save/restore conversations across sessions
  memoryEnabled: boolean;       // Enable memory system (auto-save + recall)
  harnessEnabled: boolean;      // Enable cognitive harness (identity + capabilities injection)
  toolApprovalMode: ToolApprovalMode;  // How tool calls are approved
  toolOverrides: Record<string, string>;  // Per-tool risk overrides (tool_name -> 'low'|'medium'|'high'|'critical'|'disabled')
  thinkingDepth: Record<string, ThinkingDepth>;  // Per-provider thinking depth (P1: provider-agnostic)
  minimizeToTray: boolean;         // P6: Minimize to system tray on window close
}

const DEFAULT_APP_SETTINGS: AppSettings = {
  chatPersistence: false,
  memoryEnabled: true,          // Memory on by default (Principle 8: low floor)
  harnessEnabled: true,         // Harness on by default — the cognitive layer
  toolApprovalMode: 'ask',      // Default: always ask for high-risk (safe default)
  toolOverrides: {},             // No overrides — use tool's native risk level
  thinkingDepth: {},             // No override — default = off (no reasoning budget)
  minimizeToTray: false,          // P6: Default off — closing exits the app
};

const APP_SETTINGS_KEY = 'hive_app_settings';

export function getAppSettings(): AppSettings {
  try {
    const stored = localStorage.getItem(APP_SETTINGS_KEY);
    return stored ? { ...DEFAULT_APP_SETTINGS, ...JSON.parse(stored) } : { ...DEFAULT_APP_SETTINGS };
  } catch {
    return { ...DEFAULT_APP_SETTINGS };
  }
}

export function saveAppSettings(settings: Partial<AppSettings>): void {
  const current = getAppSettings();
  const merged = { ...current, ...settings };
  localStorage.setItem(APP_SETTINGS_KEY, JSON.stringify(merged));
}

// ============================================
// Model Settings (per-model persistence)
// ============================================

/** Settings stored per model */
export interface ModelSettings {
  contextLength: number;            // Context window size
  kvOffload: boolean;               // Offload KV cache to RAM
  gpuLayers: number;                // Layers to offload to GPU (-1 = all)
  systemPrompt: string;             // Custom system prompt for this model
}

const DEFAULT_MODEL_SETTINGS: ModelSettings = {
  contextLength: 4096,
  kvOffload: false,
  gpuLayers: 99,
  systemPrompt: '',
};

const SETTINGS_KEY = 'hive_model_settings';

/** Get all model settings from localStorage */
export function getAllModelSettings(): Record<string, ModelSettings> {
  try {
    const stored = localStorage.getItem(SETTINGS_KEY);
    return stored ? JSON.parse(stored) : {};
  } catch {
    return {};
  }
}

/** Get settings for a specific model (by filename) */
export function getModelSettings(filename: string): ModelSettings {
  const all = getAllModelSettings();
  return { ...DEFAULT_MODEL_SETTINGS, ...(all[filename] || {}) };
}

/** Save settings for a specific model */
export function saveModelSettings(filename: string, settings: Partial<ModelSettings>): void {
  const all = getAllModelSettings();
  all[filename] = { ...getModelSettings(filename), ...settings };
  localStorage.setItem(SETTINGS_KEY, JSON.stringify(all));
}

/** Get default model settings */
export function getDefaultModelSettings(): ModelSettings {
  return { ...DEFAULT_MODEL_SETTINGS };
}

// ============================================
// Provider Types
// ============================================

export type ProviderType = 'local' | 'ollama' | 'openai' | 'anthropic' | 'openrouter' | 'dashscope';

export interface ProviderConfig {
  provider_type: ProviderType;
  name: string;
  endpoint: string | null;
  enabled: boolean;
  has_api_key: boolean;
}

export interface ProviderModel {
  id: string;
  name: string;
  provider: ProviderType;
  context_length: number | null;
  description: string | null;
}

export interface ProviderStatus {
  provider_type: ProviderType;
  configured: boolean;
  connected: boolean;
  error: string | null;
  models: ProviderModel[];
}

// ============================================
// Secure Storage Commands (API Keys)
// ============================================

/**
 * Store an API key securely in OS keyring
 * - Windows: Windows Credential Manager (DPAPI encrypted)
 * - macOS: Keychain (AES-256)
 * - Linux: Secret Service
 *
 * SECURITY: Keys are encrypted by the OS, never stored in plaintext
 */
export async function storeApiKey(provider: ProviderType, apiKey: string): Promise<string> {
  const result = await invoke<string>('store_api_key', { provider, apiKey });
  console.log(`[HIVE] storeApiKey result: ${result}`);
  return result;
}

/**
 * Store multiple API keys for a provider (P2: multi-key rotation).
 * Single key stored as plain string, multiple as JSON array — backwards compatible.
 */
export async function storeApiKeys(provider: ProviderType, apiKeys: string[]): Promise<string> {
  return invoke<string>('store_api_keys', { provider, apiKeys });
}

/**
 * Get the number of configured API keys for a provider.
 */
export async function getApiKeyCount(provider: ProviderType): Promise<number> {
  return invoke<number>('get_api_key_count', { provider });
}

/**
 * Check if an API key is configured (without exposing the key)
 */
export async function hasApiKey(provider: ProviderType): Promise<boolean> {
  return invoke('has_api_key', { provider });
}

/**
 * Delete an API key from secure storage
 */
export async function deleteApiKey(provider: ProviderType): Promise<void> {
  return invoke('delete_api_key', { provider });
}

// ============================================
// P6: System Tray
// ============================================

/** Set minimize-to-tray behavior (syncs Rust AtomicBool with frontend setting). */
export async function setMinimizeToTray(enabled: boolean): Promise<void> {
  return invoke('set_minimize_to_tray', { enabled });
}

/** Get current minimize-to-tray state from Rust. */
export async function getMinimizeToTray(): Promise<boolean> {
  return invoke<boolean>('get_minimize_to_tray');
}

// ============================================
// P7: Cloudflare Tunnel (Remote Access)
// ============================================

/** Start a Cloudflare tunnel exposing a local port. Returns the public URL. */
export async function tunnelStart(port: number): Promise<string> {
  return invoke<string>('tunnel_start', { port });
}

/** Stop the running Cloudflare tunnel. */
export async function tunnelStop(): Promise<void> {
  return invoke('tunnel_stop');
}

/** Get the current tunnel URL (null if not running). */
export async function tunnelStatus(): Promise<string | null> {
  return invoke<string | null>('tunnel_status');
}

// ============================================
// Encrypted Hardware Data
// ============================================

/**
 * Store hardware fingerprint encrypted (AES-256-GCM)
 * SECURITY: Data encrypted locally, encryption key in OS keyring
 * This data NEVER leaves the device
 */
export async function storeEncryptedHardwareData(data: string): Promise<void> {
  return invoke('store_encrypted_hardware_data', { data });
}

/**
 * Retrieve and decrypt hardware fingerprint
 */
export async function getEncryptedHardwareData(): Promise<string | null> {
  return invoke('get_encrypted_hardware_data');
}

// ============================================
// Provider Management Commands
// ============================================

/**
 * Get list of all configured providers
 */
export async function getProviders(): Promise<ProviderConfig[]> {
  return invoke('get_providers');
}

/**
 * Check provider connection status and get available models
 */
export async function checkProviderStatus(provider: ProviderType): Promise<ProviderStatus> {
  return invoke('check_provider_status', { provider });
}

/**
 * Chat with a cloud provider (OpenAI, Anthropic, Ollama) — non-streaming
 */
export async function chatWithProvider(
  provider: ProviderType,
  model: string,
  messages: ChatMessage[],
  thinkingDepth?: ThinkingDepth,
): Promise<string> {
  return invoke('chat_with_provider', {
    provider,
    model,
    messages: messages.map(m => ({ role: m.role, content: m.content })),
    thinking_depth: thinkingDepth || null,
  });
}

/**
 * Chat with a cloud provider using streaming.
 * Emits "cloud-chat-token" for content and "cloud-thinking-token" for reasoning.
 * Listen with onCloudChatToken() / onCloudThinkingToken() before calling this.
 * Returns StreamResponse with both content and thinking separated (P1: modularity).
 */
export async function chatWithProviderStream(
  provider: ProviderType,
  model: string,
  messages: ChatMessage[],
  streamId?: string,
  thinkingDepth?: ThinkingDepth,
): Promise<StreamResponse> {
  return invoke('chat_with_provider_stream', {
    provider,
    model,
    messages: messages.map(m => ({ role: m.role, content: m.content })),
    stream_id: streamId || undefined,
    thinking_depth: thinkingDepth || null,
  });
}

/** Payload shape for streaming token events (multi-pane aware) */
interface StreamTokenPayload {
  token: string;
  stream_id: string;
}

/**
 * Listen for streaming content tokens from cloud providers.
 * When streamId is provided, only tokens matching that stream are passed to the callback.
 * This enables concurrent multi-pane streaming without token collision.
 * Returns an unlisten function.
 */
export async function onCloudChatToken(
  callback: (token: string) => void,
  streamId?: string,
): Promise<UnlistenFn> {
  return listen<StreamTokenPayload>('cloud-chat-token', (event) => {
    // Filter by stream_id when provided (multi-pane mode)
    if (streamId && event.payload.stream_id && event.payload.stream_id !== streamId) return;
    callback(event.payload.token);
  });
}

/**
 * Listen for streaming thinking/reasoning tokens from cloud providers.
 * When streamId is provided, only tokens matching that stream are passed to the callback.
 * Returns an unlisten function.
 */
export async function onCloudThinkingToken(
  callback: (token: string) => void,
  streamId?: string,
): Promise<UnlistenFn> {
  return listen<StreamTokenPayload>('cloud-thinking-token', (event) => {
    // Filter by stream_id when provided (multi-pane mode)
    if (streamId && event.payload.stream_id && event.payload.stream_id !== streamId) return;
    callback(event.payload.token);
  });
}

// ============================================
// Tool Framework
// ============================================

import type { ToolSchema, ToolResult, ChatResponse, StreamResponse, ToolCall, HarnessContext, CapabilitySnapshot, TelegramIncoming, TelegramDaemonStatus, DiscordIncoming, DiscordDaemonStatus, ThinkingDepth } from '../types';
export type { ToolSchema, ToolResult, ChatResponse, StreamResponse, ToolCall, HarnessContext, CapabilitySnapshot, TelegramIncoming, TelegramDaemonStatus, DiscordIncoming, DiscordDaemonStatus };

/**
 * Get all available tool schemas (for sending to the model)
 */
export async function getAvailableTools(): Promise<ToolSchema[]> {
  return invoke('get_available_tools');
}

/**
 * Execute a specific tool by name with given arguments
 */
export async function executeTool(name: string, args: Record<string, unknown>): Promise<ToolResult> {
  return invoke('execute_tool', { name, arguments: args });
}

/** Append a line to the persistent app log (hive-app.log).
 *  Fire-and-forget — never blocks the tool chain. */
export function logToApp(line: string): void {
  invoke('log_to_app', { line }).catch(() => {});
}

/** Worker status entry returned by get_worker_statuses */
export interface WorkerStatus {
  id: string;
  model: string;
  provider: string;
  task: string;
  scratchpad_id: string;
  status: string;
  started_at: string;
  elapsed_seconds: number;
  idle_seconds: number;
  turns_used: number;
  max_turns: number;
  max_time_seconds: number;
  tools_executed: number;
  summary: string;
}

/**
 * Get all worker statuses (polled by WorkerPanel)
 */
export async function getWorkerStatuses(): Promise<WorkerStatus[]> {
  return invoke('get_worker_statuses');
}

/** Phase 5C: Write an activity entry to the context bus (shared agent feed). */
export async function contextBusWrite(agent: string, content: string): Promise<void> {
  return invoke('context_bus_write', { agent, content });
}

/** Phase 5C: Read compact context bus summary for volatile context injection. */
export async function contextBusSummary(): Promise<string> {
  return invoke('context_bus_summary');
}

/**
 * Set the current session's provider and model ID on the Rust side.
 * Workers and tools inherit these as defaults — they never need to guess API model IDs.
 * Called once at chat start (P2: agnostic, P7: framework survives).
 */
export async function setSessionModelContext(provider: ProviderType, modelId: string): Promise<void> {
  return invoke('set_session_model_context', { provider, modelId });
}

/**
 * Chat with a provider, passing tool schemas. Returns text or tool_calls.
 */
export async function chatWithTools(
  provider: ProviderType,
  model: string,
  messages: ChatMessage[],
  tools: ToolSchema[],
  contextLength?: number,
  thinkingDepth?: ThinkingDepth,
): Promise<ChatResponse> {
  return invoke('chat_with_tools', {
    provider,
    model,
    messages: messages.map(m => {
      const msg: Record<string, unknown> = { role: m.role, content: m.content };
      if (m.tool_call_id) {
        msg.tool_call_id = m.tool_call_id;
      }
      // Preserve tool_calls on assistant messages — without this, the API sees
      // orphaned tool_call_id references with no matching tool_calls, breaking
      // the conversation flow and slowing/confusing the model on multi-turn tool use.
      if (m.tool_calls) {
        msg.tool_calls = m.tool_calls;
      }
      return msg;
    }),
    tools,
    contextLength: contextLength ?? null,
    thinking_depth: thinkingDepth || null,
  });
}

/**
 * Check if a tool call needs user approval based on risk level and approval settings.
 * - 'ask' mode (default): high/critical tools always prompt
 * - 'session' mode: high/critical prompt once per session, then auto-approve
 * - 'auto' mode: never prompt (power user — they accept the risk)
 */
export function needsApproval(
  riskLevel: string,
  toolName?: string,
  settings?: AppSettings,
): boolean {
  const mode = settings?.toolApprovalMode ?? 'ask';

  // Auto mode: never ask (P8: high ceiling for power users)
  if (mode === 'auto') return false;

  // Check per-tool overrides first
  if (toolName && settings?.toolOverrides?.[toolName]) {
    const override = settings.toolOverrides[toolName];
    if (override === 'disabled') return true; // always block disabled tools
    // Use the override risk level instead of native
    riskLevel = override;
  }

  return riskLevel === 'high' || riskLevel === 'critical';
}

// ============================================
// Remote Channel Tool Security
// ============================================

/** Dangerous tools — blocked for remote Users, always-prompt for remote Hosts.
 *  MUST stay in sync with is_dangerous_tool() in content_security.rs (Rust gate). */
const DANGEROUS_TOOLS = new Set([
  'run_command', 'write_file', 'telegram_send', 'discord_send',
  'github_issues', 'github_prs',
  'worker_spawn',       // spawns processes with inherited tool access
  'send_to_agent',      // writes to PTY stdin — command execution vector
  'plan_execute',       // chains tool calls — can compose dangerous sequences
  'memory_import_file', // reads arbitrary local files into persistent DB
]);

/** Desktop-only tools — blocked for ALL remote origins (even Hosts) */
const DESKTOP_ONLY_TOOLS = new Set(['run_command', 'write_file']);

/**
 * Check if a tool is allowed for the given message origin.
 * Returns null if allowed, or an error message string if blocked.
 */
export function checkToolOriginAccess(toolName: string, origin: import('../types').MessageOrigin): string | null {
  if (origin === 'desktop' || origin === 'pty-agent') return null; // local origins: everything allowed

  if (origin === 'remote-host') {
    // Host over remote channel: desktop-only tools are blocked outright
    if (DESKTOP_ONLY_TOOLS.has(toolName)) {
      return `Tool '${toolName}' is desktop-only and cannot be executed over remote channels. Use the HIVE desktop UI to run this command.`;
    }
    return null; // dangerous but non-desktop-only tools: allowed (approval forced separately)
  }

  // remote-user: all dangerous tools blocked
  if (DANGEROUS_TOOLS.has(toolName)) {
    return `Tool '${toolName}' is restricted. Remote users cannot execute dangerous tools. Ask the HIVE host to run this for you.`;
  }
  return null;
}

/**
 * Get provider display info
 */
export function getProviderInfo(provider: ProviderType): { name: string; color: string; icon: string } {
  switch (provider) {
    case 'local':
      return { name: 'Local (llama.cpp)', color: 'text-green-400', icon: '🖥️' };
    case 'ollama':
      return { name: 'Ollama', color: 'text-blue-400', icon: '🦙' };
    case 'openai':
      return { name: 'OpenAI', color: 'text-emerald-400', icon: '🤖' };
    case 'anthropic':
      return { name: 'Anthropic', color: 'text-orange-400', icon: '🧠' };
    case 'openrouter':
      return { name: 'OpenRouter', color: 'text-purple-400', icon: '🔀' };
    case 'dashscope':
      return { name: 'DashScope', color: 'text-yellow-400', icon: '🌐' };
    default:
      return { name: 'Unknown', color: 'text-gray-400', icon: '❓' };
  }
}

// Context Management (Infinite Chat)
// ============================================

/**
 * Estimate token count for a string (rough approximation)
 * Rule of thumb: ~4 characters per token for English text
 */
export function estimateTokens(text: string): number {
  return Math.ceil(text.length / 4);
}

/**
 * Strip thinking tokens from model output, matching Rust-side strip_thinking() (P5: same pattern everywhere).
 * Handles /think ... /think (DashScope/Kimi) and <think>...</think> (DeepSeek R1).
 * Returns [cleanContent, thinking | null].
 */
export function stripThinking(text: string): [string, string | null] {
  const parts: string[] = [];
  let clean = text;

  // /think ... /think (DashScope Kimi K2.5)
  const slashRe = /\/think\s*([\s\S]*?)\s*\/think/g;
  let match;
  while ((match = slashRe.exec(text)) !== null) {
    if (match[1].trim()) parts.push(match[1].trim());
  }
  clean = clean.replace(slashRe, '');

  // <think>...</think> (DeepSeek R1, some Qwen)
  const xmlRe = /<think>\s*([\s\S]*?)\s*<\/think>/g;
  while ((match = xmlRe.exec(clean)) !== null) {
    if (match[1].trim()) parts.push(match[1].trim());
  }
  clean = clean.replace(xmlRe, '').trim();

  return [clean, parts.length > 0 ? parts.join('\n\n') : null];
}

/**
 * Estimate total tokens for a message array
 */
export function estimateMessagesTokens(messages: ChatMessage[]): number {
  return messages.reduce((sum, msg) => {
    // Add overhead for role formatting (~4 tokens per message)
    return sum + estimateTokens(msg.content) + 4;
  }, 0);
}

/**
 * Truncate messages to fit within token limit while preserving:
 * 1. System prompt (always kept)
 * 2. Last N messages (most recent context)
 * 3. First user message (original intent)
 *
 * Returns truncated messages array
 */
export function truncateMessagesToFit(
  messages: ChatMessage[],
  maxTokens: number,
  reserveForResponse: number = 1024
): ChatMessage[] {
  const effectiveLimit = maxTokens - reserveForResponse;

  if (messages.length === 0) return messages;

  // Separate system messages from conversation
  const systemMessages = messages.filter(m => m.role === 'system');
  const conversationMessages = messages.filter(m => m.role !== 'system');

  // Calculate system prompt tokens (always kept)
  const systemTokens = estimateMessagesTokens(systemMessages);
  const availableForConversation = effectiveLimit - systemTokens;

  if (availableForConversation <= 0) {
    // System prompt alone exceeds limit - just return system + last message
    console.warn('[HIVE] System prompt exceeds context limit');
    return [...systemMessages, ...conversationMessages.slice(-1)];
  }

  // Build conversation from the end (most recent first)
  const keptMessages: ChatMessage[] = [];
  let usedTokens = 0;

  // Always try to keep the first user message for context
  const firstUserMsg = conversationMessages.find(m => m.role === 'user');
  const firstUserTokens = firstUserMsg ? estimateTokens(firstUserMsg.content) + 4 : 0;

  // Work backwards through messages
  for (let i = conversationMessages.length - 1; i >= 0; i--) {
    const msg = conversationMessages[i];
    const msgTokens = estimateTokens(msg.content) + 4;

    // Reserve space for first user message if we haven't included it yet
    const reserveFirst = (firstUserMsg && !keptMessages.includes(firstUserMsg)) ? firstUserTokens : 0;

    if (usedTokens + msgTokens + reserveFirst <= availableForConversation) {
      keptMessages.unshift(msg);
      usedTokens += msgTokens;
    } else if (keptMessages.length === 0) {
      // Always keep at least the last message (truncated if needed)
      keptMessages.unshift(msg);
      break;
    } else {
      break;
    }
  }

  // Add first user message if not already included and we have space
  if (firstUserMsg && !keptMessages.includes(firstUserMsg) && keptMessages.length > 0) {
    const msgTokens = estimateTokens(firstUserMsg.content) + 4;
    if (usedTokens + msgTokens <= availableForConversation) {
      // Insert after any initial context but before recent messages
      keptMessages.unshift(firstUserMsg);
    }
  }

  // Repair orphaned tool results: if truncation dropped an assistant message
  // that contained tool_calls, the paired role:"tool" messages are now orphans.
  // Anthropic's API rejects these. Remove any tool messages at the start of
  // the kept conversation that have no preceding assistant message.
  while (keptMessages.length > 0 && keptMessages[0].role === 'tool') {
    keptMessages.shift();
  }

  // Log truncation
  const dropped = conversationMessages.length - keptMessages.length;
  if (dropped > 0) {
    console.log(`[HIVE] Context management: Dropped ${dropped} old messages to fit context window`);
  }

  return [...systemMessages, ...keptMessages];
}

// ============================================
// Conversation Persistence
// ============================================

export interface Conversation {
  id: string;
  title: string;
  messages: ChatMessage[];
  modelId: string;
  createdAt: string;
  updatedAt: string;
}

const CONVERSATIONS_KEY = 'hive_conversations';
const CURRENT_CONVERSATION_KEY = 'hive_current_conversation';

/** Generate a unique conversation ID */
export function generateConversationId(): string {
  return `conv_${Date.now()}_${Math.random().toString(36).substring(2, 11)}`;
}

/** Get all conversations */
export function getConversations(): Conversation[] {
  try {
    const stored = localStorage.getItem(CONVERSATIONS_KEY);
    return stored ? JSON.parse(stored) : [];
  } catch {
    return [];
  }
}

/** Get a specific conversation */
export function getConversation(id: string): Conversation | null {
  const all = getConversations();
  return all.find(c => c.id === id) || null;
}

/** Save a conversation */
export function saveConversation(conversation: Conversation): void {
  const all = getConversations();
  const index = all.findIndex(c => c.id === conversation.id);

  if (index >= 0) {
    all[index] = conversation;
  } else {
    all.unshift(conversation); // Add to beginning
  }

  // Keep only last 50 conversations
  const trimmed = all.slice(0, 50);
  localStorage.setItem(CONVERSATIONS_KEY, JSON.stringify(trimmed));
}

/** Delete a conversation */
export function deleteConversation(id: string): void {
  const all = getConversations();
  const filtered = all.filter(c => c.id !== id);
  localStorage.setItem(CONVERSATIONS_KEY, JSON.stringify(filtered));
}

/** Get current conversation ID */
export function getCurrentConversationId(): string | null {
  return localStorage.getItem(CURRENT_CONVERSATION_KEY);
}

/** Set current conversation ID */
export function setCurrentConversationId(id: string | null): void {
  if (id) {
    localStorage.setItem(CURRENT_CONVERSATION_KEY, id);
  } else {
    localStorage.removeItem(CURRENT_CONVERSATION_KEY);
  }
}

/** Generate a title from the first message */
export function generateConversationTitle(messages: ChatMessage[]): string {
  const firstUser = messages.find(m => m.role === 'user');
  if (!firstUser) return 'New Conversation';

  const content = (firstUser.content || '').trim();
  if (content.length <= 40) return content;
  return content.substring(0, 40) + '...';
}

// Server Log Reading
// ============================================

/** Read last N lines from llama-server.log */
export async function readServerLog(lines?: number): Promise<string> {
  return invoke('read_server_log', { lines: lines || null });
}

// ============================================
// Retry Logic (Exponential Backoff)
// ============================================

/**
 * Retry an async function with exponential backoff.
 * Useful for network requests that may transiently fail.
 */
export async function withRetry<T>(
  fn: () => Promise<T>,
  maxRetries = 3,
  baseDelayMs = 1000,
): Promise<T> {
  let lastError: unknown;
  for (let i = 0; i <= maxRetries; i++) {
    try {
      return await fn();
    } catch (e) {
      lastError = e;
      if (i < maxRetries) {
        const delay = baseDelayMs * Math.pow(2, i);
        await new Promise(r => setTimeout(r, delay));
      }
    }
  }
  throw lastError;
}

// ============================================
// Conversation Export / Import
// ============================================

/** Export all conversations as a JSON string for download */
export function exportConversationsToJson(): string {
  const conversations = getConversations();
  return JSON.stringify({
    version: 1,
    app: 'HIVE',
    exported: new Date().toISOString(),
    conversations,
  }, null, 2);
}

/** Import conversations from a JSON string. Returns count of imported conversations. */
export function importConversationsFromJson(json: string): number {
  const data = JSON.parse(json);
  if (!data.conversations || !Array.isArray(data.conversations)) {
    throw new Error('Invalid format: missing conversations array');
  }
  let imported = 0;
  for (const conv of data.conversations) {
    if (conv.id && conv.messages && Array.isArray(conv.messages)) {
      // Avoid overwriting existing conversations — generate new ID
      const existing = getConversation(conv.id);
      if (existing) {
        conv.id = generateConversationId();
      }
      saveConversation(conv);
      imported++;
    }
  }
  return imported;
}

// ============================================
// Cognitive Harness
// ============================================

/** Build the harness system prompt from current capabilities + user instructions */
export async function harnessBuild(
  capabilities: CapabilitySnapshot,
  userSystemPrompt?: string,
): Promise<HarnessContext> {
  return invoke('harness_build', {
    capabilities,
    userSystemPrompt: userSystemPrompt || null,
  });
}

/** Get the current identity file content (HIVE.md) */
export async function harnessGetIdentity(): Promise<string> {
  return invoke('harness_get_identity');
}

/** Save updated identity content */
export async function harnessSaveIdentity(content: string): Promise<string> {
  return invoke('harness_save_identity', { content });
}

/** Reset identity to factory default */
export async function harnessResetIdentity(): Promise<string> {
  return invoke('harness_reset_identity');
}

/** Get the path to the identity file (for external editing) */
export async function harnessGetIdentityPath(): Promise<string> {
  return invoke('harness_get_identity_path');
}

// ============================================
// Skills (Phase 4.5.5)
// ============================================

export interface SkillInfo {
  name: string;
  path: string;
  size_bytes: number;
}

/** List all skill files in ~/.hive/skills/ */
export async function harnessListSkills(): Promise<SkillInfo[]> {
  return invoke('harness_list_skills');
}

/** Read a specific skill file content */
export async function harnessReadSkill(name: string): Promise<string> {
  return invoke('harness_read_skill', { name });
}

/** Get the skills directory path */
export async function harnessGetSkillsPath(): Promise<string> {
  return invoke('harness_get_skills_path');
}

/** Open the skills directory in the system file explorer */
export async function harnessOpenSkillsDir(): Promise<void> {
  return invoke('harness_open_skills_dir');
}

/** Get relevant skills for the current user message (context injection) */
export async function harnessGetRelevantSkills(query: string): Promise<string> {
  return invoke('harness_get_relevant_skills', { query });
}

// ============================================
// Phase 4: Slot System & Orchestrator
// ============================================

import {
  type SlotRole, type SlotConfig, type SlotState,
  type VramBudget, type RouteDecision,
} from '../types';

export type { SlotRole, SlotConfig, SlotState, VramBudget, RouteDecision };

// --- Slot management ---

export async function getSlotConfigs(): Promise<SlotConfig[]> {
  return invoke('get_slot_configs');
}

export async function getSlotStates(): Promise<SlotState[]> {
  return invoke('get_slot_states');
}

export async function configureSlot(
  role: SlotRole, provider: string, model: string, vramGb: number, contextLength: number,
): Promise<SlotConfig> {
  return invoke('configure_slot', { role, provider, model, vramGb, contextLength });
}

export async function addSlotFallback(
  role: SlotRole, provider: string, model: string, vramGb: number, contextLength: number,
): Promise<SlotConfig> {
  return invoke('add_slot_fallback', { role, provider, model, vramGb, contextLength });
}

export async function getVramBudget(): Promise<VramBudget> {
  return invoke('get_vram_budget');
}

export async function setVramTotal(totalGb: number): Promise<VramBudget> {
  return invoke('set_vram_total', { totalGb });
}

// --- Orchestrator ---

export async function routeTask(task: string): Promise<RouteDecision> {
  return invoke('route_task', { task });
}

export async function getWakeContext(role: SlotRole, task: string): Promise<string> {
  return invoke('get_wake_context', { role, task });
}

export async function recordSlotWake(
  role: SlotRole, provider: string, model: string, port: number | null, vramGb: number,
): Promise<SlotState> {
  return invoke('record_slot_wake', { role, provider, model, port, vramGb });
}

export async function recordSlotSleep(role: SlotRole): Promise<{ slot: SlotRole; vram_freed_gb: number; events_recorded: number }> {
  return invoke('record_slot_sleep', { role });
}

// --- Specialist servers ---

export async function startSpecialistServer(
  slotRole: string, modelPath: string,
  gpuLayers?: number, contextLength?: number, kvOffload?: boolean,
): Promise<ServerStatus> {
  return invoke('start_specialist_server', { slotRole, modelPath, gpuLayers, contextLength, kvOffload });
}

export async function startSpecialistServerWsl(
  slotRole: string, modelPath: string,
  gpuLayers?: number, contextLength?: number, kvOffload?: boolean, llamaServerPath?: string,
): Promise<ServerStatus> {
  return invoke('start_specialist_server_wsl', { slotRole, modelPath, gpuLayers, contextLength, kvOffload, llamaServerPath });
}

export async function stopSpecialistServer(slotRole: string): Promise<void> {
  return invoke('stop_specialist_server', { slotRole });
}

export async function getSpecialistServers(): Promise<ServerStatus[]> {
  return invoke('get_specialist_servers');
}

// ============================================
// PTY Terminal (Phase 10 — NEXUS)
// ============================================

export interface PtySessionInfo {
  id: string;
  command: string;
  started_at: string;
  exited: boolean;
}

/** Spawn a new PTY session. Returns session ID (UUID).
 *  Set bridgeToChat=true to auto-inject agent output into the orchestrator chat. */
export async function ptySpawn(
  command: string, args: string[], cols: number, rows: number,
  bridgeToChat?: boolean,
): Promise<string> {
  return invoke('pty_spawn', { command, args, cols, rows, bridgeToChat: bridgeToChat ?? false });
}

/** Write input data (keystrokes) to a PTY session. */
export async function ptyWrite(sessionId: string, data: string): Promise<void> {
  return invoke('pty_write', { sessionId, data });
}

/** Resize a PTY session (cols x rows). */
export async function ptyResize(sessionId: string, cols: number, rows: number): Promise<void> {
  return invoke('pty_resize', { sessionId, cols, rows });
}

/** Kill a PTY session and clean up. */
export async function ptyKill(sessionId: string): Promise<void> {
  return invoke('pty_kill', { sessionId });
}

/** List all active PTY sessions (metadata only). */
export async function ptyList(): Promise<PtySessionInfo[]> {
  return invoke('pty_list');
}

/** Listen for PTY output events. Caller filters by session_id. */
export async function onPtyOutput(
  callback: (sessionId: string, data: string) => void
): Promise<UnlistenFn> {
  return listen<{ session_id: string; data: string }>('pty-output', (event) => {
    callback(event.payload.session_id, event.payload.data);
  });
}

/** Listen for PTY exit events. */
export async function onPtyExit(
  callback: (sessionId: string, exitCode: number | null) => void
): Promise<UnlistenFn> {
  return listen<{ session_id: string; exit_code: number | null }>('pty-exit', (event) => {
    callback(event.payload.session_id, event.payload.exit_code);
  });
}

/** Listen for PTY log events (accumulated output, ANSI-stripped, for memory logging). */
export async function onPtyLog(
  callback: (sessionId: string, agentName: string, content: string) => void
): Promise<UnlistenFn> {
  return listen<{ session_id: string; agent_name: string; content: string }>('pty-log', (event) => {
    callback(event.payload.session_id, event.payload.agent_name, event.payload.content);
  });
}

/** Check if a CLI agent command is available. Returns path if found, empty string if not. */
export async function checkAgentAvailable(command: string): Promise<string> {
  return invoke('check_agent_available', { command });
}

// ============================================
// Agent Registry (Phase 10.4 — NEXUS)
// ============================================

const CUSTOM_AGENTS_KEY = 'hive-custom-agents';

/** Load custom agents from localStorage. */
export function getCustomAgents(): import('../types').AgentConfig[] {
  try {
    const saved = localStorage.getItem(CUSTOM_AGENTS_KEY);
    if (saved) return JSON.parse(saved);
  } catch { /* ignore */ }
  return [];
}

/** Save custom agents to localStorage. */
export function saveCustomAgents(agents: import('../types').AgentConfig[]): void {
  try {
    localStorage.setItem(CUSTOM_AGENTS_KEY, JSON.stringify(agents));
  } catch { /* non-fatal */ }
}

// ============================================
// MCP Auto-Bridge (Phase 10.5.2 — NEXUS)
// ============================================

/** Set up the MCP bridge for a CLI agent (writes HIVE MCP entry into agent's config). */
export async function setupMcpBridge(agent: string): Promise<string> {
  return invoke('setup_mcp_bridge', { agent });
}

// ============================================
// Remote Channel Routing (Phase 10.5.4 — NEXUS)
// ============================================

export interface ChannelRoutingConfig {
  telegram: 'chat' | string; // 'chat' = active chat pane, or agent command to route to
  discord: 'chat' | string;
}

const CHANNEL_ROUTING_KEY = 'hive-channel-routing';

/** Load channel routing config from localStorage. Default: both route to chat. */
export function getChannelRouting(): ChannelRoutingConfig {
  try {
    const saved = localStorage.getItem(CHANNEL_ROUTING_KEY);
    if (saved) return JSON.parse(saved);
  } catch { /* ignore */ }
  return { telegram: 'chat', discord: 'chat' };
}

/** Save channel routing config to localStorage. */
export function saveChannelRouting(config: ChannelRoutingConfig): void {
  try {
    localStorage.setItem(CHANNEL_ROUTING_KEY, JSON.stringify(config));
  } catch { /* non-fatal */ }
}

// Utility
// ============================================

export function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
}

export function formatVram(mb: number): string {
  if (mb >= 1024) {
    return `${(mb / 1024).toFixed(1)} GB`;
  }
  return `${mb} MB`;
}


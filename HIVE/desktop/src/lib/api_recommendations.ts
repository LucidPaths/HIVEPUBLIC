// Recommendations API — model recommendations, speed tier, smart router
// Extracted from api.ts for modularity (P1)

import type { HfModel, HfModelFile, VramCompatibility, ProviderType, ProviderStatus, LocalModel } from './api';

// ============================================
// Model Recommendation Engine (100% local)
// ============================================

/** RAM reserved for OS/system processes — not available for model inference */
const OS_RAM_RESERVE_GB = 4;

/** Which recommendation category a model falls into */
export type RecCategory = 'fast' | 'quality' | 'brain';

/** A real recommended model with actual compatibility data from HuggingFace */
export interface RecommendedModel {
  model: HfModel;
  bestFile: HfModelFile;    // best compatible GGUF file for this hardware
  speedTier: SpeedTier;     // computed speed tier for that file
  vramNeeded: number;       // GB needed
  qualityScore?: number;    // Open LLM Leaderboard average (if available)
  category: RecCategory;    // which recommendation section it belongs to
}

/**
 * Speed tier for individual files — the noob-friendly layer on top of VramCompatibility.
 * Noob sees: icon + label. Power user sees: detail (on hover).
 */
export interface SpeedTier {
  tier: 'fast' | 'good' | 'slow' | 'too_large';
  icon: string;
  label: string;           // "Fast", "Runs well", "Slower (uses RAM)", "Too large"
  detail: string;          // technical hover text
  color: { bg: string; text: string; border: string };
}

// ============================================
// Scoring Engine — adapted from llmfit (MIT)
// https://github.com/AlexsJones/llmfit
// ============================================

/** Quantization bytes-per-parameter (includes quantization metadata overhead) */
export function quantBpp(quant: string): number {
  const q = quant.toUpperCase();
  if (q.includes('F32')) return 4.0;
  if (q.includes('F16') || q.includes('BF16')) return 2.0;
  if (q.includes('Q8')) return 1.05;
  if (q.includes('Q6')) return 0.80;
  if (q.includes('Q5')) return 0.68;
  if (q.includes('Q4') || q.includes('IQ4')) return 0.58;
  if (q.includes('Q3') || q.includes('IQ3')) return 0.48;
  if (q.includes('Q2') || q.includes('IQ2')) return 0.37;
  if (q.includes('IQ1')) return 0.22;
  return 0.58; // default Q4_K_M
}

/** Speed multiplier per quant (lower quant = faster inference, less data to move) */
function quantSpeedMult(quant: string): number {
  const q = quant.toUpperCase();
  if (q.includes('F16') || q.includes('BF16')) return 0.6;
  if (q.includes('Q8')) return 0.8;
  if (q.includes('Q6')) return 0.95;
  if (q.includes('Q5')) return 1.0;  // baseline
  if (q.includes('Q4') || q.includes('IQ4')) return 1.15;
  if (q.includes('Q3') || q.includes('IQ3')) return 1.25;
  if (q.includes('Q2') || q.includes('IQ2')) return 1.35;
  if (q.includes('IQ1')) return 1.5;
  return 1.0;
}

/** Quality penalty from quantization (0 = no loss, negative = quality degradation) */
export function quantQualityPenalty(quant: string): number {
  const q = quant.toUpperCase();
  if (q.includes('F16') || q.includes('BF16') || q.includes('Q8')) return 0;
  if (q.includes('Q6')) return -1;
  if (q.includes('Q5')) return -2;
  if (q.includes('Q4') || q.includes('IQ4')) return -5;
  if (q.includes('Q3') || q.includes('IQ3')) return -8;
  if (q.includes('Q2') || q.includes('IQ2')) return -12;
  if (q.includes('IQ1')) return -18;
  return -5;
}

/** GPU backend type for speed estimation */
export type GpuBackend = 'cuda' | 'rocm' | 'vulkan' | 'cpu';

/** Run mode for local inference */
export type RunMode = 'gpu' | 'moe_offload' | 'cpu_offload' | 'cpu_only';

/** Backend speed constant K (higher = faster hardware) */
function backendK(backend: GpuBackend): number {
  switch (backend) {
    case 'cuda': return 220;
    case 'rocm': return 180;
    case 'vulkan': return 150;
    case 'cpu': return 70;
  }
}

/**
 * Estimate tokens per second for a model on given hardware.
 * Formula: TPS = (K / params_B) * quant_speed_mult * [1.1 if cores >= 8] * run_mode_penalty
 * Adapted from llmfit (MIT)
 */
export function estimateTps(
  paramsB: number,
  quant: string,
  backend: GpuBackend,
  runMode: RunMode,
  cpuCores: number = 4,
): number {
  const k = backendK(backend);
  const params = Math.max(paramsB, 0.1);
  let tps = (k / params) * quantSpeedMult(quant);
  if (cpuCores >= 8) tps *= 1.1;
  switch (runMode) {
    case 'gpu': break;
    case 'moe_offload': tps *= 0.8; break;
    case 'cpu_offload': tps *= 0.5; break;
    case 'cpu_only': tps *= 0.3; break;
  }
  return Math.max(tps, 0.1);
}

/**
 * Memory fit score (0–100). Sweet spot is 50–80% utilization.
 * Adapted from llmfit (MIT)
 */
export function fitScore(requiredGb: number, availableGb: number): number {
  if (availableGb <= 0 || requiredGb > availableGb) return 0;
  const ratio = requiredGb / availableGb;
  if (ratio <= 0.5) return 60 + (ratio / 0.5) * 40;
  if (ratio <= 0.8) return 100;
  if (ratio <= 0.9) return 70;
  return 50;
}

/** Quantization walk-down hierarchy (best quality first) */
const QUANT_HIERARCHY = ['Q8_0', 'Q6_K', 'Q5_K_M', 'Q4_K_M', 'Q3_K_M', 'Q2_K'] as const;

/**
 * Find the best quantization that fits within a VRAM budget.
 * Walks down from Q8_0 to Q2_K. As last resort, tries halving context (min 1024).
 * Returns best fit or null if nothing works.
 * Adapted from llmfit (MIT)
 */
export function bestQuantForBudget(
  paramsB: number,
  budgetGb: number,
  contextLength: number = 4096,
): { quant: string; estimatedGb: number; contextLength: number } | null {
  const estimate = (q: string, ctx: number) => {
    const bpp = quantBpp(q);
    const modelMem = paramsB * bpp;
    const kvCache = 0.000008 * paramsB * ctx;
    const overhead = 0.5;
    return modelMem + kvCache + overhead;
  };

  // Try each quant at full context
  for (const q of QUANT_HIERARCHY) {
    const mem = estimate(q, contextLength);
    if (mem <= budgetGb) {
      return { quant: q, estimatedGb: Math.round(mem * 10) / 10, contextLength };
    }
  }

  // Try halving context (minimum 1024)
  const halfCtx = Math.max(Math.floor(contextLength / 2), 1024);
  if (halfCtx < contextLength) {
    for (const q of QUANT_HIERARCHY) {
      const mem = estimate(q, halfCtx);
      if (mem <= budgetGb) {
        return { quant: q, estimatedGb: Math.round(mem * 10) / 10, contextLength: halfCtx };
      }
    }
  }

  return null;
}

/** Infer GPU backend from vendor string */
export function vendorToBackend(vendor: string): GpuBackend {
  const v = vendor.toLowerCase();
  if (v.includes('nvidia') || v.includes('geforce') || v.includes('rtx') || v.includes('gtx')) return 'cuda';
  if (v.includes('amd') || v.includes('radeon')) return 'rocm';
  if (v.includes('intel') || v.includes('arc')) return 'vulkan';
  return 'cpu';
}

/** Reverse-engineer approximate params (B) from model weights and quantization */
export function estimateParamsB(modelWeightsGb: number, quant: string): number {
  const bpp = quantBpp(quant);
  return bpp > 0 ? modelWeightsGb / bpp : 7; // fallback 7B
}

/**
 * Compute speed tier for a file given VRAM compatibility + system RAM.
 * This is the two-layer display: noob sees icon+label, power user sees detail on hover.
 *
 * MoE-aware: if the model is MoE and active-expert VRAM fits, it gets a better tier.
 * TPS-aware: if gpuVendor is provided, includes estimated tokens/second in the detail.
 */
export function getSpeedTier(
  compat: VramCompatibility,
  ramGb: number,
  gpuVendor?: string,
  cpuCores?: number,
): SpeedTier {
  const est = compat.estimate;
  let needed = est.total_gb;
  const vram = compat.available_vram_gb;
  const totalAvailable = vram + Math.max(0, ramGb - OS_RAM_RESERVE_GB);

  // MoE: if active-expert VRAM fits in GPU, treat as GPU-runnable
  const isMoeOffload = est.is_moe && est.moe_active_gb != null && est.moe_active_gb <= vram && needed > vram;
  if (isMoeOffload) {
    needed = est.moe_active_gb!;
  }

  // Optional TPS estimation for detail text
  let tpsSuffix = '';
  if (gpuVendor) {
    const paramsB = estimateParamsB(est.model_weights_gb, est.quantization);
    const backend = vendorToBackend(gpuVendor);
    const runMode: RunMode = isMoeOffload ? 'moe_offload'
      : needed <= vram ? 'gpu'
      : needed <= totalAvailable ? 'cpu_offload' : 'cpu_only';
    const tps = estimateTps(paramsB, est.quantization, backend, runMode, cpuCores);
    tpsSuffix = ` (~${Math.round(tps)} tok/s)`;
  }

  // MoE offload fits in GPU VRAM — fast with expert switching overhead
  if (isMoeOffload) {
    const moeGb = est.moe_active_gb!;
    const offloadGb = est.total_gb - moeGb;
    return {
      tier: 'fast',
      icon: '\u26A1',
      label: 'Fast (MoE)',
      detail: `MoE: ${moeGb.toFixed(1)} GB active experts in GPU, ${offloadGb.toFixed(1)} GB inactive in RAM${tpsSuffix}`,
      color: { bg: 'bg-green-500/20', text: 'text-green-400', border: 'border-green-500/50' },
    };
  }

  if (compat.status === 'good') {
    const pct = ((vram - needed) / vram * 100).toFixed(0);
    return {
      tier: 'fast',
      icon: '\u26A1',
      label: 'Fast',
      detail: `${needed.toFixed(1)} GB — fits in GPU with ${compat.headroom_gb.toFixed(1)} GB to spare (${pct}% free)${tpsSuffix}`,
      color: { bg: 'bg-green-500/20', text: 'text-green-400', border: 'border-green-500/50' },
    };
  }

  if (compat.status === 'tight') {
    return {
      tier: 'good',
      icon: '\u2705',
      label: 'Runs well',
      detail: `${needed.toFixed(1)} GB — fits in GPU, ${compat.headroom_gb.toFixed(1)} GB headroom (may need lower context)${tpsSuffix}`,
      color: { bg: 'bg-yellow-500/20', text: 'text-yellow-400', border: 'border-yellow-500/50' },
    };
  }

  // Status is 'insufficient' for GPU-only — but can it fit with RAM?
  if (needed <= totalAvailable) {
    const onGpu = vram;
    const onRam = needed - vram;
    const gpuPct = Math.round((onGpu / needed) * 100);
    return {
      tier: 'slow',
      icon: '\uD83D\uDC22',
      label: 'Slower (uses RAM)',
      detail: `${needed.toFixed(1)} GB needed — ~${gpuPct}% on GPU, ~${onRam.toFixed(1)} GB offloaded to RAM${tpsSuffix}`,
      color: { bg: 'bg-blue-500/20', text: 'text-blue-400', border: 'border-blue-500/50' },
    };
  }

  // Doesn't fit even with RAM
  return {
    tier: 'too_large',
    icon: '\u274C',
    label: 'Too large',
    detail: `${needed.toFixed(1)} GB needed — exceeds GPU (${vram.toFixed(0)} GB) + RAM (${ramGb.toFixed(0)} GB) combined`,
    color: { bg: 'bg-red-500/20', text: 'text-red-400', border: 'border-red-500/50' },
  };
}

/**
 * Build real recommendations from actual HuggingFace model data.
 *
 * Categories based on GPU utilization (not headroom):
 * - "fast": file uses ≤75% of GPU VRAM — comfortable headroom, full speed
 * - "quality": file uses 75-100% of GPU VRAM — pushes GPU harder, better quant/bigger model
 * - "brain": file exceeds GPU, needs RAM offload — biggest/smartest, but slower
 *
 * Within each category: sorted by benchmark score → file size → downloads.
 * A model CAN appear in multiple categories with different files (e.g. Q4 = fast, Q8 = quality).
 * All computation is local — nothing sent externally.
 */
export function buildRecommendations(
  models: HfModel[],
  filesByModel: Record<string, HfModelFile[]>,
  compatByModel: Record<string, Record<string, VramCompatibility>>,
  ramGb: number,
): RecommendedModel[] {
  const results: RecommendedModel[] = [];

  for (const model of models) {
    const files = filesByModel[model.id];
    const compat = compatByModel[model.id];
    if (!files || !compat) continue;

    // Find best file for each category (largest file within each GPU utilization band)
    let fastPick: { file: HfModelFile; tier: SpeedTier; vram: number } | null = null;
    let qualityPick: { file: HfModelFile; tier: SpeedTier; vram: number } | null = null;
    let brainPick: { file: HfModelFile; tier: SpeedTier; vram: number } | null = null;

    for (const file of files) {
      const c = compat[file.filename];
      if (!c) continue;
      const tier = getSpeedTier(c, ramGb);
      if (tier.tier === 'too_large') continue;

      const utilization = c.estimate.total_gb / c.available_vram_gb;

      if (tier.tier === 'slow') {
        // Needs RAM offload → brain category
        if (!brainPick || file.size > brainPick.file.size) {
          brainPick = { file, tier, vram: c.estimate.total_gb };
        }
      } else if (utilization > 0.75) {
        // Fits in GPU but uses >75% → quality category (higher quant, better output)
        if (!qualityPick || file.size > qualityPick.file.size) {
          qualityPick = { file, tier, vram: c.estimate.total_gb };
        }
      } else {
        // Comfortable GPU fit (≤75%) → fast category
        if (!fastPick || file.size > fastPick.file.size) {
          fastPick = { file, tier, vram: c.estimate.total_gb };
        }
      }
    }

    // Add picks to results (a model can appear in multiple categories)
    if (fastPick) {
      results.push({
        model, bestFile: fastPick.file, speedTier: fastPick.tier,
        vramNeeded: fastPick.vram, qualityScore: model.qualityScore,
        category: 'fast',
      });
    }
    if (qualityPick) {
      results.push({
        model, bestFile: qualityPick.file, speedTier: qualityPick.tier,
        vramNeeded: qualityPick.vram, qualityScore: model.qualityScore,
        category: 'quality',
      });
    }
    if (brainPick) {
      results.push({
        model, bestFile: brainPick.file, speedTier: brainPick.tier,
        vramNeeded: brainPick.vram, qualityScore: model.qualityScore,
        category: 'brain',
      });
    }
  }

  // Sort within each category: composite score (quality + fit + quant penalty) → downloads
  const catOrder: Record<string, number> = { fast: 0, quality: 1, brain: 2 };
  results.sort((a, b) => {
    if (a.category !== b.category) return (catOrder[a.category] ?? 9) - (catOrder[b.category] ?? 9);
    // Composite: benchmark quality + fit utilization + quant quality penalty
    const compA = (a.qualityScore ?? 50)
      + fitScore(a.vramNeeded, a.speedTier.tier === 'slow' ? Infinity : compatByModel[a.model.id]?.[a.bestFile.filename]?.available_vram_gb ?? 8)
      + quantQualityPenalty(compatByModel[a.model.id]?.[a.bestFile.filename]?.estimate?.quantization ?? 'Q4_K_M');
    const compB = (b.qualityScore ?? 50)
      + fitScore(b.vramNeeded, b.speedTier.tier === 'slow' ? Infinity : compatByModel[b.model.id]?.[b.bestFile.filename]?.available_vram_gb ?? 8)
      + quantQualityPenalty(compatByModel[b.model.id]?.[b.bestFile.filename]?.estimate?.quantization ?? 'Q4_K_M');
    if (compA !== compB) return compB - compA;
    return b.model.downloads - a.model.downloads;
  });

  return results;
}

// ============================================
// Smart Model Router (Phase 4 — The Brain)
// ============================================

/** Task categories that HIVE can route to the best model */
export type TaskCategory = 'general' | 'coding' | 'reasoning' | 'writing' | 'tool_calling' | 'web' | 'creative';

/** A model available for routing (local or cloud) */
export interface RoutableModel {
  provider: ProviderType;
  modelId: string;
  name: string;
  contextLength: number | null;
  /** Strength scores per task category (0–100, higher = better) */
  strengths: Partial<Record<TaskCategory, number>>;
  /** Whether the model is currently loaded/ready (no startup delay) */
  ready: boolean;
}

/** The router's pick for a task category */
export interface RouteChoice {
  category: TaskCategory;
  model: RoutableModel;
  score: number;
  reason: string;
}

/** Known cloud model strengths — hardcoded benchmarks for popular models.
 *  This is a bootstrap until HIVE learns from usage patterns (Phase 5).
 *  Scores are 0–100, roughly calibrated to public benchmarks. */
const KNOWN_MODEL_STRENGTHS: Record<string, Partial<Record<TaskCategory, number>>> = {
  // OpenAI
  'gpt-4o':           { general: 88, coding: 90, reasoning: 88, writing: 85, tool_calling: 95, web: 85, creative: 82 },
  'gpt-4o-mini':      { general: 78, coding: 80, reasoning: 75, writing: 78, tool_calling: 88, web: 78, creative: 75 },
  'gpt-4-turbo':      { general: 86, coding: 88, reasoning: 86, writing: 84, tool_calling: 92, web: 84, creative: 80 },
  'o1':               { general: 90, coding: 92, reasoning: 95, writing: 82, tool_calling: 75, web: 80, creative: 78 },
  'o1-mini':          { general: 82, coding: 88, reasoning: 90, writing: 75, tool_calling: 70, web: 75, creative: 72 },
  'o3-mini':          { general: 85, coding: 90, reasoning: 92, writing: 78, tool_calling: 72, web: 78, creative: 75 },
  // Anthropic
  'claude-sonnet-4-20250514':   { general: 90, coding: 93, reasoning: 90, writing: 92, tool_calling: 92, web: 88, creative: 90 },
  'claude-opus-4-20250514':     { general: 92, coding: 95, reasoning: 93, writing: 94, tool_calling: 90, web: 90, creative: 92 },
  'claude-3-5-haiku-20241022':  { general: 80, coding: 82, reasoning: 78, writing: 82, tool_calling: 85, web: 78, creative: 78 },
  // OpenRouter popular models (approximate)
  'meta-llama/llama-3.1-405b-instruct':  { general: 88, coding: 85, reasoning: 86, writing: 85, tool_calling: 80, web: 82, creative: 83 },
  'meta-llama/llama-3.1-70b-instruct':   { general: 82, coding: 80, reasoning: 78, writing: 80, tool_calling: 75, web: 78, creative: 78 },
  'google/gemini-pro-1.5':               { general: 86, coding: 85, reasoning: 85, writing: 84, tool_calling: 88, web: 88, creative: 80 },
  'mistralai/mistral-large':             { general: 84, coding: 82, reasoning: 82, writing: 83, tool_calling: 80, web: 80, creative: 80 },
  'deepseek/deepseek-r1':                { general: 85, coding: 90, reasoning: 92, writing: 78, tool_calling: 72, web: 75, creative: 72 },
  'deepseek/deepseek-chat':              { general: 82, coding: 88, reasoning: 85, writing: 80, tool_calling: 78, web: 78, creative: 75 },
  'qwen/qwen-2.5-72b-instruct':         { general: 82, coding: 85, reasoning: 80, writing: 80, tool_calling: 78, web: 78, creative: 78 },
};

/**
 * Build a list of all models currently available for routing.
 * Combines loaded local models + connected cloud provider models.
 */
export function getRoutableModels(
  providerStatuses: Record<string, ProviderStatus>,
  selectedModel: LocalModel | null,
  serverRunning: boolean,
): RoutableModel[] {
  const models: RoutableModel[] = [];

  // Local model (only if loaded and running)
  if (selectedModel && serverRunning) {
    models.push({
      provider: 'local',
      modelId: selectedModel.filename,
      name: selectedModel.filename.replace('.gguf', ''),
      contextLength: selectedModel.context_length ? Number(selectedModel.context_length) : null,
      strengths: { general: 60 }, // local models: decent default, no benchmarks available at runtime
      ready: true,
    });
  }

  // Cloud provider models
  for (const [providerType, status] of Object.entries(providerStatuses)) {
    if (!status?.connected || !status.models) continue;

    for (const model of status.models) {
      const knownStrengths = KNOWN_MODEL_STRENGTHS[model.id];
      models.push({
        provider: providerType as ProviderType,
        modelId: model.id,
        name: model.name,
        contextLength: model.context_length ? Number(model.context_length) : null,
        strengths: knownStrengths ?? { general: 70 }, // unknown cloud model: assume decent
        ready: true,
      });
    }
  }

  return models;
}

/**
 * Smart route: pick the best available model for a given task category.
 * Returns ranked choices for the category.
 *
 * Scoring: strength for the category (or general fallback) + ready bonus.
 * Ties broken by context length (longer = better for complex tasks).
 */
export function smartRoute(
  category: TaskCategory,
  models: RoutableModel[],
): RouteChoice[] {
  if (models.length === 0) return [];

  return models
    .map(model => {
      const categoryScore = model.strengths[category] ?? model.strengths.general ?? 50;
      const readyBonus = model.ready ? 5 : 0;
      const score = categoryScore + readyBonus;
      const reason = model.strengths[category]
        ? `${model.name} scores ${categoryScore} for ${category}`
        : `${model.name} (general: ${model.strengths.general ?? 50}, no ${category} benchmark)`;

      return { category, model, score, reason };
    })
    .sort((a, b) => {
      if (b.score !== a.score) return b.score - a.score;
      // Tie-break: prefer longer context
      return (b.model.contextLength ?? 0) - (a.model.contextLength ?? 0);
    });
}

/**
 * Get the best model for each task category.
 * This is the "routing table" that HIVE uses to decide which model handles what.
 */
export function getRoutingTable(models: RoutableModel[]): Record<TaskCategory, RouteChoice | null> {
  const categories: TaskCategory[] = ['general', 'coding', 'reasoning', 'writing', 'tool_calling', 'web', 'creative'];
  const table: Record<string, RouteChoice | null> = {};

  for (const cat of categories) {
    const choices = smartRoute(cat, models);
    table[cat] = choices[0] ?? null;
  }

  return table as Record<TaskCategory, RouteChoice | null>;
}

/** Map the orchestrator's slot-based routing to task categories */
export function slotRoleToTaskCategory(role: string): TaskCategory {
  switch (role) {
    case 'coder': return 'coding';
    case 'terminal': return 'tool_calling';
    case 'webcrawl': return 'web';
    case 'toolcall': return 'tool_calling';
    case 'consciousness': return 'general';
    default: return 'general';
  }
}

/** Sort files by speed tier — fast first, then good, slow, too_large */
export function sortFilesByCompatibility(
  files: HfModelFile[],
  compatibility: Record<string, VramCompatibility>,
  ramGb: number = 0,
): HfModelFile[] {
  const tierOrder: Record<string, number> = { fast: 0, good: 1, slow: 2, too_large: 3 };
  return [...files].sort((a, b) => {
    const ca = compatibility[a.filename];
    const cb = compatibility[b.filename];
    const ta = ca ? getSpeedTier(ca, ramGb).tier : 'too_large';
    const tb = cb ? getSpeedTier(cb, ramGb).tier : 'too_large';
    const oa = tierOrder[ta] ?? 4;
    const ob = tierOrder[tb] ?? 4;
    if (oa !== ob) return oa - ob;
    return a.size - b.size;
  });
}


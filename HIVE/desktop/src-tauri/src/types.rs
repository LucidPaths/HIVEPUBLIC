//! Shared types for HIVE Desktop
//!
//! All serializable data structures used across modules.

use serde::{Deserialize, Serialize};

// ============================================
// Hardware Types
// ============================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    pub vendor: String,        // "NVIDIA", "AMD", "Intel", "Unknown"
    pub name: String,          // "NVIDIA GeForce RTX 4090"
    pub vram_mb: u64,          // VRAM in MB
    pub driver_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuInfo {
    pub name: String,          // "AMD Ryzen 9 5900X"
    pub cores: u32,            // Physical cores
    pub threads: u32,          // Logical processors
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RamInfo {
    pub total_mb: u64,         // Total RAM in MB
    pub total_gb: f64,         // Total RAM in GB (for display)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub gpus: Vec<GpuInfo>,
    pub cpu: Option<CpuInfo>,
    pub ram: Option<RamInfo>,
    pub wsl_available: bool,
    pub wsl_distro: Option<String>,
    pub recommended_backend: String, // "windows", "wsl"
}

// ============================================
// WSL Types
// ============================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WslStatus {
    pub installed: bool,
    pub distro: Option<String>,
    pub llama_server_path: Option<String>,
    pub rocm_version: Option<String>,
    pub cuda_version: Option<String>,
}

// ============================================
// Model Types
// ============================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub filename: String,
    pub size_bytes: u64,
    pub size_gb: f64,
    pub path: String,
    pub context_length: Option<u64>,   // Max context window from GGUF metadata
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerStatus {
    pub running: bool,
    pub port: u16,
    pub backend: String, // "windows" or "wsl"
    pub model_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyStatus {
    // Windows/NVIDIA dependencies
    pub windows_llama_server: Option<String>,  // Path if found, None if missing
    pub cuda_available: bool,

    // WSL/AMD dependencies
    pub wsl_installed: bool,
    pub wsl_distro: Option<String>,
    pub wsl_llama_server: Option<String>,      // Path if found, None if missing
    pub rocm_available: bool,
    pub rocm_version: Option<String>,

    // What's needed based on detected hardware
    pub recommended_backend: String,           // "windows" or "wsl"
    pub ready_to_run: bool,                    // True if all deps for recommended backend are met
    pub missing_deps: Vec<String>,             // List of what's missing
}

// ============================================
// GGUF Metadata Types (for VRAM calculation)
// ============================================

/// Metadata extracted from a GGUF file header
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GgufMetadata {
    pub architecture: Option<String>,      // e.g., "llama", "qwen", "phi"
    pub name: Option<String>,              // Human-readable model name
    pub parameter_count: Option<u64>,      // Total parameters (from metadata or estimated)
    pub quantization: Option<String>,      // e.g., "Q4_K_M", "Q5_K_M", "F16"
    pub file_type: Option<u32>,            // GGUF file_type enum value
    pub context_length: Option<u64>,       // Maximum context window
    pub embedding_length: Option<u64>,     // Hidden dimension size
    pub block_count: Option<u64>,          // Number of transformer layers
    pub head_count: Option<u64>,           // Number of attention heads
    pub head_count_kv: Option<u64>,        // Number of KV heads (for GQA)
    pub expert_count: Option<u64>,         // MoE: total experts (e.g., 8 for Mixtral 8x7B)
    pub expert_used_count: Option<u64>,    // MoE: active experts per token (e.g., 2 for Mixtral)
    pub file_size_bytes: u64,              // Actual file size
}

/// VRAM estimate breakdown
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VramEstimate {
    pub model_weights_gb: f64,             // Memory for model weights
    pub kv_cache_gb: f64,                  // Memory for KV cache at given context
    pub overhead_gb: f64,                  // CUDA/ROCm overhead + scratch space
    pub total_gb: f64,                     // Total estimated VRAM
    pub context_length: u64,               // Context length used in calculation
    pub quantization: String,              // Quantization type
    pub confidence: String,                // "high" (from metadata), "medium" (estimated), "low" (fallback)
    pub kv_offload: bool,                  // Whether KV cache is excluded (offloaded to RAM)
    pub is_moe: bool,                      // Whether this is a Mixture-of-Experts model
    pub moe_active_gb: Option<f64>,        // MoE: VRAM for active experts only (with expert offload)
}

/// VRAM compatibility status for UI badges
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VramCompatibility {
    pub estimate: VramEstimate,
    pub available_vram_gb: f64,            // User's GPU VRAM
    pub status: String,                    // "good" (green), "tight" (yellow), "insufficient" (red)
    pub headroom_gb: f64,                  // Available VRAM - estimated usage
}

// ============================================
// Provider Types
// ============================================

/// Provider type for model inference
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ProviderType {
    Local,       // llama.cpp (local GGUF models)
    Ollama,      // Ollama server
    OpenAI,      // OpenAI API
    Anthropic,   // Anthropic/Claude API
    OpenRouter,  // OpenRouter API (100+ models, OpenAI-compatible)
    DashScope,   // Alibaba DashScope API (OpenAI-compatible, kimi-k2.5 etc.)
}

/// Provider configuration (without secrets - those are in keyring)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub provider_type: ProviderType,
    pub name: String,               // Display name
    pub endpoint: Option<String>,   // Custom endpoint URL (optional for cloud providers)
    pub enabled: bool,
    pub has_api_key: bool,          // Whether API key is configured (don't expose actual key!)
}

/// Available model from a provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderModel {
    pub id: String,                 // Model identifier (e.g., "gpt-4", "claude-3-opus")
    pub name: String,               // Display name
    pub provider: ProviderType,
    pub context_length: Option<u64>,
    pub description: Option<String>,
}

/// Result of checking provider status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderStatus {
    pub provider_type: ProviderType,
    pub configured: bool,           // Has API key
    pub connected: bool,            // Successfully connected
    pub error: Option<String>,      // Error message if any
    pub models: Vec<ProviderModel>, // Available models
}

/// Encrypted hardware fingerprint (stored locally, never transmitted)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedHardwareData {
    pub encrypted_data: String,     // AES-256-GCM encrypted JSON
    pub created_at: u64,            // Unix timestamp
}

// ============================================
// Remote Channel Types
// ============================================

/// Role assigned to an incoming message based on sender identity.
/// Host = full desktop-equivalent permissions, User = restricted (no dangerous tools).
/// Used by both telegram_daemon and discord_daemon — defined here as single source of truth (P5).
/// Serializes as "host"/"user" to match TypeScript SenderRole type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SenderRole {
    Host,
    User,
}

/// Provider-agnostic thinking depth control (P2: Provider Agnosticism).
/// Maps to native parameters per provider — see providers.rs::inject_thinking_params().
/// TypeScript mirror: ThinkingDepth in types.ts — must stay in sync (P5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThinkingDepth {
    Off,
    Low,
    Medium,
    High,
}

/// Download progress event payload
#[derive(Debug, Clone, Serialize)]
pub struct DownloadProgress {
    pub downloaded: u64,
    pub total: u64,
    pub percentage: f64,
    pub filename: String,
}

/// Live resource usage metrics — polled per chat turn for situational awareness.
/// Enables the model to know its actual remaining capacity for routing decisions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveResourceMetrics {
    /// GPU VRAM currently in use (MB). None if no GPU or query failed.
    pub vram_used_mb: Option<u64>,
    /// GPU VRAM currently free (MB). None if no GPU or query failed.
    pub vram_free_mb: Option<u64>,
    /// GPU VRAM total (MB). Redundant with GpuInfo but kept for self-containment.
    pub vram_total_mb: Option<u64>,
    /// System RAM currently available/free (MB). None if query failed.
    pub ram_available_mb: Option<u64>,
    /// System RAM currently in use (MB). None if query failed.
    pub ram_used_mb: Option<u64>,
    /// GPU utilization percentage (0-100). None if unavailable.
    pub gpu_utilization: Option<u32>,
    /// GPU vendor that metrics came from ("nvidia", "amd", or "none")
    pub gpu_vendor: String,
}

//! HIVE Slot System — role-based model assignment (Phase 4)
//!
//! Slots define ROLES, not models. Any compatible model fills any slot.
//! This is the "any model fills any slot" promise from ARCHITECTURE_PRINCIPLES.md.
//!
//! Principle Lattice alignment:
//!   P1 (Bridges)    — Slots bridge task types to provider/model combinations
//!   P2 (Agnostic)   — Any provider (local, cloud, Ollama) can fill any slot
//!   P7 (Survives)   — Slot definitions are stable; models/providers evolve
//!   P8 (Low/High)   — Default slots work out-of-box; power users customize

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

// ============================================
// Slot Roles
// ============================================

/// The five core roles in HIVE's cognitive architecture.
/// From ARCHITECTURE_PRINCIPLES.md and HOT_SWAP_MECHANICS.md.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotRole {
    /// Executive function: routing, planning, conversation.
    /// Always active (smallest model, lowest VRAM).
    Consciousness,
    /// Code generation, debugging, architecture.
    Coder,
    /// Safe command execution, file operations.
    Terminal,
    /// Web scraping, research, summarization.
    WebCrawl,
    /// API interaction, tool selection, function calling.
    ToolCall,
}

impl SlotRole {
    pub fn all() -> &'static [SlotRole] {
        &[
            SlotRole::Consciousness,
            SlotRole::Coder,
            SlotRole::Terminal,
            SlotRole::WebCrawl,
            SlotRole::ToolCall,
        ]
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            SlotRole::Consciousness => "Consciousness",
            SlotRole::Coder => "Coder",
            SlotRole::Terminal => "Terminal",
            SlotRole::WebCrawl => "WebCrawl",
            SlotRole::ToolCall => "ToolCall",
        }
    }

    pub fn is_always_loaded(&self) -> bool {
        matches!(self, SlotRole::Consciousness)
    }
}

impl std::fmt::Display for SlotRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

// ============================================
// Slot Configuration
// ============================================

/// A specific model assignment for a slot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlotAssignment {
    pub provider: String,     // "local", "ollama", "openai", "anthropic"
    pub model: String,        // model filename or API model ID
    pub vram_gb: f64,         // estimated VRAM cost (0 for cloud)
    pub context_length: u32,  // context window size
}

/// Configuration for a slot: primary assignment + fallback chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlotConfig {
    pub role: SlotRole,
    pub primary: Option<SlotAssignment>,
    pub fallbacks: Vec<SlotAssignment>,
    pub enabled: bool,
}

impl SlotConfig {
    /// Get the best available assignment (primary → fallbacks in order).
    pub fn best_assignment(&self) -> Option<&SlotAssignment> {
        self.primary.as_ref().or_else(|| self.fallbacks.first())
    }

    /// Is this a cloud-only slot (0 VRAM)?
    #[allow(dead_code)] // Phase 4: used in VRAM planning once full slot lifecycle is connected
    pub fn is_cloud(&self) -> bool {
        self.best_assignment().map(|a| a.vram_gb < 0.001).unwrap_or(false)
    }
}

// ============================================
// Slot Runtime State
// ============================================

/// Current state of a slot at runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotStatus {
    /// Not loaded, no resources consumed.
    Idle,
    /// Model is being loaded into VRAM.
    Loading,
    /// Model loaded and ready for inference.
    Active,
    /// Model being unloaded, state being extracted.
    Sleeping,
}

/// Runtime state of a single slot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlotState {
    pub role: SlotRole,
    pub status: SlotStatus,
    pub assignment: Option<SlotAssignment>, // what's currently loaded
    pub server_port: Option<u16>,           // for local models: which port
    pub loaded_at: Option<String>,          // ISO8601 when loaded
    pub last_active: Option<String>,        // ISO8601 last inference
    pub vram_used_gb: f64,                  // actual VRAM consumed
}

impl SlotState {
    pub fn idle(role: SlotRole) -> Self {
        Self {
            role,
            status: SlotStatus::Idle,
            assignment: None,
            server_port: None,
            loaded_at: None,
            last_active: None,
            vram_used_gb: 0.0,
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(self.status, SlotStatus::Active)
    }

    #[allow(dead_code)] // Phase 4: symmetric to is_active(); used when full sleep/wake guards are wired
    pub fn is_idle(&self) -> bool {
        matches!(self.status, SlotStatus::Idle)
    }
}

// ============================================
// VRAM Budget
// ============================================

/// Real-time VRAM budget tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VramBudget {
    pub total_gb: f64,
    pub used_gb: f64,
    pub safety_buffer_gb: f64, // always keep this much free (default 2.0)
}

impl VramBudget {
    pub fn new(total_gb: f64) -> Self {
        Self {
            total_gb,
            used_gb: 0.0,
            safety_buffer_gb: 2.0,
        }
    }

    /// Available VRAM for loading a new model.
    pub fn available_gb(&self) -> f64 {
        (self.total_gb - self.used_gb - self.safety_buffer_gb).max(0.0)
    }

    /// Can we fit a model of this size?
    pub fn can_fit(&self, vram_gb: f64) -> bool {
        vram_gb <= self.available_gb()
    }

    /// What must be unloaded to fit this model?
    /// Returns required VRAM to free (0 if it already fits).
    pub fn deficit(&self, vram_gb: f64) -> f64 {
        (vram_gb - self.available_gb()).max(0.0)
    }
}

// ============================================
// Tauri-Managed State
// ============================================

/// Tauri-managed slot state — holds all slot configs + runtime states.
pub struct SlotsState {
    pub configs: Mutex<HashMap<SlotRole, SlotConfig>>,
    pub states: Mutex<HashMap<SlotRole, SlotState>>,
    pub vram_budget: Mutex<VramBudget>,
}

impl Default for SlotsState {
    fn default() -> Self {
        let mut configs = HashMap::new();
        let mut states = HashMap::new();

        // Initialize all slots as idle with no assignment
        for &role in SlotRole::all() {
            configs.insert(role, SlotConfig {
                role,
                primary: None,
                fallbacks: vec![],
                enabled: role == SlotRole::Consciousness, // only consciousness enabled by default
            });
            states.insert(role, SlotState::idle(role));
        }

        Self {
            configs: Mutex::new(configs),
            states: Mutex::new(states),
            vram_budget: Mutex::new(VramBudget::new(0.0)), // set on hardware detection
        }
    }
}

// ============================================
// Tauri Commands
// ============================================

/// Get all slot configurations.
#[tauri::command]
pub fn get_slot_configs(
    state: tauri::State<'_, SlotsState>,
) -> Result<Vec<SlotConfig>, String> {
    let configs = state.configs.lock().map_err(|e| format!("Lock error: {}", e))?;
    Ok(configs.values().cloned().collect())
}

/// Get all slot runtime states.
#[tauri::command]
pub fn get_slot_states(
    state: tauri::State<'_, SlotsState>,
) -> Result<Vec<SlotState>, String> {
    let states = state.states.lock().map_err(|e| format!("Lock error: {}", e))?;
    Ok(states.values().cloned().collect())
}

/// Configure a slot's model assignment.
#[tauri::command]
pub fn configure_slot(
    state: tauri::State<'_, SlotsState>,
    role: SlotRole,
    provider: String,
    model: String,
    vram_gb: f64,
    context_length: u32,
) -> Result<SlotConfig, String> {
    let mut configs = state.configs.lock().map_err(|e| format!("Lock error: {}", e))?;

    let config = configs.entry(role).or_insert(SlotConfig {
        role,
        primary: None,
        fallbacks: vec![],
        enabled: true,
    });

    config.primary = Some(SlotAssignment {
        provider: provider.clone(),
        model: model.clone(),
        vram_gb,
        context_length,
    });
    config.enabled = true;

    crate::tools::log_tools::append_to_app_log(&format!(
        "SLOTS | configured | role={} provider={} model={} vram={:.1}GB ctx={}", role, provider, model, vram_gb, context_length
    ));

    Ok(config.clone())
}

/// Add a fallback assignment to a slot.
#[tauri::command]
pub fn add_slot_fallback(
    state: tauri::State<'_, SlotsState>,
    role: SlotRole,
    provider: String,
    model: String,
    vram_gb: f64,
    context_length: u32,
) -> Result<SlotConfig, String> {
    let mut configs = state.configs.lock().map_err(|e| format!("Lock error: {}", e))?;

    let config = configs.get_mut(&role).ok_or("Slot not found")?;
    config.fallbacks.push(SlotAssignment {
        provider,
        model,
        vram_gb,
        context_length,
    });

    Ok(config.clone())
}

/// Get current VRAM budget.
#[tauri::command]
pub fn get_vram_budget(
    state: tauri::State<'_, SlotsState>,
) -> Result<VramBudget, String> {
    let budget = state.vram_budget.lock().map_err(|e| format!("Lock error: {}", e))?;
    Ok(budget.clone())
}

/// Set total VRAM (called after hardware detection).
#[tauri::command]
pub fn set_vram_total(
    state: tauri::State<'_, SlotsState>,
    total_gb: f64,
) -> Result<VramBudget, String> {
    let mut budget = state.vram_budget.lock().map_err(|e| format!("Lock error: {}", e))?;
    budget.total_gb = total_gb;
    Ok(budget.clone())
}

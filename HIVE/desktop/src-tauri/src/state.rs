//! Application state managed by Tauri

use std::collections::{HashMap, HashSet};
use std::process::Child;
use std::sync::Mutex;

/// Info about a running specialist server.
pub struct SpecialistServer {
    pub process: Child,
    pub port: u16,
    pub model_path: String,
    pub backend: String, // "windows" or "wsl"
    #[allow(dead_code)] // Phase 4: read by get_specialist_servers and routing logic
    pub slot_role: String, // "coder", "terminal", etc.
}

impl Drop for SpecialistServer {
    fn drop(&mut self) {
        let _ = self.process.kill();  // best-effort teardown
        let _ = self.process.wait();
    }
}

pub struct AppState {
    /// Main server (consciousness / single-model mode) — backwards compatible
    pub server_process: Mutex<Option<Child>>,
    pub server_port: Mutex<u16>,
    pub server_backend: Mutex<String>,
    pub wsl_distro: Mutex<Option<String>>,
    /// Phase 4: specialist servers keyed by port number
    pub specialist_servers: Mutex<HashMap<u16, SpecialistServer>>,
    /// Tracked PIDs of spawned server processes (for targeted cleanup instead of taskkill /IM)
    pub spawned_pids: Mutex<HashSet<u32>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            server_process: Mutex::new(None),
            server_port: Mutex::new(8080),
            server_backend: Mutex::new(String::from("windows")),
            wsl_distro: Mutex::new(None),
            specialist_servers: Mutex::new(HashMap::new()),
            spawned_pids: Mutex::new(HashSet::new()),
        }
    }
}

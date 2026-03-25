//! Server management — llama-server lifecycle (start, stop, status, health)

use std::path::PathBuf;
use std::process::{Command, Stdio};
use tauri::State;

use crate::paths::get_app_data_dir;
use crate::state::AppState;
use crate::tools::log_tools::append_to_app_log;
use crate::types::ServerStatus;
use crate::wsl::{wsl_cmd, windows_to_wsl_path};

// Windows-specific: hide console windows when spawning processes
#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// Start llama-server (native Windows)
#[tauri::command]
pub fn start_server_native(
    model_path: String,
    port: Option<u16>,
    gpu_layers: Option<i32>,
    context_length: Option<u32>,
    kv_offload: Option<bool>,
    state: State<'_, AppState>,
) -> Result<ServerStatus, String> {
    stop_server_internal(&state)?;

    let port = port.unwrap_or(8080);
    let ngl = gpu_layers.unwrap_or(99);
    let ctx = context_length.unwrap_or(4096);
    let kv_to_ram = kv_offload.unwrap_or(false);

    // Look for llama-server.exe
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));

    let server_paths = [
        exe_dir.join("llama-server.exe"),
        exe_dir.join("bin").join("llama-server.exe"),
        get_app_data_dir().join("bin").join("llama-server.exe"),
    ];

    let server_path = server_paths
        .iter()
        .find(|p| p.exists())
        .ok_or_else(|| "llama-server.exe not found".to_string())?;

    let mut args = vec![
        "-m".to_string(), model_path.clone(),
        "--port".to_string(), port.to_string(),
        "--host".to_string(), "127.0.0.1".to_string(),
        "-ngl".to_string(), ngl.to_string(),
        "-c".to_string(), ctx.to_string(),
    ];

    // --no-kv-offload keeps KV cache in system RAM instead of VRAM
    if kv_to_ram {
        args.push("--no-kv-offload".to_string());
    }

    // Capture server output to log file (Principle #4: Errors Are Answers)
    let log_path = get_app_data_dir().join("llama-server.log");
    let stdout_file = std::fs::File::create(&log_path).ok();
    let stderr_file = std::fs::File::options().append(true).open(&log_path).ok();

    let mut command = Command::new(server_path);
    command
        .args(&args)
        .stdout(stdout_file.map(Stdio::from).unwrap_or(Stdio::null()))
        .stderr(stderr_file.map(Stdio::from).unwrap_or(Stdio::null()));

    // Hide the console window on Windows
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);

    let child = command
        .spawn()
        .map_err(|e| {
            append_to_app_log(&format!("SERVER | start_native FAILED | {}", e));
            format!("Failed to start server: {}", e)
        })?;

    // Track PID for targeted cleanup (instead of nuclear taskkill /IM)
    if let Ok(mut pids) = state.spawned_pids.lock() {
        pids.insert(child.id());
    }

    *state.server_process.lock().map_err(|_| "Internal state lock failed".to_string())? = Some(child);
    *state.server_port.lock().map_err(|_| "Internal state lock failed".to_string())? = port;
    *state.server_backend.lock().map_err(|_| "Internal state lock failed".to_string())? = "windows".to_string();

    append_to_app_log(&format!("SERVER | start_native | port={} ngl={} ctx={} kv_offload={} | {}", port, ngl, ctx, kv_to_ram, model_path));

    Ok(ServerStatus {
        running: true,
        port,
        backend: "windows".to_string(),
        model_path: Some(model_path),
    })
}

/// Start llama-server via WSL
#[tauri::command]
pub fn start_server_wsl(
    model_path: String,
    port: Option<u16>,
    gpu_layers: Option<i32>,
    context_length: Option<u32>,
    kv_offload: Option<bool>,
    llama_server_path: Option<String>,
    state: State<'_, AppState>,
) -> Result<ServerStatus, String> {
    stop_server_internal(&state)?;

    let port = port.unwrap_or(8080);
    let ngl = gpu_layers.unwrap_or(99);
    let ctx = context_length.unwrap_or(4096);
    let kv_to_ram = kv_offload.unwrap_or(false);
    let server_bin = llama_server_path.unwrap_or_else(|| "llama-server".to_string());

    // Convert Windows path to WSL path if needed
    let wsl_model_path = windows_to_wsl_path(&model_path);

    // Build the command to run in WSL (P6: shell-escape all interpolated values)
    let kv_flag = if kv_to_ram { " --no-kv-offload" } else { "" };
    let cmd = format!(
        "'{}' -m '{}' --port {} --host 127.0.0.1 -ngl {} -c {}{}",
        crate::wsl::shell_escape(&server_bin),
        crate::wsl::shell_escape(&wsl_model_path),
        port, ngl, ctx, kv_flag
    );

    // Capture server output to log file (Principle #4: Errors Are Answers)
    let log_path = get_app_data_dir().join("llama-server.log");
    let stdout_file = std::fs::File::create(&log_path).ok();
    let stderr_file = std::fs::File::options().append(true).open(&log_path).ok();

    let mut command = wsl_cmd();
    command
        .args(["-e", "bash", "-c", &cmd])
        .stdout(stdout_file.map(Stdio::from).unwrap_or(Stdio::null()))
        .stderr(stderr_file.map(Stdio::from).unwrap_or(Stdio::null()));

    // Hide the console window on Windows
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);

    let child = command
        .spawn()
        .map_err(|e| {
            append_to_app_log(&format!("SERVER | start_wsl FAILED | {}", e));
            format!("Failed to start WSL server: {}", e)
        })?;

    if let Ok(mut pids) = state.spawned_pids.lock() {
        pids.insert(child.id());
    }

    *state.server_process.lock().map_err(|_| "Internal state lock failed".to_string())? = Some(child);
    *state.server_port.lock().map_err(|_| "Internal state lock failed".to_string())? = port;
    *state.server_backend.lock().map_err(|_| "Internal state lock failed".to_string())? = "wsl".to_string();

    append_to_app_log(&format!("SERVER | start_wsl | port={} ngl={} ctx={} kv_offload={} | {}", port, ngl, ctx, kv_to_ram, wsl_model_path));

    Ok(ServerStatus {
        running: true,
        port,
        backend: "wsl".to_string(),
        model_path: Some(wsl_model_path),
    })
}

/// Stop the running server
#[tauri::command]
pub fn stop_server(state: State<'_, AppState>) -> Result<(), String> {
    stop_server_internal(&state)
}

fn stop_server_internal(state: &State<'_, AppState>) -> Result<(), String> {
    let mut process = state.server_process.lock().map_err(|_| "Internal state lock failed".to_string())?;
    let backend = state.server_backend.lock().map_err(|_| "Internal state lock failed".to_string())?.clone();

    if let Some(mut child) = process.take() {
        // Clean PID from tracked set
        if let Ok(mut pids) = state.spawned_pids.lock() {
            pids.remove(&child.id());
        }
        if let Err(e) = child.kill() {
            eprintln!("[HIVE] SERVER | kill failed: {} — port may remain occupied", e);
        }
        let _ = child.wait(); // wait is best-effort after kill
        append_to_app_log(&format!("SERVER | stopped | backend={}", backend));
    }

    // For WSL, also kill any lingering llama-server processes
    if backend == "wsl" {
        let _ = wsl_cmd()
            .args(["-e", "pkill", "-f", "llama-server"])
            .output();
    }

    Ok(())
}

/// Kill ALL server processes — called on app exit (P0a: Process Cleanup).
/// Stops main server + all specialist servers + platform-specific orphan cleanup.
/// Takes &AppState (not State<'_>) for use from window event handlers.
pub fn cleanup_all_servers(state: &AppState) {
    // 1. Stop main server
    if let Ok(mut process) = state.server_process.lock() {
        if let Some(mut child) = process.take() {
            if let Err(e) = child.kill() {
                eprintln!("[HIVE] SERVER | cleanup kill failed: {}", e);
            }
            let _ = child.wait();
        }
    }

    // 2. Stop all specialist servers
    if let Ok(mut servers) = state.specialist_servers.lock() {
        for (port, mut server) in servers.drain() {
            let _ = server.process.kill();
            let _ = server.process.wait();
            append_to_app_log(&format!(
                "SERVER | shutdown_cleanup | role={} port={} backend={}",
                server.slot_role, port, server.backend
            ));
        }
    }

    // 3. WSL safety net — kill any lingering llama-server processes
    if let Ok(backend) = state.server_backend.lock() {
        if *backend == "wsl" {
            let _ = wsl_cmd()
                .args(["-e", "pkill", "-f", "llama-server"])
                .output();
        }
    }

    // 4. Windows safety net — kill tracked PIDs only (not nuclear taskkill /IM)
    #[cfg(windows)]
    if let Ok(mut pids) = state.spawned_pids.lock() {
        for pid in pids.drain() {
            let mut cmd = Command::new("taskkill");
            cmd.args(["/F", "/PID", &pid.to_string()]);
            cmd.creation_flags(CREATE_NO_WINDOW);
            let _ = cmd.output();
        }
    }

    append_to_app_log("SERVER | cleanup_all | All server processes terminated on exit");
}

/// Get server status
#[tauri::command]
pub fn get_server_status(state: State<'_, AppState>) -> ServerStatus {
    let port = state.server_port.lock().ok().map(|p| *p).unwrap_or(8080);
    let backend = state.server_backend.lock().ok().map(|b| b.clone()).unwrap_or_default();

    // P0b: Check if process is actually alive using try_wait() (not just is_some()).
    // A crashed llama-server previously reported as "running" because the Child handle existed.
    let running = if let Ok(mut guard) = state.server_process.lock() {
        if let Some(ref mut child) = *guard {
            match child.try_wait() {
                Ok(Some(status)) => {
                    // Process exited — clear the handle and log
                    let exit_info = if status.success() { "clean" } else { "crashed" };
                    append_to_app_log(&format!(
                        "SERVER | process_exited | backend={} status={}", backend, exit_info
                    ));
                    *guard = None;
                    false
                }
                Ok(None) => true,   // Still running
                Err(_) => {
                    *guard = None;   // Can't check — assume dead
                    false
                }
            }
        } else {
            false
        }
    } else {
        false
    };

    ServerStatus {
        running,
        port,
        backend,
        model_path: None,
    }
}

/// Read the last N lines of the server log file (for diagnostics when server fails)
#[tauri::command]
pub fn read_server_log(lines: Option<usize>) -> Result<String, String> {
    let log_path = get_app_data_dir().join("llama-server.log");
    let content = std::fs::read_to_string(&log_path)
        .map_err(|e| format!("No server log available: {}", e))?;

    let n = lines.unwrap_or(50);
    let tail: Vec<&str> = content.lines().rev().take(n).collect();
    let mut tail = tail;
    tail.reverse();
    Ok(tail.join("\n"))
}

// ================================================================
// Phase 4: Specialist Server Management
//
// Multiple llama-server instances on different ports.
// Consciousness stays on 8080 (the main server above).
// Specialists get 8081-8084.
// ================================================================

/// Port assignments for specialist slots — single source of truth (Rust side).
/// TypeScript mirror: SPECIALIST_PORTS in types.ts. Must stay in sync (P5).
pub fn port_for_slot(slot_role: &str) -> u16 {
    match slot_role {
        "consciousness" => 8080,
        "coder" => 8081,
        "terminal" => 8082,
        "webcrawl" => 8083,
        "toolcall" => 8084,
        _ => {
            eprintln!("[HIVE] WARNING: Unknown slot role '{}', using overflow port 8085", slot_role);
            8085
        }
    }
}

/// Start a specialist llama-server on a specific port (native Windows).
#[tauri::command]
pub fn start_specialist_server(
    slot_role: String,
    model_path: String,
    gpu_layers: Option<i32>,
    context_length: Option<u32>,
    kv_offload: Option<bool>,
    state: State<'_, AppState>,
) -> Result<ServerStatus, String> {
    let port = port_for_slot(&slot_role);

    // Stop any existing server on this port first
    stop_specialist_server_internal(&state, port)?;

    let ngl = gpu_layers.unwrap_or(99);
    let ctx = context_length.unwrap_or(4096);
    let kv_to_ram = kv_offload.unwrap_or(false);

    // Find llama-server binary (same search as start_server_native)
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));

    let server_paths = [
        exe_dir.join("llama-server.exe"),
        exe_dir.join("bin").join("llama-server.exe"),
        get_app_data_dir().join("bin").join("llama-server.exe"),
    ];

    let server_path = server_paths
        .iter()
        .find(|p| p.exists())
        .ok_or_else(|| "llama-server.exe not found".to_string())?;

    let mut args = vec![
        "-m".to_string(), model_path.clone(),
        "--port".to_string(), port.to_string(),
        "--host".to_string(), "127.0.0.1".to_string(),
        "-ngl".to_string(), ngl.to_string(),
        "-c".to_string(), ctx.to_string(),
    ];

    if kv_to_ram {
        args.push("--no-kv-offload".to_string());
    }

    // Per-slot log file (not shared with main server)
    let log_path = get_app_data_dir().join(format!("llama-server-{}.log", slot_role));
    let stdout_file = std::fs::File::create(&log_path).ok();
    let stderr_file = std::fs::File::options().append(true).open(&log_path).ok();

    let mut command = Command::new(server_path);
    command
        .args(&args)
        .stdout(stdout_file.map(Stdio::from).unwrap_or(Stdio::null()))
        .stderr(stderr_file.map(Stdio::from).unwrap_or(Stdio::null()));

    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);

    let child = command
        .spawn()
        .map_err(|e| {
            append_to_app_log(&format!("SERVER | specialist_start FAILED | role={} | {}", slot_role, e));
            format!("Failed to start {} server: {}", slot_role, e)
        })?;

    if let Ok(mut pids) = state.spawned_pids.lock() {
        pids.insert(child.id());
    }

    // Store in specialist_servers map
    let mut servers = state.specialist_servers.lock()
        .map_err(|_| "Internal state lock failed".to_string())?;

    servers.insert(port, crate::state::SpecialistServer {
        process: child,
        port,
        model_path: model_path.clone(),
        backend: "windows".to_string(),
        slot_role: slot_role.clone(),
    });

    append_to_app_log(&format!("SERVER | specialist_start | role={} port={} backend=windows | {}", slot_role, port, model_path));

    Ok(ServerStatus {
        running: true,
        port,
        backend: "windows".to_string(),
        model_path: Some(model_path),
    })
}

/// Start a specialist llama-server via WSL.
#[tauri::command]
pub fn start_specialist_server_wsl(
    slot_role: String,
    model_path: String,
    gpu_layers: Option<i32>,
    context_length: Option<u32>,
    kv_offload: Option<bool>,
    llama_server_path: Option<String>,
    state: State<'_, AppState>,
) -> Result<ServerStatus, String> {
    let port = port_for_slot(&slot_role);
    stop_specialist_server_internal(&state, port)?;

    let ngl = gpu_layers.unwrap_or(99);
    let ctx = context_length.unwrap_or(4096);
    let kv_to_ram = kv_offload.unwrap_or(false);
    let server_bin = llama_server_path.unwrap_or_else(|| "llama-server".to_string());
    let wsl_model_path = windows_to_wsl_path(&model_path);

    let kv_flag = if kv_to_ram { " --no-kv-offload" } else { "" };
    let cmd = format!(
        "'{}' -m '{}' --port {} --host 127.0.0.1 -ngl {} -c {}{}",
        crate::wsl::shell_escape(&server_bin),
        crate::wsl::shell_escape(&wsl_model_path),
        port, ngl, ctx, kv_flag
    );

    let log_path = get_app_data_dir().join(format!("llama-server-{}.log", slot_role));
    let stdout_file = std::fs::File::create(&log_path).ok();
    let stderr_file = std::fs::File::options().append(true).open(&log_path).ok();

    let mut command = wsl_cmd();
    command
        .args(["-e", "bash", "-c", &cmd])
        .stdout(stdout_file.map(Stdio::from).unwrap_or(Stdio::null()))
        .stderr(stderr_file.map(Stdio::from).unwrap_or(Stdio::null()));

    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);

    let child = command
        .spawn()
        .map_err(|e| {
            append_to_app_log(&format!("SERVER | specialist_start_wsl FAILED | role={} | {}", slot_role, e));
            format!("Failed to start WSL {} server: {}", slot_role, e)
        })?;

    if let Ok(mut pids) = state.spawned_pids.lock() {
        pids.insert(child.id());
    }

    let mut servers = state.specialist_servers.lock()
        .map_err(|_| "Internal state lock failed".to_string())?;

    servers.insert(port, crate::state::SpecialistServer {
        process: child,
        port,
        model_path: wsl_model_path.clone(),
        backend: "wsl".to_string(),
        slot_role: slot_role.clone(),
    });

    append_to_app_log(&format!("SERVER | specialist_start | role={} port={} backend=wsl | {}", slot_role, port, wsl_model_path));

    Ok(ServerStatus {
        running: true,
        port,
        backend: "wsl".to_string(),
        model_path: Some(wsl_model_path),
    })
}

/// Stop a specialist server by port.
#[tauri::command]
pub fn stop_specialist_server(
    slot_role: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let port = port_for_slot(&slot_role);
    stop_specialist_server_internal(&state, port)
}

fn stop_specialist_server_internal(state: &State<'_, AppState>, port: u16) -> Result<(), String> {
    let mut servers = state.specialist_servers.lock()
        .map_err(|_| "Internal state lock failed".to_string())?;

    if let Some(mut server) = servers.remove(&port) {
        if let Ok(mut pids) = state.spawned_pids.lock() {
            pids.remove(&server.process.id());
        }
        let _ = server.process.kill();
        let _ = server.process.wait();
        append_to_app_log(&format!("SERVER | specialist_stopped | role={} port={} backend={}", server.slot_role, port, server.backend));

        // For WSL, kill lingering processes on this specific port
        if server.backend == "wsl" {
            let _ = wsl_cmd()
                .args(["-e", "bash", "-c", &format!("fuser -k {}/tcp 2>/dev/null", port)])
                .output();
        }
    }

    Ok(())
}

/// Get status of all specialist servers.
/// P0b: Uses try_wait() to detect crashed specialists and removes them from the map.
#[tauri::command]
pub fn get_specialist_servers(
    state: State<'_, AppState>,
) -> Result<Vec<ServerStatus>, String> {
    let mut servers = state.specialist_servers.lock()
        .map_err(|_| "Internal state lock failed".to_string())?;

    // Check each specialist process — remove dead ones
    let dead_ports: Vec<u16> = servers.iter_mut()
        .filter_map(|(port, server)| {
            match server.process.try_wait() {
                Ok(Some(status)) => {
                    let exit_info = if status.success() { "clean" } else { "crashed" };
                    append_to_app_log(&format!(
                        "SERVER | specialist_exited | role={} port={} status={}",
                        server.slot_role, port, exit_info
                    ));
                    Some(*port)
                }
                Ok(None) => None,  // Still running
                Err(_) => Some(*port), // Can't check — assume dead
            }
        })
        .collect();

    for port in &dead_ports {
        servers.remove(port);
    }

    Ok(servers.values().map(|s| ServerStatus {
        running: true, // All remaining are verified alive by try_wait()
        port: s.port,
        backend: s.backend.clone(),
        model_path: Some(s.model_path.clone()),
    }).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- port_for_slot tests ---

    #[test]
    fn port_consciousness() {
        assert_eq!(port_for_slot("consciousness"), 8080);
    }

    #[test]
    fn port_coder() {
        assert_eq!(port_for_slot("coder"), 8081);
    }

    #[test]
    fn port_terminal() {
        assert_eq!(port_for_slot("terminal"), 8082);
    }

    #[test]
    fn port_webcrawl() {
        assert_eq!(port_for_slot("webcrawl"), 8083);
    }

    #[test]
    fn port_toolcall() {
        assert_eq!(port_for_slot("toolcall"), 8084);
    }

    #[test]
    fn port_unknown_fallback() {
        assert_eq!(port_for_slot("unknown"), 8085);
        assert_eq!(port_for_slot(""), 8085);
        assert_eq!(port_for_slot("research"), 8085);
    }

    #[test]
    fn port_all_unique() {
        let ports: Vec<u16> = ["consciousness", "coder", "terminal", "webcrawl", "toolcall"]
            .iter()
            .map(|s| port_for_slot(s))
            .collect();
        let unique: std::collections::HashSet<u16> = ports.iter().cloned().collect();
        assert_eq!(ports.len(), unique.len(), "All specialist ports must be unique");
    }

    #[test]
    fn port_range_valid() {
        for slot in &["consciousness", "coder", "terminal", "webcrawl", "toolcall"] {
            let port = port_for_slot(slot);
            assert!(port >= 8080 && port <= 8085, "Port {} for slot {} out of expected range", port, slot);
        }
    }
}

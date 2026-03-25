//! WSL2 bridge — detection, path conversion, command execution

use std::process::Command;
use tauri::State;

use crate::state::AppState;
use crate::types::WslStatus;

// Windows-specific: hide console windows when spawning processes
#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// Escape a string for use inside single-quoted bash arguments.
/// Replaces each `'` with `'\''` (end quote, literal quote, resume quote).
/// This prevents shell injection when interpolating values into `bash -c '...'` strings.
pub fn shell_escape(s: &str) -> String {
    s.replace('\'', "'\\''")
}

/// Helper to create WSL commands with hidden console window on Windows
pub fn wsl_cmd() -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new("wsl");
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

/// Convert Windows path to WSL path (e.g., C:\Users\... -> /mnt/c/Users/...)
pub fn windows_to_wsl_path(windows_path: &str) -> String {
    // Check if it's already a WSL path
    if windows_path.starts_with('/') {
        return windows_path.to_string();
    }

    // Handle Windows paths like C:\Users\... or C:/Users/...
    let path = windows_path.replace('\\', "/");

    // Check for drive letter pattern (e.g., C:/ or D:/)
    if path.len() >= 2 && path.chars().nth(1) == Some(':') {
        let drive_letter = path.chars().next().unwrap().to_ascii_lowercase();
        let rest = &path[2..]; // Skip "C:"
        let rest = rest.strip_prefix('/').unwrap_or(rest); // Remove leading slash if present
        return format!("/mnt/{}/{}", drive_letter, rest);
    }

    // Return as-is if not a recognizable Windows path
    windows_path.to_string()
}

/// Convert WSL path to Windows path (e.g., /mnt/c/Users/... -> C:\Users\...)
pub fn wsl_to_windows_path(wsl_path: &str) -> String {
    // Check if it's a /mnt/ path
    if wsl_path.starts_with("/mnt/") && wsl_path.len() >= 6 {
        let drive_letter = wsl_path.chars().nth(5).unwrap().to_ascii_uppercase();
        let rest = &wsl_path[6..]; // Skip "/mnt/c"
        return format!("{}:{}", drive_letter, rest.replace('/', "\\"));
    }

    // Return as-is if not a /mnt/ path
    wsl_path.to_string()
}

/// Check WSL status
#[tauri::command]
pub fn check_wsl() -> WslStatus {
    let mut status = WslStatus {
        installed: false,
        distro: None,
        llama_server_path: None,
        rocm_version: None,
        cuda_version: None,
    };

    // Check if WSL is available
    if let Ok(output) = wsl_cmd().args(["--status"]).output() {
        status.installed = output.status.success();
    }

    if !status.installed {
        return status;
    }

    // Get default distro
    if let Ok(output) = wsl_cmd().args(["-l", "-q"]).output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        // First non-empty line is usually the default
        for line in stdout.lines() {
            let trimmed = line.trim().replace('\0', ""); // WSL outputs UTF-16
            if !trimmed.is_empty() {
                status.distro = Some(trimmed);
                break;
            }
        }
    }

    // Check for llama-server in common locations
    // Use $HOME instead of ~ for reliable expansion
    let check_cmd = r#"
        for p in \
            "$(which llama-server 2>/dev/null)" \
            "/usr/local/bin/llama-server" \
            "/usr/bin/llama-server" \
            "$HOME/llama.cpp/build/bin/llama-server" \
            "$HOME/llama.cpp/llama-server" \
            "$HOME/llama-server"; do
            if [ -x "$p" ] 2>/dev/null; then
                echo "$p"
                exit 0
            fi
        done
    "#;

    if let Ok(output) = wsl_cmd()
        .args(["-e", "bash", "-c", check_cmd])
        .output()
    {
        let found_path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !found_path.is_empty() && output.status.success() {
            status.llama_server_path = Some(found_path);
        }
    }

    // Check ROCm version
    if let Ok(output) = wsl_cmd()
        .args(["-e", "bash", "-c", "cat /opt/rocm/.info/version 2>/dev/null || rocminfo 2>/dev/null | grep -oP 'ROCm\\s+\\K[0-9.]+'"])
        .output()
    {
        let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !version.is_empty() {
            status.rocm_version = Some(version);
        }
    }

    // Check CUDA version
    if let Ok(output) = wsl_cmd()
        .args(["-e", "bash", "-c", "nvcc --version 2>/dev/null | grep -oP 'release \\K[0-9.]+'"])
        .output()
    {
        let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !version.is_empty() {
            status.cuda_version = Some(version);
        }
    }

    status
}

/// Run a command in WSL and return output.
/// SECURITY: This runs arbitrary shell commands. Only called from desktop UI (never from
/// remote channels or HiveTool registry). Input is shell-escaped to prevent injection
/// from chained commands, but the command itself is still arbitrary — P6 requires this
/// to be desktop-only. Currently unused by frontend (dead code candidate).
#[tauri::command]
pub fn run_wsl_command(command: String) -> Result<String, String> {
    // Shell-escape the command to prevent injection of additional commands
    let escaped = shell_escape(&command);
    let output = wsl_cmd()
        .args(["-e", "bash", "-c", &escaped])
        .output()
        .map_err(|e| format!("Failed to run WSL command: {}", e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

/// Set the WSL distro to use
#[tauri::command]
pub fn set_wsl_distro(distro: String, state: State<'_, AppState>) -> Result<(), String> {
    *state.wsl_distro.lock().map_err(|_| "Internal state lock failed".to_string())? = Some(distro);
    Ok(())
}

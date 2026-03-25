//! Cloudflare Tunnel for remote access (P7).
//!
//! Spawns `cloudflared` as a child process to create a temporary tunnel
//! exposing a local port via a public Cloudflare URL. Useful for:
//!   - Remote MCP access (external coding tools like Cursor/Windsurf)
//!   - Remote llama-server access (inference from other machines)
//!
//! Requires `cloudflared` installed and on PATH.
//! No Cloudflare account needed — uses free quick tunnels (trycloudflare.com).

use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::io::{BufRead, BufReader};

use crate::tools::log_tools::append_to_app_log;

// Windows-specific: hide console windows when spawning processes
#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// Global tunnel process handle.
static TUNNEL_PROCESS: OnceLock<Mutex<Option<Child>>> = OnceLock::new();

/// Global tunnel URL (set once cloudflared prints it).
static TUNNEL_URL: OnceLock<Mutex<Option<String>>> = OnceLock::new();

/// Static compiled regex for parsing tunnel URLs (compiled once, never recompiled).
static URL_REGEX: OnceLock<regex::Regex> = OnceLock::new();

fn url_regex() -> &'static regex::Regex {
    URL_REGEX.get_or_init(|| {
        regex::Regex::new(r"https://[a-zA-Z0-9-]+\.trycloudflare\.com")
            .expect("tunnel URL regex is valid")
    })
}

fn process_lock() -> &'static Mutex<Option<Child>> {
    TUNNEL_PROCESS.get_or_init(|| Mutex::new(None))
}

fn url_lock() -> &'static Mutex<Option<String>> {
    TUNNEL_URL.get_or_init(|| Mutex::new(None))
}

/// Start a Cloudflare tunnel forwarding traffic to `http://localhost:{port}`.
/// Returns the public tunnel URL (e.g. `https://xyz.trycloudflare.com`).
#[tauri::command]
pub async fn tunnel_start(port: u16) -> Result<String, String> {
    // Port validation
    if port == 0 {
        return Err("Port 0 is not valid for tunnel forwarding.".to_string());
    }
    if port < 1024 {
        append_to_app_log(&format!(
            "TUNNEL | warning | port={} is a privileged port (<1024), this may require elevated permissions",
            port
        ));
    }

    // Race-safe: check+claim in a single lock scope using a sentinel value.
    // If already running, return existing URL. If not, set a "starting" sentinel
    // to prevent concurrent starts, then release the lock before the .await.
    {
        let mut url_guard = url_lock().lock().map_err(|e| format!("URL lock poisoned: {}", e))?;
        if let Some(ref url) = *url_guard {
            if url == "__starting__" {
                // Another tunnel_start is already in progress — don't return sentinel as URL (B2 fix)
                return Err("Tunnel start already in progress".to_string());
            }
            return Ok(url.clone());
        }
        // Claim the slot with a sentinel — concurrent callers will see this and return early
        *url_guard = Some("__starting__".to_string());
    }

    // Find cloudflared (sync, no .await)
    let cloudflared = match which_cloudflared() {
        Some(path) => path,
        None => {
            // Clear sentinel on failure
            if let Ok(mut guard) = url_lock().lock() { *guard = None; }
            return Err("cloudflared not found on PATH. Install from https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/downloads/".to_string());
        }
    };

    append_to_app_log(&format!(
        "TUNNEL | starting | port={} | binary={}",
        port, cloudflared
    ));

    // Spawn cloudflared — it prints the URL to stderr
    let mut command = Command::new(&cloudflared);
    command
        .args(["tunnel", "--url", &format!("http://localhost:{}", port)])
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);

    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => {
            if let Ok(mut guard) = url_lock().lock() { *guard = None; }
            return Err(format!("Failed to spawn cloudflared: {}", e));
        }
    };

    // Take stderr for URL parsing — the handle is consumed by spawn_blocking
    let stderr = child.stderr.take().ok_or_else(|| {
        if let Ok(mut guard) = url_lock().lock() { *guard = None; }
        "No stderr from cloudflared".to_string()
    })?;

    // Parse URL from stderr in a blocking task (avoids blocking the async runtime).
    // BufReader and stderr are moved into the closure — pipe auto-cleaned on drop.
    let found_url = tokio::task::spawn_blocking(move || {
        let reader = BufReader::new(stderr);
        let regex = url_regex();

        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(30);

        for line in reader.lines() {
            if start.elapsed() > timeout {
                break;
            }
            match line {
                Ok(text) => {
                    if let Some(mat) = regex.find(&text) {
                        return Some(mat.as_str().to_string());
                    }
                }
                Err(_) => break,
            }
        }
        None
    })
    .await
    .map_err(|e| format!("URL parse task failed: {}", e))?;

    let url = match found_url {
        Some(u) => u,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            if let Ok(mut guard) = url_lock().lock() { *guard = None; }
            return Err("Timed out waiting for cloudflared tunnel URL (30s). Check cloudflared installation.".to_string());
        }
    };

    // Store process and real URL (replacing the sentinel)
    if let Ok(mut guard) = process_lock().lock() {
        *guard = Some(child);
    }
    if let Ok(mut guard) = url_lock().lock() {
        *guard = Some(url.clone());
    }

    append_to_app_log(&format!(
        "TUNNEL | started | port={} | url={}",
        port, url
    ));

    Ok(url)
}

/// Stop the running Cloudflare tunnel.
#[tauri::command]
pub fn tunnel_stop() -> Result<(), String> {
    if let Ok(mut guard) = process_lock().lock() {
        if let Some(mut child) = guard.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    if let Ok(mut guard) = url_lock().lock() {
        *guard = None;
    }

    append_to_app_log("TUNNEL | stopped");
    Ok(())
}

/// Get the current tunnel URL (None if not running).
#[tauri::command]
pub fn tunnel_status() -> Result<Option<String>, String> {
    // Check if process is still alive — acquire process lock, then RELEASE before touching url_lock.
    // This avoids deadlock: tunnel_start acquires url_lock→process_lock, so we must NOT
    // acquire process_lock→url_lock (B3 fix: consistent lock ordering).
    let mut process_exited = false;
    if let Ok(mut proc_guard) = process_lock().lock() {
        if let Some(ref mut child) = *proc_guard {
            match child.try_wait() {
                Ok(Some(_)) => {
                    *proc_guard = None;
                    process_exited = true;
                }
                Ok(None) => {} // still running
                Err(_) => {
                    *proc_guard = None;
                    process_exited = true;
                }
            }
        }
    }
    // process_lock is now released — safe to acquire url_lock
    if process_exited {
        if let Ok(mut url_guard) = url_lock().lock() {
            *url_guard = None;
        }
        append_to_app_log("TUNNEL | crashed | cloudflared process exited unexpectedly");
        return Ok(None);
    }

    let url = url_lock()
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
        .filter(|u| u != "__starting__");  // Don't expose sentinel as a real URL
    Ok(url)
}

/// Find cloudflared binary on PATH.
/// Uses platform-appropriate command: `where` on Windows, `which` on Unix.
fn which_cloudflared() -> Option<String> {
    let finder = if cfg!(windows) { "where" } else { "which" };

    for name in &["cloudflared", "cloudflared.exe"] {
        let mut cmd = Command::new(finder);
        cmd.arg(name);

        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);

        if let Ok(output) = cmd.output() {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if !path.is_empty() {
                    return Some(path);
                }
            }
        }
    }
    None
}

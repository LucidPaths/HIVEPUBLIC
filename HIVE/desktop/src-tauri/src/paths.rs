//! Path helpers for HIVE data directories

use std::collections::HashMap;
use std::path::PathBuf;

/// Get the app data directory: %LocalAppData%/HIVE or equivalent.
/// Panics if the OS cannot determine a local data directory (P4: Errors Are Answers).
pub fn get_app_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .expect("FATAL: OS returned no local data directory — cannot determine where to store HIVE data. Set %LOCALAPPDATA% and retry.")
        .join("HIVE")
}

/// Get the models subdirectory
pub fn get_models_dir() -> PathBuf {
    get_app_data_dir().join("models")
}

/// Get the bin subdirectory (for llama-server.exe)
pub fn get_bin_dir() -> PathBuf {
    get_app_data_dir().join("bin")
}

/// Get the attachments subdirectory (for uploaded files)
pub fn get_attachments_dir() -> PathBuf {
    get_app_data_dir().join("attachments")
}

/// Return key application paths
#[tauri::command]
pub fn get_app_paths() -> HashMap<String, String> {
    let mut paths = HashMap::new();
    paths.insert("data_dir".to_string(), get_app_data_dir().to_string_lossy().to_string());
    paths.insert("models_dir".to_string(), get_models_dir().to_string_lossy().to_string());
    paths.insert("attachments_dir".to_string(), get_attachments_dir().to_string_lossy().to_string());
    paths
}

/// Save an uploaded file to the attachments directory. Returns the full path.
#[tauri::command]
pub async fn save_attachment(filename: String, data: Vec<u8>) -> Result<String, String> {
    let dir = get_attachments_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create attachments dir: {}", e))?;

    // Sanitize filename — strip path separators and traversal sequences
    let safe_name = filename.replace(['/', '\\'], "_").replace("..", "_");
    if safe_name.is_empty() || safe_name == "." {
        return Err("Invalid filename".to_string());
    }
    let path = dir.join(&safe_name);

    // Defense-in-depth: verify final path is inside attachments dir
    // (canonicalize won't work pre-write, so check the parent component)
    if path.parent() != Some(&dir) {
        return Err("Path traversal attempt blocked".to_string());
    }

    tokio::fs::write(&path, &data).await
        .map_err(|e| format!("Failed to save attachment '{}': {}", safe_name, e))?;

    Ok(path.to_string_lossy().to_string())
}

// ============================================
// Channel Settings Persistence
// ============================================
// Shared file: %LocalAppData%/HIVE/channel_settings.json
// Both Telegram and Discord daemons use this for host_ids/user_ids.
// Not encrypted — these are configuration, not secrets (P6).

/// Get channel settings file path
fn channel_settings_path() -> PathBuf {
    get_app_data_dir().join("channel_settings.json")
}

/// Load the full channel settings JSON from disk.
/// Returns empty object if file doesn't exist or is invalid.
fn load_channel_settings() -> serde_json::Value {
    let path = channel_settings_path();
    match std::fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_else(|_| serde_json::json!({})),
        Err(_) => serde_json::json!({}),
    }
}

/// Save the full channel settings JSON to disk.
fn save_channel_settings(settings: &serde_json::Value) -> Result<(), String> {
    let path = channel_settings_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create settings dir: {}", e))?;
    }
    let json = serde_json::to_string_pretty(settings)
        .map_err(|e| format!("Failed to serialize channel settings: {}", e))?;
    std::fs::write(&path, json)
        .map_err(|e| format!("Failed to write channel settings: {}", e))?;
    Ok(())
}

/// Load access lists for a specific channel (e.g., "telegram" or "discord").
/// Returns (host_ids, user_ids).
pub fn load_channel_access(channel: &str) -> (Vec<String>, Vec<String>) {
    let settings = load_channel_settings();
    let section = settings.get(channel).cloned().unwrap_or_else(|| serde_json::json!({}));

    let host_ids = section.get("host_ids")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let user_ids = section.get("user_ids")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    (host_ids, user_ids)
}

/// Save access lists for a specific channel.
pub fn save_channel_access(channel: &str, host_ids: &[String], user_ids: &[String]) -> Result<(), String> {
    let mut settings = load_channel_settings();
    settings[channel]["host_ids"] = serde_json::json!(host_ids);
    settings[channel]["user_ids"] = serde_json::json!(user_ids);
    save_channel_settings(&settings)
}

/// Load watched channels for Discord.
pub fn load_discord_watched_channels() -> Vec<String> {
    let settings = load_channel_settings();
    settings.get("discord")
        .and_then(|d| d.get("watched_channels"))
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default()
}

/// Save watched channels for Discord.
pub fn save_discord_watched_channels(channels: &[String]) -> Result<(), String> {
    let mut settings = load_channel_settings();
    settings["discord"]["watched_channels"] = serde_json::json!(channels);
    save_channel_settings(&settings)
}

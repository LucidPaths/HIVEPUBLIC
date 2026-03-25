//! Model management — listing local and WSL models

use crate::gguf::parse_gguf_header;
use crate::paths::get_models_dir;
use crate::types::ModelInfo;
use crate::wsl::{wsl_cmd, wsl_to_windows_path};

/// List models in Windows local storage
#[tauri::command]
pub fn list_local_models() -> Result<Vec<ModelInfo>, String> {
    let models_dir = get_models_dir();

    if !models_dir.exists() {
        std::fs::create_dir_all(&models_dir)
            .map_err(|e| format!("Failed to create models dir: {}", e))?;
        return Ok(vec![]);
    }

    let mut models = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&models_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "gguf").unwrap_or(false) {
                // Skip vocab test files
                let filename = path.file_name().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
                if filename.contains("vocab") {
                    continue;
                }
                if let Ok(meta) = std::fs::metadata(&path) {
                    let size_bytes = meta.len();
                    let context_length = parse_gguf_header(&path.to_string_lossy())
                        .ok()
                        .and_then(|m| m.context_length);
                    models.push(ModelInfo {
                        id: path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
                        filename: path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
                        size_bytes,
                        size_gb: size_bytes as f64 / 1024.0 / 1024.0 / 1024.0,
                        path: path.to_string_lossy().to_string(),
                        context_length,
                    });
                }
            }
        }
    }

    Ok(models)
}

/// List models in WSL filesystem
#[tauri::command(rename_all = "camelCase")]
pub fn list_wsl_models(search_paths: Vec<String>) -> Result<Vec<ModelInfo>, String> {
    let mut models = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();

    for search_path in search_paths {
        // Expand ~ and $HOME properly
        let expanded_path = if search_path.starts_with("~") {
            format!("$HOME{}", &search_path[1..])
        } else {
            search_path.clone()
        };

        // Filter out vocab test files
        // Use double quotes so $HOME expands properly
        let cmd = format!(
            "find \"{}\" -maxdepth 5 -name '*.gguf' ! -iname '*vocab*' -type f 2>/dev/null | head -100",
            expanded_path
        );

        if let Ok(output) = wsl_cmd()
            .args(["-e", "bash", "-c", &cmd])
            .output()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let path = line.trim();
                if path.is_empty() || seen_paths.contains(path) {
                    continue;
                }
                seen_paths.insert(path.to_string());

                // Get file size (shell_escape path to prevent injection — B11 fix, P6)
                let size_cmd = format!("stat -c%s {} 2>/dev/null", crate::wsl::shell_escape(path));
                let size_bytes: u64 = wsl_cmd()
                    .args(["-e", "bash", "-c", &size_cmd])
                    .output()
                    .ok()
                    .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok())
                    .unwrap_or(0);

                let filename = path.rsplit('/').next().unwrap_or(path).to_string();
                let id = filename.trim_end_matches(".gguf").to_string();

                // For models on /mnt/c/ (Windows bridge), parse GGUF directly
                let context_length = if path.starts_with("/mnt/") {
                    let win_path = wsl_to_windows_path(path);
                    parse_gguf_header(&win_path).ok().and_then(|m| m.context_length)
                } else {
                    None
                };

                models.push(ModelInfo {
                    id,
                    filename,
                    size_bytes,
                    size_gb: size_bytes as f64 / 1024.0 / 1024.0 / 1024.0,
                    path: path.to_string(),
                    context_length,
                });
            }
        }
    }

    Ok(models)
}

/// Get models directory path
#[tauri::command]
pub fn get_models_directory() -> String {
    get_models_dir().to_string_lossy().to_string()
}

/// Open models directory in file explorer
#[tauri::command]
pub fn open_models_directory() -> Result<(), String> {
    let models_dir = get_models_dir();

    if !models_dir.exists() {
        std::fs::create_dir_all(&models_dir)
            .map_err(|e| format!("Failed to create dir: {}", e))?;
    }

    #[cfg(windows)]
    std::process::Command::new("explorer")
        .arg(&models_dir)
        .spawn()
        .map_err(|e| format!("Failed to open explorer: {}", e))?;

    Ok(())
}

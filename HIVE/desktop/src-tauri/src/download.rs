//! Model download — streaming downloads to Windows and WSL filesystems

use futures_util::StreamExt;
use std::io::Write;
use std::process::Stdio;
use tauri::Emitter;

use crate::http_client::hive_http_client;
use crate::paths::get_models_dir;
use crate::tools::log_tools::append_to_app_log;
use crate::types::DownloadProgress;
use crate::wsl::wsl_cmd;

/// Download a model file directly to disk (streaming, no memory issues)
#[tauri::command]
pub async fn download_model(
    app: tauri::AppHandle,
    url: String,
    filename: String,
) -> Result<String, String> {
    let models_dir = get_models_dir();

    // Create models directory if it doesn't exist
    if !models_dir.exists() {
        std::fs::create_dir_all(&models_dir)
            .map_err(|e| format!("Failed to create models dir: {}", e))?;
    }

    // Validate filename — no path traversal (P6: Secrets Stay Secret)
    if filename.contains("..") || filename.contains('/') || filename.contains('\\') {
        return Err(format!("Invalid filename '{}' — must not contain path separators or '..'", filename));
    }

    let file_path = models_dir.join(&filename);

    // SSRF protection — block internal/private URLs (P6)
    crate::content_security::validate_url_ssrf(&url)?;

    let client = hive_http_client()?;
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !response.status().is_success() {
        append_to_app_log(&format!("DOWNLOAD | failed | {} | status={}", filename, response.status()));
        return Err(format!("Download failed with status: {}", response.status()));
    }

    // Get content length
    let total_size = response.content_length().unwrap_or(0);
    append_to_app_log(&format!("DOWNLOAD | started | {} | size={:.1}MB", filename, total_size as f64 / 1_048_576.0));

    // Open file for writing
    let mut file = std::fs::File::create(&file_path)
        .map_err(|e| format!("Failed to create file: {}", e))?;

    // Stream the response body
    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();
    let mut last_emit = std::time::Instant::now();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Download error: {}", e))?;
        file.write_all(&chunk)
            .map_err(|e| format!("Write error: {}", e))?;

        downloaded += chunk.len() as u64;

        // Emit progress event every 100ms to avoid overwhelming the frontend
        if last_emit.elapsed().as_millis() >= 100 || downloaded == total_size {
            let percentage = if total_size > 0 {
                (downloaded as f64 / total_size as f64) * 100.0
            } else {
                0.0
            };

            let progress = DownloadProgress {
                downloaded,
                total: total_size,
                percentage,
                filename: filename.clone(),
            };

            // Emit event to frontend
            let _ = app.emit("download-progress", &progress);
            last_emit = std::time::Instant::now();
        }
    }

    append_to_app_log(&format!("DOWNLOAD | completed | {} | {:.1}MB", filename, downloaded as f64 / 1_048_576.0));
    Ok(file_path.to_string_lossy().to_string())
}

/// Get file size from URL (HEAD request)
#[tauri::command]
pub async fn get_remote_file_size(url: String) -> Result<u64, String> {
    crate::content_security::validate_url_ssrf(&url)?;
    let client = hive_http_client()?;
    let response = client
        .head(&url)
        .send()
        .await
        .map_err(|e| format!("HEAD request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("HEAD request failed with status: {}", response.status()));
    }

    Ok(response.content_length().unwrap_or(0))
}

/// Download a model file directly to WSL filesystem (for WSL backend)
#[tauri::command]
pub async fn download_model_wsl(
    app: tauri::AppHandle,
    url: String,
    filename: String,
) -> Result<String, String> {
    // Validate filename — no path traversal (P6: Secrets Stay Secret)
    if filename.contains("..") || filename.contains('/') || filename.contains('\\') {
        return Err(format!("Invalid filename '{}' — must not contain path separators or '..'", filename));
    }

    // SSRF protection — block internal/private URLs (P6)
    crate::content_security::validate_url_ssrf(&url)?;

    // Create models directory in WSL if it doesn't exist
    let mkdir_cmd = "mkdir -p \"$HOME/models\"";
    let mkdir_result = wsl_cmd()
        .args(["-e", "bash", "-c", mkdir_cmd])
        .output()
        .map_err(|e| format!("Failed to create WSL models directory: {}", e))?;

    if !mkdir_result.status.success() {
        return Err(format!(
            "Failed to create WSL models directory: {}",
            String::from_utf8_lossy(&mkdir_result.stderr)
        ));
    }

    // Get total file size first (for progress reporting)
    let client = hive_http_client()?;
    let head_response = client
        .head(&url)
        .send()
        .await
        .map_err(|e| format!("HEAD request failed: {}", e))?;

    let total_size = head_response.content_length().unwrap_or(0);
    append_to_app_log(&format!("DOWNLOAD | started_wsl | {} | size={:.1}MB", filename, total_size as f64 / 1_048_576.0));

    // Expand $HOME to actual path for file size checking
    let home_output = wsl_cmd()
        .args(["-e", "bash", "-c", "echo $HOME"])
        .output()
        .map_err(|e| format!("Failed to get WSL home directory: {}", e))?;

    let wsl_home = String::from_utf8_lossy(&home_output.stdout).trim().to_string();
    let absolute_file_path = format!("{}/models/{}", wsl_home, filename);

    // Start curl download in WSL (P6: shell-escape paths and URLs)
    let curl_cmd = format!(
        "curl -L -A 'HIVE-Desktop/1.0' --connect-timeout 30 --max-time 7200 -o '{}' '{}'",
        crate::wsl::shell_escape(&absolute_file_path),
        crate::wsl::shell_escape(&url),
    );

    let mut child = wsl_cmd()
        .args(["-e", "bash", "-c", &curl_cmd])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start download: {}", e))?;

    // Poll file size for progress updates
    let filename_clone = filename.clone();
    let poll_interval = std::time::Duration::from_millis(250);
    let mut last_emit = std::time::Instant::now();

    loop {
        // Check if process is still running
        match child.try_wait() {
            Ok(Some(status)) => {
                // Process finished
                if !status.success() {
                    let mut error_msg = "Download failed in WSL".to_string();
                    if let Some(mut stderr) = child.stderr.take() {
                        let mut err_output = String::new();
                        if std::io::Read::read_to_string(&mut stderr, &mut err_output).is_ok() && !err_output.is_empty() {
                            error_msg = format!("Download failed: {}", err_output.trim());
                        }
                    }
                    return Err(error_msg);
                }
                // Emit final progress
                let progress = DownloadProgress {
                    downloaded: total_size,
                    total: total_size,
                    percentage: 100.0,
                    filename: filename_clone.clone(),
                };
                let _ = app.emit("download-progress", &progress);
                break;
            }
            Ok(None) => {
                // Still running - check file size and emit progress
                if last_emit.elapsed() >= poll_interval {
                    let size_cmd = format!("stat -c%s '{}' 2>/dev/null || echo 0", absolute_file_path);
                    if let Ok(size_output) = wsl_cmd()
                        .args(["-e", "bash", "-c", &size_cmd])
                        .output()
                    {
                        let downloaded: u64 = String::from_utf8_lossy(&size_output.stdout)
                            .trim()
                            .parse()
                            .unwrap_or(0);

                        let percentage = if total_size > 0 {
                            (downloaded as f64 / total_size as f64) * 100.0
                        } else {
                            0.0
                        };

                        let progress = DownloadProgress {
                            downloaded,
                            total: total_size,
                            percentage,
                            filename: filename_clone.clone(),
                        };
                        let _ = app.emit("download-progress", &progress);
                    }
                    last_emit = std::time::Instant::now();
                }
                // Small sleep to avoid busy-waiting
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
            Err(e) => {
                return Err(format!("Error checking download status: {}", e));
            }
        }
    }

    // Verify the file exists and return the WSL path
    let verify_cmd = format!("test -f '{}' && echo 'ok'", absolute_file_path);
    let verify_output = wsl_cmd()
        .args(["-e", "bash", "-c", &verify_cmd])
        .output()
        .map_err(|e| format!("Failed to verify download: {}", e))?;

    if String::from_utf8_lossy(&verify_output.stdout).trim() != "ok" {
        append_to_app_log(&format!("DOWNLOAD | wsl_verify_failed | {}", filename));
        return Err("Download completed but file not found".to_string());
    }

    append_to_app_log(&format!("DOWNLOAD | completed_wsl | {} | {}", filename, absolute_file_path));
    Ok(absolute_file_path)
}

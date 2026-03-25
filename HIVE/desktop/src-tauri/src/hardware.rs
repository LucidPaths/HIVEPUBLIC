//! Hardware detection — GPU, CPU, RAM, and dependency checks

use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

use crate::paths::get_bin_dir;
use crate::types::*;
use crate::wsl::check_wsl;

/// Create a Command that hides the console window on Windows.
/// Prevents CMD/PowerShell windows from flashing on screen.
fn hidden_cmd(program: &str) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

/// Find llama-server.exe on Windows, returns path if found
pub fn find_windows_llama_server() -> Option<String> {
    // Get the directory where HIVE.exe is located
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    // Check multiple locations in priority order
    let search_paths = [
        exe_dir.join("llama-server.exe"),                    // Next to HIVE.exe
        exe_dir.join("bin").join("llama-server.exe"),        // bin subfolder next to HIVE.exe
        get_bin_dir().join("llama-server.exe"),              // %LocalAppData%/HIVE/bin/
    ];

    for path in &search_paths {
        if path.exists() {
            return Some(path.to_string_lossy().to_string());
        }
    }

    None
}

/// Check if CUDA is available on Windows (via nvidia-smi)
pub fn check_windows_cuda() -> bool {
    hidden_cmd("nvidia-smi")
        .arg("--query-gpu=driver_version")
        .arg("--format=csv,noheader")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Estimate VRAM from GPU name when detection fails (WMI 4GB cap, missing drivers, etc.)
/// Adapted from llmfit (MIT) — https://github.com/AlexsJones/llmfit
fn estimate_vram_from_name(name: &str) -> f64 {
    let n = name.to_lowercase();
    // NVIDIA RTX 50 series
    if n.contains("5090") { return 32.0; }
    if n.contains("5080") { return 16.0; }
    if n.contains("5070 ti") { return 16.0; }
    if n.contains("5070") { return 12.0; }
    if n.contains("5060 ti") { return 16.0; }
    if n.contains("5060") { return 8.0; }
    // NVIDIA RTX 40 series
    if n.contains("4090") { return 24.0; }
    if n.contains("4080 super") { return 16.0; }
    if n.contains("4080") { return 16.0; }
    if n.contains("4070 ti super") { return 16.0; }
    if n.contains("4070 ti") { return 12.0; }
    if n.contains("4070 super") { return 12.0; }
    if n.contains("4070") { return 12.0; }
    if n.contains("4060 ti") { return 16.0; }
    if n.contains("4060") { return 8.0; }
    // NVIDIA RTX 30 series
    if n.contains("3090") { return 24.0; }
    if n.contains("3080 ti") { return 12.0; }
    if n.contains("3080") { return 10.0; }
    if n.contains("3070 ti") { return 8.0; }
    if n.contains("3070") { return 8.0; }
    if n.contains("3060 ti") { return 8.0; }
    if n.contains("3060") { return 12.0; }
    // NVIDIA RTX 20 series
    if n.contains("2080 ti") { return 11.0; }
    if n.contains("2080 super") { return 8.0; }
    if n.contains("2080") { return 8.0; }
    if n.contains("2070 super") { return 8.0; }
    if n.contains("2070") { return 8.0; }
    if n.contains("2060 super") { return 8.0; }
    if n.contains("2060") { return 6.0; }
    // NVIDIA data center / workstation
    if n.contains("h100") { return 80.0; }
    if n.contains("a100") { return 80.0; }
    if n.contains("l40") { return 48.0; }
    if n.contains("a6000") { return 48.0; }
    if n.contains("a10") { return 24.0; }
    if n.contains("t4") { return 16.0; }
    // AMD RX 9000 (RDNA 4)
    if n.contains("9070 xt") { return 16.0; }
    if n.contains("9070") { return 12.0; }
    // AMD RX 7000 (RDNA 3)
    if n.contains("7900 xtx") { return 24.0; }
    if n.contains("7900 xt") { return 20.0; }
    if n.contains("7900 gre") { return 16.0; }
    if n.contains("7800 xt") { return 16.0; }
    if n.contains("7700 xt") { return 12.0; }
    if n.contains("7600") { return 8.0; }
    // AMD RX 6000 (RDNA 2)
    if n.contains("6950") { return 16.0; }
    if n.contains("6900") { return 16.0; }
    if n.contains("6800") { return 16.0; }
    if n.contains("6750") { return 12.0; }
    if n.contains("6700") { return 12.0; }
    if n.contains("6650") { return 8.0; }
    if n.contains("6600") { return 8.0; }
    // AMD Instinct (data center)
    if n.contains("mi300") { return 192.0; }
    if n.contains("mi250") { return 128.0; }
    if n.contains("mi210") { return 64.0; }
    if n.contains("mi100") { return 32.0; }
    // Generic fallbacks
    if n.contains("rtx") { return 8.0; }
    if n.contains("gtx") { return 4.0; }
    if n.contains("radeon") || n.contains("rx") { return 8.0; }
    0.0
}

/// Detect GPUs using PowerShell with registry fallback for accurate VRAM
#[tauri::command]
pub fn detect_gpus() -> Vec<GpuInfo> {
    let mut gpus = Vec::new();

    // Use PowerShell with registry for accurate VRAM (WMI AdapterRAM is 32-bit limited to 4GB)
    let ps_script = r#"
        $gpuList = @()

        # Get GPU info from WMI
        $wmiGpus = Get-CimInstance -ClassName Win32_VideoController

        # Get VRAM from registry (64-bit accurate values)
        $regPath = "HKLM:\SYSTEM\ControlSet001\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}"
        $regKeys = Get-ChildItem -Path $regPath -ErrorAction SilentlyContinue | Where-Object { $_.PSChildName -match "^\d+$" }

        $regVram = @{}
        foreach ($key in $regKeys) {
            try {
                $props = Get-ItemProperty -Path $key.PSPath -ErrorAction SilentlyContinue
                $name = $props."DriverDesc"
                $vram = $props."HardwareInformation.qwMemorySize"
                if ($name -and $vram) {
                    $regVram[$name] = $vram
                }
            } catch {}
        }

        foreach ($gpu in $wmiGpus) {
            $vram = $gpu.AdapterRAM
            # If VRAM is 4GB or less (might be truncated), try registry
            if ($vram -le 4294967295 -and $regVram.ContainsKey($gpu.Name)) {
                $vram = $regVram[$gpu.Name]
            }

            $gpuList += [PSCustomObject]@{
                Name = $gpu.Name
                AdapterRAM = $vram
                DriverVersion = $gpu.DriverVersion
            }
        }

        $gpuList | ConvertTo-Json -Compress
    "#;

    if let Ok(output) = hidden_cmd("powershell")
        .args(["-NoProfile", "-Command", ps_script])
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);

        // PowerShell returns single object without array brackets, or array for multiple
        let json_str = stdout.trim();
        if json_str.is_empty() {
            return gpus;
        }

        // Try parsing as array first, then as single object
        let items: Vec<serde_json::Value> = if json_str.starts_with('[') {
            serde_json::from_str(json_str).unwrap_or_default()
        } else {
            // Single GPU - wrap in array
            match serde_json::from_str::<serde_json::Value>(json_str) {
                Ok(v) => vec![v],
                Err(_) => vec![],
            }
        };

        for item in items {
            let name = item.get("Name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            // AdapterRAM - now should be accurate from registry
            let vram_bytes: u64 = item.get("AdapterRAM")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            let driver = item.get("DriverVersion")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let vendor = if name.to_uppercase().contains("NVIDIA") {
                "NVIDIA"
            } else if name.to_uppercase().contains("AMD") || name.to_uppercase().contains("RADEON") {
                "AMD"
            } else if name.to_uppercase().contains("INTEL") {
                "Intel"
            } else {
                "Unknown"
            };

            if !name.is_empty() {
                gpus.push(GpuInfo {
                    vendor: vendor.to_string(),
                    name,
                    vram_mb: vram_bytes / 1024 / 1024,
                    driver_version: driver,
                });
            }
        }
    }

    // Apply name-based VRAM fallback for any GPU with 0 VRAM (detection completely failed)
    for gpu in &mut gpus {
        if gpu.vram_mb == 0 {
            let estimated = estimate_vram_from_name(&gpu.name);
            if estimated > 0.0 {
                gpu.vram_mb = (estimated * 1024.0) as u64;
            }
        }
    }

    // If PowerShell failed, try WMIC as fallback
    if gpus.is_empty() {
        if let Ok(output) = hidden_cmd("wmic")
            .args(["path", "win32_VideoController", "get", "Name,AdapterRAM,DriverVersion", "/format:csv"])
            .output()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines().skip(1) {
                if line.trim().is_empty() {
                    continue;
                }
                // CSV format: Node,AdapterRAM,DriverVersion,Name (alphabetical after Node)
                let parts: Vec<&str> = line.split(',').collect();
                if parts.len() >= 4 {
                    let vram_bytes: u64 = parts.get(1).and_then(|s| s.trim().parse().ok()).unwrap_or(0);
                    let driver = parts.get(2).map(|s| s.trim().to_string()).unwrap_or_default();
                    let name = parts.get(3).map(|s| s.trim().to_string()).unwrap_or_default();

                    let vendor = if name.to_uppercase().contains("NVIDIA") {
                        "NVIDIA"
                    } else if name.to_uppercase().contains("AMD") || name.to_uppercase().contains("RADEON") {
                        "AMD"
                    } else if name.to_uppercase().contains("INTEL") {
                        "Intel"
                    } else {
                        "Unknown"
                    };

                    if !name.is_empty() {
                        gpus.push(GpuInfo {
                            vendor: vendor.to_string(),
                            name,
                            vram_mb: vram_bytes / 1024 / 1024,
                            driver_version: driver,
                        });
                    }
                }
            }
        }

        // Apply name-based fallback for WMIC path too (WMI has 32-bit AdapterRAM cap)
        for gpu in &mut gpus {
            let estimated = estimate_vram_from_name(&gpu.name);
            if gpu.vram_mb == 0 && estimated > 0.0 {
                gpu.vram_mb = (estimated * 1024.0) as u64;
            } else if (gpu.vram_mb as f64) <= 4096.0 && estimated > 4.1 {
                // WMI 32-bit cap: reports ≤4GB for GPUs that actually have more
                gpu.vram_mb = (estimated * 1024.0) as u64;
            }
        }
    }

    gpus
}

/// Detect CPU information (used internally by get_system_info)
pub fn detect_cpu() -> Option<CpuInfo> {
    let ps_script = r#"
        $cpu = Get-CimInstance -ClassName Win32_Processor | Select-Object -First 1
        [PSCustomObject]@{
            Name = $cpu.Name
            Cores = $cpu.NumberOfCores
            Threads = $cpu.NumberOfLogicalProcessors
        } | ConvertTo-Json -Compress
    "#;

    if let Ok(output) = hidden_cmd("powershell")
        .args(["-NoProfile", "-Command", ps_script])
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(stdout.trim()) {
            let name = json.get("Name")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown CPU")
                .trim()
                .to_string();
            let cores = json.get("Cores")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            let threads = json.get("Threads")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;

            return Some(CpuInfo { name, cores, threads });
        }
    }

    None
}

/// Detect RAM information (used internally by get_system_info)
pub fn detect_ram() -> Option<RamInfo> {
    let ps_script = r#"
        $ram = (Get-CimInstance -ClassName Win32_ComputerSystem).TotalPhysicalMemory
        [PSCustomObject]@{
            TotalBytes = $ram
        } | ConvertTo-Json -Compress
    "#;

    if let Ok(output) = hidden_cmd("powershell")
        .args(["-NoProfile", "-Command", ps_script])
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(stdout.trim()) {
            let total_bytes = json.get("TotalBytes")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            let total_mb = total_bytes / 1024 / 1024;
            let total_gb = total_mb as f64 / 1024.0;

            return Some(RamInfo { total_mb, total_gb });
        }
    }

    None
}

/// Get full system info including WSL status
#[tauri::command]
pub fn get_system_info() -> SystemInfo {
    let gpus = detect_gpus();
    let cpu = detect_cpu();
    let ram = detect_ram();
    let wsl_status = check_wsl();

    // Determine recommended backend
    let has_amd = gpus.iter().any(|g| g.vendor == "AMD");
    let has_nvidia = gpus.iter().any(|g| g.vendor == "NVIDIA");

    let recommended_backend = if has_amd && wsl_status.installed && wsl_status.rocm_version.is_some() {
        "wsl"
    } else if has_nvidia {
        "windows"
    } else if wsl_status.installed && wsl_status.rocm_version.is_some() {
        "wsl"
    } else {
        "windows"
    };

    SystemInfo {
        gpus,
        cpu,
        ram,
        wsl_available: wsl_status.installed,
        wsl_distro: wsl_status.distro,
        recommended_backend: recommended_backend.to_string(),
    }
}

/// Check all dependencies and return comprehensive status
#[tauri::command]
pub fn check_dependencies() -> DependencyStatus {
    let gpus = detect_gpus();
    let wsl = check_wsl();

    // Determine recommended backend based on GPU
    let has_amd = gpus.iter().any(|g| g.vendor == "AMD");
    let has_nvidia = gpus.iter().any(|g| g.vendor == "NVIDIA");

    let recommended_backend = if has_amd && wsl.installed {
        "wsl".to_string()
    } else if has_nvidia {
        "windows".to_string()
    } else {
        "windows".to_string() // Default to Windows for CPU-only
    };

    // Check Windows dependencies
    let windows_llama_server = find_windows_llama_server();
    let cuda_available = check_windows_cuda();

    // Build missing deps list
    let mut missing_deps = Vec::new();

    if recommended_backend == "windows" {
        if windows_llama_server.is_none() {
            missing_deps.push("llama-server.exe (Windows)".to_string());
        }
        if has_nvidia && !cuda_available {
            missing_deps.push("NVIDIA CUDA drivers".to_string());
        }
    } else {
        // WSL/AMD backend
        if !wsl.installed {
            missing_deps.push("WSL2 (Windows Subsystem for Linux)".to_string());
        } else {
            if wsl.llama_server_path.is_none() {
                missing_deps.push("llama-server (in WSL)".to_string());
            }
            if has_amd && wsl.rocm_version.is_none() {
                missing_deps.push("ROCm (in WSL)".to_string());
            }
        }
    }

    let ready_to_run = missing_deps.is_empty();

    DependencyStatus {
        windows_llama_server,
        cuda_available,
        wsl_installed: wsl.installed,
        wsl_distro: wsl.distro,
        wsl_llama_server: wsl.llama_server_path,
        rocm_available: wsl.rocm_version.is_some(),
        rocm_version: wsl.rocm_version,
        recommended_backend,
        ready_to_run,
        missing_deps,
    }
}

/// Download llama-server for Windows from GitHub releases
#[tauri::command]
pub async fn download_llama_server(app: tauri::AppHandle) -> Result<String, String> {
    use futures_util::StreamExt;
    use std::io::Write;

    let bin_dir = get_bin_dir();

    // Create bin directory if it doesn't exist
    if !bin_dir.exists() {
        std::fs::create_dir_all(&bin_dir)
            .map_err(|e| format!("Failed to create bin dir: {}", e))?;
    }

    // llama.cpp releases URL - using CUDA build for NVIDIA support
    let release_url = "https://github.com/ggerganov/llama.cpp/releases/latest";

    let client = crate::http_client::hive_http_client()?;

    // Get the redirect to find latest version
    let response = client
        .get(release_url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch release info: {}", e))?;

    let final_url = response.url().to_string();
    let version = final_url
        .rsplit('/')
        .next()
        .unwrap_or("latest")
        .to_string();

    // Validate version string — prevent path traversal in constructed URLs (P6)
    if version.contains("..") || version.contains('/') || version.contains('\\') || version.is_empty() {
        return Err(format!("Invalid version '{}' extracted from GitHub redirect", version));
    }

    // Construct download URL for Windows CUDA build
    // NOTE: Update CUDA_VERSION when llama.cpp ships new CUDA builds (P7: Framework Survives)
    const CUDA_VERSION: &str = "cu12.2.0";
    let download_url = format!(
        "https://github.com/ggerganov/llama.cpp/releases/download/{}/llama-{}-bin-win-cuda-{}-x64.zip",
        version, version, CUDA_VERSION
    );

    // SSRF validation on constructed URLs (P6: defense-in-depth)
    crate::content_security::validate_url_ssrf(&download_url)?;

    // Download the zip file
    let zip_path = bin_dir.join("llama-server.zip");

    let response = client
        .get(&download_url)
        .send()
        .await
        .map_err(|e| format!("Failed to download llama-server: {}", e))?;

    if !response.status().is_success() {
        // CUDA build not found — fall back to CPU-only (may mean CUDA_VERSION needs updating)
        eprintln!("[HIVE] DOWNLOAD | CUDA build not found ({}), falling back to CPU-only build", CUDA_VERSION);
        let fallback_url = format!(
            "https://github.com/ggerganov/llama.cpp/releases/download/{}/llama-{}-bin-win-noavx-x64.zip",
            version, version
        );

        // SSRF validation on fallback URL (P6)
        crate::content_security::validate_url_ssrf(&fallback_url)?;

        let response = client
            .get(&fallback_url)
            .send()
            .await
            .map_err(|e| format!("Failed to download llama-server (fallback): {}", e))?;

        if !response.status().is_success() {
            return Err(format!("Download failed: HTTP {}", response.status()));
        }

        // Stream the fallback download
        let total_size = response.content_length().unwrap_or(0);
        let mut downloaded: u64 = 0;
        let mut file = std::fs::File::create(&zip_path)
            .map_err(|e| format!("Failed to create zip file: {}", e))?;

        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| format!("Download error: {}", e))?;
            file.write_all(&chunk)
                .map_err(|e| format!("Write error: {}", e))?;
            downloaded += chunk.len() as u64;

            let progress = crate::types::DownloadProgress {
                downloaded,
                total: total_size,
                percentage: if total_size > 0 { (downloaded as f64 / total_size as f64) * 100.0 } else { 0.0 },
                filename: "llama-server.zip".to_string(),
            };
            let _ = tauri::Emitter::emit(&app, "download-progress", &progress);
        }
    } else {
        // Stream the download
        let total_size = response.content_length().unwrap_or(0);
        let mut downloaded: u64 = 0;
        let mut file = std::fs::File::create(&zip_path)
            .map_err(|e| format!("Failed to create zip file: {}", e))?;

        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| format!("Download error: {}", e))?;
            file.write_all(&chunk)
                .map_err(|e| format!("Write error: {}", e))?;
            downloaded += chunk.len() as u64;

            let progress = crate::types::DownloadProgress {
                downloaded,
                total: total_size,
                percentage: if total_size > 0 { (downloaded as f64 / total_size as f64) * 100.0 } else { 0.0 },
                filename: "llama-server.zip".to_string(),
            };
            let _ = tauri::Emitter::emit(&app, "download-progress", &progress);
        }
    }

    // Extract llama-server.exe from zip using PowerShell
    // Paths are HIVE-controlled (get_bin_dir), but validate anyway (P6 defense-in-depth)
    let zip_str = zip_path.to_string_lossy();
    let bin_str = bin_dir.to_string_lossy();
    for (label, val) in [("zip_path", &zip_str), ("bin_dir", &bin_str)] {
        if val.contains('\0') || val.contains('\n') || val.contains('\r') {
            return Err(format!("Invalid {} — contains null or newline characters", label));
        }
    }
    let extract_script = format!(
        r#"
        $zip = '{}'
        $dest = '{}'
        Expand-Archive -Path $zip -DestinationPath $dest -Force
        # Find and move llama-server.exe to bin dir
        $server = Get-ChildItem -Path $dest -Recurse -Filter 'llama-server.exe' | Select-Object -First 1
        if ($server) {{
            Move-Item -Path $server.FullName -Destination (Join-Path $dest 'llama-server.exe') -Force
        }}
        # Cleanup extracted folders, keep only llama-server.exe
        Get-ChildItem -Path $dest -Directory | Remove-Item -Recurse -Force
        Remove-Item -Path $zip -Force
        "#,
        zip_str.replace('\'', "''"),
        bin_str.replace('\'', "''")
    );

    let extract_result = hidden_cmd("powershell")
        .args(["-NoProfile", "-Command", &extract_script])
        .output()
        .map_err(|e| format!("Failed to extract zip: {}", e))?;

    if !extract_result.status.success() {
        let stderr = String::from_utf8_lossy(&extract_result.stderr);
        return Err(format!("Extraction failed: {}", stderr));
    }

    // Verify the file exists
    let server_path = bin_dir.join("llama-server.exe");
    if server_path.exists() {
        Ok(server_path.to_string_lossy().to_string())
    } else {
        Err("llama-server.exe not found after extraction".to_string())
    }
}

/// Get live GPU VRAM and system RAM usage.
/// Called event-driven: once at startup, and again when starting/stopping a local model.
/// NOT called per chat turn — the frontend caches the result in React state.
///
/// NVIDIA: Uses nvidia-smi CSV output (fast, ~50ms)
/// AMD: Uses rocm-smi via WSL (fallback, ~200ms)
/// RAM: Uses PowerShell Win32_OperatingSystem (fast, ~100ms)
///
/// All fields are Optional — graceful degradation if any query fails (P4).
#[tauri::command]
pub fn get_live_resource_usage() -> LiveResourceMetrics {
    let mut metrics = LiveResourceMetrics {
        vram_used_mb: None,
        vram_free_mb: None,
        vram_total_mb: None,
        ram_available_mb: None,
        ram_used_mb: None,
        gpu_utilization: None,
        gpu_vendor: "none".to_string(),
    };

    // === NVIDIA GPU metrics via nvidia-smi ===
    if let Ok(output) = hidden_cmd("nvidia-smi")
        .args([
            "--query-gpu=memory.used,memory.free,memory.total,utilization.gpu",
            "--format=csv,noheader,nounits",
        ])
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Format: "used_mb, free_mb, total_mb, util%"  (one line per GPU, we take first)
            if let Some(line) = stdout.lines().next() {
                let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
                if parts.len() >= 4 {
                    metrics.vram_used_mb = parts[0].parse().ok();
                    metrics.vram_free_mb = parts[1].parse().ok();
                    metrics.vram_total_mb = parts[2].parse().ok();
                    metrics.gpu_utilization = parts[3].parse().ok();
                    metrics.gpu_vendor = "nvidia".to_string();
                }
            }
        }
    }

    // === AMD GPU metrics via rocm-smi (WSL fallback) ===
    if metrics.gpu_vendor == "none" {
        if let Ok(output) = hidden_cmd("wsl")
            .args(["--", "rocm-smi", "--showmeminfo", "vram", "--csv"])
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                // rocm-smi CSV: headers then "GPU, VRAM Total Used (B), VRAM Total (B)"
                for line in stdout.lines().skip(1) {
                    let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
                    if parts.len() >= 3 {
                        let used_bytes: u64 = parts[1].parse().unwrap_or(0);
                        let total_bytes: u64 = parts[2].parse().unwrap_or(0);
                        if total_bytes > 0 {
                            metrics.vram_used_mb = Some(used_bytes / 1024 / 1024);
                            metrics.vram_total_mb = Some(total_bytes / 1024 / 1024);
                            metrics.vram_free_mb = Some((total_bytes - used_bytes) / 1024 / 1024);
                            metrics.gpu_vendor = "amd".to_string();
                        }
                        break; // First GPU only
                    }
                }
            }
        }
    }

    // === Live RAM usage via PowerShell ===
    let ram_script = r#"
        $os = Get-CimInstance -ClassName Win32_OperatingSystem
        [PSCustomObject]@{
            FreePhysicalMemoryKB = $os.FreePhysicalMemory
            TotalVisibleMemoryKB = $os.TotalVisibleMemorySize
        } | ConvertTo-Json -Compress
    "#;

    if let Ok(output) = hidden_cmd("powershell")
        .args(["-NoProfile", "-Command", ram_script])
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(stdout.trim()) {
                let free_kb = json.get("FreePhysicalMemoryKB")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let total_kb = json.get("TotalVisibleMemoryKB")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);

                if total_kb > 0 {
                    metrics.ram_available_mb = Some(free_kb / 1024);
                    metrics.ram_used_mb = Some((total_kb - free_kb) / 1024);
                }
            }
        }
    }

    metrics
}

/// Get the path where llama-server should be installed
#[tauri::command]
pub fn get_llama_server_install_path() -> String {
    get_bin_dir().join("llama-server.exe").to_string_lossy().to_string()
}

//! GGUF file parsing and VRAM estimation

use crate::types::{GgufMetadata, VramEstimate, VramCompatibility};

/// GGUF file_type enum values to quantization names
/// Based on llama.cpp ggml-common.h
fn file_type_to_quantization(file_type: u32) -> String {
    match file_type {
        0 => "F32".to_string(),
        1 => "F16".to_string(),
        2 => "Q4_0".to_string(),
        3 => "Q4_1".to_string(),
        6 => "Q5_0".to_string(),
        7 => "Q5_1".to_string(),
        8 => "Q8_0".to_string(),
        9 => "Q8_1".to_string(),
        10 => "Q2_K".to_string(),
        11 => "Q3_K_S".to_string(),
        12 => "Q3_K_M".to_string(),
        13 => "Q3_K_L".to_string(),
        14 => "Q4_K_S".to_string(),
        15 => "Q4_K_M".to_string(),
        16 => "Q5_K_S".to_string(),
        17 => "Q5_K_M".to_string(),
        18 => "Q6_K".to_string(),
        19 => "IQ2_XXS".to_string(),
        20 => "IQ2_XS".to_string(),
        21 => "IQ3_XXS".to_string(),
        22 => "IQ1_S".to_string(),
        23 => "IQ4_NL".to_string(),
        24 => "IQ3_S".to_string(),
        25 => "IQ2_S".to_string(),
        26 => "IQ4_XS".to_string(),
        27 => "IQ1_M".to_string(),
        28 => "BF16".to_string(),
        _ => format!("UNKNOWN_{}", file_type),
    }
}

/// Bits per weight for each quantization type
fn quantization_bits_per_weight(quant: &str) -> f64 {
    let quant_upper = quant.to_uppercase();
    match quant_upper.as_str() {
        "F32" => 32.0,
        "F16" | "BF16" => 16.0,
        "Q8_0" | "Q8_1" | "Q8_K" => 8.0,
        "Q6_K" => 6.5625,
        "Q5_0" | "Q5_1" | "Q5_K" | "Q5_K_S" | "Q5_K_M" => 5.5,
        "Q4_0" | "Q4_1" | "Q4_K" | "Q4_K_S" | "Q4_K_M" | "IQ4_NL" | "IQ4_XS" => 4.5,
        "Q3_K" | "Q3_K_S" | "Q3_K_M" | "Q3_K_L" | "IQ3_S" | "IQ3_XXS" => 3.4375,
        "Q2_K" | "IQ2_S" | "IQ2_XS" | "IQ2_XXS" => 2.625,
        "IQ1_S" | "IQ1_M" => 1.75,
        _ => 4.5, // Default to Q4 if unknown
    }
}

/// Extract quantization type from filename
fn extract_quant_from_filename(filename: &str) -> Option<String> {
    let filename_upper = filename.to_uppercase();

    let patterns = [
        "Q8_0", "Q8_1", "Q8_K",
        "Q6_K",
        "Q5_K_M", "Q5_K_S", "Q5_K", "Q5_0", "Q5_1",
        "Q4_K_M", "Q4_K_S", "Q4_K", "Q4_0", "Q4_1",
        "Q3_K_L", "Q3_K_M", "Q3_K_S", "Q3_K",
        "Q2_K",
        "IQ4_XS", "IQ4_NL",
        "IQ3_XXS", "IQ3_S",
        "IQ2_XXS", "IQ2_XS", "IQ2_S",
        "IQ1_M", "IQ1_S",
        "F16", "FP16", "BF16", "F32", "FP32",
    ];

    for pattern in patterns {
        if filename_upper.contains(pattern) {
            let normalized = pattern
                .replace("FP16", "F16")
                .replace("FP32", "F32");
            return Some(normalized);
        }
    }

    None
}

/// Extract approximate parameter count from filename (e.g., "7B", "14B", "70B")
fn extract_params_from_filename(filename: &str) -> Option<u64> {
    let filename_upper = filename.to_uppercase();
    let chars: Vec<char> = filename_upper.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == 'B' && i > 0 {
            let end = i;
            let mut start = i - 1;

            while start > 0 && (chars[start].is_ascii_digit() || chars[start] == '.') {
                start -= 1;
            }
            if !chars[start].is_ascii_digit() && chars[start] != '.' {
                start += 1;
            }

            if start < end {
                let num_str: String = chars[start..end].iter().collect();
                if let Ok(num) = num_str.parse::<f64>() {
                    return Some((num * 1_000_000_000.0) as u64);
                }
            }
        }
        i += 1;
    }

    None
}

/// Parse GGUF file header to extract metadata
/// This reads only the header and metadata, not the tensor data
pub fn parse_gguf_header(path: &str) -> Result<GgufMetadata, String> {
    use std::io::{Read, Seek, SeekFrom};

    let mut file = std::fs::File::open(path)
        .map_err(|e| format!("Failed to open file: {}", e))?;

    let file_size = file.metadata()
        .map_err(|e| format!("Failed to get file metadata: {}", e))?
        .len();

    // Read magic number (4 bytes)
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic)
        .map_err(|e| format!("Failed to read magic: {}", e))?;

    if &magic != b"GGUF" {
        return Err("Not a valid GGUF file (invalid magic)".to_string());
    }

    // Read version (4 bytes, little-endian u32)
    let mut version_bytes = [0u8; 4];
    file.read_exact(&mut version_bytes)
        .map_err(|e| format!("Failed to read version: {}", e))?;
    let _version = u32::from_le_bytes(version_bytes);

    // Read tensor count (8 bytes, little-endian u64)
    let mut tensor_count_bytes = [0u8; 8];
    file.read_exact(&mut tensor_count_bytes)
        .map_err(|e| format!("Failed to read tensor count: {}", e))?;
    let _tensor_count = u64::from_le_bytes(tensor_count_bytes);

    // Read metadata KV count (8 bytes, little-endian u64)
    let mut kv_count_bytes = [0u8; 8];
    file.read_exact(&mut kv_count_bytes)
        .map_err(|e| format!("Failed to read KV count: {}", e))?;
    let kv_count = u64::from_le_bytes(kv_count_bytes);

    let mut metadata = GgufMetadata {
        architecture: None,
        name: None,
        parameter_count: None,
        quantization: None,
        file_type: None,
        context_length: None,
        embedding_length: None,
        block_count: None,
        head_count: None,
        head_count_kv: None,
        expert_count: None,
        expert_used_count: None,
        file_size_bytes: file_size,
    };

    // Parse metadata key-value pairs
    for _ in 0..kv_count.min(1000) {
        let mut key_len_bytes = [0u8; 8];
        if file.read_exact(&mut key_len_bytes).is_err() {
            break;
        }
        let key_len = u64::from_le_bytes(key_len_bytes) as usize;

        if key_len > 1024 {
            break;
        }

        let mut key_bytes = vec![0u8; key_len];
        if file.read_exact(&mut key_bytes).is_err() {
            break;
        }
        let key = String::from_utf8_lossy(&key_bytes).to_string();

        let mut value_type_bytes = [0u8; 4];
        if file.read_exact(&mut value_type_bytes).is_err() {
            break;
        }
        let value_type = u32::from_le_bytes(value_type_bytes);

        match value_type {
            // UINT32
            4 => {
                let mut val_bytes = [0u8; 4];
                if file.read_exact(&mut val_bytes).is_err() {
                    break;
                }
                let val = u32::from_le_bytes(val_bytes);

                if key == "general.file_type" {
                    metadata.file_type = Some(val);
                    metadata.quantization = Some(file_type_to_quantization(val));
                }
                if key.ends_with(".context_length") {
                    metadata.context_length = Some(val as u64);
                } else if key.ends_with(".embedding_length") {
                    metadata.embedding_length = Some(val as u64);
                } else if key.ends_with(".block_count") {
                    metadata.block_count = Some(val as u64);
                } else if key.ends_with(".attention.head_count") {
                    metadata.head_count = Some(val as u64);
                } else if key.ends_with(".attention.head_count_kv") {
                    metadata.head_count_kv = Some(val as u64);
                } else if key.ends_with(".expert_count") {
                    metadata.expert_count = Some(val as u64);
                } else if key.ends_with(".expert_used_count") {
                    metadata.expert_used_count = Some(val as u64);
                }
            }
            // UINT64
            6 => {
                let mut val_bytes = [0u8; 8];
                if file.read_exact(&mut val_bytes).is_err() {
                    break;
                }
                let val = u64::from_le_bytes(val_bytes);

                if key.ends_with(".context_length") {
                    metadata.context_length = Some(val);
                } else if key.ends_with(".embedding_length") {
                    metadata.embedding_length = Some(val);
                } else if key.ends_with(".block_count") {
                    metadata.block_count = Some(val);
                } else if key.ends_with(".attention.head_count") {
                    metadata.head_count = Some(val);
                } else if key.ends_with(".attention.head_count_kv") {
                    metadata.head_count_kv = Some(val);
                }
            }
            // STRING
            8 => {
                let mut str_len_bytes = [0u8; 8];
                if file.read_exact(&mut str_len_bytes).is_err() {
                    break;
                }
                let str_len = u64::from_le_bytes(str_len_bytes) as usize;

                if str_len > 10240 {
                    if file.seek(SeekFrom::Current(str_len as i64)).is_err() { break; }
                    continue;
                }

                let mut str_bytes = vec![0u8; str_len];
                if file.read_exact(&mut str_bytes).is_err() {
                    break;
                }
                let val = String::from_utf8_lossy(&str_bytes).to_string();

                if key == "general.architecture" {
                    metadata.architecture = Some(val);
                } else if key == "general.name" {
                    metadata.name = Some(val);
                }
            }
            // Other types - skip (break on seek failure to avoid parsing garbage — B6 fix)
            0 | 1 => { if file.seek(SeekFrom::Current(1)).is_err() { break; } }
            2 | 3 => { if file.seek(SeekFrom::Current(2)).is_err() { break; } }
            5 => { if file.seek(SeekFrom::Current(4)).is_err() { break; } }
            7 => { if file.seek(SeekFrom::Current(8)).is_err() { break; } }
            9 => { if file.seek(SeekFrom::Current(4)).is_err() { break; } }
            10 => { if file.seek(SeekFrom::Current(1)).is_err() { break; } }
            11 => {
                // Array: 4-byte element type + 8-byte count + count*element_size data
                let mut arr_type = [0u8; 4];
                let mut arr_count = [0u8; 8];
                if file.read_exact(&mut arr_type).is_err() || file.read_exact(&mut arr_count).is_err() {
                    break;
                }
                let elem_type = u32::from_le_bytes(arr_type);
                let count = u64::from_le_bytes(arr_count);
                // Reject absurdly large arrays to prevent integer overflow on count*elem_size (B7 fix)
                if count > 100_000_000 { break; }
                let count = count as i64;
                let elem_size: i64 = match elem_type {
                    0 | 1 | 10 => 1,   // uint8, int8, bool
                    2 | 3 => 2,         // uint16, int16
                    4 | 5 | 9 => 4,     // uint32, int32, float32
                    7 | 8 | 12 => 8,    // uint64, int64, float64
                    _ => { break; }      // string arrays or nested arrays — bail out
                };
                if file.seek(SeekFrom::Current(count * elem_size)).is_err() {
                    break;
                }
            }
            12 => { if file.seek(SeekFrom::Current(8)).is_err() { break; } }
            _ => break,
        }
    }

    Ok(metadata)
}

/// Calculate VRAM estimate from GGUF metadata or file info
fn calculate_vram_estimate(
    metadata: Option<&GgufMetadata>,
    file_size_bytes: u64,
    filename: &str,
    context_length: u64,
    include_kv_cache: bool,
) -> VramEstimate {
    let (quantization, param_count, confidence) = if let Some(meta) = metadata {
        let quant = meta.quantization.clone()
            .or_else(|| extract_quant_from_filename(filename))
            .unwrap_or_else(|| "Q4_K_M".to_string());

        let params = meta.parameter_count.or_else(|| {
            let bpw = quantization_bits_per_weight(&quant);
            Some(((file_size_bytes as f64 * 8.0) / bpw) as u64)
        }).or_else(|| extract_params_from_filename(filename));

        let conf = if meta.quantization.is_some() && meta.block_count.is_some() {
            "high"
        } else if params.is_some() {
            "medium"
        } else {
            "low"
        };

        (quant, params, conf)
    } else {
        let quant = extract_quant_from_filename(filename)
            .unwrap_or_else(|| "Q4_K_M".to_string());
        let params = extract_params_from_filename(filename).or_else(|| {
            let bpw = quantization_bits_per_weight(&quant);
            Some(((file_size_bytes as f64 * 8.0) / bpw) as u64)
        });
        (quant, params, "medium")
    };

    let params_billions = param_count.unwrap_or(7_000_000_000) as f64 / 1_000_000_000.0;
    let bpw = quantization_bits_per_weight(&quantization);

    let model_weights_gb = (params_billions * 1_000_000_000.0 * bpw) / 8.0 / 1024.0 / 1024.0 / 1024.0;

    let layers = if let Some(meta) = metadata {
        meta.block_count.unwrap_or_else(|| (params_billions * 4.0) as u64)
    } else {
        (params_billions * 4.0) as u64
    };

    let hidden_dim = if let Some(meta) = metadata {
        meta.embedding_length.unwrap_or(4096)
    } else {
        if params_billions >= 65.0 { 8192 }
        else if params_billions >= 30.0 { 6656 }
        else if params_billions >= 13.0 { 5120 }
        else if params_billions >= 7.0 { 4096 }
        else if params_billions >= 3.0 { 3072 }
        else { 2048 }
    };

    let kv_cache_gb = (2.0 * context_length as f64 * layers as f64 * hidden_dim as f64 * 2.0)
        / 1024.0 / 1024.0 / 1024.0;

    let overhead_gb = 0.55 + (params_billions * 0.08);

    let total_gb = if include_kv_cache {
        model_weights_gb + kv_cache_gb + overhead_gb
    } else {
        model_weights_gb + overhead_gb
    };

    // MoE-aware VRAM: compute active-expert-only VRAM for expert offload mode
    // Adapted from llmfit (MIT) — https://github.com/AlexsJones/llmfit
    let (is_moe, moe_active_gb) = if let Some(meta) = metadata {
        if let (Some(expert_count), Some(expert_used)) = (meta.expert_count, meta.expert_used_count) {
            if expert_count > 1 && expert_used < expert_count {
                // MoE: shared layers ~12% of params (attention+embedding+norm),
                // expert layers ~88% (MLP/FFN). Only active experts need VRAM.
                let shared_fraction = 0.12;
                let expert_fraction = 1.0 - shared_fraction;
                let active_fraction = shared_fraction
                    + expert_fraction * (expert_used as f64 / expert_count as f64);
                let active_weights_gb = model_weights_gb * active_fraction * 1.1; // 10% safety margin
                let active_total = if include_kv_cache {
                    active_weights_gb + kv_cache_gb + overhead_gb
                } else {
                    active_weights_gb + overhead_gb
                };
                (true, Some((active_total * 100.0).round() / 100.0))
            } else {
                (false, None)
            }
        } else {
            (false, None)
        }
    } else {
        // No metadata — detect MoE from filename (mixtral, moe, deepseek-moe, etc.)
        let name_lower = filename.to_lowercase();
        let detected_moe = name_lower.contains("mixtral")
            || name_lower.contains("-moe")
            || name_lower.contains("_moe")
            || name_lower.contains("switch");
        (detected_moe, None) // detected but no expert counts → can't compute active VRAM
    };

    VramEstimate {
        model_weights_gb: (model_weights_gb * 100.0).round() / 100.0,
        kv_cache_gb: (kv_cache_gb * 100.0).round() / 100.0,
        overhead_gb: (overhead_gb * 100.0).round() / 100.0,
        total_gb: (total_gb * 100.0).round() / 100.0,
        context_length,
        quantization,
        confidence: confidence.to_string(),
        kv_offload: !include_kv_cache,
        is_moe,
        moe_active_gb,
    }
}

/// Get GGUF metadata from a local file
#[tauri::command]
pub fn get_gguf_metadata(path: String) -> Result<GgufMetadata, String> {
    parse_gguf_header(&path)
}

/// Estimate VRAM for a model file
#[tauri::command]
pub fn estimate_model_vram(
    path: Option<String>,
    file_size_bytes: u64,
    filename: String,
    context_length: Option<u64>,
    include_kv_cache: Option<bool>,
) -> VramEstimate {
    let ctx = context_length.unwrap_or(4096);
    let kv = include_kv_cache.unwrap_or(true);

    let metadata = path.and_then(|p| parse_gguf_header(&p).ok());

    calculate_vram_estimate(metadata.as_ref(), file_size_bytes, &filename, ctx, kv)
}

/// Check VRAM compatibility for a model
#[tauri::command]
pub fn check_vram_compatibility(
    file_size_bytes: u64,
    filename: String,
    available_vram_mb: u64,
    context_length: Option<u64>,
    include_kv_cache: Option<bool>,
) -> VramCompatibility {
    let ctx = context_length.unwrap_or(4096);
    let kv = include_kv_cache.unwrap_or(true);
    let available_vram_gb = available_vram_mb as f64 / 1024.0;

    let estimate = calculate_vram_estimate(None, file_size_bytes, &filename, ctx, kv);
    let headroom_gb = available_vram_gb - estimate.total_gb;

    let status = if headroom_gb > 2.0 {
        "good"
    } else if headroom_gb > 0.0 {
        "tight"
    } else {
        "insufficient"
    };

    VramCompatibility {
        estimate,
        available_vram_gb: (available_vram_gb * 100.0).round() / 100.0,
        status: status.to_string(),
        headroom_gb: (headroom_gb * 100.0).round() / 100.0,
    }
}

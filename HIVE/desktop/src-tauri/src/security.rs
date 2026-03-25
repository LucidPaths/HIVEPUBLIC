//! Secure storage — AES-256-GCM encrypted file storage for API keys and hardware data
//!
//! File: ~/.hive/secrets.enc
//! No OS dependencies — works on Windows, Linux, macOS

use std::collections::HashMap;
use std::path::PathBuf;

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rand::RngCore;

use crate::paths::get_app_data_dir;
use crate::types::EncryptedHardwareData;

/// Secret account names — each integration is a "door" with a user-provided "key"
const SECRET_OPENAI: &str = "openai";
const SECRET_ANTHROPIC: &str = "anthropic";
const SECRET_OLLAMA: &str = "ollama";
const SECRET_OPENROUTER: &str = "openrouter";
const SECRET_DASHSCOPE: &str = "dashscope";
const SECRET_TELEGRAM: &str = "telegram";
const SECRET_DISCORD: &str = "discord";
const SECRET_GITHUB: &str = "github";
const SECRET_BRAVE: &str = "brave";
const SECRET_JINA: &str = "jina";
const SECRET_DISCORD_CHANNEL: &str = "discord_channel_id";

/// AES-256-GCM nonce size (96 bits = 12 bytes)
const NONCE_SIZE: usize = 12;

/// Get the secrets file path: ~/.hive/secrets.enc
fn get_secrets_path() -> Result<PathBuf, String> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| "Cannot determine home directory")?;
    Ok(PathBuf::from(home).join(".hive").join("secrets.enc"))
}

/// Derive 256-bit encryption key from machine-specific data using SHA-256.
/// Deterministic: same machine always produces the same key.
fn derive_machine_key() -> [u8; 32] {
    use sha2::{Sha256, Digest};

    let user = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "user".to_string());
    let host = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "host".to_string());

    let mut hasher = Sha256::new();
    hasher.update(format!("HIVE:v2:{}:{}:AES256GCM", user, host).as_bytes());
    hasher.finalize().into()
}

/// Legacy key derivation (v1) — kept only for transparent migration of existing secrets.enc files.
/// Uses DefaultHasher (SipHash) which is NOT cryptographic. Remove once all users have migrated.
fn derive_machine_key_v1() -> [u8; 32] {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let user = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "user".to_string());
    let host = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "host".to_string());

    let mut h1 = DefaultHasher::new();
    format!("HIVE:{}:{}:v1", user, host).hash(&mut h1);
    let mut h2 = DefaultHasher::new();
    format!("{}:SECURE:{}", host, user).hash(&mut h2);
    let mut h3 = DefaultHasher::new();
    format!("{}:{}", h1.finish(), h2.finish()).hash(&mut h3);
    let mut h4 = DefaultHasher::new();
    format!("FINAL:{}:{}:{}", h1.finish(), h2.finish(), h3.finish()).hash(&mut h4);

    let mut key = [0u8; 32];
    key[0..8].copy_from_slice(&h1.finish().to_le_bytes());
    key[8..16].copy_from_slice(&h2.finish().to_le_bytes());
    key[16..24].copy_from_slice(&h3.finish().to_le_bytes());
    key[24..32].copy_from_slice(&h4.finish().to_le_bytes());
    key
}

/// Encrypt bytes with AES-256-GCM (always uses current SHA-256 key)
fn encrypt_aes(plaintext: &[u8]) -> Result<String, String> {
    encrypt_aes_with_key(plaintext, &derive_machine_key())
}

/// Encrypt bytes with a specific key
fn encrypt_aes_with_key(plaintext: &[u8], key: &[u8; 32]) -> Result<String, String> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|_| "Encryption init failed".to_string())?;

    let mut nonce_bytes = [0u8; NONCE_SIZE];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher.encrypt(nonce, plaintext)
        .map_err(|_| "Encryption failed".to_string())?;

    let mut result = nonce_bytes.to_vec();
    result.extend(ciphertext);
    Ok(BASE64.encode(&result))
}

/// Decrypt with a specific key
fn decrypt_aes_with_key(encrypted_b64: &str, key: &[u8; 32]) -> Result<String, String> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|_| "Decryption init failed".to_string())?;

    let encrypted = BASE64.decode(encrypted_b64)
        .map_err(|_| "Invalid encrypted data".to_string())?;

    if encrypted.len() < NONCE_SIZE {
        return Err("Data too short".to_string());
    }

    let (nonce_bytes, ciphertext) = encrypted.split_at(NONCE_SIZE);
    let plaintext = cipher.decrypt(Nonce::from_slice(nonce_bytes), ciphertext)
        .map_err(|_| "Decryption failed".to_string())?;

    String::from_utf8(plaintext).map_err(|_| "Invalid UTF-8".to_string())
}

/// Decrypt AES-256-GCM encrypted data — tries SHA-256 key first, falls back to legacy SipHash key
fn decrypt_aes(encrypted_b64: &str) -> Result<String, String> {
    // Try current (SHA-256) key first
    if let Ok(result) = decrypt_aes_with_key(encrypted_b64, &derive_machine_key()) {
        return Ok(result);
    }
    // Fall back to legacy (SipHash) key for migration
    decrypt_aes_with_key(encrypted_b64, &derive_machine_key_v1())
}

/// Load secrets from encrypted file. Auto-migrates from legacy (SipHash) to SHA-256 key.
fn load_secrets() -> Result<HashMap<String, String>, String> {
    let path = get_secrets_path()?;
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let encrypted = std::fs::read_to_string(&path)
        .map_err(|e| format!("Read failed: {}", e))?;

    // Try current key first
    let new_key = derive_machine_key();
    if let Ok(json) = decrypt_aes_with_key(&encrypted, &new_key) {
        return serde_json::from_str(&json).map_err(|e| format!("Parse failed: {}", e));
    }

    // Try legacy key — if it works, re-encrypt with new key (transparent migration)
    let legacy_key = derive_machine_key_v1();
    let json = decrypt_aes_with_key(&encrypted, &legacy_key)?;
    let secrets: HashMap<String, String> = serde_json::from_str(&json)
        .map_err(|e| format!("Parse failed: {}", e))?;

    // Re-encrypt with new SHA-256 key
    let new_encrypted = encrypt_aes_with_key(json.as_bytes(), &new_key)?;
    if let Err(e) = std::fs::write(&path, &new_encrypted) {
        eprintln!("[HIVE] Warning: failed to migrate secrets to new key: {}", e);
    } else {
        eprintln!("[HIVE] Migrated secrets.enc from legacy key to SHA-256 key");
    }

    Ok(secrets)
}

/// Save secrets to encrypted file
fn save_secrets(secrets: &HashMap<String, String>) -> Result<(), String> {
    let path = get_secrets_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Dir create failed: {}", e))?;
    }
    let json = serde_json::to_string(secrets)
        .map_err(|e| format!("Serialize failed: {}", e))?;
    let encrypted = encrypt_aes(json.as_bytes())?;
    std::fs::write(&path, &encrypted)
        .map_err(|e| format!("Write failed: {}", e))
}

/// Store a secret (API key)
fn store_secret(name: &str, value: &str) -> Result<(), String> {
    let mut secrets = load_secrets()?;
    secrets.insert(name.to_string(), value.to_string());
    save_secrets(&secrets)?;
    eprintln!("[HIVE] Stored secret: {}", name);
    Ok(())
}

/// Get a secret (API key)
fn get_secret(name: &str) -> Option<String> {
    load_secrets().ok()?.get(name).cloned()
}

/// Delete a secret (API key)
fn delete_secret(name: &str) -> Result<(), String> {
    let mut secrets = load_secrets()?;
    secrets.remove(name);
    save_secrets(&secrets)?;
    eprintln!("[HIVE] Deleted secret: {}", name);
    Ok(())
}

/// Check if a secret exists
pub fn has_secret(name: &str) -> bool {
    get_secret(name).is_some()
}

/// Encrypt hardware/system data
fn encrypt_data(plaintext: &[u8]) -> Result<String, String> {
    encrypt_aes(plaintext)
}

/// Decrypt hardware/system data
fn decrypt_data(encrypted_b64: &str) -> Result<Vec<u8>, String> {
    let decrypted = decrypt_aes(encrypted_b64)?;
    Ok(decrypted.into_bytes())
}

// ============================================
// Tauri Commands — API Key Storage
// ============================================

/// Store an API key securely (AES-256-GCM encrypted)
#[tauri::command]
pub fn store_api_key(provider: String, api_key: String) -> Result<String, String> {
    let name = match provider.as_str() {
        "openai" => SECRET_OPENAI,
        "anthropic" => SECRET_ANTHROPIC,
        "ollama" => SECRET_OLLAMA,
        "openrouter" => SECRET_OPENROUTER,
        "dashscope" => SECRET_DASHSCOPE,
        "telegram" => SECRET_TELEGRAM,
        "discord" => SECRET_DISCORD,
        "github" => SECRET_GITHUB,
        "brave" => SECRET_BRAVE,
        "jina" => SECRET_JINA,
        "discord_channel_id" => SECRET_DISCORD_CHANNEL,
        _ => return Err(format!("Unknown provider '{}'. Valid: openai, anthropic, ollama, openrouter, dashscope, telegram, discord, discord_channel_id, github, brave, jina", provider)),
    };

    if api_key.trim().is_empty() {
        return Err("API key cannot be empty".to_string());
    }

    eprintln!("[HIVE] Storing API key for: {}", name);
    store_secret(name, api_key.trim())?;
    Ok("API key stored securely".to_string())
}

/// Check if an API key exists
#[tauri::command]
pub fn has_api_key(provider: String) -> Result<bool, String> {
    let name = match provider.as_str() {
        "openai" => SECRET_OPENAI,
        "anthropic" => SECRET_ANTHROPIC,
        "ollama" => SECRET_OLLAMA,
        "openrouter" => SECRET_OPENROUTER,
        "dashscope" => SECRET_DASHSCOPE,
        "telegram" => SECRET_TELEGRAM,
        "discord" => SECRET_DISCORD,
        "github" => SECRET_GITHUB,
        "brave" => SECRET_BRAVE,
        "jina" => SECRET_JINA,
        "discord_channel_id" => SECRET_DISCORD_CHANNEL,
        _ => return Err(format!("Unknown provider '{}'", provider)),
    };
    Ok(has_secret(name))
}

/// Delete an API key
#[tauri::command]
pub fn delete_api_key(provider: String) -> Result<(), String> {
    let name = match provider.as_str() {
        "openai" => SECRET_OPENAI,
        "anthropic" => SECRET_ANTHROPIC,
        "ollama" => SECRET_OLLAMA,
        "openrouter" => SECRET_OPENROUTER,
        "dashscope" => SECRET_DASHSCOPE,
        "telegram" => SECRET_TELEGRAM,
        "discord" => SECRET_DISCORD,
        "github" => SECRET_GITHUB,
        "brave" => SECRET_BRAVE,
        "jina" => SECRET_JINA,
        "discord_channel_id" => SECRET_DISCORD_CHANNEL,
        _ => return Err(format!("Unknown provider '{}'", provider)),
    };
    delete_secret(name)
}

/// Get API key for internal use (provider API calls).
/// For multi-key providers (openai, anthropic, openrouter, dashscope), uses round-robin rotation.
/// For single-key providers (telegram, github, etc.), returns the stored value directly.
pub fn get_api_key_internal(provider: &str) -> Option<String> {
    let name = match provider {
        "openai" => SECRET_OPENAI,
        "anthropic" => SECRET_ANTHROPIC,
        "ollama" => SECRET_OLLAMA,
        "openrouter" => SECRET_OPENROUTER,
        "dashscope" => SECRET_DASHSCOPE,
        "telegram" => SECRET_TELEGRAM,
        "discord" => SECRET_DISCORD,
        "github" => SECRET_GITHUB,
        "brave" => SECRET_BRAVE,
        "jina" => SECRET_JINA,
        "discord_channel_id" => SECRET_DISCORD_CHANNEL,
        _ => return None,
    };

    // Multi-key rotation for cloud chat providers (P2: multi-key failover)
    match provider {
        "openai" | "anthropic" | "openrouter" | "dashscope" => get_next_api_key(name),
        _ => get_secret(name),
    }
}

// ============================================
// Multi-Key Rotation (P2)
// ============================================

use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
use std::sync::OnceLock;

/// Per-provider round-robin counters for key rotation.
static KEY_COUNTERS: OnceLock<std::sync::Mutex<HashMap<String, AtomicUsize>>> = OnceLock::new();

fn key_counters() -> &'static std::sync::Mutex<HashMap<String, AtomicUsize>> {
    KEY_COUNTERS.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

/// Get all API keys for a provider (backwards-compatible).
/// If the stored value is a JSON array, returns all keys.
/// If it's a plain string, wraps it as a single-element vec.
pub fn get_api_keys_for_provider(secret_name: &str) -> Vec<String> {
    let raw = match get_secret(secret_name) {
        Some(v) => v,
        None => return Vec::new(),
    };

    // Try parsing as JSON array first (new multi-key format)
    if raw.starts_with('[') {
        if let Ok(keys) = serde_json::from_str::<Vec<String>>(&raw) {
            return keys.into_iter().filter(|k| !k.trim().is_empty()).collect();
        }
    }

    // Fall back to plain string (legacy single-key format)
    if raw.trim().is_empty() {
        Vec::new()
    } else {
        vec![raw]
    }
}

/// Get the next API key for a provider using round-robin rotation.
/// Returns None if no keys are configured.
fn get_next_api_key(secret_name: &str) -> Option<String> {
    let keys = get_api_keys_for_provider(secret_name);
    if keys.is_empty() {
        return None;
    }
    if keys.len() == 1 {
        return Some(keys[0].clone());
    }

    // Round-robin via atomic counter
    if let Ok(mut counters) = key_counters().lock() {
        let counter = counters
            .entry(secret_name.to_string())
            .or_insert_with(|| AtomicUsize::new(0));
        let idx = counter.fetch_add(1, AtomicOrdering::Relaxed) % keys.len();
        Some(keys[idx].clone())
    } else {
        Some(keys[0].clone()) // Lock poisoned — return first key as fallback
    }
}

/// Store multiple API keys for a provider. Backwards-compatible:
/// single key stored as plain string, multiple as JSON array.
#[tauri::command]
pub fn store_api_keys(provider: String, api_keys: Vec<String>) -> Result<String, String> {
    let name = match provider.as_str() {
        "openai" => SECRET_OPENAI,
        "anthropic" => SECRET_ANTHROPIC,
        "openrouter" => SECRET_OPENROUTER,
        "dashscope" => SECRET_DASHSCOPE,
        _ => return Err(format!("Multi-key not supported for '{}'", provider)),
    };

    let keys: Vec<String> = api_keys.into_iter()
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
        .collect();

    if keys.is_empty() {
        return Err("No valid API keys provided".to_string());
    }

    let value = if keys.len() == 1 {
        keys[0].clone() // Single key — store as plain string (backwards compat)
    } else {
        serde_json::to_string(&keys).map_err(|e| format!("Serialize failed: {}", e))?
    };

    store_secret(name, &value)?;
    crate::tools::log_tools::append_to_app_log(&format!(
        "SECURITY | keys_stored | provider={} count={}", provider, keys.len()
    ));
    Ok(format!("{} API key(s) stored for {}", keys.len(), provider))
}

/// Get the number of configured API keys for a provider.
#[tauri::command]
pub fn get_api_key_count(provider: String) -> Result<usize, String> {
    let name = match provider.as_str() {
        "openai" => SECRET_OPENAI,
        "anthropic" => SECRET_ANTHROPIC,
        "openrouter" => SECRET_OPENROUTER,
        "dashscope" => SECRET_DASHSCOPE,
        _ => {
            // Map known single-key providers via constant; unknown fall through to raw name
            let mapped = match provider.as_str() {
                "telegram" => SECRET_TELEGRAM,
                "discord" => SECRET_DISCORD,
                "discord_channel_id" => SECRET_DISCORD_CHANNEL,
                "github" => SECRET_GITHUB,
                "brave" => SECRET_BRAVE,
                "jina" => SECRET_JINA,
                other => other,
            };
            return Ok(if has_secret(mapped) { 1 } else { 0 });
        }
    };
    Ok(get_api_keys_for_provider(name).len())
}

// ============================================
// Tauri Commands — Hardware Data Encryption
// ============================================

/// Encrypt and store hardware fingerprint locally
/// SECURITY: Data is AES-256-GCM encrypted, key derived from machine identity
/// This data NEVER leaves the device
#[tauri::command]
pub fn store_encrypted_hardware_data(data: String) -> Result<(), String> {
    let encrypted = encrypt_data(data.as_bytes())?;

    let hardware_data = EncryptedHardwareData {
        encrypted_data: encrypted,
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
    };

    // Store in app data directory
    let data_dir = get_app_data_dir();
    if !data_dir.exists() {
        std::fs::create_dir_all(&data_dir)
            .map_err(|_| "Failed to create data directory")?;
    }

    let file_path = data_dir.join("hardware.enc");
    let json = serde_json::to_string(&hardware_data)
        .map_err(|_| "Failed to serialize data")?;

    std::fs::write(&file_path, json)
        .map_err(|_| "Failed to write encrypted data")?;

    Ok(())
}

/// Retrieve and decrypt hardware fingerprint
#[tauri::command]
pub fn get_encrypted_hardware_data() -> Result<Option<String>, String> {
    let file_path = get_app_data_dir().join("hardware.enc");

    if !file_path.exists() {
        return Ok(None);
    }

    let json = std::fs::read_to_string(&file_path)
        .map_err(|_| "Failed to read encrypted data")?;

    let hardware_data: EncryptedHardwareData = serde_json::from_str(&json)
        .map_err(|_| "Failed to parse encrypted data")?;

    let decrypted = decrypt_data(&hardware_data.encrypted_data)?;

    String::from_utf8(decrypted)
        .map(Some)
        .map_err(|_| "Failed to decode decrypted data".to_string())
}

// ============================================
// Tests
// ============================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_derivation_deterministic() {
        let key1 = derive_machine_key();
        let key2 = derive_machine_key();
        assert_eq!(key1, key2, "Key derivation must be deterministic");
    }

    #[test]
    fn test_key_is_256_bits() {
        let key = derive_machine_key();
        assert_eq!(key.len(), 32, "AES-256 key must be 32 bytes");
    }

    #[test]
    fn test_new_key_differs_from_legacy() {
        let new_key = derive_machine_key();
        let legacy_key = derive_machine_key_v1();
        assert_ne!(new_key, legacy_key, "New SHA-256 key must differ from legacy SipHash key");
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let plaintext = "Hello, HIVE! API key: sk-test-12345";
        let encrypted = encrypt_aes(plaintext.as_bytes()).expect("Encryption should succeed");
        let decrypted = decrypt_aes(&encrypted).expect("Decryption should succeed");
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_decrypt_empty_string() {
        let plaintext = "";
        let encrypted = encrypt_aes(plaintext.as_bytes()).expect("Encryption should succeed");
        let decrypted = decrypt_aes(&encrypted).expect("Decryption should succeed");
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_produces_different_ciphertext() {
        // AES-GCM uses random nonces, so encrypting the same plaintext twice
        // should produce different ciphertexts
        let plaintext = b"same data";
        let enc1 = encrypt_aes(plaintext).unwrap();
        let enc2 = encrypt_aes(plaintext).unwrap();
        assert_ne!(enc1, enc2, "Random nonces should produce different ciphertexts");
    }

    #[test]
    fn test_decrypt_with_wrong_data_fails() {
        let result = decrypt_aes("not-valid-base64!!!");
        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_with_short_data_fails() {
        let short = BASE64.encode(&[0u8; 4]); // Too short for nonce
        let result = decrypt_aes(&short);
        assert!(result.is_err());
    }

    #[test]
    fn test_legacy_key_decrypt_fallback() {
        // Encrypt with legacy key, verify decrypt_aes falls back to it
        let plaintext = "legacy data";
        let legacy_key = derive_machine_key_v1();
        let encrypted = encrypt_aes_with_key(plaintext.as_bytes(), &legacy_key)
            .expect("Legacy encryption should succeed");
        let decrypted = decrypt_aes(&encrypted)
            .expect("Decrypt should fall back to legacy key");
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_decrypt_unicode() {
        let plaintext = "API key with unicode: 日本語テスト 🔐";
        let encrypted = encrypt_aes(plaintext.as_bytes()).unwrap();
        let decrypted = decrypt_aes(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }
}

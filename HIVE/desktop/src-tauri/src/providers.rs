//! Provider management — chat with local, Ollama, OpenAI, and Anthropic

/// Anthropic API version (P7: single source of truth for all Anthropic calls)
pub const ANTHROPIC_API_VERSION: &str = "2023-06-01";

use crate::http_client::hive_http_client;
use crate::provider_stream::{
    chat_dashscope, check_dashscope_status, stream_anthropic, stream_dashscope, stream_ollama,
    stream_openai, stream_openrouter,
};
use crate::provider_tools::{
    chat_anthropic_with_tools, chat_local_with_tools, chat_openai_format_with_tools,
};
use crate::security::{get_api_key_internal, has_secret};
use crate::tools::log_tools::append_to_app_log;
use crate::tools::{ToolCall, ToolSchema};
use crate::types::*;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use tauri::AppHandle;

pub(crate) const DASHSCOPE_BASE: &str = "https://coding-intl.dashscope.aliyuncs.com/v1";

/// Parse a thinking depth string from the frontend into the enum.
pub(crate) fn parse_thinking_depth(s: &Option<String>) -> Option<ThinkingDepth> {
    s.as_deref().and_then(|v| match v {
        "off" => Some(ThinkingDepth::Off),
        "low" => Some(ThinkingDepth::Low),
        "medium" => Some(ThinkingDepth::Medium),
        "high" => Some(ThinkingDepth::High),
        _ => None,
    })
}

/// Inject provider-specific thinking/reasoning parameters into a request body (P2: unified).
/// Returns extra HTTP headers needed (e.g., Anthropic beta header for interleaved thinking).
///
/// Provider mapping:
///   - Anthropic: thinking.type:"adaptive" + output_config.effort + beta header
///   - OpenAI:    reasoning_effort (o-series models)
///   - DashScope: enable_thinking + thinking_budget
///   - OpenRouter: reasoning_effort (pass-through to underlying provider)
///   - Ollama/Local: no-op (graceful skip — P4)
pub(crate) fn inject_thinking_params(
    body: &mut serde_json::Value,
    provider: &str,
    depth: ThinkingDepth,
) -> Vec<(String, String)> {
    let mut extra_headers: Vec<(String, String)> = Vec::new();

    if depth == ThinkingDepth::Off {
        // DashScope: explicitly disable if set
        if provider == "dashscope" {
            body["enable_thinking"] = serde_json::json!(false);
        }
        return extra_headers;
    }

    let effort = match depth {
        ThinkingDepth::Low => "low",
        ThinkingDepth::Medium => "medium",
        ThinkingDepth::High => "high",
        ThinkingDepth::Off => unreachable!(),
    };

    match provider {
        "anthropic" => {
            body["thinking"] = serde_json::json!({ "type": "adaptive" });
            body["output_config"] = serde_json::json!({ "effort": effort });
            extra_headers.push((
                "anthropic-beta".to_string(),
                "interleaved-thinking-2025-05-14".to_string(),
            ));
            // Anthropic thinking models need higher max_tokens
            if body.get("max_tokens").and_then(|v| v.as_u64()).unwrap_or(0) < 16384 {
                body["max_tokens"] = serde_json::json!(16384);
            }
        }
        "openai" => {
            body["reasoning_effort"] = serde_json::json!(effort);
            // o-series uses max_completion_tokens, not max_tokens
            if let Some(max) = body.get("max_tokens").cloned() {
                if let Some(obj) = body.as_object_mut() {
                    obj.remove("max_tokens");
                }
                body["max_completion_tokens"] = max;
            }
        }
        "dashscope" => {
            body["enable_thinking"] = serde_json::json!(true);
            let budget = match depth {
                ThinkingDepth::Low => 4096,
                ThinkingDepth::Medium => 16384,
                ThinkingDepth::High => 65536,
                ThinkingDepth::Off => unreachable!(),
            };
            body["thinking_budget"] = serde_json::json!(budget);
        }
        "openrouter" => {
            body["reasoning_effort"] = serde_json::json!(effort);
        }
        // "ollama", "local" — no thinking control available, graceful skip
        _ => {}
    }

    extra_headers
}

// ============================================
// Chat Retry on Transient Errors (P4)
// ============================================

/// HTTP status codes that indicate transient server errors worth retrying.
const RETRYABLE_STATUS_CODES: &[&str] = &["(429)", "(500)", "(502)", "(503)", "(529)"];

/// Check if an error string contains a retryable HTTP status code.
fn is_retryable_error(error: &str) -> bool {
    RETRYABLE_STATUS_CODES.iter().any(|code| error.contains(code))
}

/// Retry an async cloud chat call on transient HTTP errors.
/// Max 2 retries with 1s/2s delay. Combined with multi-key rotation (P2 + P4):
/// on 429, the next retry automatically uses the next key via get_api_key_internal().
pub(crate) async fn with_chat_retry<F, Fut, T>(
    provider: &str,
    f: F,
) -> Result<T, String>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, String>>,
{
    let max_retries = 2u32;
    let mut last_err = String::new();

    for attempt in 0..=max_retries {
        match f().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                last_err = e;
                if attempt < max_retries && is_retryable_error(&last_err) {
                    let delay_ms = 1000u64 * (1 << attempt); // 1s, 2s
                    append_to_app_log(&format!(
                        "PROVIDER | retry | provider={} attempt={}/{} delay={}ms | {}",
                        provider, attempt + 1, max_retries, delay_ms, last_err
                    ));
                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                    continue;
                }
                return Err(last_err);
            }
        }
    }

    Err(last_err)
}

/// Sanitize API error response bodies before including in error messages.
/// Strips anything that looks like an API key to prevent secret leakage
/// through error strings → MAGMA events → prompt injection (P6: Secrets Stay Secret).
pub(crate) fn sanitize_api_error(body: &str) -> String {
    // Strip common API key patterns: sk-..., sk_live_..., key-..., etc.
    // Regex is compiled once and cached via OnceLock (was recompiling on every call).
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(r"(?i)(sk[-_][a-zA-Z0-9_-]{20,}|key[-_][a-zA-Z0-9_-]{20,}|bearer\s+[a-zA-Z0-9_-]{20,})")
            .expect("sanitize_api_error regex is valid")
    });
    let sanitized = re.replace_all(body, "[REDACTED]").to_string();
    // Truncate to prevent massive error bodies filling memory (char-safe)
    crate::content_security::safe_truncate(&sanitized, 500)
}

/// Response from a tool-aware chat request.
/// Either the model replied with text, or it wants to call tools.
/// `thinking` carries reasoning tokens (DashScope `/think`, Anthropic thinking blocks,
/// OpenAI `reasoning_content`) separated from the user-facing content (P1: modularity).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ChatResponse {
    #[serde(rename = "text")]
    Text { content: String, thinking: Option<String> },
    #[serde(rename = "tool_calls")]
    ToolCalls { content: Option<String>, thinking: Option<String>, tool_calls: Vec<ToolCall> },
}

/// Streaming chat response — includes both content and thinking (P2: provider-agnostic).
/// Returned by `chat_with_provider_stream` so the frontend can render them separately.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamResponse {
    pub content: String,
    pub thinking: Option<String>,
}

/// Payload for streaming token events — includes stream_id for multi-pane filtering.
/// Multiple panes can stream simultaneously; each listens only for its own stream_id.
#[derive(Debug, Clone, Serialize)]
pub struct StreamTokenPayload {
    pub token: String,
    pub stream_id: String,
}

/// Strip `/think ... /think` blocks from model output, returning (clean_content, thinking).
/// Handles DashScope/Kimi K2.5 thinking tags. Also strips `<think>...</think>` (DeepSeek R1 format).
/// If no thinking tags found, returns the original content unchanged.
pub(crate) fn strip_thinking(text: &str) -> (String, Option<String>) {
    // Cached regex compilation — these are called ~9 times per stream response.
    static RE_XML: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static RE_SLASH: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    // Match <think>...</think> (DeepSeek R1, some Qwen models)
    let re_xml = RE_XML.get_or_init(|| regex::Regex::new(r"(?s)<think>\s*(.*?)\s*</think>").unwrap());
    // Match /think ... /think (DashScope Kimi K2.5)
    let re_slash = RE_SLASH.get_or_init(|| regex::Regex::new(r"(?s)/think\s*(.*?)\s*/think").unwrap());

    let mut thinking_parts = Vec::new();
    let mut clean = text.to_string();

    // Extract <think> blocks FIRST — more specific pattern, prevents the /think regex
    // from accidentally matching the /think inside </think> closing tags.
    for cap in re_xml.captures_iter(&clean) {
        if let Some(m) = cap.get(1) {
            let content = m.as_str().trim();
            if !content.is_empty() {
                thinking_parts.push(content.to_string());
            }
        }
    }
    clean = re_xml.replace_all(&clean, "").to_string();

    // Extract /think blocks from the already-cleaned text (no </think> tags remain)
    for cap in re_slash.captures_iter(&clean) {
        if let Some(m) = cap.get(1) {
            let content = m.as_str().trim();
            if !content.is_empty() {
                thinking_parts.push(content.to_string());
            }
        }
    }
    clean = re_slash.replace_all(&clean, "").to_string();

    // Strip orphaned tags — e.g. lone </think> when the opening was in reasoning_content,
    // or an unclosed <think> at the start of a truncated response.
    clean = clean.replace("</think>", "").replace("<think>", "");

    // Clean up whitespace artifacts
    let clean = clean.trim().to_string();
    let thinking = if thinking_parts.is_empty() {
        None
    } else {
        Some(thinking_parts.join("\n\n"))
    };
    (clean, thinking)
}

/// Extract `reasoning_content` from an OpenAI-compatible message object (DeepSeek, some Qwen).
pub(crate) fn extract_reasoning_content(message: &serde_json::Value) -> Option<String> {
    message.get("reasoning_content")
        .and_then(|r| r.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_string())
}

// ============================================
// Session Model Context (P2: Provider Agnostic, P7: Framework Survives)
// ============================================

/// The framework's source of truth for the current session's model identity.
/// Set by the frontend when a chat starts — workers and tools inherit it
/// instead of relying on LLMs to guess their own API model IDs.
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct SessionModelContext {
    pub provider: String,
    pub model_id: String,
}

static SESSION_CTX: OnceLock<RwLock<Option<SessionModelContext>>> = OnceLock::new();

fn session_ctx() -> &'static RwLock<Option<SessionModelContext>> {
    SESSION_CTX.get_or_init(|| RwLock::new(None))
}

/// Set the current session's provider and model ID.
/// Called by the frontend at chat start — the framework knows the API model ID,
/// so workers/tools never need to guess it.
#[tauri::command]
pub async fn set_session_model_context(provider: String, model_id: String) {
    let mut ctx = session_ctx().write().await;
    *ctx = Some(SessionModelContext { provider, model_id });
}

/// Get the current session's provider and model ID (for tools like worker_spawn).
pub async fn get_session_model_context() -> Option<SessionModelContext> {
    session_ctx().read().await.clone()
}

// ============================================
// Tauri Commands
// ============================================

/// Get list of configured providers
#[tauri::command]
pub fn get_providers() -> Result<Vec<ProviderConfig>, String> {
    Ok(vec![
        ProviderConfig {
            provider_type: ProviderType::Local,
            name: "Local (llama.cpp)".to_string(),
            endpoint: None,
            enabled: true,
            has_api_key: false,
        },
        ProviderConfig {
            provider_type: ProviderType::Ollama,
            name: "Ollama".to_string(),
            endpoint: Some("http://localhost:11434".to_string()),
            enabled: true,
            has_api_key: has_secret("ollama"),
        },
        ProviderConfig {
            provider_type: ProviderType::OpenAI,
            name: "OpenAI".to_string(),
            endpoint: Some("https://api.openai.com/v1".to_string()),
            enabled: true,
            has_api_key: has_secret("openai"),
        },
        ProviderConfig {
            provider_type: ProviderType::Anthropic,
            name: "Anthropic".to_string(),
            endpoint: Some("https://api.anthropic.com/v1".to_string()),
            enabled: true,
            has_api_key: has_secret("anthropic"),
        },
        ProviderConfig {
            provider_type: ProviderType::OpenRouter,
            name: "OpenRouter".to_string(),
            endpoint: Some("https://openrouter.ai/api/v1".to_string()),
            enabled: true,
            has_api_key: has_secret("openrouter"),
        },
        ProviderConfig {
            provider_type: ProviderType::DashScope,
            name: "DashScope (Alibaba)".to_string(),
            endpoint: Some("https://coding-intl.dashscope.aliyuncs.com/v1".to_string()),
            enabled: true,
            has_api_key: has_secret("dashscope"),
        },
    ])
}

/// Check provider connection and get available models
#[tauri::command]
pub async fn check_provider_status(provider: String) -> Result<ProviderStatus, String> {
    match provider.as_str() {
        "ollama" => check_ollama_status().await,
        "openai" => check_openai_status().await,
        "anthropic" => check_anthropic_status().await,
        "openrouter" => check_openrouter_status().await,
        "dashscope" => check_dashscope_status().await,
        "local" => Ok(ProviderStatus {
            provider_type: ProviderType::Local,
            configured: true,
            connected: true,
            error: None,
            models: vec![],
        }),
        _ => Err("Invalid provider".to_string()),
    }
}

/// Chat with a cloud provider
#[tauri::command]
pub async fn chat_with_provider(
    provider: String,
    model: String,
    messages: Vec<serde_json::Value>,
    thinking_depth: Option<String>,
) -> Result<String, String> {
    let depth = parse_thinking_depth(&thinking_depth);
    // P4: Wrap cloud provider calls with retry on transient errors (429, 500, 502, 503, 529)
    let p = provider.clone();
    let result = with_chat_retry(&p, || {
        let m = model.clone();
        let msgs = messages.clone();
        let d = depth;
        let prov = p.clone();
        async move {
            match prov.as_str() {
                "openai" => chat_openai(m, msgs, d).await,
                "anthropic" => chat_anthropic(m, msgs, d).await,
                "ollama" => chat_ollama(m, msgs).await,
                "openrouter" => chat_openrouter(m, msgs, d).await,
                "dashscope" => chat_dashscope(m, msgs, d).await,
                _ => Err("Invalid provider".to_string()),
            }
        }
    }).await;
    if let Err(ref e) = result {
        append_to_app_log(&format!("PROVIDER | chat_error | {}:{} | {}", provider, model, e));
    }
    result
}

/// Chat with a cloud provider using streaming (SSE).
/// Emits "cloud-chat-token" for content and "cloud-thinking-token" for reasoning.
/// Returns StreamResponse with both content and thinking separated (P1: modularity).
#[tauri::command]
pub async fn chat_with_provider_stream(
    app: AppHandle,
    provider: String,
    model: String,
    messages: Vec<serde_json::Value>,
    stream_id: Option<String>,
    thinking_depth: Option<String>,
) -> Result<StreamResponse, String> {
    // Default stream_id for backwards compatibility (single-pane mode)
    let sid = stream_id.unwrap_or_default();
    let depth = parse_thinking_depth(&thinking_depth);
    let result = match provider.as_str() {
        "openai" => stream_openai(app, model.clone(), messages, sid, depth).await,
        "anthropic" => stream_anthropic(app, model.clone(), messages, sid, depth).await,
        "ollama" => stream_ollama(app, model.clone(), messages, sid).await,
        "openrouter" => stream_openrouter(app, model.clone(), messages, sid, depth).await,
        "dashscope" => stream_dashscope(app, model.clone(), messages, sid, depth).await,
        _ => Err("Invalid provider".to_string()),
    };
    if let Err(ref e) = result {
        append_to_app_log(&format!("PROVIDER | stream_error | {}:{} | {}", provider, model, e));
    }
    result
}

/// Chat with a provider, passing tool schemas. Returns text or tool_calls.
/// `context_length` enables the local provider to pick a compact tool format
/// for small context windows (<=16K), saving ~5K+ tokens. Cloud providers
/// ignore it (they use native tool APIs where token cost is fixed).
#[tauri::command]
pub async fn chat_with_tools(
    provider: String,
    model: String,
    messages: Vec<serde_json::Value>,
    tools: Vec<ToolSchema>,
    context_length: Option<u64>,
    port: Option<u16>,
    thinking_depth: Option<String>,
) -> Result<ChatResponse, String> {
    let depth = parse_thinking_depth(&thinking_depth);
    // P4: Wrap cloud provider calls with retry on transient errors
    let p = provider.clone();
    let result = with_chat_retry(&p, || {
        let prov = p.clone();
        let m = model.clone();
        let msgs = messages.clone();
        let t = tools.clone();
        let d = depth;
        let ctx = context_length;
        let pt = port;
        async move {
            match prov.as_str() {
                "openai" | "ollama" | "openrouter" | "dashscope" => chat_openai_format_with_tools(
                    &prov, m, msgs, t, d,
                ).await,
                "anthropic" => chat_anthropic_with_tools(m, msgs, t, d).await,
                "local" => chat_local_with_tools(msgs, t, ctx, pt.unwrap_or(8080)).await,
                _ => Err("Invalid provider".to_string()),
            }
        }
    }).await;
    if let Err(ref e) = result {
        append_to_app_log(&format!("PROVIDER | tool_chat_error | {}:{} | {}", provider, model, e));
    }
    result
}

// ============================================
// Internal provider implementations
// ============================================

async fn check_ollama_status() -> Result<ProviderStatus, String> {
    let client = hive_http_client()?;

    let response = client
        .get("http://localhost:11434/api/tags")
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await;

    match response {
        Ok(resp) if resp.status().is_success() => {
            let mut models = Vec::new();

            if let Ok(json) = resp.json::<serde_json::Value>().await {
                if let Some(model_list) = json.get("models").and_then(|m| m.as_array()) {
                    for model in model_list {
                        if let Some(name) = model.get("name").and_then(|n| n.as_str()) {
                            models.push(ProviderModel {
                                id: name.to_string(),
                                name: name.to_string(),
                                provider: ProviderType::Ollama,
                                context_length: None,
                                description: model.get("details")
                                    .and_then(|d| d.get("family"))
                                    .and_then(|f| f.as_str())
                                    .map(|s| s.to_string()),
                            });
                        }
                    }
                }
            }

            Ok(ProviderStatus {
                provider_type: ProviderType::Ollama,
                configured: true,
                connected: true,
                error: None,
                models,
            })
        }
        Ok(resp) => Ok(ProviderStatus {
            provider_type: ProviderType::Ollama,
            configured: false,
            connected: false,
            error: Some(format!("Ollama returned status: {}", resp.status())),
            models: vec![],
        }),
        Err(_) => Ok(ProviderStatus {
            provider_type: ProviderType::Ollama,
            configured: false,
            connected: false,
            error: Some("Ollama server not running".to_string()),
            models: vec![],
        }),
    }
}

async fn check_openai_status() -> Result<ProviderStatus, String> {
    let api_key = match get_api_key_internal("openai") {
        Some(key) => key,
        None => {
            return Ok(ProviderStatus {
                provider_type: ProviderType::OpenAI,
                configured: false,
                connected: false,
                error: Some("API key not configured".to_string()),
                models: vec![],
            });
        }
    };

    let client = hive_http_client()?;
    let response = client
        .get("https://api.openai.com/v1/models")
        .header("Authorization", format!("Bearer {}", api_key))
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await;

    match response {
        Ok(resp) if resp.status().is_success() => {
            let mut models = Vec::new();

            if let Ok(json) = resp.json::<serde_json::Value>().await {
                if let Some(data) = json.get("data").and_then(|d| d.as_array()) {
                    let chat_prefixes = [
                        "gpt-4o", "gpt-4o-mini", "gpt-4-turbo", "gpt-4", "gpt-3.5-turbo",
                        "o1", "o3", "o4", "chatgpt-4o",
                    ];
                    // Exclude non-chat models (embeddings, tts, whisper, dall-e, etc.)
                    let exclude_prefixes = [
                        "text-embedding", "tts-", "whisper-", "dall-e", "davinci",
                        "babbage", "curie", "ada", "gpt-4o-transcribe", "gpt-4o-mini-transcribe",
                        "gpt-4o-realtime", "gpt-4o-mini-realtime", "gpt-4o-audio",
                        "gpt-image", "codex-",
                    ];

                    for model in data {
                        if let Some(id) = model.get("id").and_then(|i| i.as_str()) {
                            let is_chat = chat_prefixes.iter().any(|m| id.starts_with(m));
                            let is_excluded = exclude_prefixes.iter().any(|m| id.starts_with(m));
                            if is_chat && !is_excluded {
                                models.push(ProviderModel {
                                    id: id.to_string(),
                                    name: id.to_string(),
                                    provider: ProviderType::OpenAI,
                                    context_length: Some(match id {
                                        _ if id.starts_with("o1") || id.starts_with("o3") || id.starts_with("o4") => 200000,
                                        _ if id.starts_with("gpt-4o") => 128000,
                                        _ if id.starts_with("gpt-4-turbo") => 128000,
                                        _ if id.starts_with("gpt-4") => 8192,
                                        _ => 128000,
                                    }),
                                    description: None,
                                });
                            }
                        }
                    }
                }
            }

            models.sort_by(|a, b| a.name.cmp(&b.name));

            Ok(ProviderStatus {
                provider_type: ProviderType::OpenAI,
                configured: true,
                connected: true,
                error: None,
                models,
            })
        }
        Ok(resp) if resp.status() == 401 => Ok(ProviderStatus {
            provider_type: ProviderType::OpenAI,
            configured: true,
            connected: false,
            error: Some("Invalid API key".to_string()),
            models: vec![],
        }),
        Ok(resp) => Ok(ProviderStatus {
            provider_type: ProviderType::OpenAI,
            configured: true,
            connected: false,
            error: Some(format!("API error: {}", resp.status())),
            models: vec![],
        }),
        Err(_) => Ok(ProviderStatus {
            provider_type: ProviderType::OpenAI,
            configured: true,
            connected: false,
            error: Some("Network error".to_string()),
            models: vec![],
        }),
    }
}

async fn check_anthropic_status() -> Result<ProviderStatus, String> {
    let api_key = match get_api_key_internal("anthropic") {
        Some(key) => key,
        None => {
            return Ok(ProviderStatus {
                provider_type: ProviderType::Anthropic,
                configured: false,
                connected: false,
                error: Some("API key not configured".to_string()),
                models: vec![],
            });
        }
    };

    // Fetch models dynamically from Anthropic's /v1/models endpoint (P2, P7)
    let client = hive_http_client()?;
    let mut models = Vec::new();
    let mut after_id: Option<String> = None;

    // Paginate — Anthropic defaults to 20 per page
    loop {
        let mut url = "https://api.anthropic.com/v1/models?limit=100".to_string();
        if let Some(ref cursor) = after_id {
            url.push_str(&format!("&after_id={}", urlencoding::encode(cursor)));
        }

        let response = client
            .get(&url)
            .header("x-api-key", &api_key)
            .header("anthropic-version", ANTHROPIC_API_VERSION)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await;

        match response {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(json) = resp.json::<serde_json::Value>().await {
                    if let Some(data) = json.get("data").and_then(|d| d.as_array()) {
                        for model in data {
                            let id = model.get("id").and_then(|i| i.as_str()).unwrap_or("");
                            if id.is_empty() { continue; }

                            let name = model.get("display_name")
                                .and_then(|n| n.as_str())
                                .unwrap_or(id)
                                .to_string();

                            models.push(ProviderModel {
                                id: id.to_string(),
                                name,
                                provider: ProviderType::Anthropic,
                                context_length: None,
                                description: None,
                            });
                        }
                    }

                    // Continue pagination if more pages exist
                    let has_more = json.get("has_more").and_then(|h| h.as_bool()).unwrap_or(false);
                    if has_more {
                        after_id = json.get("last_id").and_then(|l| l.as_str()).map(|s| s.to_string());
                        // Guard: if last_id is missing despite has_more, break to avoid infinite loop
                        if after_id.is_none() { break; }
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
            Ok(resp) if resp.status() == 401 => {
                return Ok(ProviderStatus {
                    provider_type: ProviderType::Anthropic,
                    configured: true,
                    connected: false,
                    error: Some("Invalid API key".to_string()),
                    models: vec![],
                });
            }
            Ok(resp) => {
                return Ok(ProviderStatus {
                    provider_type: ProviderType::Anthropic,
                    configured: true,
                    connected: false,
                    error: Some(format!("API error: {}", resp.status())),
                    models: vec![],
                });
            }
            Err(_) => {
                return Ok(ProviderStatus {
                    provider_type: ProviderType::Anthropic,
                    configured: true,
                    connected: false,
                    error: Some("Network error".to_string()),
                    models: vec![],
                });
            }
        }
    }

    // Sort newest first (API returns newest first, but sort by name for consistent display)
    models.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(ProviderStatus {
        provider_type: ProviderType::Anthropic,
        configured: true,
        connected: true,
        error: None,
        models,
    })
}

async fn check_openrouter_status() -> Result<ProviderStatus, String> {
    let api_key = match get_api_key_internal("openrouter") {
        Some(key) => key,
        None => {
            return Ok(ProviderStatus {
                provider_type: ProviderType::OpenRouter,
                configured: false,
                connected: false,
                error: Some("API key not configured".to_string()),
                models: vec![],
            });
        }
    };

    let client = hive_http_client()?;
    let response = client
        .get("https://openrouter.ai/api/v1/models")
        .header("Authorization", format!("Bearer {}", api_key))
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await;

    match response {
        Ok(resp) if resp.status().is_success() => {
            let mut models = Vec::new();

            if let Ok(json) = resp.json::<serde_json::Value>().await {
                if let Some(data) = json.get("data").and_then(|d| d.as_array()) {
                    for model in data {
                        let id = model.get("id").and_then(|i| i.as_str()).unwrap_or("");
                        if id.is_empty() { continue; }

                        let name = model.get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or(id)
                            .to_string();

                        let context_length = model.get("context_length")
                            .and_then(|c| c.as_u64());

                        let description = model.get("description")
                            .and_then(|d| d.as_str())
                            .map(|s| crate::content_security::safe_truncate(s, 77));

                        models.push(ProviderModel {
                            id: id.to_string(),
                            name,
                            provider: ProviderType::OpenRouter,
                            context_length,
                            description,
                        });
                    }
                }
            }

            // Sort by name for consistent display
            models.sort_by(|a, b| a.name.cmp(&b.name));

            Ok(ProviderStatus {
                provider_type: ProviderType::OpenRouter,
                configured: true,
                connected: true,
                error: None,
                models,
            })
        }
        Ok(resp) if resp.status() == 401 => Ok(ProviderStatus {
            provider_type: ProviderType::OpenRouter,
            configured: true,
            connected: false,
            error: Some("Invalid API key".to_string()),
            models: vec![],
        }),
        Ok(resp) => Ok(ProviderStatus {
            provider_type: ProviderType::OpenRouter,
            configured: true,
            connected: false,
            error: Some(format!("API error: {}", resp.status())),
            models: vec![],
        }),
        Err(_) => Ok(ProviderStatus {
            provider_type: ProviderType::OpenRouter,
            configured: true,
            connected: false,
            error: Some("Network error".to_string()),
            models: vec![],
        }),
    }
}

/// Chat with OpenRouter (OpenAI-compatible, non-streaming)
/// Unified non-streaming chat for all OpenAI-compatible providers (P3: don't reinvent the wheel).
/// Provider-specific logic is just the endpoint URL and key name — everything else is shared.
pub(crate) async fn chat_openai_compatible(provider: &str, model: String, messages: Vec<serde_json::Value>, depth: Option<ThinkingDepth>) -> Result<String, String> {
    let (endpoint, label) = openai_compat_endpoint(provider)?;
    let api_key = get_api_key_internal(provider)
        .ok_or_else(|| format!("{} API key not configured", label))?;

    let mut body = serde_json::json!({
        "model": model,
        "messages": messages,
    });

    // P1: Inject thinking depth params if set
    let extra_headers = if let Some(d) = depth {
        inject_thinking_params(&mut body, provider, d)
    } else {
        vec![]
    };

    let client = hive_http_client()?;
    let mut req = client
        .post(&endpoint)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .timeout(std::time::Duration::from_secs(120));

    for (k, v) in &extra_headers {
        req = req.header(k.as_str(), v.as_str());
    }

    let response = req.json(&body)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("{} API error ({}): {}", label, status, sanitize_api_error(&body)));
    }

    let json: serde_json::Value = response.json().await
        .map_err(|_| "Failed to parse response")?;

    let message = json.get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .ok_or("Invalid response format")?;

    let raw_content = message.get("content")
        .and_then(|c| c.as_str())
        .unwrap_or("");

    // Strip thinking tokens from content (P5: fix the pattern for all OpenAI-compat providers)
    let (clean, _thinking) = strip_thinking(raw_content);
    Ok(clean)
}

/// Endpoint dispatch for OpenAI-compatible providers. Returns (url, display_label).
pub(crate) fn openai_compat_endpoint(provider: &str) -> Result<(String, &'static str), String> {
    match provider {
        "openai" => Ok(("https://api.openai.com/v1/chat/completions".to_string(), "OpenAI")),
        "openrouter" => Ok(("https://openrouter.ai/api/v1/chat/completions".to_string(), "OpenRouter")),
        "dashscope" => Ok((format!("{}/chat/completions", DASHSCOPE_BASE), "DashScope")),
        _ => Err(format!("Not an OpenAI-compatible provider: {}", provider)),
    }
}

async fn chat_openrouter(model: String, messages: Vec<serde_json::Value>, depth: Option<ThinkingDepth>) -> Result<String, String> {
    chat_openai_compatible("openrouter", model, messages, depth).await
}

async fn chat_openai(model: String, messages: Vec<serde_json::Value>, depth: Option<ThinkingDepth>) -> Result<String, String> {
    chat_openai_compatible("openai", model, messages, depth).await
}

async fn chat_anthropic(model: String, messages: Vec<serde_json::Value>, depth: Option<ThinkingDepth>) -> Result<String, String> {
    let api_key = get_api_key_internal("anthropic")
        .ok_or_else(|| "Anthropic API key not configured".to_string())?;

    // Convert messages to Anthropic format (separate system message)
    let system_parts: Vec<&str> = messages.iter()
        .filter(|m| m.get("role").and_then(|r| r.as_str()) == Some("system"))
        .filter_map(|m| m.get("content").and_then(|c| c.as_str()))
        .collect();
    let system_message = if system_parts.is_empty() {
        None
    } else {
        Some(system_parts.join("\n\n"))
    };

    let chat_messages: Vec<_> = messages.iter()
        .filter(|m| m.get("role").and_then(|r| r.as_str()) != Some("system"))
        .cloned()
        .collect();

    let mut body = serde_json::json!({
        "model": model,
        "max_tokens": 4096,
        "messages": chat_messages,
    });

    if let Some(sys) = system_message {
        body["system"] = serde_json::Value::String(sys);
    }

    // P1: Inject thinking depth params if set
    let extra_headers = if let Some(d) = depth {
        inject_thinking_params(&mut body, "anthropic", d)
    } else {
        vec![]
    };

    let client = hive_http_client()?;
    let mut req = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_API_VERSION)
        .header("Content-Type", "application/json")
        .timeout(std::time::Duration::from_secs(120));

    for (k, v) in &extra_headers {
        req = req.header(k.as_str(), v.as_str());
    }

    let response = req.json(&body)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Anthropic API error ({}): {}", status, sanitize_api_error(&body)));
    }

    let json: serde_json::Value = response.json().await
        .map_err(|_| "Failed to parse response")?;

    // Anthropic returns content blocks — extract text, skip thinking blocks (P5: all providers)
    let content_blocks = json.get("content").and_then(|c| c.as_array());
    let raw_content = if let Some(blocks) = content_blocks {
        blocks.iter()
            .filter(|b| b.get("type").and_then(|t| t.as_str()) != Some("thinking"))
            .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join("")
    } else {
        String::new()
    };
    let (clean, _thinking) = strip_thinking(&raw_content);
    Ok(clean)
}

async fn chat_ollama(model: String, messages: Vec<serde_json::Value>) -> Result<String, String> {
    let client = hive_http_client()?;
    let response = client
        .post("http://localhost:11434/api/chat")
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": model,
            "messages": messages,
            "stream": false,
        }))
        .timeout(std::time::Duration::from_secs(300))
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Ollama chat error ({}): {}", status, sanitize_api_error(&body)));
    }

    let json: serde_json::Value = response.json().await
        .map_err(|_| "Failed to parse response")?;

    let raw_content = json.get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("");
    let (clean, _thinking) = strip_thinking(raw_content);
    Ok(clean)
}


// Tool-aware chat implementations moved to provider_tools.rs
// Streaming chat implementations moved to provider_stream.rs

#[cfg(test)]
mod tests {
    use super::*;

    // --- strip_thinking tests ---

    #[test]
    fn strip_thinking_no_tags() {
        let (content, thinking) = strip_thinking("Hello, world!");
        assert_eq!(content, "Hello, world!");
        assert!(thinking.is_none());
    }

    #[test]
    fn strip_thinking_xml_tags() {
        let input = "<think>Let me reason about this...</think>The answer is 42.";
        let (content, thinking) = strip_thinking(input);
        assert_eq!(content, "The answer is 42.");
        assert_eq!(thinking.unwrap(), "Let me reason about this...");
    }

    #[test]
    fn strip_thinking_slash_tags() {
        let input = "/think I need to consider the edge cases /think Here's my answer.";
        let (content, thinking) = strip_thinking(input);
        assert_eq!(content, "Here's my answer.");
        assert_eq!(thinking.unwrap(), "I need to consider the edge cases");
    }

    #[test]
    fn strip_thinking_multiline_xml() {
        let input = "<think>\nLine 1\nLine 2\nLine 3\n</think>\nFinal answer.";
        let (content, thinking) = strip_thinking(input);
        assert_eq!(content, "Final answer.");
        let t = thinking.unwrap();
        assert!(t.contains("Line 1"));
        assert!(t.contains("Line 3"));
    }

    #[test]
    fn strip_thinking_orphaned_close_tag() {
        let input = "</think>Clean content here.";
        let (content, thinking) = strip_thinking(input);
        assert_eq!(content, "Clean content here.");
        assert!(thinking.is_none());
    }

    #[test]
    fn strip_thinking_orphaned_open_tag() {
        let input = "<think>Partial reasoning that got cut off";
        let (content, thinking) = strip_thinking(input);
        assert!(!content.contains("<think>"));
        assert!(thinking.is_none());
    }

    #[test]
    fn strip_thinking_empty_thinking_block() {
        let input = "<think></think>Content.";
        let (content, thinking) = strip_thinking(input);
        assert_eq!(content, "Content.");
        assert!(thinking.is_none(), "Empty thinking block should be None");
    }

    #[test]
    fn strip_thinking_whitespace_only_thinking() {
        let input = "<think>   \n  \n  </think>Content.";
        let (content, thinking) = strip_thinking(input);
        assert_eq!(content, "Content.");
        assert!(thinking.is_none(), "Whitespace-only thinking should be None");
    }

    #[test]
    fn strip_thinking_multiple_blocks() {
        let input = "<think>First thought</think>Middle text.<think>Second thought</think>End.";
        let (content, thinking) = strip_thinking(input);
        assert!(content.contains("Middle text."), "Content should include text between blocks: {:?}", content);
        assert!(content.contains("End."), "Content should include trailing text: {:?}", content);
        let t = thinking.unwrap();
        assert!(t.contains("First thought"), "Thinking should contain first block");
        assert!(t.contains("Second thought"), "Thinking should contain second block");
    }

    // --- sanitize_api_error tests ---

    #[test]
    fn sanitize_redacts_sk_keys() {
        let body = r#"{"error":"Invalid key","key":"sk-abc123def456ghi789jkl012mno345"}"#;
        let sanitized = sanitize_api_error(body);
        assert!(!sanitized.contains("sk-abc123"), "API key must be redacted");
        assert!(sanitized.contains("[REDACTED]"));
    }

    #[test]
    fn sanitize_redacts_bearer_tokens() {
        let body = r#"Authorization: Bearer eyJhbGciOiJSUzI1NiIsInR5c..."#;
        let sanitized = sanitize_api_error(body);
        assert!(!sanitized.contains("eyJhbGci"), "Bearer token must be redacted");
        assert!(sanitized.contains("[REDACTED]"));
    }

    #[test]
    fn sanitize_truncates_large_bodies() {
        let large_body = "x".repeat(1000);
        let sanitized = sanitize_api_error(&large_body);
        assert!(sanitized.len() < 600, "Large bodies must be truncated");
        assert!(sanitized.ends_with("..."), "Truncated body should end with '...'");
    }

    #[test]
    fn sanitize_preserves_normal_errors() {
        let normal = "Connection refused: localhost:8080";
        let sanitized = sanitize_api_error(normal);
        assert_eq!(sanitized, normal, "Normal errors should pass through unchanged");
    }

    #[test]
    fn sanitize_redacts_key_prefix_variants() {
        let body = "key-abcdef1234567890abcdef1234567890";
        let sanitized = sanitize_api_error(body);
        assert!(sanitized.contains("[REDACTED]"));
    }

    // --- extract_reasoning_content tests ---

    #[test]
    fn extract_reasoning_present() {
        let msg = serde_json::json!({
            "content": "The answer is 42.",
            "reasoning_content": "I need to calculate 6 * 7..."
        });
        let result = extract_reasoning_content(&msg);
        assert_eq!(result.unwrap(), "I need to calculate 6 * 7...");
    }

    #[test]
    fn extract_reasoning_absent() {
        let msg = serde_json::json!({
            "content": "The answer is 42."
        });
        let result = extract_reasoning_content(&msg);
        assert!(result.is_none());
    }

    #[test]
    fn extract_reasoning_empty_string() {
        let msg = serde_json::json!({
            "content": "Answer.",
            "reasoning_content": "   "
        });
        let result = extract_reasoning_content(&msg);
        assert!(result.is_none(), "Whitespace-only reasoning should be None");
    }

    #[test]
    fn extract_reasoning_null() {
        let msg = serde_json::json!({
            "content": "Answer.",
            "reasoning_content": null
        });
        let result = extract_reasoning_content(&msg);
        assert!(result.is_none(), "null reasoning should be None");
    }

    // --- parse_thinking_depth tests ---

    #[test]
    fn parse_depth_off() {
        assert_eq!(parse_thinking_depth(&Some("off".to_string())), Some(ThinkingDepth::Off));
    }

    #[test]
    fn parse_depth_low() {
        assert_eq!(parse_thinking_depth(&Some("low".to_string())), Some(ThinkingDepth::Low));
    }

    #[test]
    fn parse_depth_medium() {
        assert_eq!(parse_thinking_depth(&Some("medium".to_string())), Some(ThinkingDepth::Medium));
    }

    #[test]
    fn parse_depth_high() {
        assert_eq!(parse_thinking_depth(&Some("high".to_string())), Some(ThinkingDepth::High));
    }

    #[test]
    fn parse_depth_none() {
        assert_eq!(parse_thinking_depth(&None), None);
    }

    #[test]
    fn parse_depth_unknown() {
        assert_eq!(parse_thinking_depth(&Some("turbo".to_string())), None);
    }

    // --- inject_thinking_params tests ---

    #[test]
    fn inject_thinking_off_noop_for_openai() {
        let mut body = serde_json::json!({"max_tokens": 4096});
        let headers = inject_thinking_params(&mut body, "openai", ThinkingDepth::Off);
        assert!(headers.is_empty());
        assert!(body.get("reasoning_effort").is_none());
    }

    #[test]
    fn inject_thinking_off_disables_dashscope() {
        let mut body = serde_json::json!({});
        inject_thinking_params(&mut body, "dashscope", ThinkingDepth::Off);
        assert_eq!(body["enable_thinking"], serde_json::json!(false));
    }

    #[test]
    fn inject_thinking_anthropic_sets_adaptive_and_header() {
        let mut body = serde_json::json!({"max_tokens": 1024});
        let headers = inject_thinking_params(&mut body, "anthropic", ThinkingDepth::Medium);
        assert_eq!(body["thinking"]["type"], "adaptive");
        assert_eq!(body["output_config"]["effort"], "medium");
        assert_eq!(body["max_tokens"], 16384, "Should bump max_tokens to 16384");
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].0, "anthropic-beta");
    }

    #[test]
    fn inject_thinking_anthropic_preserves_high_max_tokens() {
        let mut body = serde_json::json!({"max_tokens": 32768});
        inject_thinking_params(&mut body, "anthropic", ThinkingDepth::High);
        assert_eq!(body["max_tokens"], 32768, "Should NOT lower existing high max_tokens");
    }

    #[test]
    fn inject_thinking_openai_renames_max_tokens() {
        let mut body = serde_json::json!({"max_tokens": 4096});
        let headers = inject_thinking_params(&mut body, "openai", ThinkingDepth::Low);
        assert!(headers.is_empty());
        assert_eq!(body["reasoning_effort"], "low");
        assert_eq!(body["max_completion_tokens"], 4096);
        assert!(body.get("max_tokens").is_none(), "max_tokens should be removed for o-series");
    }

    #[test]
    fn inject_thinking_dashscope_budget_scaling() {
        let mut body = serde_json::json!({});
        inject_thinking_params(&mut body, "dashscope", ThinkingDepth::Low);
        assert_eq!(body["thinking_budget"], 4096);
        assert_eq!(body["enable_thinking"], true);

        let mut body = serde_json::json!({});
        inject_thinking_params(&mut body, "dashscope", ThinkingDepth::High);
        assert_eq!(body["thinking_budget"], 65536);
    }

    #[test]
    fn inject_thinking_openrouter_sets_effort() {
        let mut body = serde_json::json!({});
        inject_thinking_params(&mut body, "openrouter", ThinkingDepth::High);
        assert_eq!(body["reasoning_effort"], "high");
    }

    #[test]
    fn inject_thinking_ollama_noop() {
        let mut body = serde_json::json!({"messages": []});
        let headers = inject_thinking_params(&mut body, "ollama", ThinkingDepth::High);
        assert!(headers.is_empty());
        assert!(body.get("reasoning_effort").is_none());
        assert!(body.get("enable_thinking").is_none());
    }

    #[test]
    fn inject_thinking_local_noop() {
        let mut body = serde_json::json!({});
        let headers = inject_thinking_params(&mut body, "local", ThinkingDepth::Medium);
        assert!(headers.is_empty());
    }

    // --- is_retryable_error tests ---

    #[test]
    fn retryable_429() {
        assert!(is_retryable_error("Rate limited (429)"));
    }

    #[test]
    fn retryable_500() {
        assert!(is_retryable_error("Internal server error (500)"));
    }

    #[test]
    fn retryable_502() {
        assert!(is_retryable_error("Bad gateway (502)"));
    }

    #[test]
    fn retryable_503() {
        assert!(is_retryable_error("Service unavailable (503)"));
    }

    #[test]
    fn retryable_529() {
        assert!(is_retryable_error("Overloaded (529)"));
    }

    #[test]
    fn not_retryable_400() {
        assert!(!is_retryable_error("Bad request (400)"));
    }

    #[test]
    fn not_retryable_401() {
        assert!(!is_retryable_error("Unauthorized (401)"));
    }

    #[test]
    fn not_retryable_plain() {
        assert!(!is_retryable_error("Connection refused"));
    }

    // --- openai_compat_endpoint tests ---

    #[test]
    fn endpoint_openai() {
        let (url, label) = openai_compat_endpoint("openai").unwrap();
        assert!(url.contains("api.openai.com"));
        assert_eq!(label, "OpenAI");
    }

    #[test]
    fn endpoint_openrouter() {
        let (url, label) = openai_compat_endpoint("openrouter").unwrap();
        assert!(url.contains("openrouter.ai"));
        assert_eq!(label, "OpenRouter");
    }

    #[test]
    fn endpoint_dashscope() {
        let (url, label) = openai_compat_endpoint("dashscope").unwrap();
        assert!(url.contains("dashscope"));
        assert_eq!(label, "DashScope");
    }

    #[test]
    fn endpoint_unknown_errors() {
        assert!(openai_compat_endpoint("anthropic").is_err());
        assert!(openai_compat_endpoint("local").is_err());
    }
}

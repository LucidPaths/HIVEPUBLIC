//! Streaming chat implementations (SSE) for all providers.
//!
//! Extracted from providers.rs for modularity (P1).
//! - OpenAI-compatible SSE streaming (shared for OpenAI, OpenRouter, DashScope)
//! - Anthropic SSE streaming
//! - Ollama streaming
//! - DashScope status check + chat (grouped here as it's primarily streaming-oriented)

use crate::http_client::hive_http_client;
use crate::providers::{
    chat_openai_compatible, inject_thinking_params, openai_compat_endpoint, sanitize_api_error,
    strip_thinking, StreamResponse, StreamTokenPayload,
};
use crate::security::get_api_key_internal;
use crate::types::*;
use futures_util::StreamExt;
use tauri::{AppHandle, Emitter};

// ============================================
// Streaming chat implementations (SSE)
// ============================================

/// Stream OpenAI chat completions, emitting tokens via Tauri events.
/// Unified streaming chat for all OpenAI-compatible providers (P3: don't reinvent the wheel).
/// SSE parser is shared — only endpoint URL and key name differ per provider.
/// Emits "cloud-chat-token" for content, "cloud-thinking-token" for reasoning (P1: separate streams).
async fn stream_openai_compatible(
    app: AppHandle,
    provider: &str,
    model: String,
    messages: Vec<serde_json::Value>,
    stream_id: String,
    depth: Option<ThinkingDepth>,
) -> Result<StreamResponse, String> {
    let (endpoint, label) = openai_compat_endpoint(provider)?;
    let api_key = get_api_key_internal(provider)
        .ok_or_else(|| format!("{} API key not configured", label))?;

    let mut body = serde_json::json!({
        "model": model,
        "messages": messages,
        "stream": true,
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

    let mut full_response = String::new();
    let mut full_thinking = String::new();
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut done = false;

    while let Some(chunk) = stream.next().await {
        if done { break; }
        let chunk = chunk.map_err(|e| format!("Stream error: {}", e))?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        // Process complete SSE lines
        while let Some(pos) = buffer.find('\n') {
            let line = buffer[..pos].trim().to_string();
            buffer = buffer[pos + 1..].to_string();

            if line.starts_with("data: ") {
                let data = &line[6..];
                if data == "[DONE]" {
                    done = true;
                    break;
                }
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                    let delta = json.get("choices")
                        .and_then(|c| c.get(0))
                        .and_then(|c| c.get("delta"));

                    if let Some(delta) = delta {
                        // Check for reasoning_content (DeepSeek, some Qwen models)
                        if let Some(thinking_token) = delta.get("reasoning_content")
                            .and_then(|r| r.as_str())
                        {
                            full_thinking.push_str(thinking_token);
                            let _ = app.emit("cloud-thinking-token", StreamTokenPayload {
                                token: thinking_token.to_string(),
                                stream_id: stream_id.clone(),
                            });
                        }

                        // Regular content tokens
                        if let Some(token) = delta.get("content").and_then(|c| c.as_str()) {
                            full_response.push_str(token);
                            let _ = app.emit("cloud-chat-token", StreamTokenPayload {
                                token: token.to_string(),
                                stream_id: stream_id.clone(),
                            });
                        }
                    }
                }
            }
        }
    }

    // Post-process: strip any /think or <think> tags that leaked into content stream
    // (some providers embed thinking in the content field instead of reasoning_content)
    let (clean_content, inline_thinking) = strip_thinking(&full_response);
    if let Some(it) = inline_thinking {
        if full_thinking.is_empty() {
            full_thinking = it;
        } else {
            full_thinking.push_str("\n\n");
            full_thinking.push_str(&it);
        }
    }

    let thinking = if full_thinking.trim().is_empty() { None } else { Some(full_thinking) };
    Ok(StreamResponse { content: clean_content, thinking })
}

pub(crate) async fn stream_openai(
    app: AppHandle,
    model: String,
    messages: Vec<serde_json::Value>,
    stream_id: String,
    depth: Option<ThinkingDepth>,
) -> Result<StreamResponse, String> {
    stream_openai_compatible(app, "openai", model, messages, stream_id, depth).await
}

// ============================================
// DashScope (Alibaba) — OpenAI-compatible
// ============================================

use crate::providers::DASHSCOPE_BASE;

pub(crate) async fn check_dashscope_status() -> Result<ProviderStatus, String> {
    let api_key = match get_api_key_internal("dashscope") {
        Some(key) => key,
        None => {
            return Ok(ProviderStatus {
                provider_type: ProviderType::DashScope,
                configured: false,
                connected: false,
                error: Some("API key not configured".to_string()),
                models: vec![],
            });
        }
    };

    // DashScope Coding Plan — validate key with a minimal request, return bundled models.
    let client = hive_http_client()?;
    let response = client
        .post(format!("{}/chat/completions", DASHSCOPE_BASE))
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": "kimi-k2.5",
            "messages": [{"role": "user", "content": "hi"}],
            "max_tokens": 1,
        }))
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await;

    let dashscope_models = vec![
        ProviderModel {
            id: "kimi-k2.5".to_string(),
            name: "Kimi K2.5".to_string(),
            provider: ProviderType::DashScope,
            context_length: Some(131072),
            description: Some("Kimi K2.5 with thinking mode".to_string()),
        },
        ProviderModel {
            id: "qwen3-max".to_string(),
            name: "Qwen3 Max".to_string(),
            provider: ProviderType::DashScope,
            context_length: Some(32768),
            description: Some("Alibaba's flagship model".to_string()),
        },
        ProviderModel {
            id: "qwen-plus".to_string(),
            name: "Qwen Plus".to_string(),
            provider: ProviderType::DashScope,
            context_length: Some(131072),
            description: Some("Balanced performance and cost".to_string()),
        },
        ProviderModel {
            id: "qwen-turbo".to_string(),
            name: "Qwen Turbo".to_string(),
            provider: ProviderType::DashScope,
            context_length: Some(131072),
            description: Some("Fast and cost-effective".to_string()),
        },
    ];

    match response {
        Ok(resp) if resp.status().is_success() => {
            Ok(ProviderStatus {
                provider_type: ProviderType::DashScope,
                configured: true,
                connected: true,
                error: None,
                models: dashscope_models,
            })
        }
        Ok(resp) => {
            // Coding Plan endpoint only supports POST /chat/completions — any non-200
            // could be auth, model-not-found, rate limit, etc. Surface the real error
            // and still return the hardcoded model list so the user can try manually.
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            let is_auth_error = status == 401 || status == 403;
            let hint = if is_auth_error {
                format!("Auth failed ({}): {}", status, sanitize_api_error(&body))
            } else {
                format!("Status check {} (may be transient): {}", status, sanitize_api_error(&body))
            };
            Ok(ProviderStatus {
                provider_type: ProviderType::DashScope,
                configured: true,
                connected: !is_auth_error,
                error: Some(hint),
                models: if is_auth_error { vec![] } else { dashscope_models },
            })
        }
        Err(e) => Ok(ProviderStatus {
            provider_type: ProviderType::DashScope,
            configured: true,
            connected: false,
            error: Some(format!("Connection failed: {}", e)),
            models: vec![],
        }),
    }
}

/// Chat with DashScope (OpenAI-compatible, non-streaming)
pub(crate) async fn chat_dashscope(model: String, messages: Vec<serde_json::Value>, depth: Option<ThinkingDepth>) -> Result<String, String> {
    chat_openai_compatible("dashscope", model, messages, depth).await
}

pub(crate) async fn stream_dashscope(
    app: AppHandle,
    model: String,
    messages: Vec<serde_json::Value>,
    stream_id: String,
    depth: Option<ThinkingDepth>,
) -> Result<StreamResponse, String> {
    stream_openai_compatible(app, "dashscope", model, messages, stream_id, depth).await
}

/// Stream Anthropic messages API, emitting tokens via Tauri events.
/// Separates thinking blocks from text content (P1: modularity, P2: provider-agnostic output).
pub(crate) async fn stream_anthropic(
    app: AppHandle,
    model: String,
    messages: Vec<serde_json::Value>,
    stream_id: String,
    depth: Option<ThinkingDepth>,
) -> Result<StreamResponse, String> {
    let api_key = get_api_key_internal("anthropic")
        .ok_or_else(|| "Anthropic API key not configured".to_string())?;

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
        "stream": true,
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
        .header("anthropic-version", crate::providers::ANTHROPIC_API_VERSION)
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

    let mut full_response = String::new();
    let mut full_thinking = String::new();
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    // Track which content block we're in (Anthropic sends thinking as a separate block)
    let mut current_block_type = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Stream error: {}", e))?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(pos) = buffer.find('\n') {
            let line = buffer[..pos].trim().to_string();
            buffer = buffer[pos + 1..].to_string();

            if line.starts_with("data: ") {
                let data = &line[6..];
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                    let event_type = json.get("type").and_then(|t| t.as_str()).unwrap_or("");

                    // Track content block type (text vs thinking)
                    if event_type == "content_block_start" {
                        current_block_type = json.get("content_block")
                            .and_then(|b| b.get("type"))
                            .and_then(|t| t.as_str())
                            .unwrap_or("text")
                            .to_string();
                    }

                    if event_type == "content_block_delta" {
                        let delta = json.get("delta");
                        let delta_type = delta
                            .and_then(|d| d.get("type"))
                            .and_then(|t| t.as_str())
                            .unwrap_or("");

                        if delta_type == "thinking_delta" {
                            // Anthropic thinking block delta
                            if let Some(token) = delta
                                .and_then(|d| d.get("thinking"))
                                .and_then(|t| t.as_str())
                            {
                                full_thinking.push_str(token);
                                let _ = app.emit("cloud-thinking-token", StreamTokenPayload {
                                    token: token.to_string(),
                                    stream_id: stream_id.clone(),
                                });
                            }
                        } else if current_block_type == "thinking" {
                            // Fallback: block was marked as thinking type
                            if let Some(token) = delta
                                .and_then(|d| d.get("text"))
                                .and_then(|t| t.as_str())
                            {
                                full_thinking.push_str(token);
                                let _ = app.emit("cloud-thinking-token", StreamTokenPayload {
                                    token: token.to_string(),
                                    stream_id: stream_id.clone(),
                                });
                            }
                        } else if let Some(token) = delta
                            .and_then(|d| d.get("text"))
                            .and_then(|t| t.as_str())
                        {
                            full_response.push_str(token);
                            let _ = app.emit("cloud-chat-token", StreamTokenPayload {
                                token: token.to_string(),
                                stream_id: stream_id.clone(),
                            });
                        }
                    }
                }
            }
        }
    }

    let thinking = if full_thinking.trim().is_empty() { None } else { Some(full_thinking) };
    Ok(StreamResponse { content: full_response, thinking })
}

/// Stream Ollama chat completions, emitting tokens via Tauri events.
pub(crate) async fn stream_ollama(
    app: AppHandle,
    model: String,
    messages: Vec<serde_json::Value>,
    stream_id: String,
) -> Result<StreamResponse, String> {
    let client = hive_http_client()?;
    let response = client
        .post("http://localhost:11434/api/chat")
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": model,
            "messages": messages,
            "stream": true,
        }))
        .timeout(std::time::Duration::from_secs(300))
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Ollama stream error ({}): {}", status, sanitize_api_error(&body)));
    }

    let mut full_response = String::new();
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Stream error: {}", e))?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        // Ollama streams NDJSON (one JSON object per line)
        while let Some(pos) = buffer.find('\n') {
            let line = buffer[..pos].trim().to_string();
            buffer = buffer[pos + 1..].to_string();

            if line.is_empty() {
                continue;
            }

            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line) {
                if let Some(token) = json
                    .get("message")
                    .and_then(|m| m.get("content"))
                    .and_then(|c| c.as_str())
                {
                    full_response.push_str(token);
                    let _ = app.emit("cloud-chat-token", StreamTokenPayload {
                        token: token.to_string(),
                        stream_id: stream_id.clone(),
                    });
                }
            }
        }
    }

    // Post-process: strip thinking tags from Ollama responses (e.g. DeepSeek R1 via Ollama)
    let (clean_content, thinking) = strip_thinking(&full_response);
    Ok(StreamResponse { content: clean_content, thinking })
}

/// Stream OpenRouter chat completions (OpenAI-compatible SSE), emitting tokens via Tauri events.
pub(crate) async fn stream_openrouter(
    app: AppHandle,
    model: String,
    messages: Vec<serde_json::Value>,
    stream_id: String,
    depth: Option<ThinkingDepth>,
) -> Result<StreamResponse, String> {
    stream_openai_compatible(app, "openrouter", model, messages, stream_id, depth).await
}

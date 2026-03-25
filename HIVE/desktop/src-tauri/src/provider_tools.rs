//! Tool-aware chat implementations for all providers.
//!
//! Extracted from providers.rs for modularity (P1).
//! - Tool schema formatting (OpenAI, Anthropic, text-based for local)
//! - Tool call parsing (OpenAI native, Hermes XML, Kimi, DeepSeek, Mistral, bare JSON)
//! - Tool-enabled chat functions for OpenAI-compat, Anthropic, and local providers

use crate::http_client::hive_http_client;
use crate::providers::{
    extract_reasoning_content, inject_thinking_params, sanitize_api_error, strip_thinking,
    ChatResponse, DASHSCOPE_BASE,
};
use crate::security::get_api_key_internal;
use crate::tools::{ToolCall, ToolSchema};
use crate::types::ThinkingDepth;

// ============================================
// Tool-aware chat implementations
// ============================================

/// Convert HIVE tool schemas to OpenAI-format tools array.
fn tools_to_openai_format(tools: &[ToolSchema]) -> Vec<serde_json::Value> {
    tools.iter().map(|t| {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": t.name,
                "description": t.description,
                "parameters": t.parameters,
            }
        })
    }).collect()
}

/// Convert HIVE tool schemas to Anthropic-format tools array.
fn tools_to_anthropic_format(tools: &[ToolSchema]) -> Vec<serde_json::Value> {
    tools.iter().map(|t| {
        serde_json::json!({
            "name": t.name,
            "description": t.description,
            "input_schema": t.parameters,
        })
    }).collect()
}

/// Parse tool calls from an OpenAI-format response.
fn parse_openai_tool_calls(message: &serde_json::Value) -> Option<Vec<ToolCall>> {
    let tool_calls = message.get("tool_calls")?.as_array()?;

    let calls: Vec<ToolCall> = tool_calls.iter().filter_map(|tc| {
        let id = tc.get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("call_0")
            .to_string();

        let function = tc.get("function")?;
        let name = function.get("name")?.as_str()?.to_string();
        let args_str = function.get("arguments")
            .and_then(|v| v.as_str())
            .unwrap_or("{}");

        let arguments = serde_json::from_str(args_str).unwrap_or(serde_json::json!({}));

        Some(ToolCall { id, name, arguments })
    }).collect();

    if calls.is_empty() { None } else { Some(calls) }
}

/// Chat with OpenAI-compatible APIs (OpenAI, Ollama, local llama-server) with tools.
pub(crate) async fn chat_openai_format_with_tools(
    provider: &str,
    model: String,
    messages: Vec<serde_json::Value>,
    tools: Vec<ToolSchema>,
    depth: Option<ThinkingDepth>,
) -> Result<ChatResponse, String> {
    let (url, auth_header) = match provider {
        "openai" => {
            let api_key = get_api_key_internal("openai")
                .ok_or_else(|| "OpenAI API key not configured".to_string())?;
            (
                "https://api.openai.com/v1/chat/completions".to_string(),
                Some(format!("Bearer {}", api_key)),
            )
        }
        "openrouter" => {
            let api_key = get_api_key_internal("openrouter")
                .ok_or_else(|| "OpenRouter API key not configured".to_string())?;
            (
                "https://openrouter.ai/api/v1/chat/completions".to_string(),
                Some(format!("Bearer {}", api_key)),
            )
        }
        "ollama" => (
            "http://localhost:11434/v1/chat/completions".to_string(),
            None,
        ),
        "dashscope" => {
            let api_key = get_api_key_internal("dashscope")
                .ok_or_else(|| "DashScope API key not configured".to_string())?;
            (
                format!("{}/chat/completions", DASHSCOPE_BASE),
                Some(format!("Bearer {}", api_key)),
            )
        }
        _ => return Err(format!("Unknown OpenAI-format provider: {}", provider)),
    };

    // DashScope/Kimi K2.5 supports native OpenAI-compatible tool calling via tools[] API
    // parameter + tool_choice:"auto". The API returns structured tool_calls arrays with
    // finish_reason:"tool_calls". Text-based injection caused unreliable tool calling
    // after restart (Kimi would output raw <tool_call> tags or native special tokens
    // as text instead of structured calls). Fallback parsers in the cascade below still
    // catch any tool calls that leak into content as text (Kimi native tokens, Hermes,
    // DeepSeek, Mistral, bare JSON).
    let mut body = serde_json::json!({
        "model": model,
        "messages": messages,
    });

    if !tools.is_empty() {
        body["tools"] = serde_json::Value::Array(tools_to_openai_format(&tools));
        body["tool_choice"] = serde_json::json!("auto");
    }

    // P1: Inject thinking depth params if set
    let extra_headers = if let Some(d) = depth {
        inject_thinking_params(&mut body, provider, d)
    } else {
        vec![]
    };

    let client = hive_http_client()?;
    let mut req = client.post(&url)
        .header("Content-Type", "application/json")
        .timeout(std::time::Duration::from_secs(120));

    if let Some(auth) = auth_header {
        req = req.header("Authorization", auth);
    }
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
        return Err(format!("API error ({}): {}", status, sanitize_api_error(&body)));
    }

    let json: serde_json::Value = response.json().await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    let message = json.get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .ok_or("Invalid response format")?;

    let finish_reason = json.get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("finish_reason"))
        .and_then(|f| f.as_str())
        .unwrap_or("stop");

    // Extract thinking from `reasoning_content` field (DeepSeek, some Qwen) or inline /think tags
    let reasoning = extract_reasoning_content(message);
    let raw_content = message.get("content")
        .and_then(|c| c.as_str())
        .unwrap_or("");
    let (clean_content, inline_thinking) = strip_thinking(raw_content);
    // Merge both sources of thinking (field-level + inline tags)
    let thinking = match (reasoning, inline_thinking) {
        (Some(r), Some(i)) => Some(format!("{}\n\n{}", r, i)),
        (Some(r), None) => Some(r),
        (None, i) => i,
    };

    // Check if the model wants to call tools
    if finish_reason == "tool_calls" || finish_reason == "tool" {
        if let Some(tool_calls) = parse_openai_tool_calls(message) {
            let content = if clean_content.is_empty() { None } else { Some(clean_content) };
            return Ok(ChatResponse::ToolCalls { content, thinking, tool_calls });
        }
    }

    // Also check for tool_calls even if finish_reason doesn't indicate it
    // (some providers set it differently)
    if let Some(tool_calls) = parse_openai_tool_calls(message) {
        let content = if clean_content.is_empty() { None } else { Some(clean_content.clone()) };
        return Ok(ChatResponse::ToolCalls { content, thinking, tool_calls });
    }

    // Fallback: extract tool calls from text content (provider-agnostic, P2).
    // Models emit tool call tokens in their native format, and cloud APIs (DashScope,
    // OpenRouter) sometimes fail to parse them into the structured tool_calls array.
    // This is the same approach vLLM uses as its PRIMARY parsing mechanism — every
    // vLLM tool parser extracts structured calls from model text output.
    // Cascade: Kimi → DeepSeek → Mistral → Hermes/Qwen (most specific first).
    if let Some(tool_calls) = parse_kimi_tool_calls_from_text(&clean_content) {
        let text_content = strip_kimi_tool_call_blocks(&clean_content);
        let text_opt = if text_content.is_empty() { None } else { Some(text_content) };
        return Ok(ChatResponse::ToolCalls { content: text_opt, thinking, tool_calls });
    }
    if let Some(tool_calls) = parse_deepseek_tool_calls_from_text(&clean_content) {
        let text_content = strip_deepseek_tool_call_blocks(&clean_content);
        let text_opt = if text_content.is_empty() { None } else { Some(text_content) };
        return Ok(ChatResponse::ToolCalls { content: text_opt, thinking, tool_calls });
    }
    if let Some(tool_calls) = parse_mistral_tool_calls_from_text(&clean_content) {
        let text_content = strip_mistral_tool_call_blocks(&clean_content);
        let text_opt = if text_content.is_empty() { None } else { Some(text_content) };
        return Ok(ChatResponse::ToolCalls { content: text_opt, thinking, tool_calls });
    }
    if let Some(tool_calls) = parse_tool_calls_from_text(&clean_content) {
        let text_content = strip_tool_call_blocks(&clean_content);
        let text_opt = if text_content.is_empty() { None } else { Some(text_content) };
        return Ok(ChatResponse::ToolCalls { content: text_opt, thinking, tool_calls });
    }
    // Last resort: bare JSON tool call in markdown code blocks (no <tool_call> tags).
    // Kimi K2.5 sometimes outputs ```json\n{"name":"tool","arguments":{...}}\n```
    // instead of <tool_call> tags despite being told not to.
    if let Some(tool_calls) = parse_bare_json_tool_calls(&clean_content) {
        let text_content = strip_bare_json_tool_call_blocks(&clean_content);
        let text_opt = if text_content.is_empty() { None } else { Some(text_content) };
        return Ok(ChatResponse::ToolCalls { content: text_opt, thinking, tool_calls });
    }

    // Regular text response
    Ok(ChatResponse::Text { content: clean_content, thinking })
}

/// Chat with Anthropic API with tools.
pub(crate) async fn chat_anthropic_with_tools(
    model: String,
    messages: Vec<serde_json::Value>,
    tools: Vec<ToolSchema>,
    depth: Option<ThinkingDepth>,
) -> Result<ChatResponse, String> {
    let api_key = get_api_key_internal("anthropic")
        .ok_or_else(|| "Anthropic API key not configured".to_string())?;

    // Separate system message
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

    if !tools.is_empty() {
        body["tools"] = serde_json::Value::Array(tools_to_anthropic_format(&tools));
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
        return Err(format!("API error ({}): {}", status, sanitize_api_error(&body)));
    }

    let json: serde_json::Value = response.json().await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    let stop_reason = json.get("stop_reason")
        .and_then(|s| s.as_str())
        .unwrap_or("end_turn");

    let content_blocks = json.get("content")
        .and_then(|c| c.as_array())
        .cloned()
        .unwrap_or_default();

    // Collect text, thinking, and tool_use blocks separately (P1: modularity)
    let mut text_parts = Vec::new();
    let mut thinking_parts = Vec::new();
    let mut tool_calls = Vec::new();

    for block in &content_blocks {
        let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");

        match block_type {
            "text" => {
                if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                    text_parts.push(text.to_string());
                }
            }
            "thinking" => {
                if let Some(text) = block.get("thinking").and_then(|t| t.as_str()) {
                    thinking_parts.push(text.to_string());
                }
            }
            "tool_use" => {
                let id = block.get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("call_0")
                    .to_string();
                let name = block.get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let arguments = block.get("input")
                    .cloned()
                    .unwrap_or(serde_json::json!({}));

                tool_calls.push(ToolCall { id, name, arguments });
            }
            _ => {}
        }
    }

    // Also strip any inline /think or <think> tags from text content
    let raw_text = text_parts.join("\n");
    let (clean_text, inline_thinking) = strip_thinking(&raw_text);
    if let Some(it) = inline_thinking {
        thinking_parts.push(it);
    }
    let thinking = if thinking_parts.is_empty() { None } else { Some(thinking_parts.join("\n\n")) };

    if !tool_calls.is_empty() || stop_reason == "tool_use" {
        let content = if clean_text.is_empty() { None } else { Some(clean_text) };
        return Ok(ChatResponse::ToolCalls { content, thinking, tool_calls });
    }

    Ok(ChatResponse::Text { content: clean_text, thinking })
}

/// Build a system prompt section that describes available tools.
/// Uses the Hermes/Qwen `<tools>` + `<tool_call>` format — the native format
/// for Nanbeige, Qwen, Hermes, and the llama.cpp generic fallback.
fn build_tools_system_prompt(tools: &[ToolSchema]) -> String {
    // Build JSON tool definitions array (Hermes format)
    let tool_defs: Vec<serde_json::Value> = tools.iter().map(|t| {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": t.name,
                "description": t.description,
                "parameters": t.parameters
            }
        })
    }).collect();

    let tools_json = serde_json::to_string_pretty(&tool_defs).unwrap_or_default();

    let mut prompt = String::from(
        "# Tools\n\n\
         You have access to tools. You may call one or more of them to assist with the user's request.\n\n\
         ## When to use tools\n\
         - When the task requires action: sending messages, reading files, running commands, searching the web, fetching URLs.\n\
         - Don't use tools for questions you can answer from your own knowledge.\n\n\
         ## How to call a tool\n\
         Output a JSON object inside <tool_call> tags. Include the function name and arguments:\n\n\
         <tool_call>\n\
         {\"name\": \"function_name\", \"arguments\": {\"param\": \"value\"}}\n\
         </tool_call>\n\n\
         ## Rules\n\
         1. When calling a tool, output ONLY the <tool_call> block(s) — no preamble, no narration, no extra text.\n\
         2. For multiple tools, use separate <tool_call> blocks (one per call).\n\
         3. After a tool runs, you receive the result inside <tool_response> tags. \
         Use it to formulate your answer — this is your turn to speak to the user.\n\
         4. You can chain tool calls across turns for multi-step tasks.\n\
         5. Tool names are case-sensitive. Use them exactly as listed.\n\
         6. Either SPEAK or CALL TOOLS — never both in the same turn. \
         You will always get another turn after the tool result to explain what happened.\n\n\
         ## Examples\n\n\
         User: \"what's on my desktop?\"\n\
         <tool_call>\n\
         {\"name\": \"list_directory\", \"arguments\": {\"path\": \"C:\\\\Users\\\\user\\\\Desktop\"}}\n\
         </tool_call>\n\n\
         User: \"hey, how's it going?\"\n\
         Hey! I'm doing well. What's on your mind?\n\
         (No tool call — this is casual conversation.)\n\n\
         User: \"search for rust async patterns\"\n\
         <tool_call>\n\
         {\"name\": \"web_search\", \"arguments\": {\"query\": \"rust async patterns\"}}\n\
         </tool_call>\n\n\
         ## Available tools\n\n\
         You are provided with function signatures within <tools></tools> XML tags:\n"
    );

    prompt.push_str("<tools>\n");
    prompt.push_str(&tools_json);
    prompt.push_str("\n</tools>\n");

    prompt
}

/// Compact tool prompt for small context windows (<=16K tokens).
/// Uses ~300 tokens instead of ~7K by replacing verbose JSON schemas with
/// a tight table format. The model still gets: name, description, param names+types.
/// This is the difference between tools being usable or impossible on a 4-8K model.
///
/// Format inspired by arXiv:2511.03728 (Efficient On-Device Agents) which showed
/// compact schemas achieve 6x lower overhead with equivalent tool call accuracy.
fn build_tools_system_prompt_compact(tools: &[ToolSchema]) -> String {
    let mut prompt = String::from(
        "# Tools\n\n\
         Call tools using <tool_call> XML tags.\n\n\
         ## EXACT format (follow precisely)\n\
         <tool_call>\n\
         {\"name\": \"tool_name\", \"arguments\": {\"param\": \"value\"}}\n\
         </tool_call>\n\n\
         ## Rules\n\
         - Output ONLY <tool_call> block(s) when calling tools — no other text in the same turn\n\
         - Put raw JSON inside — no ```json prefix, no markdown code blocks, no indentation\n\
         - One tool call per <tool_call> block (multiple blocks OK)\n\
         - Either SPEAK or CALL TOOLS — never both. You get another turn after the tool result to talk\n\
         - NEVER wrap <tool_call> inside markdown ``` fences — the parser cannot see it\n\n\
         ## Available tools\n"
    );

    // Build compact tool table: name | params | description
    for tool in tools {
        // Extract parameter names and types from JSON schema
        let params_compact = if let Some(props) = tool.parameters.get("properties").and_then(|p| p.as_object()) {
            let required: Vec<&str> = tool.parameters.get("required")
                .and_then(|r| r.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default();

            props.iter().map(|(name, schema)| {
                let typ = schema.get("type").and_then(|t| t.as_str()).unwrap_or("string");
                let req = if required.contains(&name.as_str()) { "" } else { "?" };
                format!("{}{}:{}", name, req, typ)
            }).collect::<Vec<_>>().join(", ")
        } else {
            String::new()
        };

        // First sentence of description only (before period or newline)
        let short_desc = tool.description
            .split_once('.')
            .map(|(first, _)| first.trim())
            .or_else(|| tool.description.split_once('\n').map(|(first, _)| first.trim()))
            .unwrap_or(tool.description.trim());

        prompt.push_str(&format!("- **{}**({}) — {}\n", tool.name, params_compact, short_desc));
    }

    prompt
}

/// Parse <tool_call> blocks from model text output.
/// Returns None if no valid tool calls found.
/// Forgiving parser: handles missing braces, markdown wrapping, truncated closing tags.
/// Small models (3B) often truncate `</tool_call>` → `</tool_ca` or omit it entirely.
/// JSON is self-delimiting, so we can extract tool calls even without the closing tag.
fn parse_tool_calls_from_text(text: &str) -> Option<Vec<ToolCall>> {
    let mut calls = Vec::new();
    let mut search_from = 0;

    while let Some(start) = text[search_from..].find("<tool_call>") {
        let abs_start = search_from + start + "<tool_call>".len();

        // Try to find proper closing tag first, fall back to end-of-text
        let (json_region, next_search) = if let Some(end) = text[abs_start..].find("</tool_call>") {
            let abs_end = abs_start + end;
            (text[abs_start..abs_end].trim().to_string(), abs_end + "</tool_call>".len())
        } else {
            // No closing tag — model output was truncated (common with 3B models).
            // Use everything after <tool_call> as the candidate region.
            // Also handle partial closing tags like </tool_ca, </too, etc.
            let remaining = text[abs_start..].trim().to_string();
            let cleaned = remaining
                .trim_end_matches(|c: char| c == '<' || c == '/')
                .trim_end()
                .to_string();
            // Strip any partial </tool... suffix
            let cleaned = if let Some(partial) = cleaned.rfind("</tool") {
                cleaned[..partial].trim().to_string()
            } else if let Some(partial) = cleaned.rfind("</too") {
                cleaned[..partial].trim().to_string()
            } else if let Some(partial) = cleaned.rfind("</to") {
                cleaned[..partial].trim().to_string()
            } else if let Some(partial) = cleaned.rfind("</t") {
                cleaned[..partial].trim().to_string()
            } else if let Some(partial) = cleaned.rfind("</") {
                cleaned[..partial].trim().to_string()
            } else {
                cleaned
            };
            (cleaned, text.len()) // consumed everything
        };

        let mut json_str = json_region;

        // Strip markdown code fences if model wrapped JSON in ```
        if json_str.starts_with("```") {
            json_str = json_str
                .trim_start_matches("```json")
                .trim_start_matches("```")
                .trim_end_matches("```")
                .trim()
                .to_string();
        }

        if json_str.is_empty() {
            search_from = next_search;
            continue;
        }

        // Try parsing as-is first, then with limited repair (M11: max 1 missing brace)
        // Aggressive repairs (double-brace, quote+brace) can construct valid tool calls
        // from attacker-crafted partial JSON — limited to single brace + truncation only.
        let parsed = serde_json::from_str::<serde_json::Value>(&json_str)
            .or_else(|_| {
                // Common model mistake: missing outer closing brace
                serde_json::from_str::<serde_json::Value>(&format!("{}}}", json_str))
            })
            .or_else(|_| {
                // Last resort: find the last '}' and try parsing up to there
                if let Some(pos) = json_str.rfind('}') {
                    serde_json::from_str::<serde_json::Value>(&json_str[..=pos])
                } else {
                    Err(serde_json::from_str::<serde_json::Value>("!").unwrap_err())
                }
            });

        if let Ok(parsed) = parsed {
            let name = parsed.get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            if !name.is_empty() {
                let arguments = parsed.get("arguments")
                    .cloned()
                    .unwrap_or(serde_json::json!({}));

                calls.push(ToolCall {
                    id: format!("call_{}", calls.len()),
                    name,
                    arguments,
                });
            }
        }

        search_from = next_search;
    }

    if calls.is_empty() { None } else { Some(calls) }
}

/// Parse Kimi-native tool call tokens that leaked into the content field.
/// Format: <|tool_call_begin|>functions.func_name:idx<|tool_call_argument_begin|>{...}<|tool_call_end|>
/// This is a KNOWN Kimi K2/K2.5 issue documented by Moonshot AI: special tokens sometimes
/// appear in `content` instead of being mapped to the OpenAI `tool_calls` array.
fn parse_kimi_tool_calls_from_text(text: &str) -> Option<Vec<ToolCall>> {
    let mut calls = Vec::new();
    let mut search_from = 0;
    let begin_tag = "<|tool_call_begin|>";
    let arg_tag = "<|tool_call_argument_begin|>";
    let end_tag = "<|tool_call_end|>";

    while let Some(start) = text[search_from..].find(begin_tag) {
        let abs_start = search_from + start + begin_tag.len();

        // Extract the function ID (e.g., "functions.get_weather:0")
        let remaining = &text[abs_start..];
        let arg_pos = remaining.find(arg_tag);
        if arg_pos.is_none() {
            search_from = abs_start;
            continue;
        }
        let arg_pos = arg_pos.unwrap();
        let func_id = remaining[..arg_pos].trim();

        // Extract function name from "functions.name:idx" format
        let name = func_id
            .strip_prefix("functions.")
            .and_then(|s| s.split(':').next())
            .unwrap_or(func_id)
            .to_string();

        if name.is_empty() {
            search_from = abs_start + arg_pos;
            continue;
        }

        // Extract JSON arguments
        let json_start = abs_start + arg_pos + arg_tag.len();
        let json_end = text[json_start..].find(end_tag)
            .map(|e| json_start + e)
            .unwrap_or(text.len());
        let json_str = text[json_start..json_end].trim();

        let arguments = serde_json::from_str(json_str).unwrap_or(serde_json::json!({}));

        calls.push(ToolCall {
            id: func_id.to_string(),
            name,
            arguments,
        });

        search_from = json_end + end_tag.len().min(text.len() - json_end);
    }

    if calls.is_empty() { None } else { Some(calls) }
}

/// Parse DeepSeek V3/V3.1 tool call tokens that leaked into content.
/// Format uses Unicode punctuation: <｜tool▁call▁begin｜>name<｜tool▁sep｜>{json}<｜tool▁call▁end｜>
/// Note: ｜ is U+FF5C (fullwidth vertical bar), ▁ is U+2581 (lower one eighth block).
fn parse_deepseek_tool_calls_from_text(text: &str) -> Option<Vec<ToolCall>> {
    let mut calls = Vec::new();
    let mut search_from = 0;
    let begin_tag = "<｜tool▁call▁begin｜>";
    let sep_tag = "<｜tool▁sep｜>";
    let end_tag = "<｜tool▁call▁end｜>";

    while let Some(start) = text[search_from..].find(begin_tag) {
        let abs_start = search_from + start + begin_tag.len();
        let remaining = &text[abs_start..];

        // Find the separator between name and arguments
        let sep_pos = match remaining.find(sep_tag) {
            Some(p) => p,
            None => { search_from = abs_start; continue; }
        };
        let name = remaining[..sep_pos].trim().to_string();
        if name.is_empty() {
            search_from = abs_start + sep_pos;
            continue;
        }

        // Extract JSON arguments (may be wrapped in ```json ... ```)
        let json_start = abs_start + sep_pos + sep_tag.len();
        let json_end = text[json_start..].find(end_tag)
            .map(|e| json_start + e)
            .unwrap_or(text.len());
        let mut json_str = text[json_start..json_end].trim().to_string();

        // DeepSeek wraps args in markdown code fences
        if json_str.starts_with("```") {
            json_str = json_str
                .trim_start_matches("```json")
                .trim_start_matches("```")
                .trim_end_matches("```")
                .trim()
                .to_string();
        }

        let arguments = serde_json::from_str(&json_str).unwrap_or(serde_json::json!({}));

        calls.push(ToolCall {
            id: format!("call_{}", calls.len()),
            name,
            arguments,
        });

        search_from = json_end + end_tag.len().min(text.len() - json_end);
    }

    if calls.is_empty() { None } else { Some(calls) }
}

/// Strip DeepSeek tool call tokens from text content.
fn strip_deepseek_tool_call_blocks(text: &str) -> String {
    let mut result = text.to_string();
    let section_begin = "<｜tool▁calls▁begin｜>";
    let section_end = "<｜tool▁calls▁end｜>";
    let begin_tag = "<｜tool▁call▁begin｜>";
    let end_tag = "<｜tool▁call▁end｜>";
    result = result.replace(section_begin, "").replace(section_end, "");
    while let Some(start) = result.find(begin_tag) {
        if let Some(end) = result[start..].find(end_tag) {
            let abs_end = start + end + end_tag.len();
            result = format!("{}{}", &result[..start], &result[abs_end..]);
        } else {
            result = result[..start].to_string();
            break;
        }
    }
    result.trim().to_string()
}

/// Parse Mistral [TOOL_CALLS] format that leaked into content.
/// Pre-v11 format: [TOOL_CALLS][{"name":"add","arguments":{"a":3.5,"b":4}}]
/// v11+ format: [TOOL_CALLS]add{"a":3.5,"b":4}
fn parse_mistral_tool_calls_from_text(text: &str) -> Option<Vec<ToolCall>> {
    let marker = "[TOOL_CALLS]";
    let start = text.find(marker)?;
    let after = text[start + marker.len()..].trim();

    let mut calls = Vec::new();

    // Try pre-v11 format: JSON array of objects
    if after.starts_with('[') {
        if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(after) {
            for obj in arr {
                let name = obj.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                if name.is_empty() { continue; }
                let arguments = obj.get("arguments").cloned().unwrap_or(serde_json::json!({}));
                calls.push(ToolCall {
                    id: format!("call_{}", calls.len()),
                    name,
                    arguments,
                });
            }
        }
    }

    // Try v11+ format: name{args} separated by [TOOL_CALLS]
    if calls.is_empty() {
        let parts: Vec<&str> = text[start..].split(marker).filter(|s| !s.is_empty()).collect();
        for part in parts {
            let trimmed = part.trim();
            // Find where the function name ends and JSON begins
            if let Some(brace_pos) = trimmed.find('{') {
                let name = trimmed[..brace_pos].trim().to_string();
                if name.is_empty() { continue; }
                let json_str = &trimmed[brace_pos..];
                let arguments = serde_json::from_str(json_str).unwrap_or(serde_json::json!({}));
                calls.push(ToolCall {
                    id: format!("call_{}", calls.len()),
                    name,
                    arguments,
                });
            }
        }
    }

    if calls.is_empty() { None } else { Some(calls) }
}

/// Strip Mistral [TOOL_CALLS] blocks from text content.
fn strip_mistral_tool_call_blocks(text: &str) -> String {
    if let Some(start) = text.find("[TOOL_CALLS]") {
        text[..start].trim().to_string()
    } else {
        text.to_string()
    }
}

/// Strip Kimi-native tool call tokens from text content.
fn strip_kimi_tool_call_blocks(text: &str) -> String {
    let mut result = text.to_string();
    // Remove the section wrapper if present
    result = result.replace("<|tool_calls_section_begin|>", "")
                   .replace("<|tool_calls_section_end|>", "");
    // Remove individual tool call blocks
    while let Some(start) = result.find("<|tool_call_begin|>") {
        if let Some(end) = result[start..].find("<|tool_call_end|>") {
            let abs_end = start + end + "<|tool_call_end|>".len();
            result = format!("{}{}", &result[..start], &result[abs_end..]);
        } else {
            result = result[..start].to_string();
            break;
        }
    }
    result.trim().to_string()
}

/// Strip <tool_call>...</tool_call> blocks from text, returning the remaining content.
/// Handles truncated closing tags (e.g., `</tool_ca`, `</too`) — strips from `<tool_call>` to end.
fn strip_tool_call_blocks(text: &str) -> String {
    let mut result = text.to_string();
    while let Some(start) = result.find("<tool_call>") {
        if let Some(end) = result[start..].find("</tool_call>") {
            let abs_end = start + end + "</tool_call>".len();
            result = format!("{}{}", &result[..start], &result[abs_end..]);
        } else {
            // No complete closing tag — strip everything from <tool_call> to end
            // (model output was truncated, nothing useful after this)
            result = result[..start].to_string();
            break;
        }
    }
    result.trim().to_string()
}

/// Last-resort fallback: parse bare JSON tool calls from markdown code blocks.
/// Catches models (Kimi K2.5) that output ```json\n{"name":"tool","arguments":{...}}\n```
/// instead of using <tool_call> tags. Only matches JSON with both "name" and "arguments" keys.
fn parse_bare_json_tool_calls(text: &str) -> Option<Vec<ToolCall>> {
    let mut calls = Vec::new();
    let mut search_from = 0;

    while let Some(fence_start) = text[search_from..].find("```") {
        let abs_fence = search_from + fence_start;
        let after_fence = abs_fence + 3;

        // Skip optional language tag (json, JSON, etc.)
        let content_start = if text[after_fence..].starts_with("json") {
            after_fence + 4
        } else {
            after_fence
        };
        // Skip leading whitespace/newline after fence
        let content_start = text[content_start..].find(|c: char| !c.is_whitespace())
            .map(|i| content_start + i)
            .unwrap_or(content_start);

        // Find closing ```
        if let Some(end) = text[content_start..].find("```") {
            let abs_end = content_start + end;
            let json_str = text[content_start..abs_end].trim();

            // Only try parsing if it looks like it could be a tool call JSON
            if json_str.starts_with('{') && json_str.contains("\"name\"") && json_str.contains("\"arguments\"") {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str) {
                    let name = parsed.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let has_args = parsed.get("arguments").and_then(|v| v.as_object()).is_some();
                    if !name.is_empty() && has_args {
                        calls.push(ToolCall {
                            id: format!("call_{}", calls.len()),
                            name: name.to_string(),
                            arguments: parsed.get("arguments").cloned().unwrap_or(serde_json::json!({})),
                        });
                    }
                }
            }

            search_from = abs_end + 3;
        } else {
            // No closing fence — try parsing everything after as truncated JSON
            let json_str = text[content_start..].trim()
                .trim_end_matches('`')
                .trim();
            if json_str.starts_with('{') && json_str.contains("\"name\"") && json_str.contains("\"arguments\"") {
                // Try with brace repair (same as parse_tool_calls_from_text)
                let parsed = serde_json::from_str::<serde_json::Value>(json_str)
                    .or_else(|_| serde_json::from_str::<serde_json::Value>(&format!("{}}}", json_str)))
                    .or_else(|_| serde_json::from_str::<serde_json::Value>(&format!("{}}}}}", json_str)))
                    .or_else(|_| serde_json::from_str::<serde_json::Value>(&format!("{}\"}}}}}}", json_str)));
                if let Ok(parsed) = parsed {
                    let name = parsed.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let has_args = parsed.get("arguments").and_then(|v| v.as_object()).is_some();
                    if !name.is_empty() && has_args {
                        calls.push(ToolCall {
                            id: format!("call_{}", calls.len()),
                            name: name.to_string(),
                            arguments: parsed.get("arguments").cloned().unwrap_or(serde_json::json!({})),
                        });
                    }
                }
            }
            break;
        }
    }

    if calls.is_empty() { None } else { Some(calls) }
}

/// Strip markdown code blocks containing tool call JSON from text.
fn strip_bare_json_tool_call_blocks(text: &str) -> String {
    let mut result = String::new();
    let mut search_from = 0;

    while let Some(fence_start) = text[search_from..].find("```") {
        let abs_fence = search_from + fence_start;
        let after_fence = abs_fence + 3;

        let content_start = if text[after_fence..].starts_with("json") {
            after_fence + 4
        } else {
            after_fence
        };

        if let Some(end) = text[content_start..].find("```") {
            let abs_end = content_start + end;
            let json_str = text[content_start..abs_end].trim();

            // Only strip if this code block is actually a tool call
            if json_str.starts_with('{') && json_str.contains("\"name\"") && json_str.contains("\"arguments\"") {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str) {
                    if parsed.get("name").and_then(|v| v.as_str()).is_some()
                        && parsed.get("arguments").and_then(|v| v.as_object()).is_some()
                    {
                        // Skip this code block — it's a tool call
                        result.push_str(&text[search_from..abs_fence]);
                        search_from = abs_end + 3;
                        continue;
                    }
                }
            }

            // Not a tool call code block — keep it
            result.push_str(&text[search_from..abs_end + 3]);
            search_from = abs_end + 3;
        } else {
            break;
        }
    }

    result.push_str(&text[search_from..]);
    result.trim().to_string()
}

/// Merge consecutive messages with the same role into one.
/// Many local models (Ministral, Llama, etc.) enforce strict role alternation
/// (user → assistant → user → assistant). Tool results converted to "user"
/// can create consecutive same-role messages, which crashes these models.
fn merge_consecutive_roles(messages: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
    let mut merged: Vec<serde_json::Value> = Vec::new();

    for msg in messages {
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("").to_string();
        let content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("").to_string();

        if let Some(last) = merged.last_mut() {
            let last_role = last.get("role").and_then(|r| r.as_str()).unwrap_or("");
            if last_role == role && role != "system" {
                // Same role — merge content
                let last_content = last.get("content").and_then(|c| c.as_str()).unwrap_or("");
                *last = serde_json::json!({
                    "role": role,
                    "content": format!("{}\n\n{}", last_content, content)
                });
                continue;
            }
        }

        merged.push(msg);
    }

    merged
}

/// Chat with local llama-server with tools via system prompt injection.
/// Works with ANY model — no --jinja or native tool API required.
/// Tool descriptions are injected into the system prompt, and the model's
/// text output is parsed for <tool_call> blocks.
///
/// When context_length <= 16K, uses a compact tool format (~300 tokens)
/// instead of full JSON schemas (~7K tokens). This is critical for small
/// orchestrator models — HIVE's primary use case (P8: Low Floor).
pub(crate) async fn chat_local_with_tools(
    messages: Vec<serde_json::Value>,
    tools: Vec<ToolSchema>,
    context_length: Option<u64>,
    port: u16,
) -> Result<ChatResponse, String> {
    // Pick compact vs full format based on context window size.
    // 16K threshold: full schemas are ~7K tokens, leaving only ~9K for
    // conversation + memory + response. Below 16K, compact is mandatory.
    let ctx = context_length.unwrap_or(4096);
    let tools_prompt = if ctx <= 16384 {
        build_tools_system_prompt_compact(&tools)
    } else {
        build_tools_system_prompt(&tools)
    };

    let mut api_messages = Vec::new();

    // Find existing system message, or create one
    // Also convert role:"tool" → role:"user" (local models don't understand tool role)
    let mut has_system = false;
    for msg in &messages {
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
        if role == "system" {
            let existing = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");
            api_messages.push(serde_json::json!({
                "role": "system",
                "content": format!("{}\n\n{}", existing, tools_prompt)
            }));
            has_system = true;
        } else if role == "tool" {
            // Convert tool results to user messages for local models
            // Uses <tool_response> tags — the Hermes/Nanbeige native format
            let content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");
            api_messages.push(serde_json::json!({
                "role": "user",
                "content": format!("<tool_response>\n{}\n</tool_response>", content)
            }));
        } else {
            api_messages.push(msg.clone());
        }
    }

    if !has_system {
        // Prepend system message with tools prompt
        api_messages.insert(0, serde_json::json!({
            "role": "system",
            "content": tools_prompt
        }));
    }

    // Enforce strict role alternation for local models
    let api_messages = merge_consecutive_roles(api_messages);

    let body = serde_json::json!({
        "messages": api_messages,
        // llama.cpp KV cache: reuse prefix tokens from previous request on slot 0.
        // The stable system prompt is identical across turns, so llama-server skips
        // re-evaluating those tokens. Major perf win for local models.
        "cache_prompt": true,
        "id_slot": 0,
    });

    let client = hive_http_client()?;
    let response = client
        .post(format!("http://127.0.0.1:{}/v1/chat/completions", port))
        .header("Content-Type", "application/json")
        .json(&body)
        .timeout(std::time::Duration::from_secs(300))
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let err_body = response.text().await.unwrap_or_default();
        return Err(format!("Local server error ({}): {}", status, sanitize_api_error(&err_body)));
    }

    let json: serde_json::Value = response.json().await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    let message = json.get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .ok_or("Invalid response format")?;

    let content = message.get("content")
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();

    // Strip thinking from local model output (DeepSeek R1 uses <think>, Qwen uses /think)
    let (clean_content, thinking) = strip_thinking(&content);

    // Parse tool calls from the model's text output
    if let Some(tool_calls) = parse_tool_calls_from_text(&clean_content) {
        let text_content = strip_tool_call_blocks(&clean_content);
        let text_opt = if text_content.is_empty() { None } else { Some(text_content) };
        return Ok(ChatResponse::ToolCalls { content: text_opt, thinking, tool_calls });
    }

    Ok(ChatResponse::Text { content: clean_content, thinking })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::RiskLevel;

    fn sample_tools() -> Vec<ToolSchema> {
        vec![
            ToolSchema {
                name: "read_file".to_string(),
                description: "Read a file from disk".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "File path"}
                    },
                    "required": ["path"]
                }),
                risk_level: RiskLevel::Low,
            },
            ToolSchema {
                name: "web_search".to_string(),
                description: "Search the web".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {"type": "string"}
                    },
                    "required": ["query"]
                }),
                risk_level: RiskLevel::Medium,
            },
        ]
    }

    // --- tools_to_openai_format tests ---

    #[test]
    fn openai_format_wraps_in_function_type() {
        let tools = sample_tools();
        let formatted = tools_to_openai_format(&tools);
        assert_eq!(formatted.len(), 2);
        assert_eq!(formatted[0]["type"], "function");
        assert_eq!(formatted[0]["function"]["name"], "read_file");
        assert_eq!(formatted[1]["function"]["name"], "web_search");
    }

    #[test]
    fn openai_format_preserves_parameters() {
        let tools = sample_tools();
        let formatted = tools_to_openai_format(&tools);
        assert_eq!(formatted[0]["function"]["parameters"]["type"], "object");
        assert!(formatted[0]["function"]["parameters"]["properties"]["path"].is_object());
    }

    #[test]
    fn openai_format_empty_tools() {
        let formatted = tools_to_openai_format(&[]);
        assert!(formatted.is_empty());
    }

    // --- tools_to_anthropic_format tests ---

    #[test]
    fn anthropic_format_uses_input_schema() {
        let tools = sample_tools();
        let formatted = tools_to_anthropic_format(&tools);
        assert_eq!(formatted.len(), 2);
        assert_eq!(formatted[0]["name"], "read_file");
        // Anthropic uses input_schema, NOT parameters
        assert!(formatted[0].get("input_schema").is_some());
        assert!(formatted[0].get("parameters").is_none());
    }

    #[test]
    fn anthropic_format_no_type_wrapper() {
        let tools = sample_tools();
        let formatted = tools_to_anthropic_format(&tools);
        // Anthropic tools are flat objects, no {"type":"function"} wrapper
        assert!(formatted[0].get("type").is_none());
    }

    // --- parse_openai_tool_calls tests ---

    #[test]
    fn parse_openai_single_call() {
        let msg = serde_json::json!({
            "tool_calls": [{
                "id": "call_abc123",
                "type": "function",
                "function": {
                    "name": "read_file",
                    "arguments": "{\"path\": \"/tmp/test.txt\"}"
                }
            }]
        });
        let calls = parse_openai_tool_calls(&msg).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "read_file");
        assert_eq!(calls[0].id, "call_abc123");
        assert_eq!(calls[0].arguments["path"], "/tmp/test.txt");
    }

    #[test]
    fn parse_openai_multiple_calls() {
        let msg = serde_json::json!({
            "tool_calls": [
                {"id": "c1", "function": {"name": "read_file", "arguments": "{}"}},
                {"id": "c2", "function": {"name": "web_search", "arguments": "{\"query\":\"rust\"}"}}
            ]
        });
        let calls = parse_openai_tool_calls(&msg).unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[1].name, "web_search");
    }

    #[test]
    fn parse_openai_empty_array() {
        let msg = serde_json::json!({"tool_calls": []});
        assert!(parse_openai_tool_calls(&msg).is_none());
    }

    #[test]
    fn parse_openai_no_tool_calls_field() {
        let msg = serde_json::json!({"content": "Hello"});
        assert!(parse_openai_tool_calls(&msg).is_none());
    }

    #[test]
    fn parse_openai_missing_id_defaults() {
        let msg = serde_json::json!({
            "tool_calls": [{
                "function": {"name": "read_file", "arguments": "{}"}
            }]
        });
        let calls = parse_openai_tool_calls(&msg).unwrap();
        assert_eq!(calls[0].id, "call_0");
    }

    // --- parse_tool_calls_from_text (Hermes XML) tests ---

    #[test]
    fn parse_hermes_basic() {
        let text = r#"<tool_call>{"name":"read_file","arguments":{"path":"/tmp/a.txt"}}</tool_call>"#;
        let calls = parse_tool_calls_from_text(text).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "read_file");
        assert_eq!(calls[0].arguments["path"], "/tmp/a.txt");
    }

    #[test]
    fn parse_hermes_multiple() {
        let text = "<tool_call>{\"name\":\"read_file\",\"arguments\":{}}</tool_call>\n<tool_call>{\"name\":\"web_search\",\"arguments\":{\"query\":\"test\"}}</tool_call>";
        let calls = parse_tool_calls_from_text(text).unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].id, "call_0");
        assert_eq!(calls[1].id, "call_1");
    }

    #[test]
    fn parse_hermes_truncated_closing_tag() {
        let text = r#"<tool_call>{"name":"memory_save","arguments":{"content":"hello"}}</tool_ca"#;
        let calls = parse_tool_calls_from_text(text).unwrap();
        assert_eq!(calls[0].name, "memory_save");
    }

    #[test]
    fn parse_hermes_missing_closing_brace() {
        let text = r#"<tool_call>{"name":"read_file","arguments":{"path":"/tmp/a.txt"}</tool_call>"#;
        let calls = parse_tool_calls_from_text(text).unwrap();
        assert_eq!(calls[0].name, "read_file");
    }

    #[test]
    fn parse_hermes_markdown_wrapped() {
        let text = "<tool_call>```json\n{\"name\":\"read_file\",\"arguments\":{\"path\":\"/tmp\"}}\n```</tool_call>";
        let calls = parse_tool_calls_from_text(text).unwrap();
        assert_eq!(calls[0].name, "read_file");
    }

    #[test]
    fn parse_hermes_no_tool_calls() {
        assert!(parse_tool_calls_from_text("Hello, how can I help?").is_none());
    }

    #[test]
    fn parse_hermes_empty_json() {
        let text = "<tool_call></tool_call>";
        assert!(parse_tool_calls_from_text(text).is_none());
    }

    // --- parse_kimi_tool_calls_from_text tests ---

    #[test]
    fn parse_kimi_basic() {
        let text = "<|tool_call_begin|>functions.get_weather:0<|tool_call_argument_begin|>{\"city\":\"Tokyo\"}<|tool_call_end|>";
        let calls = parse_kimi_tool_calls_from_text(text).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "get_weather");
        assert_eq!(calls[0].arguments["city"], "Tokyo");
    }

    #[test]
    fn parse_kimi_no_tokens() {
        assert!(parse_kimi_tool_calls_from_text("Normal text").is_none());
    }

    // --- parse_deepseek_tool_calls_from_text tests ---

    #[test]
    fn parse_deepseek_basic() {
        let text = "<\u{ff5c}tool\u{2581}call\u{2581}begin\u{ff5c}>get_weather<\u{ff5c}tool\u{2581}sep\u{ff5c}>{\"city\":\"Berlin\"}<\u{ff5c}tool\u{2581}call\u{2581}end\u{ff5c}>";
        let calls = parse_deepseek_tool_calls_from_text(text).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "get_weather");
        assert_eq!(calls[0].arguments["city"], "Berlin");
    }

    #[test]
    fn parse_deepseek_no_tokens() {
        assert!(parse_deepseek_tool_calls_from_text("Normal text").is_none());
    }

    // --- parse_mistral_tool_calls_from_text tests ---

    #[test]
    fn parse_mistral_pre_v11_array() {
        let text = "[TOOL_CALLS][{\"name\":\"add\",\"arguments\":{\"a\":3.5,\"b\":4}}]";
        let calls = parse_mistral_tool_calls_from_text(text).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "add");
    }

    #[test]
    fn parse_mistral_v11_inline() {
        let text = "[TOOL_CALLS]get_weather{\"city\":\"Paris\"}";
        let calls = parse_mistral_tool_calls_from_text(text).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "get_weather");
        assert_eq!(calls[0].arguments["city"], "Paris");
    }

    #[test]
    fn parse_mistral_no_marker() {
        assert!(parse_mistral_tool_calls_from_text("No tool calls here").is_none());
    }

    // --- parse_bare_json_tool_calls tests ---

    #[test]
    fn parse_bare_json_in_code_block() {
        let text = "Here's my tool call:\n```json\n{\"name\":\"web_search\",\"arguments\":{\"query\":\"rust lang\"}}\n```";
        let calls = parse_bare_json_tool_calls(text).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "web_search");
    }

    #[test]
    fn parse_bare_json_not_tool_call() {
        let text = "```json\n{\"key\":\"value\"}\n```";
        assert!(parse_bare_json_tool_calls(text).is_none());
    }

    #[test]
    fn parse_bare_json_no_code_blocks() {
        assert!(parse_bare_json_tool_calls("Hello world").is_none());
    }

    // --- merge_consecutive_roles tests ---

    #[test]
    fn merge_consecutive_user_messages() {
        let msgs = vec![
            serde_json::json!({"role": "user", "content": "Hello"}),
            serde_json::json!({"role": "user", "content": "How are you?"}),
        ];
        let merged = merge_consecutive_roles(msgs);
        assert_eq!(merged.len(), 1);
        let content = merged[0]["content"].as_str().unwrap();
        assert!(content.contains("Hello"));
        assert!(content.contains("How are you?"));
    }

    #[test]
    fn merge_preserves_alternating() {
        let msgs = vec![
            serde_json::json!({"role": "user", "content": "Hi"}),
            serde_json::json!({"role": "assistant", "content": "Hello"}),
            serde_json::json!({"role": "user", "content": "Bye"}),
        ];
        let merged = merge_consecutive_roles(msgs);
        assert_eq!(merged.len(), 3);
    }

    #[test]
    fn merge_does_not_merge_system() {
        let msgs = vec![
            serde_json::json!({"role": "system", "content": "You are helpful"}),
            serde_json::json!({"role": "system", "content": "Be concise"}),
        ];
        let merged = merge_consecutive_roles(msgs);
        assert_eq!(merged.len(), 2, "System messages should NOT be merged");
    }

    #[test]
    fn merge_empty_input() {
        let merged = merge_consecutive_roles(vec![]);
        assert!(merged.is_empty());
    }
}

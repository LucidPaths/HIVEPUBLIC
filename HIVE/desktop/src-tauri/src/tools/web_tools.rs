//! Web tools — fetch URLs, search the web, extract structured data
//!
//! web_fetch:   Jina Reader (r.jina.ai) → clean markdown. Falls back to direct + html2text.
//!              Now also extracts and lists links found on the page for research/crawling.
//! web_search:  Jina Search (s.jina.ai) → structured results with full content. DDG fallback.
//! web_extract: Parse structured data from HTML — tables, links, JSON-LD, CSS selectors.
//! read_pdf:    Extract text from PDF files (local or URL).

use super::{HiveTool, RiskLevel, ToolResult};
use crate::content_security::{validate_url_ssrf, wrap_external_content};
use serde_json::json;

/// Walk the error source chain to build a full diagnostic message.
fn full_error_chain(err: &reqwest::Error) -> String {
    let mut msg = err.to_string();
    let mut source = std::error::Error::source(err);
    while let Some(cause) = source {
        msg.push_str(&format!(" → {}", cause));
        source = std::error::Error::source(cause);
    }
    msg
}

/// Truncate text to a max character count (safe for multi-byte UTF-8).
fn truncate_chars(text: String, max_chars: usize) -> String {
    let char_count = text.chars().count();
    if char_count > max_chars {
        let truncated: String = text.chars().take(max_chars).collect();
        format!(
            "{}\n\n... [content truncated, showing first {} of {} characters]",
            truncated, max_chars, char_count
        )
    } else {
        text
    }
}

/// Build a browser-like HTTP client.
fn browser_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
             AppleWebKit/537.36 (KHTML, like Gecko) \
             Chrome/131.0.0.0 Safari/537.36"
        )
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))
}

/// Fetch a URL via Jina Reader — free, no API key, returns clean markdown.
async fn fetch_via_jina(url: &str) -> Result<String, String> {
    let jina_url = format!("https://r.jina.ai/{}", urlencoding::encode(url));

    let client = reqwest::Client::builder()
        .user_agent("HIVE-Desktop/1.0")
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let response = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        client.get(&jina_url)
            .header("Accept", "text/markdown")
            .send(),
    ).await
        .map_err(|_| "Jina Reader timed out after 20 seconds".to_string())?
        .map_err(|e| full_error_chain(&e))?;

    if !response.status().is_success() {
        return Err(format!("Jina returned HTTP {}", response.status().as_u16()));
    }

    response.text().await
        .map_err(|e| format!("Failed to read response: {}", e))
}

/// Fetch a URL directly and return (content_text, raw_html).
/// Returns the text conversion + raw HTML for link/data extraction.
async fn fetch_direct_with_html(url: &str) -> Result<(String, String), String> {
    let client = browser_client()?;

    let response = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        client.get(url)
            .header("Accept", "text/html,application/xhtml+xml,*/*;q=0.8")
            .send(),
    ).await
        .map_err(|_| format!("Request to '{}' timed out after 15 seconds", url))?
        .map_err(|e| format!("Failed to fetch '{}': {}", url, full_error_chain(&e)))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("HTTP {} for '{}'{}", status.as_u16(), url,
            match status.as_u16() {
                403 => " — site is blocking automated access",
                404 => " — page not found",
                429 => " — rate limited, try again later",
                500..=599 => " — server error",
                _ => "",
            }));
    }

    let content_type = response.headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let body = response.text().await
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    if content_type.contains("html") {
        let text = html2text::from_read(body.as_bytes(), 120);
        Ok((text, body))
    } else {
        Ok((body.clone(), body))
    }
}

/// Search via Jina Search (s.jina.ai) — returns top 5 results with content as markdown.
/// Same family as Jina Reader (r.jina.ai) already used for web_fetch.
/// Uses API key for higher rate limits if configured, otherwise free tier (IP-based limits).
async fn search_via_jina(query: &str) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .user_agent("HIVE-Desktop/1.0")
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let url = format!("https://s.jina.ai/{}", urlencoding::encode(query));

    let mut request = client.get(&url)
        .header("Accept", "text/markdown");

    // Use Jina API key if available (higher rate limits)
    if let Some(key) = crate::security::get_api_key_internal("jina") {
        request = request.header("Authorization", format!("Bearer {}", key));
    }

    let response = tokio::time::timeout(
        std::time::Duration::from_secs(25),
        request.send(),
    ).await
        .map_err(|_| "Jina Search timed out after 25 seconds".to_string())?
        .map_err(|e| full_error_chain(&e))?;

    if !response.status().is_success() {
        return Err(format!("Jina Search returned HTTP {}", response.status().as_u16()));
    }

    response.text().await
        .map_err(|e| format!("Failed to read Jina Search response: {}", e))
}

/// Search via Brave Search API — requires API key (free tier: 2000 queries/month).
/// Returns structured results with titles, URLs, and descriptions.
async fn search_via_brave(query: &str) -> Result<String, String> {
    let api_key = crate::security::get_api_key_internal("brave")
        .ok_or("No Brave Search API key configured")?;

    let client = reqwest::Client::builder()
        .user_agent("HIVE-Desktop/1.0")
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let response = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        client.get("https://api.search.brave.com/res/v1/web/search")
            .header("X-Subscription-Token", &api_key)
            .header("Accept", "application/json")
            .query(&[("q", query), ("count", "5")])
            .send(),
    ).await
        .map_err(|_| "Brave Search timed out after 15 seconds".to_string())?
        .map_err(|e| full_error_chain(&e))?;

    if !response.status().is_success() {
        return Err(format!("Brave Search returned HTTP {}", response.status().as_u16()));
    }

    let json: serde_json::Value = response.json().await
        .map_err(|e| format!("Failed to parse Brave Search response: {}", e))?;

    // Extract web results into readable text
    let mut output = String::new();
    if let Some(results) = json.get("web").and_then(|w| w.get("results")).and_then(|r| r.as_array()) {
        for (i, result) in results.iter().enumerate() {
            let title = result.get("title").and_then(|v| v.as_str()).unwrap_or("(no title)");
            let url = result.get("url").and_then(|v| v.as_str()).unwrap_or("");
            let desc = result.get("description").and_then(|v| v.as_str()).unwrap_or("");
            output.push_str(&format!("{}. {}\n   {}\n   {}\n\n", i + 1, title, url, desc));
        }
    }

    if output.is_empty() {
        return Err("Brave Search returned no results".to_string());
    }
    Ok(output)
}

/// Search via SearXNG public instance — meta-search engine, no API key needed.
/// Tries multiple public instances for resilience.
async fn search_via_searxng(query: &str) -> Result<String, String> {
    let instances = [
        "https://search.sapti.me",
        "https://searx.be",
        "https://search.bus-hit.me",
    ];

    let client = browser_client()?;
    let mut last_err = String::from("All SearXNG instances failed");

    for (i, instance) in instances.iter().enumerate() {
        if i > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        let url = format!("{}/search", instance);

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            client.get(&url)
                .query(&[("q", query), ("format", "json"), ("language", "en")])
                .send(),
        ).await;

        let response = match result {
            Ok(Ok(resp)) if resp.status().is_success() => resp,
            Ok(Ok(resp)) => {
                last_err = format!("{} returned HTTP {}", instance, resp.status().as_u16());
                continue;
            }
            Ok(Err(e)) => {
                last_err = format!("{}: {}", instance, e);
                continue;
            }
            Err(_) => {
                last_err = format!("{} timed out", instance);
                continue;
            }
        };

        let json: serde_json::Value = match response.json().await {
            Ok(j) => j,
            Err(e) => {
                last_err = format!("{}: failed to parse JSON: {}", instance, e);
                continue;
            }
        };

        // Extract results
        if let Some(results) = json.get("results").and_then(|r| r.as_array()) {
            if results.is_empty() {
                last_err = format!("{} returned no results", instance);
                continue;
            }

            let mut output = String::new();
            for (i, result) in results.iter().take(8).enumerate() {
                let title = result.get("title").and_then(|v| v.as_str()).unwrap_or("(no title)");
                let url = result.get("url").and_then(|v| v.as_str()).unwrap_or("");
                let content = result.get("content").and_then(|v| v.as_str()).unwrap_or("");
                output.push_str(&format!("{}. {}\n   {}\n   {}\n\n", i + 1, title, url, content));
            }

            if !output.is_empty() {
                return Ok(output);
            }
        }

        last_err = format!("{} returned unparseable results", instance);
    }

    Err(last_err)
}

/// Extract links from raw HTML using scraper. Returns up to max_links.
fn extract_links_from_html(html: &str, base_url: &str) -> Vec<(String, String)> {
    use scraper::{Html, Selector};

    let document = Html::parse_document(html);
    let selector = match Selector::parse("a[href]") {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let mut links: Vec<(String, String)> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for element in document.select(&selector) {
        let href = match element.value().attr("href") {
            Some(h) => h.trim().to_string(),
            None => continue,
        };

        // Skip anchors, javascript:, mailto:, empty
        if href.is_empty() || href.starts_with('#') || href.starts_with("javascript:")
            || href.starts_with("mailto:") || href.starts_with("tel:") {
            continue;
        }

        // Resolve relative URLs
        let full_url = if href.starts_with("http://") || href.starts_with("https://") {
            href.clone()
        } else if href.starts_with("//") {
            format!("https:{}", href)
        } else if href.starts_with('/') {
            // Absolute path — join with base domain
            let domain: String = base_url.split('/').take(3).collect::<Vec<_>>().join("/");
            if !domain.is_empty() {
                format!("{}{}", domain, href)
            } else {
                continue;
            }
        } else {
            // Relative path
            let base = base_url.rsplit_once('/').map(|(b, _)| b).unwrap_or(base_url);
            format!("{}/{}", base, href)
        };

        if seen.contains(&full_url) {
            continue;
        }
        seen.insert(full_url.clone());

        // Get link text
        let text: String = element.text().collect::<Vec<_>>().join(" ")
            .split_whitespace().collect::<Vec<_>>().join(" ");
        let text = if text.chars().count() > 80 {
            format!("{}...", text.chars().take(77).collect::<String>())
        } else if text.is_empty() {
            "(no text)".to_string()
        } else {
            text
        };

        links.push((full_url, text));

        if links.len() >= 50 {
            break;
        }
    }

    links
}

// ============================================
// web_fetch (enhanced with link extraction)
// ============================================

pub struct WebFetchTool;

#[async_trait::async_trait]
impl HiveTool for WebFetchTool {
    fn name(&self) -> &str { "web_fetch" }

    fn description(&self) -> &str {
        "Fetch a web page and return its content as clean, readable text. Also extracts \
         links found on the page for follow-up research. Set extract_links=true to get \
         a list of links. Handles JavaScript-rendered pages, removes ads and navigation."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to fetch"
                },
                "extract_links": {
                    "type": "boolean",
                    "description": "If true, append a list of links found on the page (for research/crawling). Default: false."
                }
            },
            "required": ["url"]
        })
    }

    // Medium: can make outbound requests to arbitrary URLs. Prompt-injected instructions
    // in scraped content could trigger data exfiltration to attacker-controlled URLs (P6).
    fn risk_level(&self) -> RiskLevel { RiskLevel::Medium }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let url = params.get("url")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: url")?;

        let extract_links = params.get("extract_links")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Ok(ToolResult {
                content: format!("Invalid URL '{}': must start with http:// or https://", url),
                is_error: true,
            });
        }

        // SSRF protection — block requests to private/internal IPs
        if let Err(e) = validate_url_ssrf(url) {
            return Ok(ToolResult {
                content: format!("Security: {}", e),
                is_error: true,
            });
        }

        // Primary: Jina Reader — handles JS, ads, readability extraction
        // For link extraction, we also need raw HTML, so do a direct fetch alongside
        let jina_result = fetch_via_jina(url).await;

        let (text, links_section) = if extract_links {
            // Need raw HTML for link extraction — do direct fetch
            let (fallback_text, raw_html) = match fetch_direct_with_html(url).await {
                Ok(r) => r,
                Err(_e) => {
                    // If direct fetch fails, just use Jina without links
                    let text = jina_result.unwrap_or_else(|je| format!("Fetch failed: {}", je));
                    return Ok(ToolResult {
                        content: truncate_chars(text, 30_000),
                        is_error: false,
                    });
                }
            };

            // Extract links from raw HTML
            let links = extract_links_from_html(&raw_html, url);
            let links_text = if links.is_empty() {
                "\n\n--- Links ---\nNo links found on this page.".to_string()
            } else {
                let formatted: Vec<String> = links.iter()
                    .map(|(href, text)| format!("- [{}]({})", text, href))
                    .collect();
                format!("\n\n--- Links ({} found) ---\n{}", links.len(), formatted.join("\n"))
            };

            // Prefer Jina for content, use direct as fallback
            let content = match jina_result {
                Ok(c) if !c.trim().is_empty() => c,
                _ => fallback_text,
            };

            (content, links_text)
        } else {
            // No link extraction needed — standard fetch
            let text = match jina_result {
                Ok(content) if !content.trim().is_empty() => content,
                Ok(_) | Err(_) => {
                    match fetch_direct_with_html(url).await {
                        Ok((text, _)) => text,
                        Err(e) => return Ok(ToolResult {
                            content: e,
                            is_error: true,
                        }),
                    }
                }
            };
            (text, String::new())
        };

        let full_content = format!("{}{}", text, links_section);
        Ok(ToolResult {
            content: wrap_external_content(
                &format!("Web Fetch: {}", url),
                &truncate_chars(full_content, 28_000),  // leave room for wrapper
            ),
            is_error: false,
        })
    }
}

// ============================================
// web_search
// ============================================

pub struct WebSearchTool;

#[async_trait::async_trait]
impl HiveTool for WebSearchTool {
    fn name(&self) -> &str { "web_search" }

    fn description(&self) -> &str {
        "Search the web for a query and return results. Use this when you need to find \
         current information, look up facts, or discover URLs to read with web_fetch. \
         Returns search results with titles, URLs, and snippets."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query"
                }
            },
            "required": ["query"]
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::Low }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let query = params.get("query")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: query")?;

        // Fallback chain: Jina → Brave → SearXNG. No DDG (CAPTCHA-prone).
        let mut errors: Vec<String> = Vec::new();

        // 1. Jina Search (primary — free, full-content results)
        match search_via_jina(query).await {
            Ok(text) if !text.trim().is_empty() => {
                return Ok(ToolResult {
                    content: wrap_external_content(
                        &format!("Web Search: {}", query),
                        &truncate_chars(text, 13_000),
                    ),
                    is_error: false,
                });
            }
            Ok(_) => errors.push("Jina: empty results".into()),
            Err(e) => {
                eprintln!("[HIVE web_search] Jina failed: {}", e);
                errors.push(format!("Jina: {}", e));
            }
        }

        // 2. Brave Search (if API key configured — free tier 2000/month)
        match search_via_brave(query).await {
            Ok(text) if !text.trim().is_empty() => {
                return Ok(ToolResult {
                    content: wrap_external_content(
                        &format!("Web Search: {}", query),
                        &truncate_chars(text, 13_000),
                    ),
                    is_error: false,
                });
            }
            Ok(_) => errors.push("Brave: empty results".into()),
            Err(e) => {
                if !e.contains("No Brave Search API key") {
                    eprintln!("[HIVE web_search] Brave failed: {}", e);
                }
                errors.push(format!("Brave: {}", e));
            }
        }

        // 3. SearXNG public instances (last resort, no key needed)
        match search_via_searxng(query).await {
            Ok(text) if !text.trim().is_empty() => {
                return Ok(ToolResult {
                    content: wrap_external_content(
                        &format!("Web Search: {}", query),
                        &truncate_chars(text, 13_000),
                    ),
                    is_error: false,
                });
            }
            Ok(_) => errors.push("SearXNG: empty results".into()),
            Err(e) => {
                eprintln!("[HIVE web_search] SearXNG failed: {}", e);
                errors.push(format!("SearXNG: {}", e));
            }
        }

        // All engines failed
        Ok(ToolResult {
            content: format!(
                "All search engines failed for '{}'. Tried: {}. \
                 Tip: Add a free Brave Search API key in Settings → Integrations for more reliable search.",
                query,
                errors.join("; ")
            ),
            is_error: true,
        })
    }
}

// ============================================
// web_extract — structured data from HTML
// ============================================

pub struct WebExtractTool;

#[async_trait::async_trait]
impl HiveTool for WebExtractTool {
    fn name(&self) -> &str { "web_extract" }

    fn description(&self) -> &str {
        "Extract structured data from a web page. Can extract: \
         tables (as CSV), all links (with text and URLs), JSON-LD structured data, \
         or content matching a CSS selector. Use this when you need specific data \
         from a page rather than the full text content."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to extract data from"
                },
                "extract": {
                    "type": "string",
                    "description": "What to extract: 'tables' (HTML tables as CSV), 'links' (all links with text), 'json_ld' (JSON-LD structured data), or a CSS selector like 'div.article p' or 'h2'"
                }
            },
            "required": ["url", "extract"]
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::Medium }  // Outbound HTTP to user-supplied URL (same surface as web_fetch)

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let url = params.get("url")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: url")?;

        let extract = params.get("extract")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: extract")?;

        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Ok(ToolResult {
                content: format!("Invalid URL '{}': must start with http:// or https://", url),
                is_error: true,
            });
        }

        // SSRF protection
        if let Err(e) = validate_url_ssrf(url) {
            return Ok(ToolResult {
                content: format!("Security: {}", e),
                is_error: true,
            });
        }

        // Fetch raw HTML
        let client = browser_client()?;
        let response = tokio::time::timeout(
            std::time::Duration::from_secs(15),
            client.get(url).send(),
        ).await
            .map_err(|_| format!("Request timed out for '{}'", url))?
            .map_err(|e| format!("Failed to fetch '{}': {}", url, full_error_chain(&e)))?;

        if !response.status().is_success() {
            return Ok(ToolResult {
                content: format!("HTTP {} for '{}'", response.status().as_u16(), url),
                is_error: true,
            });
        }

        let html = response.text().await
            .map_err(|e| format!("Failed to read response: {}", e))?;

        use scraper::{Html, Selector};
        let document = Html::parse_document(&html);

        let result = match extract {
            "tables" => extract_tables(&document),
            "links" => {
                let links = extract_links_from_html(&html, url);
                if links.is_empty() {
                    "No links found on this page.".to_string()
                } else {
                    links.iter()
                        .map(|(href, text)| format!("- [{}]({})", text, href))
                        .collect::<Vec<_>>()
                        .join("\n")
                }
            }
            "json_ld" => extract_json_ld(&document),
            selector => {
                // Treat as CSS selector
                match Selector::parse(selector) {
                    Ok(sel) => {
                        let matches: Vec<String> = document.select(&sel)
                            .take(50)
                            .map(|el| el.text().collect::<Vec<_>>().join(" ").trim().to_string())
                            .filter(|t| !t.is_empty())
                            .collect();
                        if matches.is_empty() {
                            format!("No elements matching '{}' found on the page.", selector)
                        } else {
                            format!("Found {} matching elements:\n\n{}",
                                matches.len(),
                                matches.iter()
                                    .enumerate()
                                    .map(|(i, t)| format!("{}. {}", i + 1, t))
                                    .collect::<Vec<_>>()
                                    .join("\n\n"))
                        }
                    }
                    Err(e) => format!("Invalid CSS selector '{}': {:?}", selector, e),
                }
            }
        };

        Ok(ToolResult {
            content: wrap_external_content(
                &format!("Web Extract ({}): {}", extract, url),
                &truncate_chars(result, 18_000),
            ),
            is_error: false,
        })
    }
}

/// Extract all HTML tables from a document as CSV-like text.
fn extract_tables(document: &scraper::Html) -> String {
    use scraper::Selector;

    let table_sel = match Selector::parse("table") {
        Ok(s) => s,
        Err(_) => return "Failed to parse table selector".to_string(),
    };
    let tr_sel = Selector::parse("tr").unwrap();
    let th_sel = Selector::parse("th").unwrap();
    let td_sel = Selector::parse("td").unwrap();

    let tables: Vec<String> = document.select(&table_sel)
        .take(10) // max 10 tables
        .enumerate()
        .map(|(table_idx, table)| {
            let mut rows: Vec<Vec<String>> = Vec::new();

            for row in table.select(&tr_sel).take(100) {
                let cells: Vec<String> = row.select(&th_sel)
                    .chain(row.select(&td_sel))
                    .map(|cell| {
                        cell.text().collect::<Vec<_>>().join(" ")
                            .split_whitespace().collect::<Vec<_>>().join(" ")
                    })
                    .collect();

                if !cells.is_empty() {
                    rows.push(cells);
                }
            }

            if rows.is_empty() {
                return String::new();
            }

            // Format as aligned text table
            let col_count = rows.iter().map(|r| r.len()).max().unwrap_or(0);
            let mut col_widths = vec![0usize; col_count];
            for row in &rows {
                for (i, cell) in row.iter().enumerate() {
                    if i < col_count {
                        col_widths[i] = col_widths[i].max(cell.len()).min(40);
                    }
                }
            }

            let formatted_rows: Vec<String> = rows.iter()
                .map(|row| {
                    row.iter()
                        .enumerate()
                        .map(|(i, cell)| {
                            let w = col_widths.get(i).copied().unwrap_or(20);
                            if cell.len() > w { format!("{:.width$}", cell, width = w) }
                            else { format!("{:<width$}", cell, width = w) }
                        })
                        .collect::<Vec<_>>()
                        .join(" | ")
                })
                .collect();

            let header_sep = col_widths.iter()
                .map(|w| "-".repeat(*w))
                .collect::<Vec<_>>()
                .join("-+-");

            if formatted_rows.len() > 1 {
                format!("### Table {}\n{}\n{}\n{}",
                    table_idx + 1,
                    formatted_rows[0],
                    header_sep,
                    formatted_rows[1..].join("\n"))
            } else {
                format!("### Table {}\n{}", table_idx + 1, formatted_rows.join("\n"))
            }
        })
        .filter(|t| !t.is_empty())
        .collect();

    if tables.is_empty() {
        "No tables found on this page.".to_string()
    } else {
        tables.join("\n\n")
    }
}

/// Extract JSON-LD structured data from script tags.
fn extract_json_ld(document: &scraper::Html) -> String {
    use scraper::Selector;

    let sel = match Selector::parse("script[type='application/ld+json']") {
        Ok(s) => s,
        Err(_) => return "Failed to parse selector".to_string(),
    };

    let blocks: Vec<String> = document.select(&sel)
        .take(10)
        .filter_map(|el| {
            let raw = el.text().collect::<Vec<_>>().join("");
            // Pretty-print the JSON
            match serde_json::from_str::<serde_json::Value>(&raw) {
                Ok(val) => Some(serde_json::to_string_pretty(&val).unwrap_or(raw)),
                Err(_) => Some(raw),
            }
        })
        .collect();

    if blocks.is_empty() {
        "No JSON-LD structured data found on this page.".to_string()
    } else {
        format!("Found {} JSON-LD block(s):\n\n{}", blocks.len(), blocks.join("\n\n---\n\n"))
    }
}

// ============================================
// read_pdf — extract text from PDF files
// ============================================

pub struct ReadPdfTool;

#[async_trait::async_trait]
impl HiveTool for ReadPdfTool {
    fn name(&self) -> &str { "read_pdf" }

    fn description(&self) -> &str {
        "Read and extract text from a PDF file. Accepts a local file path or a URL. \
         Returns the text content of the PDF. Use this for reading documents, papers, \
         reports, etc."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Local file path or URL to the PDF"
                }
            },
            "required": ["path"]
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::Low }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let path = params.get("path")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: path")?;

        let pdf_bytes = if path.starts_with("http://") || path.starts_with("https://") {
            // SSRF protection for URL-based PDF fetch
            if let Err(e) = validate_url_ssrf(path) {
                return Ok(ToolResult {
                    content: format!("Security: {}", e),
                    is_error: true,
                });
            }
            // Download PDF from URL
            let client = browser_client()?;
            let response = tokio::time::timeout(
                std::time::Duration::from_secs(30),
                client.get(path).send(),
            ).await
                .map_err(|_| format!("PDF download timed out for '{}'", path))?
                .map_err(|e| format!("Failed to download PDF: {}", full_error_chain(&e)))?;

            if !response.status().is_success() {
                return Ok(ToolResult {
                    content: format!("HTTP {} downloading PDF from '{}'", response.status().as_u16(), path),
                    is_error: true,
                });
            }

            response.bytes().await
                .map_err(|e| format!("Failed to read PDF bytes: {}", e))?
                .to_vec()
        } else {
            // Read local file
            tokio::fs::read(path).await
                .map_err(|e| format!("Failed to read PDF file '{}': {}", path, e))?
        };

        // Extract text from PDF — catch_unwind guards against panics in pdf_extract
        // on malformed input (M29: untrusted PDFs must not crash the process — P4/P7)
        let extract_result = std::panic::catch_unwind(|| {
            pdf_extract::extract_text_from_mem(&pdf_bytes)
        });

        match extract_result {
            Ok(Ok(text)) => {
                let text = text.trim().to_string();
                if text.is_empty() {
                    Ok(ToolResult {
                        content: "PDF contains no extractable text (may be image-based/scanned).".to_string(),
                        is_error: false,
                    })
                } else {
                    Ok(ToolResult {
                        content: wrap_external_content(
                            "PDF Extract",
                            &truncate_chars(
                                format!("[PDF: {} pages worth of text, {} chars]\n\n{}",
                                    text.matches('\n').count() / 40 + 1,  // rough page estimate
                                    text.len(),
                                    text),
                                28_000,  // leave room for boundary markers
                            ),
                        ),
                        is_error: false,
                    })
                }
            }
            Ok(Err(e)) => Ok(ToolResult {
                content: format!("Failed to extract text from PDF: {}", e),
                is_error: true,
            }),
            Err(_) => Ok(ToolResult {
                content: "PDF extraction panicked — file is likely malformed or corrupted.".to_string(),
                is_error: true,
            }),
        }
    }
}

//! Shared HTTP client with HIVE User-Agent header

/// Create an HTTP client with HIVE User-Agent (required by HuggingFace, good practice for all APIs)
pub fn hive_http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent("HIVE-Desktop/1.0")
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))
}

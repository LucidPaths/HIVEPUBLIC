//! GitHub Tools — "doors and keys" integration
//!
//! HIVE ships the door (this code). The user provides the key (Personal Access Token).
//! No key = tools don't register. With key = tools appear in capability manifest.
//!
//! Uses the GitHub REST API directly via reqwest (no extra dependencies).
//! Reference: https://docs.github.com/en/rest
//!
//! Provider-agnostic: ANY model that supports tool use can drive these tools.

use super::{HiveTool, RiskLevel, ToolResult};
use crate::content_security::wrap_external_content;
use serde_json::json;

const GITHUB_API_BASE: &str = "https://api.github.com";

/// Validate repo format: must be exactly "owner/repo" with no path traversal (P6).
fn validate_repo(repo: &str) -> Result<(), String> {
    if !repo.contains('/') || repo.matches('/').count() != 1
        || repo.contains("..") || repo.contains('?') || repo.contains('#')
    {
        return Err(format!("Invalid repo format '{}' — expected 'owner/repo'", repo));
    }
    Ok(())
}

/// Get the GitHub PAT from encrypted storage.
fn get_github_token() -> Option<String> {
    crate::security::get_api_key_internal("github")
}

/// Build an authenticated GitHub API client.
fn github_client(token: &str) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent("HIVE-Desktop/1.0")
        .default_headers({
            let mut headers = reqwest::header::HeaderMap::new();
            headers.insert(
                reqwest::header::AUTHORIZATION,
                reqwest::header::HeaderValue::from_str(&format!("Bearer {}", token))
                    .map_err(|_| "Invalid token format")?,
            );
            headers.insert(
                reqwest::header::ACCEPT,
                reqwest::header::HeaderValue::from_static("application/vnd.github+json"),
            );
            headers.insert(
                "X-GitHub-Api-Version",
                reqwest::header::HeaderValue::from_static("2022-11-28"),
            );
            headers
        })
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))
}

/// Truncate text for tool output.
fn truncate(text: String, max: usize) -> String {
    if text.chars().count() > max {
        let truncated: String = text.chars().take(max).collect();
        format!("{}\n\n... [truncated, showing first {} chars]", truncated, max)
    } else {
        text
    }
}

// ============================================
// github_issues — List and search issues
// ============================================

pub struct GitHubIssuesTool;

#[async_trait::async_trait]
impl HiveTool for GitHubIssuesTool {
    fn name(&self) -> &str { "github_issues" }

    fn description(&self) -> &str {
        "List, search, or get details about GitHub issues. Can list issues for a repo, \
         get a specific issue by number, or search across repos. Requires a GitHub \
         Personal Access Token configured in Settings."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "repo": {
                    "type": "string",
                    "description": "Repository in 'owner/repo' format (e.g., 'LucidPaths/HiveMind')"
                },
                "action": {
                    "type": "string",
                    "description": "Action: 'list' (list issues), 'get' (get specific issue), 'create' (create new issue), 'search' (search issues)"
                },
                "number": {
                    "type": "integer",
                    "description": "Issue number (for 'get' action)"
                },
                "title": {
                    "type": "string",
                    "description": "Issue title (for 'create' action)"
                },
                "body": {
                    "type": "string",
                    "description": "Issue body/description (for 'create' action)"
                },
                "query": {
                    "type": "string",
                    "description": "Search query (for 'search' action). Uses GitHub search syntax."
                },
                "state": {
                    "type": "string",
                    "description": "Filter by state: 'open', 'closed', or 'all'. Default: 'open'."
                },
                "labels": {
                    "type": "string",
                    "description": "Comma-separated list of labels to filter by."
                }
            },
            "required": ["action"]
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::Medium }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let token = get_github_token().ok_or(
            "GitHub not configured. Add your Personal Access Token in Settings → Integrations."
        )?;

        let action = params.get("action")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: action")?;

        let client = github_client(&token)?;

        match action {
            "list" => {
                let repo = params.get("repo").and_then(|v| v.as_str())
                    .ok_or("Missing 'repo' parameter for list action (e.g., 'owner/repo')")?;
                let state = params.get("state").and_then(|v| v.as_str()).unwrap_or("open");

                validate_repo(repo)?;
                let mut url = format!("{}/repos/{}/issues?state={}&per_page=30",
                    GITHUB_API_BASE, repo, urlencoding::encode(state));
                if let Some(labels) = params.get("labels").and_then(|v| v.as_str()) {
                    url.push_str(&format!("&labels={}", urlencoding::encode(labels)));
                }

                let response = client.get(&url).send().await
                    .map_err(|e| format!("GitHub API request failed: {}", e))?;

                let status = response.status();
                let body = response.text().await.unwrap_or_default();

                if !status.is_success() {
                    return Ok(ToolResult {
                        content: format!("GitHub API error (HTTP {}): {}", status, body),
                        is_error: true,
                    });
                }

                let issues: Vec<serde_json::Value> = serde_json::from_str(&body)
                    .map_err(|e| format!("Failed to parse response: {}", e))?;

                if issues.is_empty() {
                    return Ok(ToolResult {
                        content: format!("No {} issues found in {}", state, repo),
                        is_error: false,
                    });
                }

                let formatted: Vec<String> = issues.iter().map(|issue| {
                    let number = issue.get("number").and_then(|v| v.as_i64()).unwrap_or(0);
                    let title = issue.get("title").and_then(|v| v.as_str()).unwrap_or("?");
                    let state = issue.get("state").and_then(|v| v.as_str()).unwrap_or("?");
                    let user = issue.get("user").and_then(|u| u.get("login")).and_then(|v| v.as_str()).unwrap_or("?");
                    let labels: Vec<&str> = issue.get("labels")
                        .and_then(|l| l.as_array())
                        .map(|arr| arr.iter().filter_map(|l| l.get("name").and_then(|v| v.as_str())).collect())
                        .unwrap_or_default();
                    let comments = issue.get("comments").and_then(|v| v.as_i64()).unwrap_or(0);

                    let label_str = if labels.is_empty() { String::new() } else { format!(" [{}]", labels.join(", ")) };
                    format!("#{} ({}) {} — by @{}{} ({} comments)", number, state, title, user, label_str, comments)
                }).collect();

                let content = format!("{} issues in {}:\n\n{}", formatted.len(), repo, formatted.join("\n"));
                Ok(ToolResult {
                    content: wrap_external_content("GitHub Issues", &truncate(content, 20_000)),
                    is_error: false,
                })
            }

            "get" => {
                let repo = params.get("repo").and_then(|v| v.as_str())
                    .ok_or("Missing 'repo' parameter")?;
                validate_repo(repo)?;
                let number = params.get("number").and_then(|v| v.as_i64())
                    .ok_or("Missing 'number' parameter for get action")?;

                let url = format!("{}/repos/{}/issues/{}", GITHUB_API_BASE, repo, number);
                let response = client.get(&url).send().await
                    .map_err(|e| format!("GitHub API request failed: {}", e))?;

                let status = response.status();
                let body = response.text().await.unwrap_or_default();

                if !status.is_success() {
                    return Ok(ToolResult {
                        content: format!("GitHub API error (HTTP {}): {}", status, body),
                        is_error: true,
                    });
                }

                let issue: serde_json::Value = serde_json::from_str(&body)
                    .map_err(|e| format!("Failed to parse response: {}", e))?;

                let title = issue.get("title").and_then(|v| v.as_str()).unwrap_or("?");
                let state = issue.get("state").and_then(|v| v.as_str()).unwrap_or("?");
                let user = issue.get("user").and_then(|u| u.get("login")).and_then(|v| v.as_str()).unwrap_or("?");
                let body_text = issue.get("body").and_then(|v| v.as_str()).unwrap_or("(no description)");
                let created = issue.get("created_at").and_then(|v| v.as_str()).unwrap_or("?");
                let comments = issue.get("comments").and_then(|v| v.as_i64()).unwrap_or(0);

                let content = format!(
                    "Issue #{} — {}\nState: {} | Author: @{} | Created: {} | Comments: {}\n\n{}",
                    number, title, state, user, created, comments, body_text
                );

                // Also fetch comments if there are any
                let comments_text = if comments > 0 {
                    let comments_url = format!("{}/repos/{}/issues/{}/comments?per_page=20", GITHUB_API_BASE, repo, number);
                    if let Ok(resp) = client.get(&comments_url).send().await {
                        if let Ok(body) = resp.text().await {
                            if let Ok(comments_arr) = serde_json::from_str::<Vec<serde_json::Value>>(&body) {
                                let formatted: Vec<String> = comments_arr.iter().map(|c| {
                                    let author = c.get("user").and_then(|u| u.get("login")).and_then(|v| v.as_str()).unwrap_or("?");
                                    let body = c.get("body").and_then(|v| v.as_str()).unwrap_or("");
                                    let date = c.get("created_at").and_then(|v| v.as_str()).unwrap_or("?");
                                    format!("@{} ({}): {}", author, date, body)
                                }).collect();
                                format!("\n\n--- Comments ---\n{}", formatted.join("\n\n"))
                            } else { String::new() }
                        } else { String::new() }
                    } else { String::new() }
                } else { String::new() };

                Ok(ToolResult {
                    content: wrap_external_content("GitHub Issue", &truncate(format!("{}{}", content, comments_text), 25_000)),
                    is_error: false,
                })
            }

            "create" => {
                let repo = params.get("repo").and_then(|v| v.as_str())
                    .ok_or("Missing 'repo' parameter for create action")?;
                validate_repo(repo)?;
                let title = params.get("title").and_then(|v| v.as_str())
                    .ok_or("Missing 'title' parameter for create action")?;
                let body = params.get("body").and_then(|v| v.as_str()).unwrap_or("");

                let url = format!("{}/repos/{}/issues", GITHUB_API_BASE, repo);
                let mut payload = json!({ "title": title, "body": body });

                if let Some(labels) = params.get("labels").and_then(|v| v.as_str()) {
                    let label_list: Vec<&str> = labels.split(',').map(|s| s.trim()).collect();
                    payload["labels"] = json!(label_list);
                }

                let response = client.post(&url).json(&payload).send().await
                    .map_err(|e| format!("GitHub API request failed: {}", e))?;

                let status = response.status();
                let body = response.text().await.unwrap_or_default();

                if !status.is_success() {
                    return Ok(ToolResult {
                        content: format!("Failed to create issue (HTTP {}): {}", status, body),
                        is_error: true,
                    });
                }

                let created: serde_json::Value = serde_json::from_str(&body)
                    .map_err(|e| format!("GitHub returned invalid JSON: {}", e))?;
                let number = created.get("number").and_then(|v| v.as_i64()).unwrap_or(0);
                let html_url = created.get("html_url").and_then(|v| v.as_str()).unwrap_or("?");

                Ok(ToolResult {
                    content: format!("Issue #{} created successfully: {}", number, html_url),
                    is_error: false,
                })
            }

            "search" => {
                let query = params.get("query").and_then(|v| v.as_str())
                    .ok_or("Missing 'query' parameter for search action")?;

                let url = format!("{}/search/issues?q={}&per_page=20", GITHUB_API_BASE,
                    urlencoding::encode(query));

                let response = client.get(&url).send().await
                    .map_err(|e| format!("GitHub API request failed: {}", e))?;

                let status = response.status();
                let body = response.text().await.unwrap_or_default();

                if !status.is_success() {
                    return Ok(ToolResult {
                        content: format!("GitHub search failed (HTTP {}): {}", status, body),
                        is_error: true,
                    });
                }

                let result: serde_json::Value = serde_json::from_str(&body)
                    .map_err(|e| format!("GitHub returned invalid JSON: {}", e))?;
                let total = result.get("total_count").and_then(|v| v.as_i64()).unwrap_or(0);
                let items = result.get("items").and_then(|v| v.as_array()).cloned().unwrap_or_default();

                if items.is_empty() {
                    return Ok(ToolResult {
                        content: format!("No results found for: {}", query),
                        is_error: false,
                    });
                }

                let formatted: Vec<String> = items.iter().map(|item| {
                    let number = item.get("number").and_then(|v| v.as_i64()).unwrap_or(0);
                    let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("?");
                    let repo = item.get("repository_url").and_then(|v| v.as_str())
                        .and_then(|u| u.rsplit('/').take(2).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("/").into())
                        .unwrap_or_else(|| "?".to_string());
                    let state = item.get("state").and_then(|v| v.as_str()).unwrap_or("?");
                    format!("{}#{} ({}) {}", repo, number, state, title)
                }).collect();

                let content = format!(
                    "Search: '{}' — {} total results (showing {}):\n\n{}",
                    query, total, formatted.len(), formatted.join("\n")
                );

                Ok(ToolResult {
                    content: wrap_external_content("GitHub Search", &truncate(content, 15_000)),
                    is_error: false,
                })
            }

            _ => Ok(ToolResult {
                content: format!("Unknown action '{}'. Use: list, get, create, search", action),
                is_error: true,
            }),
        }
    }
}

// ============================================
// github_prs — List and manage pull requests
// ============================================

pub struct GitHubPRsTool;

#[async_trait::async_trait]
impl HiveTool for GitHubPRsTool {
    fn name(&self) -> &str { "github_prs" }

    fn description(&self) -> &str {
        "List or get details about GitHub pull requests. Can list PRs for a repo or \
         get a specific PR by number including diff stats and reviews. \
         Requires a GitHub Personal Access Token."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "repo": {
                    "type": "string",
                    "description": "Repository in 'owner/repo' format"
                },
                "action": {
                    "type": "string",
                    "description": "Action: 'list' (list PRs), 'get' (get specific PR with details), 'comment' (add comment)"
                },
                "number": {
                    "type": "integer",
                    "description": "PR number (for 'get' and 'comment' actions)"
                },
                "body": {
                    "type": "string",
                    "description": "Comment text (for 'comment' action)"
                },
                "state": {
                    "type": "string",
                    "description": "Filter by state: 'open', 'closed', or 'all'. Default: 'open'."
                }
            },
            "required": ["repo", "action"]
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::Medium }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let token = get_github_token().ok_or(
            "GitHub not configured. Add your Personal Access Token in Settings → Integrations."
        )?;

        let repo = params.get("repo").and_then(|v| v.as_str())
            .ok_or("Missing required parameter: repo")?;
        validate_repo(repo)?;
        let action = params.get("action").and_then(|v| v.as_str())
            .ok_or("Missing required parameter: action")?;

        let client = github_client(&token)?;

        match action {
            "list" => {
                let state = params.get("state").and_then(|v| v.as_str()).unwrap_or("open");
                let url = format!("{}/repos/{}/pulls?state={}&per_page=30", GITHUB_API_BASE, repo, urlencoding::encode(state));

                let response = client.get(&url).send().await
                    .map_err(|e| format!("GitHub API request failed: {}", e))?;

                let status = response.status();
                let body = response.text().await.unwrap_or_default();

                if !status.is_success() {
                    return Ok(ToolResult {
                        content: format!("GitHub API error (HTTP {}): {}", status, body),
                        is_error: true,
                    });
                }

                let prs: Vec<serde_json::Value> = serde_json::from_str(&body)
                    .map_err(|e| format!("GitHub returned invalid JSON: {}", e))?;

                if prs.is_empty() {
                    return Ok(ToolResult {
                        content: format!("No {} pull requests in {}", state, repo),
                        is_error: false,
                    });
                }

                let formatted: Vec<String> = prs.iter().map(|pr| {
                    let number = pr.get("number").and_then(|v| v.as_i64()).unwrap_or(0);
                    let title = pr.get("title").and_then(|v| v.as_str()).unwrap_or("?");
                    let user = pr.get("user").and_then(|u| u.get("login")).and_then(|v| v.as_str()).unwrap_or("?");
                    let head = pr.get("head").and_then(|h| h.get("ref")).and_then(|v| v.as_str()).unwrap_or("?");
                    let base = pr.get("base").and_then(|b| b.get("ref")).and_then(|v| v.as_str()).unwrap_or("?");
                    let draft = pr.get("draft").and_then(|v| v.as_bool()).unwrap_or(false);
                    let draft_str = if draft { " [DRAFT]" } else { "" };
                    format!("#{} {} — by @{} ({} → {}){}", number, title, user, head, base, draft_str)
                }).collect();

                let content = format!("{} PRs in {}:\n\n{}", formatted.len(), repo, formatted.join("\n"));
                Ok(ToolResult {
                    content: wrap_external_content("GitHub PRs", &truncate(content, 20_000)),
                    is_error: false,
                })
            }

            "get" => {
                let number = params.get("number").and_then(|v| v.as_i64())
                    .ok_or("Missing 'number' parameter for get action")?;

                let url = format!("{}/repos/{}/pulls/{}", GITHUB_API_BASE, repo, number);
                let response = client.get(&url).send().await
                    .map_err(|e| format!("GitHub API request failed: {}", e))?;

                let status = response.status();
                let body = response.text().await.unwrap_or_default();

                if !status.is_success() {
                    return Ok(ToolResult {
                        content: format!("GitHub API error (HTTP {}): {}", status, body),
                        is_error: true,
                    });
                }

                let pr: serde_json::Value = serde_json::from_str(&body)
                    .map_err(|e| format!("GitHub returned invalid JSON: {}", e))?;

                let title = pr.get("title").and_then(|v| v.as_str()).unwrap_or("?");
                let state = pr.get("state").and_then(|v| v.as_str()).unwrap_or("?");
                let user = pr.get("user").and_then(|u| u.get("login")).and_then(|v| v.as_str()).unwrap_or("?");
                let body_text = pr.get("body").and_then(|v| v.as_str()).unwrap_or("(no description)");
                let created = pr.get("created_at").and_then(|v| v.as_str()).unwrap_or("?");
                let head = pr.get("head").and_then(|h| h.get("ref")).and_then(|v| v.as_str()).unwrap_or("?");
                let base = pr.get("base").and_then(|b| b.get("ref")).and_then(|v| v.as_str()).unwrap_or("?");
                let additions = pr.get("additions").and_then(|v| v.as_i64()).unwrap_or(0);
                let deletions = pr.get("deletions").and_then(|v| v.as_i64()).unwrap_or(0);
                let changed_files = pr.get("changed_files").and_then(|v| v.as_i64()).unwrap_or(0);
                let mergeable = pr.get("mergeable").and_then(|v| v.as_bool());
                let comments = pr.get("comments").and_then(|v| v.as_i64()).unwrap_or(0);
                let review_comments = pr.get("review_comments").and_then(|v| v.as_i64()).unwrap_or(0);

                let mergeable_str = match mergeable {
                    Some(true) => "Yes",
                    Some(false) => "No (conflicts)",
                    None => "Unknown",
                };

                let content = format!(
                    "PR #{} — {}\n\
                     State: {} | Author: @{} | Created: {}\n\
                     Branch: {} → {}\n\
                     Changes: +{} -{} across {} files\n\
                     Mergeable: {} | Comments: {} | Review comments: {}\n\n\
                     {}",
                    number, title, state, user, created,
                    head, base, additions, deletions, changed_files,
                    mergeable_str, comments, review_comments,
                    body_text
                );

                Ok(ToolResult {
                    content: wrap_external_content("GitHub PR", &truncate(content, 25_000)),
                    is_error: false,
                })
            }

            "comment" => {
                let number = params.get("number").and_then(|v| v.as_i64())
                    .ok_or("Missing 'number' parameter for comment action")?;
                let comment_body = params.get("body").and_then(|v| v.as_str())
                    .ok_or("Missing 'body' parameter for comment action")?;

                let url = format!("{}/repos/{}/issues/{}/comments", GITHUB_API_BASE, repo, number);
                let payload = json!({ "body": comment_body });

                let response = client.post(&url).json(&payload).send().await
                    .map_err(|e| format!("GitHub API request failed: {}", e))?;

                let status = response.status();
                if !status.is_success() {
                    let body = response.text().await.unwrap_or_default();
                    return Ok(ToolResult {
                        content: format!("Failed to post comment (HTTP {}): {}", status, body),
                        is_error: true,
                    });
                }

                Ok(ToolResult {
                    content: format!("Comment posted on PR #{}", number),
                    is_error: false,
                })
            }

            _ => Ok(ToolResult {
                content: format!("Unknown action '{}'. Use: list, get, comment", action),
                is_error: true,
            }),
        }
    }
}

// ============================================
// github_repos — List and search repositories
// ============================================

pub struct GitHubReposTool;

#[async_trait::async_trait]
impl HiveTool for GitHubReposTool {
    fn name(&self) -> &str { "github_repos" }

    fn description(&self) -> &str {
        "List your GitHub repositories or get details about a specific repo. \
         Can list repos for the authenticated user or any user/org. \
         Requires a GitHub Personal Access Token."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "description": "Action: 'list' (your repos), 'get' (repo details), 'list_user' (a user's repos)"
                },
                "repo": {
                    "type": "string",
                    "description": "Repository in 'owner/repo' format (for 'get' action)"
                },
                "username": {
                    "type": "string",
                    "description": "GitHub username (for 'list_user' action)"
                },
                "sort": {
                    "type": "string",
                    "description": "Sort by: 'updated', 'created', 'pushed', 'name'. Default: 'updated'."
                }
            },
            "required": ["action"]
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::Low }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let token = get_github_token().ok_or(
            "GitHub not configured. Add your Personal Access Token in Settings → Integrations."
        )?;

        let action = params.get("action").and_then(|v| v.as_str())
            .ok_or("Missing required parameter: action")?;
        let sort = params.get("sort").and_then(|v| v.as_str()).unwrap_or("updated");

        let client = github_client(&token)?;

        match action {
            "list" => {
                let url = format!("{}/user/repos?sort={}&per_page=30&type=all", GITHUB_API_BASE, urlencoding::encode(sort));
                let response = client.get(&url).send().await
                    .map_err(|e| format!("GitHub API request failed: {}", e))?;

                let status = response.status();
                let body = response.text().await.unwrap_or_default();

                if !status.is_success() {
                    return Ok(ToolResult {
                        content: format!("GitHub API error (HTTP {}): {}", status, body),
                        is_error: true,
                    });
                }

                let repos: Vec<serde_json::Value> = serde_json::from_str(&body)
                    .map_err(|e| format!("GitHub returned invalid JSON: {}", e))?;
                let formatted = format_repos(&repos);

                Ok(ToolResult {
                    content: wrap_external_content("GitHub Repos", &truncate(formatted, 20_000)),
                    is_error: false,
                })
            }

            "get" => {
                let repo = params.get("repo").and_then(|v| v.as_str())
                    .ok_or("Missing 'repo' parameter for get action")?;
                validate_repo(repo)?;

                let url = format!("{}/repos/{}", GITHUB_API_BASE, repo);
                let response = client.get(&url).send().await
                    .map_err(|e| format!("GitHub API request failed: {}", e))?;

                let status = response.status();
                let body = response.text().await.unwrap_or_default();

                if !status.is_success() {
                    return Ok(ToolResult {
                        content: format!("GitHub API error (HTTP {}): {}", status, body),
                        is_error: true,
                    });
                }

                let repo_data: serde_json::Value = serde_json::from_str(&body)
                    .map_err(|e| format!("GitHub returned invalid JSON: {}", e))?;
                let name = repo_data.get("full_name").and_then(|v| v.as_str()).unwrap_or("?");
                let description = repo_data.get("description").and_then(|v| v.as_str()).unwrap_or("(no description)");
                let language = repo_data.get("language").and_then(|v| v.as_str()).unwrap_or("?");
                let stars = repo_data.get("stargazers_count").and_then(|v| v.as_i64()).unwrap_or(0);
                let forks = repo_data.get("forks_count").and_then(|v| v.as_i64()).unwrap_or(0);
                let issues = repo_data.get("open_issues_count").and_then(|v| v.as_i64()).unwrap_or(0);
                let default_branch = repo_data.get("default_branch").and_then(|v| v.as_str()).unwrap_or("main");
                let private = repo_data.get("private").and_then(|v| v.as_bool()).unwrap_or(false);
                let visibility = if private { "Private" } else { "Public" };

                let content = format!(
                    "{} ({})\n{}\n\n\
                     Language: {} | Stars: {} | Forks: {} | Open issues: {}\n\
                     Default branch: {} | Visibility: {}",
                    name, visibility, description,
                    language, stars, forks, issues,
                    default_branch, visibility
                );

                Ok(ToolResult {
                    content: wrap_external_content("GitHub Repo", &content),
                    is_error: false,
                })
            }

            "list_user" => {
                let username = params.get("username").and_then(|v| v.as_str())
                    .ok_or("Missing 'username' parameter for list_user action")?;

                let url = format!("{}/users/{}/repos?sort={}&per_page=30", GITHUB_API_BASE, urlencoding::encode(username), urlencoding::encode(sort));
                let response = client.get(&url).send().await
                    .map_err(|e| format!("GitHub API request failed: {}", e))?;

                let status = response.status();
                let body = response.text().await.unwrap_or_default();

                if !status.is_success() {
                    return Ok(ToolResult {
                        content: format!("GitHub API error (HTTP {}): {}", status, body),
                        is_error: true,
                    });
                }

                let repos: Vec<serde_json::Value> = serde_json::from_str(&body)
                    .map_err(|e| format!("GitHub returned invalid JSON: {}", e))?;
                let formatted = format_repos(&repos);

                Ok(ToolResult {
                    content: wrap_external_content("GitHub Repos", &truncate(formatted, 20_000)),
                    is_error: false,
                })
            }

            _ => Ok(ToolResult {
                content: format!("Unknown action '{}'. Use: list, get, list_user", action),
                is_error: true,
            }),
        }
    }
}

/// Format a list of repos for display.
fn format_repos(repos: &[serde_json::Value]) -> String {
    if repos.is_empty() {
        return "No repositories found.".to_string();
    }

    let formatted: Vec<String> = repos.iter().map(|repo| {
        let name = repo.get("full_name").and_then(|v| v.as_str()).unwrap_or("?");
        let description = repo.get("description").and_then(|v| v.as_str()).unwrap_or("");
        let language = repo.get("language").and_then(|v| v.as_str()).unwrap_or("?");
        let stars = repo.get("stargazers_count").and_then(|v| v.as_i64()).unwrap_or(0);
        let private = repo.get("private").and_then(|v| v.as_bool()).unwrap_or(false);
        let visibility = if private { " [private]" } else { "" };
        let desc = if description.is_empty() { String::new() } else { format!(" — {}", description) };
        format!("{}{}{} ({}, {} stars)", name, visibility, desc, language, stars)
    }).collect();

    format!("{} repositories:\n\n{}", formatted.len(), formatted.join("\n"))
}

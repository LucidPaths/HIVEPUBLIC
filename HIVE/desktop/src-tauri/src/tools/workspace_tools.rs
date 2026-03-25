//! Workspace tools — repo cloning, file tree, code search
//!
//! Enables models to explore and analyze external repositories autonomously.
//! Workspaces are isolated temp directories with auto-generated IDs.
//!
//! Tools:
//!   repo_clone   — shallow-clone a git repo into a workspace
//!   file_tree    — recursive directory listing with depth/pattern filtering
//!   code_search  — regex search across files with context lines
//!
//! Principle alignment:
//!   P1 (Modularity) — Each tool is self-contained. Workspace state is global, not Tauri-coupled.
//!   P3 (Simplicity) — Uses git CLI (not git2 lib), regex crate (already a dep), std::fs.
//!   P6 (Secrets)    — HTTPS-only clones. Workspace paths are sandboxed to temp dir.
//!   P8 (Low/High)   — Simple clone+search for beginners. Pattern/depth controls for power users.

use super::{HiveTool, RiskLevel, ToolResult};
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tokio::sync::RwLock;

// ============================================
// Global Workspace Registry
// ============================================

static WORKSPACES: OnceLock<RwLock<HashMap<String, WorkspaceInfo>>> = OnceLock::new();

fn workspaces() -> &'static RwLock<HashMap<String, WorkspaceInfo>> {
    WORKSPACES.get_or_init(|| RwLock::new(HashMap::new()))
}

struct WorkspaceInfo {
    path: PathBuf,
}

/// Resolve a workspace_id to its path, or use path directly if not a workspace ID.
async fn resolve_workspace_path(workspace_id_or_path: &str) -> Result<PathBuf, String> {
    // Try workspace registry first
    let ws = workspaces().read().await;
    if let Some(info) = ws.get(workspace_id_or_path) {
        return Ok(info.path.clone());
    }
    drop(ws);

    // Fall back to direct path
    let path = PathBuf::from(workspace_id_or_path);
    if path.exists() {
        Ok(path)
    } else {
        Err(format!(
            "Not a valid workspace ID or path: '{}'. Clone a repo first with repo_clone.",
            workspace_id_or_path
        ))
    }
}

// ============================================
// repo_clone
// ============================================

pub struct RepoCloneTool;

#[async_trait::async_trait]
impl HiveTool for RepoCloneTool {
    fn name(&self) -> &str { "repo_clone" }

    fn description(&self) -> &str {
        "Clone a git repository into an isolated workspace for analysis. Returns a workspace_id \
         for use with file_tree and code_search. Shallow clone by default (depth=1) for speed."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "Git repository URL (HTTPS only). Example: https://github.com/user/repo"
                },
                "branch": {
                    "type": "string",
                    "description": "Branch to clone (default: repo's default branch)"
                },
                "depth": {
                    "type": "integer",
                    "description": "Shallow clone depth (default: 1). Use 0 for full history."
                }
            },
            "required": ["url"]
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::Medium }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let url = params.get("url")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: url")?;

        if !url.starts_with("https://") {
            return Ok(ToolResult {
                content: "Only HTTPS URLs allowed (security). Example: https://github.com/user/repo".to_string(),
                is_error: true,
            });
        }

        let branch = params.get("branch").and_then(|v| v.as_str());
        let depth = params.get("depth").and_then(|v| v.as_u64()).unwrap_or(1);

        // Generate workspace ID from repo name
        let repo_name = url.rsplit('/').next().unwrap_or("repo").trim_end_matches(".git");
        let workspace_id = format!("ws_{}_{}", repo_name, chrono::Utc::now().timestamp());

        let workspace_dir = std::env::temp_dir()
            .join("hive_workspaces")
            .join(&workspace_id);

        if let Err(e) = tokio::fs::create_dir_all(&workspace_dir).await {
            return Ok(ToolResult {
                content: format!("Failed to create workspace directory: {}", e),
                is_error: true,
            });
        }

        // Build git clone command
        let mut args: Vec<String> = vec!["clone".into()];
        if depth > 0 {
            args.push("--depth".into());
            args.push(depth.to_string());
        }
        if let Some(b) = branch {
            args.push("--branch".into());
            args.push(b.to_string());
        }
        args.push("--".into());
        args.push(url.to_string());
        args.push(workspace_dir.to_string_lossy().to_string());

        let mut cmd = tokio::process::Command::new("git");
        cmd.args(&args);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        let output = match tokio::time::timeout(
            std::time::Duration::from_secs(120),
            cmd.output(),
        ).await {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => {
                let _ = tokio::fs::remove_dir_all(&workspace_dir).await;
                return Ok(ToolResult {
                    content: format!("Git clone failed: {}. Is git installed and on PATH?", e),
                    is_error: true,
                });
            }
            Err(_) => {
                let _ = tokio::fs::remove_dir_all(&workspace_dir).await;
                return Ok(ToolResult {
                    content: "Git clone timed out after 120 seconds".to_string(),
                    is_error: true,
                });
            }
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let _ = tokio::fs::remove_dir_all(&workspace_dir).await;
            return Ok(ToolResult {
                content: format!("Git clone failed: {}", stderr),
                is_error: true,
            });
        }

        let file_count = count_files_recursive(&workspace_dir).await;

        workspaces().write().await.insert(workspace_id.clone(), WorkspaceInfo {
            path: workspace_dir.clone(),
        });

        Ok(ToolResult {
            content: format!(
                "Cloned successfully.\n  workspace_id: {}\n  path: {}\n  files: {}\n\n\
                 Use file_tree(workspace_id=\"{}\") to explore structure.\n\
                 Use code_search(workspace_id=\"{}\", pattern=\"...\") to find code.",
                workspace_id,
                workspace_dir.display(),
                file_count,
                workspace_id,
                workspace_id,
            ),
            is_error: false,
        })
    }
}

/// Count files recursively, skipping .git
async fn count_files_recursive(dir: &Path) -> usize {
    let mut count = 0;
    let mut stack = vec![dir.to_path_buf()];

    while let Some(current) = stack.pop() {
        let mut entries = match tokio::fs::read_dir(&current).await {
            Ok(e) => e,
            Err(_) => continue,
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            let name = entry.file_name();
            if name == ".git" {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else {
                count += 1;
            }
        }
    }
    count
}

// ============================================
// file_tree
// ============================================

pub struct FileTreeTool;

#[async_trait::async_trait]
impl HiveTool for FileTreeTool {
    fn name(&self) -> &str { "file_tree" }

    fn description(&self) -> &str {
        "List the directory structure of a workspace or local path as a tree. \
         Shows files and directories with sizes. Supports depth limiting and \
         glob pattern filtering. Use workspace_id from repo_clone or any local path."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "workspace_id": {
                    "type": "string",
                    "description": "Workspace ID from repo_clone, or a local directory path"
                },
                "path": {
                    "type": "string",
                    "description": "Subdirectory within the workspace to list (default: root)"
                },
                "max_depth": {
                    "type": "integer",
                    "description": "Maximum directory depth to show (default: 3)"
                },
                "include_pattern": {
                    "type": "string",
                    "description": "Only show files matching this pattern (e.g., '*.py', '*.rs')"
                },
                "exclude_pattern": {
                    "type": "string",
                    "description": "Hide entries matching these patterns, comma-separated (e.g., 'node_modules,__pycache__,.git')"
                }
            },
            "required": ["workspace_id"]
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::Low }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let ws_id = params.get("workspace_id")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: workspace_id")?;

        let base = resolve_workspace_path(ws_id).await?;
        let subpath = params.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let root = if subpath.is_empty() { base.clone() } else { base.join(subpath) };

        if !root.exists() {
            return Ok(ToolResult {
                content: format!("Path not found: '{}'", root.display()),
                is_error: true,
            });
        }

        let max_depth = params.get("max_depth")
            .and_then(|v| v.as_u64())
            .unwrap_or(3) as usize;

        let include = params.get("include_pattern")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let exclude_str = params.get("exclude_pattern")
            .and_then(|v| v.as_str())
            .unwrap_or(".git,node_modules,__pycache__,.venv,target");
        let excludes: Vec<&str> = exclude_str.split(',').map(|s| s.trim()).collect();

        let mut output = String::new();
        let mut total_files = 0usize;
        let mut total_dirs = 0usize;

        // Iterative tree building using a stack of (dir, prefix, depth)
        // We process one level at a time, pushing children onto a deferred list
        build_tree_iterative(
            &root, max_depth, include, &excludes,
            &mut output, &mut total_files, &mut total_dirs,
        ).await;

        let header = format!(
            "{}/  ({} files, {} directories, depth: {})\n",
            root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| root.display().to_string()),
            total_files, total_dirs, max_depth,
        );

        Ok(ToolResult {
            content: format!("{}{}", header, output),
            is_error: false,
        })
    }
}

/// Build a text tree iteratively (no async_recursion crate needed).
/// Uses a stack of (directory, prefix, depth) entries processed in order.
async fn build_tree_iterative(
    root: &Path,
    max_depth: usize,
    include: &str,
    excludes: &[&str],
    output: &mut String,
    total_files: &mut usize,
    total_dirs: &mut usize,
) {
    // Stack entries: (path, prefix_string, depth)
    // We process in FIFO order to maintain tree ordering, so use VecDeque
    let mut queue: std::collections::VecDeque<(PathBuf, String, usize)> = std::collections::VecDeque::new();
    queue.push_back((root.to_path_buf(), String::new(), 0));

    while let Some((dir, prefix, depth)) = queue.pop_front() {
        if depth >= max_depth {
            continue;
        }

        let mut entries: Vec<(String, PathBuf, bool, u64)> = Vec::new();
        let mut reader = match tokio::fs::read_dir(&dir).await {
            Ok(r) => r,
            Err(_) => continue,
        };

        while let Ok(Some(entry)) = reader.next_entry().await {
            let name = entry.file_name().to_string_lossy().to_string();
            if excludes.iter().any(|ex| name == *ex) {
                continue;
            }

            let path = entry.path();
            let is_dir = path.is_dir();
            let size = if !is_dir {
                tokio::fs::metadata(&path).await.map(|m| m.len()).unwrap_or(0)
            } else {
                0
            };

            if !include.is_empty() && !is_dir && !matches_simple_glob(include, &name) {
                continue;
            }

            entries.push((name, path, is_dir, size));
        }

        // Sort: directories first, then by name
        entries.sort_by(|a, b| {
            b.2.cmp(&a.2).then(a.0.to_lowercase().cmp(&b.0.to_lowercase()))
        });

        // We need to insert child directory queue entries in reverse order
        // so they come out in the right order from the front of the queue
        let count = entries.len();
        let mut child_queue_entries: Vec<(PathBuf, String, usize)> = Vec::new();

        for (i, (name, path, is_dir, size)) in entries.iter().enumerate() {
            let is_last = i == count - 1;
            let connector = if is_last { "└── " } else { "├── " };
            let child_prefix = format!("{}{}", prefix, if is_last { "    " } else { "│   " });

            if *is_dir {
                *total_dirs += 1;
                output.push_str(&format!("{}{}{}/\n", prefix, connector, name));
                child_queue_entries.push((path.clone(), child_prefix, depth + 1));
            } else {
                *total_files += 1;
                let size_str = format_size(*size);
                output.push_str(&format!("{}{}{}  ({})\n", prefix, connector, name, size_str));
            }
        }

        // Insert child dirs at the front of the queue (in order) so they're processed
        // before siblings at the same level — this preserves depth-first tree ordering
        for (i, entry) in child_queue_entries.into_iter().enumerate() {
            queue.insert(i, entry);
        }
    }
}

/// Simple glob matching — supports *.ext and prefix* patterns.
/// Uses char-safe slicing to avoid panics on multi-byte UTF-8 (B1 fix — P5).
fn matches_simple_glob(pattern: &str, name: &str) -> bool {
    if let Some(suffix) = pattern.strip_prefix('*') {
        name.ends_with(suffix)
    } else if let Some(prefix) = pattern.strip_suffix('*') {
        name.starts_with(prefix)
    } else {
        name == pattern
    }
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

// ============================================
// code_search
// ============================================

pub struct CodeSearchTool;

#[async_trait::async_trait]
impl HiveTool for CodeSearchTool {
    fn name(&self) -> &str { "code_search" }

    fn description(&self) -> &str {
        "Search for a pattern across all files in a workspace or directory. \
         Supports regex patterns. Returns matching files with line numbers and \
         context. Use workspace_id from repo_clone or any local path."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "workspace_id": {
                    "type": "string",
                    "description": "Workspace ID from repo_clone, or a local directory path"
                },
                "pattern": {
                    "type": "string",
                    "description": "Search pattern (regex supported). Examples: 'LoraConfig', 'fn\\s+main', 'import.*torch'"
                },
                "language": {
                    "type": "string",
                    "description": "Filter by file extension (e.g., 'py', 'rs', 'js', 'ts')"
                },
                "path": {
                    "type": "string",
                    "description": "Subdirectory to scope the search (default: entire workspace)"
                },
                "context_lines": {
                    "type": "integer",
                    "description": "Lines of context around each match (default: 2, max: 10)"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of matches to return (default: 30, max: 100)"
                }
            },
            "required": ["workspace_id", "pattern"]
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::Low }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let ws_id = params.get("workspace_id")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: workspace_id")?;

        let pattern_str = params.get("pattern")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: pattern")?;

        let base = resolve_workspace_path(ws_id).await?;
        let subpath = params.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let search_root = if subpath.is_empty() { base.clone() } else { base.join(subpath) };

        let language = params.get("language").and_then(|v| v.as_str()).unwrap_or("");
        let context_lines = params.get("context_lines")
            .and_then(|v| v.as_u64())
            .unwrap_or(2)
            .min(10) as usize;
        let max_results = params.get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(30)
            .min(100) as usize;

        // Compile regex
        let re = match regex::RegexBuilder::new(pattern_str)
            .case_insensitive(false)
            .build()
        {
            Ok(r) => r,
            Err(e) => return Ok(ToolResult {
                content: format!("Invalid regex pattern '{}': {}", pattern_str, e),
                is_error: true,
            }),
        };

        // Collect all searchable files
        let ext_filter = if language.is_empty() { None } else { Some(language) };
        let files = collect_files(&search_root, ext_filter).await;

        let mut matches: Vec<SearchMatch> = Vec::new();
        let mut files_searched = 0usize;
        let mut files_with_matches = 0usize;

        for file_path in &files {
            files_searched += 1;

            let content = match tokio::fs::read_to_string(file_path).await {
                Ok(c) => c,
                Err(_) => continue, // Skip binary/unreadable files
            };

            let lines: Vec<&str> = content.lines().collect();
            let mut file_had_match = false;

            for (line_idx, line) in lines.iter().enumerate() {
                if re.is_match(line) {
                    if !file_had_match {
                        files_with_matches += 1;
                        file_had_match = true;
                    }

                    // Extract context
                    let start = line_idx.saturating_sub(context_lines);
                    let end = (line_idx + context_lines + 1).min(lines.len());
                    let snippet: Vec<String> = lines[start..end]
                        .iter()
                        .enumerate()
                        .map(|(i, l)| {
                            let ln = start + i + 1;
                            let marker = if start + i == line_idx { ">" } else { " " };
                            format!("{} {:>4} | {}", marker, ln, l)
                        })
                        .collect();

                    let rel_path = file_path.strip_prefix(&base)
                        .unwrap_or(file_path)
                        .to_string_lossy()
                        .to_string();

                    matches.push(SearchMatch {
                        file: rel_path,
                        line: line_idx + 1,
                        snippet: snippet.join("\n"),
                    });

                    if matches.len() >= max_results {
                        break;
                    }
                }
            }

            if matches.len() >= max_results {
                break;
            }
        }

        if matches.is_empty() {
            return Ok(ToolResult {
                content: format!(
                    "No matches for '{}' in {} files searched.",
                    pattern_str, files_searched
                ),
                is_error: false,
            });
        }

        let mut output = format!(
            "Found {} matches in {} files ({} files searched):\n\n",
            matches.len(), files_with_matches, files_searched
        );

        for m in &matches {
            output.push_str(&format!("── {}:{} ──\n{}\n\n", m.file, m.line, m.snippet));
        }

        if matches.len() >= max_results {
            output.push_str(&format!(
                "[Results capped at {}. Use 'path' or 'language' to narrow scope.]\n",
                max_results
            ));
        }

        // Truncate if too long
        let max_chars = 25_000;
        if output.chars().count() > max_chars {
            let truncated: String = output.chars().take(max_chars).collect();
            output = format!("{}\n\n[... output truncated at {} chars]", truncated, max_chars);
        }

        Ok(ToolResult {
            content: output,
            is_error: false,
        })
    }
}

struct SearchMatch {
    file: String,
    line: usize,
    snippet: String,
}

/// Collect all searchable files under a directory, skipping binary/hidden/vendor dirs
async fn collect_files(dir: &Path, ext_filter: Option<&str>) -> Vec<PathBuf> {
    let skip_dirs = [".git", "node_modules", "__pycache__", ".venv", "target", ".tox", "dist", "build", ".next"];
    let mut files = Vec::new();
    let mut stack = vec![dir.to_path_buf()];

    while let Some(current) = stack.pop() {
        let mut reader = match tokio::fs::read_dir(&current).await {
            Ok(r) => r,
            Err(_) => continue,
        };

        while let Ok(Some(entry)) = reader.next_entry().await {
            let name = entry.file_name().to_string_lossy().to_string();
            let path = entry.path();

            if path.is_dir() {
                if !skip_dirs.contains(&name.as_str()) && !name.starts_with('.') {
                    stack.push(path);
                }
            } else {
                // Extension filter
                if let Some(ext) = ext_filter {
                    let file_ext = path.extension()
                        .map(|e| e.to_string_lossy().to_string())
                        .unwrap_or_default();
                    if file_ext != ext {
                        continue;
                    }
                }

                // Skip obviously binary files by extension
                let binary_exts = ["png", "jpg", "jpeg", "gif", "ico", "woff", "woff2", "ttf",
                    "eot", "mp3", "mp4", "zip", "tar", "gz", "pdf", "exe", "dll", "so",
                    "dylib", "bin", "dat", "db", "sqlite", "gguf", "safetensors", "pt", "onnx"];
                let file_ext = path.extension()
                    .map(|e| e.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                if binary_exts.contains(&file_ext.as_str()) {
                    continue;
                }

                files.push(path);
            }
        }
    }

    files.sort();
    files
}

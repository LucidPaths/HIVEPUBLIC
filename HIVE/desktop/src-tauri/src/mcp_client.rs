//! HIVE MCP Client — Consume external MCP servers to expand HIVE's tool repertoire.
//!
//! Connects to external MCP servers (community tools, custom servers) and makes
//! their tools available alongside HIVE's built-in tools. The model sees one
//! unified tool list — it doesn't know or care which tools are native vs MCP.
//!
//! Architecture:
//!   1. User configures MCP server (name, command/URL, transport type)
//!   2. HIVE connects via stdio (local process) or HTTP (remote server)
//!   3. Discovers tools via list_tools() + resources via list_resources()
//!   4. Wraps each as McpProxyTool/McpResourceTool implementing HiveTool
//!   5. Registers them in the shared ToolRegistry
//!   6. Model can now call external tools + browse resources transparently
//!
//! Transports (P1: Bridges, P2: Provider Agnostic):
//!   - stdio: spawns local process (e.g. npx @mcp/server-filesystem)
//!   - http:  connects to remote URL (e.g. http://mcp.example.com/sse)

use std::collections::HashMap;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use rmcp::model::{CallToolRequestParams, ReadResourceRequestParams};
use rmcp::service::{RoleClient, RunningService};
use rmcp::ServiceExt;
use rmcp::transport::child_process::TokioChildProcess;

use crate::tools::{HiveTool, RiskLevel, ToolResult, ToolState};

// ============================================
// Configuration Types
// ============================================

/// Configuration for an external MCP server.
/// P5: source of truth for transport types — mirrored in api_integrations.ts::McpServerConfig
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Human-readable name (used as tool prefix: "mcp_<name>_<tool>")
    pub name: String,
    /// Command to launch the server (stdio transport only)
    #[serde(default)]
    pub command: String,
    /// Arguments for the command (stdio transport only)
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment variables (stdio transport only)
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Transport type: "stdio" (default) or "http"
    #[serde(default = "default_transport")]
    pub transport: String,
    /// URL for HTTP transport (e.g., "http://localhost:3001/mcp")
    #[serde(default)]
    pub url: Option<String>,
}

fn default_transport() -> String {
    "stdio".to_string()
}

/// Status of a connected MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConnectionInfo {
    pub name: String,
    pub command: String,
    pub tools: Vec<String>,
    pub connected: bool,
    /// Transport used: "stdio" or "http"
    pub transport: String,
    /// URL if HTTP transport
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

// ============================================
// Client Manager
// ============================================

/// Manages connections to external MCP servers.
pub struct McpClientManager {
    connections: RwLock<HashMap<String, McpConnection>>,
}

struct McpConnection {
    config: McpServerConfig,
    service: Arc<RunningService<RoleClient, ()>>,
    tool_names: Vec<String>,
}

impl McpClientManager {
    pub fn new() -> Self {
        Self {
            connections: RwLock::new(HashMap::new()),
        }
    }

    /// Connect to an MCP server, discover its tools + resources, and register them.
    pub async fn connect(
        &self,
        config: McpServerConfig,
        tool_state: &ToolState,
    ) -> Result<Vec<String>, String> {
        let name = config.name.clone();

        // Don't reconnect if already connected
        {
            let conns = self.connections.read().await;
            if conns.contains_key(&name) {
                return Err(format!("MCP server '{}' is already connected", name));
            }
        }

        // Connect via the appropriate transport
        let service = match config.transport.as_str() {
            "http" => self.connect_http(&config).await?,
            _ => self.connect_stdio(&config).await?,
        };
        let service = Arc::new(service);

        // Discover tools
        let tools_result = service
            .list_tools(Default::default())
            .await
            .map_err(|e| format!("Failed to list tools from '{}': {:?}", name, e))?;

        let mut tool_names = Vec::new();
        let mut registry = tool_state.registry.write().await;

        for tool in &tools_result.tools {
            // Prefix tool name to avoid collisions: mcp_<server>_<tool>
            let prefixed_name = format!("mcp_{}_{}", name, tool.name);

            let proxy = McpProxyTool {
                prefixed_name: prefixed_name.clone(),
                original_name: tool.name.to_string(),
                server_name: name.clone(),
                description: tool
                    .description
                    .as_deref()
                    .unwrap_or("External MCP tool")
                    .to_string(),
                schema: serde_json::json!({
                    "type": "object",
                    "properties": serde_json::Value::Object(
                        tool.input_schema.get("properties")
                            .and_then(|v| v.as_object())
                            .cloned()
                            .unwrap_or_default()
                    ),
                    "required": tool.input_schema.get("required")
                        .cloned()
                        .unwrap_or(serde_json::Value::Array(vec![]))
                }),
                service: Arc::clone(&service),
            };

            tool_names.push(prefixed_name.clone());
            registry.register(proxy);
        }

        // Discover resources (best-effort, P4 — not all servers support this)
        if let Ok(resources_result) = service.list_resources(Default::default()).await {
            if !resources_result.resources.is_empty() {
                let resource_count = resources_result.resources.len();

                // Register list_resources tool
                let list_name = format!("mcp_{}_list_resources", name);
                let list_tool = McpResourceListTool {
                    prefixed_name: list_name.clone(),
                    server_name: name.clone(),
                    service: Arc::clone(&service),
                };
                tool_names.push(list_name.clone());
                registry.register(list_tool);

                // Register read_resource tool
                let read_name = format!("mcp_{}_read_resource", name);
                let read_tool = McpResourceReadTool {
                    prefixed_name: read_name.clone(),
                    server_name: name.clone(),
                    service: Arc::clone(&service),
                };
                tool_names.push(read_name.clone());
                registry.register(read_tool);

                crate::tools::log_tools::append_to_app_log(&format!(
                    "MCP | resources_discovered | server={} | {} resources, registered list+read tools",
                    name, resource_count
                ));
            }
        }

        // Store connection
        let connection = McpConnection {
            config,
            service,
            tool_names: tool_names.clone(),
        };
        self.connections.write().await.insert(name.clone(), connection);

        crate::tools::log_tools::append_to_app_log(&format!(
            "MCP | connected | server={} | {} tools: {}", name, tool_names.len(), tool_names.join(", ")
        ));

        Ok(tool_names)
    }

    /// Connect via stdio transport (spawn local process).
    async fn connect_stdio(&self, config: &McpServerConfig) -> Result<RunningService<RoleClient, ()>, String> {
        let mut cmd = tokio::process::Command::new(&config.command);
        cmd.args(&config.args);
        for (k, v) in &config.env {
            cmd.env(k, v);
        }

        let child = TokioChildProcess::new(cmd)
            .map_err(|e| format!("Failed to spawn MCP server '{}': {}", config.name, e))?;

        let service: RunningService<RoleClient, ()> = ().serve(child)
            .await
            .map_err(|e| format!("MCP handshake failed for '{}': {:?}", config.name, e))?;

        crate::tools::log_tools::append_to_app_log(&format!(
            "MCP | transport=stdio | server={} | command={}", config.name, config.command
        ));

        Ok(service)
    }

    /// Connect via streamable HTTP transport (remote server).
    async fn connect_http(&self, config: &McpServerConfig) -> Result<RunningService<RoleClient, ()>, String> {
        let url = config.url.as_deref()
            .ok_or_else(|| format!("MCP server '{}': HTTP transport requires a URL", config.name))?;

        // P6: Prevent SSRF — block connections to localhost, private IPs, cloud metadata
        crate::content_security::validate_url_ssrf(url)
            .map_err(|e| format!("MCP server '{}': {}", config.name, e))?;

        use rmcp::transport::StreamableHttpClientTransport;

        let transport = StreamableHttpClientTransport::from_uri(url);

        let service = ().serve(transport)
            .await
            .map_err(|e| format!("MCP HTTP handshake failed for '{}': {:?}", config.name, e))?;

        crate::tools::log_tools::append_to_app_log(&format!(
            "MCP | transport=http | server={} | url={}", config.name, url
        ));

        Ok(service)
    }

    /// Disconnect from an MCP server and unregister its tools.
    pub async fn disconnect(
        &self,
        name: &str,
        tool_state: &ToolState,
    ) -> Result<(), String> {
        let conn = self
            .connections
            .write()
            .await
            .remove(name)
            .ok_or_else(|| format!("MCP server '{}' is not connected", name))?;

        // Unregister all tools from this server
        let mut registry = tool_state.registry.write().await;
        for tool_name in &conn.tool_names {
            registry.unregister(tool_name);
        }

        // Cancel the service via token (kills the subprocess)
        conn.service.cancellation_token().cancel();

        crate::tools::log_tools::append_to_app_log(&format!(
            "MCP | disconnected | server={} | unregistered {} tools", name, conn.tool_names.len()
        ));

        Ok(())
    }

    /// List all active connections.
    pub async fn list_connections(&self) -> Vec<McpConnectionInfo> {
        let conns = self.connections.read().await;
        conns
            .values()
            .map(|c| McpConnectionInfo {
                name: c.config.name.clone(),
                command: c.config.command.clone(),
                tools: c.tool_names.clone(),
                connected: true,
                transport: c.config.transport.clone(),
                url: c.config.url.clone(),
            })
            .collect()
    }
}

impl Default for McpClientManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================
// Proxy Tool — wraps an MCP tool as a HiveTool
// ============================================

/// A proxy that makes an external MCP tool look like a native HiveTool.
/// Registered in the ToolRegistry so models see it alongside built-in tools.
struct McpProxyTool {
    prefixed_name: String,
    original_name: String,
    #[allow(dead_code)]
    server_name: String,
    description: String,
    schema: serde_json::Value,
    /// Shared reference to the MCP client service for this server.
    service: Arc<RunningService<RoleClient, ()>>,
}

#[async_trait::async_trait]
impl HiveTool for McpProxyTool {
    fn name(&self) -> &str {
        &self.prefixed_name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.schema.clone()
    }

    fn risk_level(&self) -> RiskLevel {
        // External tools are medium risk by default — user approved the connection
        RiskLevel::Medium
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let arguments = params.as_object().cloned();

        let result = self
            .service
            .call_tool(CallToolRequestParams {
                name: self.original_name.clone().into(),
                arguments,
                meta: None,
                task: None,
            })
            .await
            .map_err(|e| format!("MCP tool call failed: {:?}", e))?;

        // Convert MCP result to HIVE ToolResult
        let content = result
            .content
            .iter()
            .filter_map(|c| c.raw.as_text().map(|t| t.text.clone()))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(ToolResult {
            content,
            is_error: result.is_error.unwrap_or(false),
        })
    }
}

// ============================================
// Resource Tools — browse MCP server resources
// ============================================

/// Lists all resources available on an MCP server.
struct McpResourceListTool {
    prefixed_name: String,
    #[allow(dead_code)]
    server_name: String,
    service: Arc<RunningService<RoleClient, ()>>,
}

#[async_trait::async_trait]
impl HiveTool for McpResourceListTool {
    fn name(&self) -> &str {
        &self.prefixed_name
    }

    fn description(&self) -> &str {
        "List all resources available on this MCP server. Returns resource names and URIs."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Low // Read-only resource listing
    }

    async fn execute(&self, _params: serde_json::Value) -> Result<ToolResult, String> {
        let result = self
            .service
            .list_resources(Default::default())
            .await
            .map_err(|e| format!("Failed to list resources: {:?}", e))?;

        if result.resources.is_empty() {
            return Ok(ToolResult {
                content: "No resources available.".to_string(),
                is_error: false,
            });
        }

        let listing = result
            .resources
            .iter()
            .map(|r| {
                let desc = r.description.as_deref().unwrap_or("");
                let mime = r.mime_type.as_deref().unwrap_or("unknown");
                format!("- {} ({})\n  URI: {}\n  {}", r.name, mime, r.uri, desc)
            })
            .collect::<Vec<_>>()
            .join("\n");

        Ok(ToolResult {
            content: format!("{} resources found:\n{}", result.resources.len(), listing),
            is_error: false,
        })
    }
}

/// Reads a specific resource from an MCP server by URI.
struct McpResourceReadTool {
    prefixed_name: String,
    #[allow(dead_code)]
    server_name: String,
    service: Arc<RunningService<RoleClient, ()>>,
}

#[async_trait::async_trait]
impl HiveTool for McpResourceReadTool {
    fn name(&self) -> &str {
        &self.prefixed_name
    }

    fn description(&self) -> &str {
        "Read a specific resource from this MCP server by URI."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "uri": {
                    "type": "string",
                    "description": "The resource URI to read (from list_resources output)"
                }
            },
            "required": ["uri"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Low // Read-only resource access
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        let uri = params.get("uri")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: uri")?;

        let result = self
            .service
            .read_resource(ReadResourceRequestParams {
                uri: uri.to_string(),
                meta: None,
            })
            .await
            .map_err(|e| format!("Failed to read resource '{}': {:?}", uri, e))?;

        let content = result
            .contents
            .iter()
            .filter_map(|c| match c {
                rmcp::model::ResourceContents::TextResourceContents { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");

        if content.is_empty() {
            return Ok(ToolResult {
                content: format!("Resource '{}' returned empty content.", uri),
                is_error: false,
            });
        }

        Ok(ToolResult {
            content,
            is_error: false,
        })
    }
}

// ============================================
// Tauri State
// ============================================

/// Tauri-managed state for MCP client connections.
pub struct McpClientState {
    pub manager: McpClientManager,
}

impl Default for McpClientState {
    fn default() -> Self {
        Self {
            manager: McpClientManager::new(),
        }
    }
}

// ============================================
// Tauri Commands
// ============================================

/// Connect to an external MCP server. Returns list of discovered tool names.
#[tauri::command]
pub async fn mcp_connect(
    config: McpServerConfig,
    mcp_state: tauri::State<'_, McpClientState>,
    tool_state: tauri::State<'_, ToolState>,
) -> Result<Vec<String>, String> {
    mcp_state.manager.connect(config, &tool_state).await
}

/// Disconnect from an MCP server. Unregisters all its tools.
#[tauri::command]
pub async fn mcp_disconnect(
    name: String,
    mcp_state: tauri::State<'_, McpClientState>,
    tool_state: tauri::State<'_, ToolState>,
) -> Result<(), String> {
    mcp_state.manager.disconnect(&name, &tool_state).await
}

/// List all active MCP server connections.
#[tauri::command]
pub async fn mcp_list_connections(
    mcp_state: tauri::State<'_, McpClientState>,
) -> Result<Vec<McpConnectionInfo>, String> {
    Ok(mcp_state.manager.list_connections().await)
}

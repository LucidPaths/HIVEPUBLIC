//! HIVE MCP Server — Exposes all HiveTools via Model Context Protocol.
//!
//! When HIVE is launched with `--mcp`, it runs as a headless MCP server on stdio
//! instead of starting the Tauri GUI. Claude Code (or any MCP client) can then
//! use HIVE's 33+ tools: memory, web search, telegram, discord, file ops, etc.
//!
//! Claude Code config (~/.claude/claude_code_config.json):
//! ```json
//! { "mcpServers": { "hive": { "command": "hive-desktop", "args": ["--mcp"] } } }
//! ```

use std::sync::Arc;
use rmcp::ServerHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, ListToolsResult,
    PaginatedRequestParams, RawContent, ServerCapabilities, ServerInfo, Tool,
};
use rmcp::model::AnnotateAble;
use rmcp::ErrorData;
use rmcp::ServiceExt;
use rmcp::service::{RequestContext, RoleServer};
use rmcp::transport::io::stdio;

use crate::tools::{create_default_registry, ToolRegistry};

/// MCP server wrapper around HIVE's ToolRegistry.
/// All registered HiveTools become MCP tools automatically.
pub struct HiveMcpServer {
    registry: ToolRegistry,
}

impl HiveMcpServer {
    pub fn new() -> Self {
        Self {
            registry: create_default_registry(),
        }
    }

    /// Convert a HIVE tool schema to an MCP Tool definition.
    fn to_mcp_tool(s: &crate::tools::ToolSchema) -> Tool {
        let input_schema = s.parameters.as_object().cloned().unwrap_or_default();
        Tool {
            name: s.name.clone().into(),
            description: Some(s.description.clone().into()),
            input_schema: Arc::new(input_schema),
            title: None,
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        }
    }
}

impl ServerHandler for HiveMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "HIVE orchestration harness — 33+ tools for memory, web, files, integrations, \
                 and multi-model routing. Provider-agnostic: local models, OpenAI, Anthropic, \
                 OpenRouter, DashScope, Ollama."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, ErrorData>> + Send + '_ {
        async move {
            let tools: Vec<Tool> = self.registry.schemas().iter().map(Self::to_mcp_tool).collect();
            Ok(ListToolsResult {
                tools,
                next_cursor: None,
                meta: None,
            })
        }
    }

    fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<CallToolResult, ErrorData>> + Send + '_ {
        async move {
            let tool_name = request.name.to_string();
            let args = match request.arguments {
                Some(map) => serde_json::Value::Object(map),
                None => serde_json::Value::Object(Default::default()),
            };

            match self.registry.execute(&tool_name, args).await {
                Ok(result) => {
                    let content = vec![RawContent::text(result.content).no_annotation()];
                    if result.is_error {
                        Ok(CallToolResult::error(content))
                    } else {
                        Ok(CallToolResult::success(content))
                    }
                }
                Err(e) => Ok(CallToolResult::error(vec![
                    RawContent::text(e).no_annotation(),
                ])),
            }
        }
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.registry
            .schemas()
            .iter()
            .find(|s| s.name == name)
            .map(Self::to_mcp_tool)
    }
}

/// Run HIVE as a headless MCP server on stdio.
/// Called from main() when `--mcp` flag is present.
pub async fn run() {
    let server = HiveMcpServer::new();
    eprintln!("[HIVE MCP] Starting MCP server on stdio...");
    eprintln!(
        "[HIVE MCP] {} tools available",
        server.registry.schemas().len()
    );

    match server.serve(stdio()).await {
        Ok(running) => {
            eprintln!("[HIVE MCP] Server initialized, waiting for requests...");
            match running.waiting().await {
                Ok(reason) => eprintln!("[HIVE MCP] Server stopped: {:?}", reason),
                Err(e) => eprintln!("[HIVE MCP] Server error: {:?}", e),
            }
        }
        Err(e) => {
            eprintln!("[HIVE MCP] Failed to start: {:?}", e);
            std::process::exit(1);
        }
    }
}

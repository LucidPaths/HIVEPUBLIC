//! Plan execution tool — structured multi-step tool chaining for HIVE
//!
//! Allows models to declare a multi-step plan upfront. The TypeScript layer
//! intercepts this tool call and executes steps sequentially with variable
//! substitution, conditions, and progress tracking.
//!
//! This is critical for smaller models (Kimi K2.5, Nanbeige, etc.) that
//! can formulate a plan but lose the thread when chaining tools turn-by-turn.
//! Instead of relying on the model to remember what to do next, the harness
//! becomes the executor and the model becomes the planner.
//!
//! Principle alignment:
//!   P1 (Modularity)  — Plan is separate from execution. Model plans, harness executes.
//!   P2 (Agnostic)    — Any model can output JSON. No provider-specific features needed.
//!   P3 (Simplicity)  — Simple JSON schema. Variable substitution is string replacement.
//!   P4 (Errors)      — Each step's error is captured; plan continues or fails gracefully.
//!   P8 (Low/High)    — Simple 2-step chains (low floor). Conditional branching (high ceiling).

use super::{HiveTool, RiskLevel, ToolResult};
use serde_json::json;

pub struct PlanExecuteTool;

#[async_trait::async_trait]
impl HiveTool for PlanExecuteTool {
    fn name(&self) -> &str { "plan_execute" }

    fn description(&self) -> &str {
        "Execute a multi-step plan. Use this when a task requires chaining multiple tools \
         in sequence. You declare all steps upfront, and the system executes them automatically \
         with variable passing between steps.\n\n\
         KEY RULES:\n\
         - Each step calls ONE tool with its arguments\n\
         - Use 'save_as' to store a step's result as a variable name\n\
         - Use '$variable_name' in later steps' args to inject a previous result\n\
         - Use 'condition' to skip a step if a variable is empty or errored\n\
         - Steps execute in order. If a step errors, its save_as holds the error text\n\n\
         EXAMPLE — Research and save:\n\
         {\"goal\":\"Research AI safety\",\"steps\":[\n\
           {\"tool\":\"web_search\",\"args\":{\"query\":\"AI safety 2025\"},\"save_as\":\"results\"},\n\
           {\"tool\":\"web_fetch\",\"args\":{\"url\":\"$results\"},\"save_as\":\"page\"},\n\
           {\"tool\":\"memory_save\",\"args\":{\"content\":\"$page\",\"category\":\"research\"}}\n\
         ]}\n\n\
         EXAMPLE — Conditional notify:\n\
         {\"goal\":\"Check Discord and notify\",\"steps\":[\n\
           {\"tool\":\"discord_read\",\"args\":{\"channel_id\":\"123\"},\"save_as\":\"msgs\"},\n\
           {\"tool\":\"telegram_send\",\"args\":{\"chat_id\":\"456\",\"text\":\"Update: $msgs\"},\"condition\":\"$msgs\"}\n\
         ]}"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "goal": {
                    "type": "string",
                    "description": "What this plan aims to accomplish (shown to user as progress)"
                },
                "steps": {
                    "type": "array",
                    "description": "Ordered list of tool calls to execute sequentially",
                    "items": {
                        "type": "object",
                        "properties": {
                            "tool": {
                                "type": "string",
                                "description": "Name of the tool to call"
                            },
                            "args": {
                                "type": "object",
                                "description": "Arguments for the tool. Use $variable to inject results from earlier steps."
                            },
                            "save_as": {
                                "type": "string",
                                "description": "Store this step's result as a variable (referenced via $name in later steps)"
                            },
                            "condition": {
                                "type": "string",
                                "description": "Only execute if this expression is truthy. Use $variable to check a prior result."
                            }
                        },
                        "required": ["tool", "args"]
                    },
                    "minItems": 2,
                    "maxItems": 15
                }
            },
            "required": ["goal", "steps"]
        })
    }

    fn risk_level(&self) -> RiskLevel { RiskLevel::Medium }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, String> {
        // Validation only — actual plan execution happens in TypeScript (App.tsx).
        // The TS layer intercepts plan_execute before this runs.
        // This is a fallback that validates the plan structure.

        let goal = params.get("goal")
            .and_then(|v| v.as_str())
            .unwrap_or("(no goal)");

        let steps = match params.get("steps").and_then(|v| v.as_array()) {
            Some(s) => s,
            None => return Ok(ToolResult {
                content: "Missing required parameter: steps (must be an array of tool call objects). \
                         If you need to run a single tool, call it directly instead of using plan_execute. \
                         If you want a background worker for async tasks, use worker_spawn instead.".to_string(),
                is_error: true,
            }),
        };

        if steps.len() < 2 {
            return Ok(ToolResult {
                content: "Plan needs at least 2 steps. For a single tool call, just call it directly.".to_string(),
                is_error: true,
            });
        }

        if steps.len() > 15 {
            return Ok(ToolResult {
                content: format!("Plan has {} steps (max 15). Break into smaller plans.", steps.len()),
                is_error: true,
            });
        }

        // Validate each step has required fields
        for (i, step) in steps.iter().enumerate() {
            if step.get("tool").and_then(|v| v.as_str()).is_none() {
                return Ok(ToolResult {
                    content: format!("Step {} missing 'tool' field.", i + 1),
                    is_error: true,
                });
            }
            if step.get("args").is_none() {
                return Ok(ToolResult {
                    content: format!("Step {} missing 'args' field.", i + 1),
                    is_error: true,
                });
            }
        }

        Ok(ToolResult {
            content: format!(
                "PLAN_VALIDATED: {} steps for goal: \"{}\". Awaiting execution by harness.",
                steps.len(), goal
            ),
            is_error: false,
        })
    }
}

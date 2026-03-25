# CLI Bridge Improvement Plan — From PTY Scraping to Structured Subprocess I/O

**Author:** Claude (design roadmap session)
**Date:** 2026-03-13
**Branch:** `claude/cli-bridge-improvements-2USJB`
**Status:** Design document — implementation pending

---

## Table of Contents

1. [Strategic Context: Why Not the Agent SDK](#1-strategic-context-why-not-the-agent-sdk)
2. [Current Architecture Audit](#2-current-architecture-audit)
3. [Target Architecture](#3-target-architecture)
4. [Per-Agent Capability Matrix](#4-per-agent-capability-matrix)
5. [Implementation Plan](#5-implementation-plan)
6. [Critical Design Decisions](#6-critical-design-decisions)
7. [Risk Assessment](#7-risk-assessment)
8. [Migration Strategy](#8-migration-strategy)

---

## 1. Strategic Context: Why Not the Agent SDK

### The Economics

The Claude Agent SDK (Python/TypeScript) uses **API billing** — per-token metered costs. For serious coding tasks, that's $5-50+ per session. HIVE's architecture treats a Claude Code **Max subscription** ($200/month, unlimited usage) as a shared resource. The SDK path trades a flat-rate subscription for metered API costs, which destroys the economics for the multi-user Discord routing use case.

### The Ecosystem Risk

Anthropic has historically taken action against third-party products that build on their ecosystem in ways they don't endorse (see: OpenClaw). A desktop orchestrator that multiplexes their subscription model sits in a sensitive area. The Agent SDK route makes HIVE *more* visible as a competing product. The CLI route makes HIVE look like what it is: a user running their own CLI tools on their own machine.

### The Correct Strategic Position

HIVE drives Claude Code (and Codex, Aider, etc.) through **their own CLIs** — the exact same interface a human uses. This is:

1. **Using the product as designed.** Claude Code is a CLI tool. It accepts stdin, produces stdout. Programmatic terminal interaction is what terminals are for.
2. **Multiple sessions are explicitly supported.** Opening 10 terminal tabs, each running `claude`, each with a different task — that's normal usage.
3. **No account sharing occurs.** Credentials never leave the machine. Discord users talk to HIVE, which talks to Claude Code. Users never authenticate with Anthropic.
4. **Provider-agnostic (P2).** The same subprocess pattern works for Claude Code, Codex, Aider, or any future CLI agent. No vendor lock-in.

### What Changes

We stop relying on silence-based PTY scraping for the **programmatic bridge** and instead use each CLI tool's native structured output mode. The visual terminal pane (xterm.js) stays exactly as-is for human observation.

---

## 2. Current Architecture Audit

### What Exists (`pty_manager.rs` + `agent_tools.rs`)

```
┌─ HIVE Orchestrator (chat model) ─┐
│                                    │
│  send_to_agent(session_id, input)  │──→ PTY stdin (raw keystrokes + \n)
│  read_agent_output(session_id)     │←── PTY output buffer (ANSI-stripped circular buffer)
│  list_agents()                     │←── Session registry
│                                    │
│  [PASSIVE] agent-bridge-monitor    │←── Silence-based response detection
│    12s silence threshold           │      → "agent-response" Tauri event
│    8s min delivery interval        │      → injected into orchestrator chat
│    30-char min content             │
│    Jaccard dedup (0.70)            │
│    4000-char truncation            │
└────────────────────────────────────┘
```

### What Works

- **PTY spawn/kill/resize** — solid, uses `portable-pty` (Wezterm). No issues.
- **Visual terminal pane** — xterm.js rendering, HIVE zinc/amber theme. No issues.
- **Output buffer** — 500-line circular buffer with ANSI stripping. Clean.
- **Agent registry** — BUILTIN_AGENTS + custom agents, command normalization. Good.
- **Channel routing** — Telegram/Discord messages can route to terminal agents. Good.

### What's Fragile

| Problem | Impact | Root Cause |
|---------|--------|------------|
| **Silence-based detection** (12s threshold) | Extended thinking models go silent for 8-15s mid-thought, then resume. Bridge can't distinguish "thinking pause" from "done talking" | No structured signal from the agent that it's done |
| **Raw PTY I/O** | `send_to_agent` writes keystrokes. Works for simple prompts. Breaks on multi-line input, special characters, prompts that need specific formatting | Treating a structured CLI as a dumb terminal |
| **ANSI scraping** | Output buffer strips ANSI codes, but Claude Code's output includes markdown, tool call blocks, thinking blocks, progress indicators | No semantic structure — just text lines |
| **No session identity** | HIVE spawns a PTY → gets a HIVE session_id. But Claude Code has its own session_id inside. No linkage | PTY layer doesn't understand what's running inside it |
| **No completion detection** | `send_to_agent` returns immediately after writing stdin. Orchestrator has no idea when the task is done, only that silence occurred | Fire-and-forget I/O model |
| **No error propagation** | If Claude Code fails a task, the orchestrator sees the error text in the output buffer (if it catches it) but has no structured error signal | Text scraping for error detection |
| **No cost/token awareness** | Claude Code tracks its own usage, but HIVE can't see it | No metadata channel |

### The Two-Bridge Problem

The current code tries to use **one mechanism** (PTY I/O + silence detection) for **two different needs**:

1. **Human observation** — a user watching Claude Code work in the terminal pane (xterm.js)
2. **Programmatic orchestration** — HIVE's chat model delegating tasks and reading results

These are fundamentally different. The terminal pane needs real-time character streaming. The programmatic bridge needs structured request/response with completion signals.

---

## 3. Target Architecture

### Two-Tier Bridge

```
┌─ HIVE Orchestrator ──────────────────────────────────────────────────┐
│                                                                       │
│  TIER 1: Structured Subprocess (Claude Code, Codex)                   │
│  ─────────────────────────────────────────────────────                │
│  Command::new("claude")                                               │
│    .args(["-p", prompt, "--output-format", "json",                    │
│           "--session-id", user_session_id,                            │
│           "--allowedTools", "Read,Edit,Bash"])                         │
│    .stdout(Stdio::piped())                                            │
│    .stderr(Stdio::piped())                                            │
│                                                                       │
│  → Parse JSON from stdout                                             │
│  → result, session_id, cost_usd, duration_ms, num_turns              │
│  → Multi-turn via --resume <session_id>                               │
│  → Structured error from exit code + stderr                           │
│  → Streaming via --output-format stream-json (optional)               │
│                                                                       │
│  TIER 2: PTY with Pattern Matching (Aider, Shell, dumb CLIs)         │
│  ─────────────────────────────────────────────────────                │
│  portable-pty (existing infrastructure)                                │
│  + expectrl-style prompt detection (replaces silence-based)           │
│  + strip-ansi-escapes for output cleaning                             │
│  + per-agent prompt patterns (configurable)                           │
│                                                                       │
│  SHARED: Visual terminal pane (xterm.js) — unchanged                  │
│  ─────────────────────────────────────────────────────                │
│  Terminal panes continue using PTY for ALL agents (human observation) │
│  The structured subprocess tier is an ADDITIONAL programmatic channel │
└───────────────────────────────────────────────────────────────────────┘
```

### Key Insight: Two Channels, Not One

For Claude Code and Codex, HIVE maintains **two channels simultaneously**:

1. **PTY channel** — for the visual terminal pane. User watches in real-time. Unchanged.
2. **Subprocess channel** — for programmatic orchestration. `claude -p` with JSON output. Structured, reliable, no scraping.

These are independent. The PTY shows the interactive session. The subprocess handles task delegation. They don't need to be the same process.

For Aider and dumb CLIs (Tier 2), there's only the PTY channel, but with improved pattern-based detection instead of silence-based.

---

## 4. Per-Agent Capability Matrix

### Claude Code (`claude`)

| Feature | CLI Support | How |
|---------|------------|-----|
| Non-interactive mode | Yes | `claude -p "prompt"` |
| Structured JSON output | Yes | `--output-format json` → `{ result, session_id, cost_usd, duration_ms, num_turns }` |
| Streaming JSON | Yes | `--output-format stream-json` → NDJSON events |
| Multi-turn sessions | Yes | `--session-id <uuid>` on first call, `--resume <session_id>` on subsequent |
| Continue last session | Yes | `--continue` |
| Tool control | Yes | `--allowedTools "Read,Edit,Bash(git *)"` with prefix matching |
| Custom system prompt | Yes | `--append-system-prompt "..."` or `--system-prompt "..."` (full replace) |
| JSON schema enforcement | Yes | `--json-schema '{...}'` → output in `structured_output` field |
| Working directory | Yes | Runs in CWD by default, respects project CLAUDE.md |
| MCP servers | Yes | Configured via project settings, available in `-p` mode |
| Session forking | Yes | `--continue --fork-session` |
| Exit codes | Yes | 0 = success, non-zero = failure |
| Cost tracking | Yes | `cost_usd` in JSON output |

**Verdict:** Full Tier 1 support. Subprocess with JSON output is the correct approach.

### OpenAI Codex (`codex`)

| Feature | CLI Support | How |
|---------|------------|-----|
| Non-interactive mode | Yes | `codex exec "prompt"` |
| Structured JSON output | Yes | `codex exec --json` → JSONL stream |
| Schema-constrained output | Yes | `--output-schema <path>` |
| Multi-turn sessions | Yes | `codex exec resume --last "prompt"` or `codex exec resume <SESSION_ID>` |
| Tool/sandbox control | Yes | `--sandbox workspace-write`, `--sandbox danger-full-access` |
| Auto-approval | Yes | `--full-auto` |
| Last message extraction | Yes | `-o <path>` writes final message to file |
| Exit codes | Yes | Non-zero on failure |

**Verdict:** Full Tier 1 support. Use `codex exec --json` for structured output.

### Aider (`aider`)

| Feature | CLI Support | How |
|---------|------------|-----|
| Non-interactive mode | Partial | `aider --message "prompt" --yes` — single message, then exits |
| Structured JSON output | **No** | No JSON output mode |
| Multi-turn sessions | **No** | `--message` is fire-and-forget. Must use interactive mode for multi-turn |
| Tool/edit control | Partial | `--no-auto-commits`, `--dry-run` |
| Streaming control | Yes | `--no-stream` for batch output |

**Verdict:** Tier 2 only. For single-shot tasks, `--message --yes --no-stream` works as a subprocess. For multi-turn, must use PTY with pattern-based prompt detection.

### Shell (`bash`/`cmd`/`powershell`)

**Verdict:** Tier 2 always. PTY is the correct interface for interactive shells.

---

## 5. Implementation Plan

### Phase 1: Structured Subprocess Engine (Core)

**Goal:** New Rust module `subprocess_bridge.rs` that runs CLI agents via `Command::new()` with structured JSON I/O.

#### 1.1 — `SubprocessBridge` Core

```rust
// New file: HIVE/desktop/src-tauri/src/subprocess_bridge.rs

/// A managed subprocess execution for CLI agents with structured output.
/// Replaces PTY I/O for agents that support --print/--json modes.
pub struct SubprocessBridge {
    /// Agent-specific command builder
    agent: AgentType,
    /// Session tracking (agent's own session ID for multi-turn)
    session_id: Option<String>,
    /// Working directory
    cwd: PathBuf,
    /// Running child process handle (for cancellation)
    child: Option<Child>,
}

pub enum AgentType {
    ClaudeCode {
        allowed_tools: Vec<String>,
        append_system_prompt: Option<String>,
        json_schema: Option<String>,
    },
    Codex {
        sandbox: CodexSandbox,
        full_auto: bool,
        output_schema: Option<PathBuf>,
    },
    Aider {
        /// Aider only supports single-shot in subprocess mode
        no_auto_commits: bool,
    },
}

pub enum CodexSandbox {
    ReadOnly,
    WorkspaceWrite,
    DangerFullAccess,
}

/// Result from a structured subprocess execution
pub struct SubprocessResult {
    pub text: String,           // The agent's text response
    pub session_id: Option<String>, // Agent's session ID (for multi-turn)
    pub cost_usd: Option<f64>,  // Token cost (Claude Code, Codex)
    pub duration_ms: u64,       // Wall-clock duration
    pub num_turns: Option<u32>, // Agent's internal turn count
    pub exit_code: i32,         // Process exit code
    pub structured_output: Option<serde_json::Value>, // If JSON schema was used
}
```

**Important considerations:**

- Use `#[cfg(windows)] CREATE_NO_WINDOW` on all `Command::new()` calls (existing pattern from `wsl_cmd()`)
- Capture both stdout (JSON response) and stderr (progress/errors) separately
- Set reasonable timeouts per agent type (Claude Code tasks can run 5-30 minutes)
- Track child PID in `spawned_pids` for cleanup on app exit (existing pattern)

#### 1.2 — Claude Code Subprocess Implementation

```rust
impl SubprocessBridge {
    pub async fn execute_claude_code(&mut self, prompt: &str) -> Result<SubprocessResult, String> {
        let mut cmd = Command::new("claude");
        cmd.arg("-p").arg(prompt);
        cmd.arg("--output-format").arg("json");

        if let Some(ref sid) = self.session_id {
            cmd.arg("--resume").arg(sid);
        }

        if let AgentType::ClaudeCode { ref allowed_tools, ref append_system_prompt, ref json_schema } = self.agent {
            if !allowed_tools.is_empty() {
                cmd.arg("--allowedTools").arg(allowed_tools.join(","));
            }
            if let Some(ref prompt) = append_system_prompt {
                cmd.arg("--append-system-prompt").arg(prompt);
            }
            if let Some(ref schema) = json_schema {
                cmd.arg("--json-schema").arg(schema);
            }
        }

        cmd.current_dir(&self.cwd);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);

        let child = cmd.spawn().map_err(|e| format!("Failed to spawn claude: {}", e))?;
        // ... track PID, wait for completion, parse JSON output
    }
}
```

#### 1.3 — Codex Subprocess Implementation

```rust
pub async fn execute_codex(&mut self, prompt: &str) -> Result<SubprocessResult, String> {
    let mut cmd = Command::new("codex");
    cmd.arg("exec").arg(prompt);
    cmd.arg("--json"); // JSONL output

    if let AgentType::Codex { ref sandbox, full_auto, ref output_schema } = self.agent {
        match sandbox {
            CodexSandbox::WorkspaceWrite => { cmd.arg("--sandbox").arg("workspace-write"); }
            CodexSandbox::DangerFullAccess => { cmd.arg("--sandbox").arg("danger-full-access"); }
            _ => {} // read-only is default
        }
        if full_auto { cmd.arg("--full-auto"); }
        if let Some(ref schema) = output_schema {
            cmd.arg("--output-schema").arg(schema);
        }
    }

    // Codex outputs JSONL — accumulate events, extract final result
    // ...
}
```

#### 1.4 — Aider Subprocess Implementation (Single-Shot Only)

```rust
pub async fn execute_aider(&mut self, prompt: &str) -> Result<SubprocessResult, String> {
    let mut cmd = Command::new("aider");
    cmd.arg("--message").arg(prompt);
    cmd.arg("--yes");
    cmd.arg("--no-stream"); // Batch output for reliable capture

    if let AgentType::Aider { no_auto_commits } = self.agent {
        if no_auto_commits { cmd.arg("--no-auto-commits"); }
    }

    // Aider outputs plain text — capture stdout as-is
    // No session_id (single-shot only)
    // ...
}
```

### Phase 2: HiveTool Integration

**Goal:** New tools that use the subprocess bridge instead of PTY I/O.

#### 2.1 — `delegate_to_agent` Tool (Replaces `send_to_agent` for structured agents)

```rust
// New tool: delegate_to_agent
// Runs a task via subprocess with structured output, waits for completion, returns result.
//
// Unlike send_to_agent (fire-and-forget PTY write), this:
// - Waits for the agent to complete
// - Returns structured result (text, cost, session_id)
// - Supports multi-turn via automatic session resumption
// - Has clear error propagation

pub struct DelegateToAgentTool;

// Schema:
// {
//   "agent": "claude-code" | "codex" | "aider",
//   "task": "string — the prompt/task description",
//   "session_id": "optional — resume a previous session",
//   "allowed_tools": ["Read", "Edit", "Bash"],  // Claude Code only
//   "sandbox": "workspace-write",                 // Codex only
//   "cwd": "/optional/working/directory"
// }
```

This tool is **synchronous from the model's perspective** — it blocks until the subprocess completes and returns the full result. This is a major improvement over the current fire-and-forget pattern where the model sends input and then has to poll `read_agent_output` and guess when it's done.

#### 2.2 — Keep Existing PTY Tools

`send_to_agent`, `read_agent_output`, `list_agents` remain unchanged. They serve the PTY tier (Aider interactive, shell, human-observable terminal panes). No code removed.

#### 2.3 — Tool Routing Logic

The orchestrating model chooses:
- `delegate_to_agent` for structured task delegation (Claude Code, Codex, Aider single-shot)
- `send_to_agent` for interactive PTY sessions (shell commands, Aider multi-turn, manual control)

This is a model-level decision based on the tool descriptions, not hardcoded routing.

### Phase 3: Session Management

**Goal:** Per-user session tracking for multi-turn conversations across Discord/Telegram.

#### 3.1 — Session Registry

```rust
/// Maps external user identity → agent session IDs
/// Enables: Discord user A has an ongoing Claude Code conversation,
///          Discord user B has a separate one
struct SessionRegistry {
    /// Key: (channel, user_id), Value: { agent_type, session_id, last_active, cwd }
    sessions: HashMap<(String, String), AgentSession>,
}

struct AgentSession {
    agent_type: AgentType,
    session_id: String,      // The agent's own session ID
    last_active: Instant,
    cwd: PathBuf,
    total_cost_usd: f64,     // Accumulated cost for this user's session
}
```

#### 3.2 — Session Lifecycle

1. **First message from user** → spawn fresh subprocess → capture `session_id` from JSON response → store in registry
2. **Subsequent messages** → look up `session_id` from registry → pass via `--resume <session_id>` (Claude Code) or `codex exec resume <session_id>` (Codex)
3. **Session expiry** → after configurable idle timeout (default: 1 hour), clear from registry. Claude Code sessions persist on disk anyway.
4. **Explicit reset** → user says "start fresh" or similar → clear session from registry

#### 3.3 — Cost Tracking

Claude Code returns `cost_usd` in its JSON output. Accumulate per-user and expose via `integration_status` tool so the orchestrating model can be cost-aware.

### Phase 4: Streaming Support (Optional Enhancement)

**Goal:** Real-time progress for long-running tasks.

#### 4.1 — Stream-JSON Mode for Claude Code

Instead of waiting for the full response, parse `--output-format stream-json` events in real-time:

```rust
// Read stdout line by line (NDJSON)
// Each line is a JSON event: { type: "stream_event", event: { delta: { type: "text_delta", text: "..." } } }
// Emit progress to frontend via Tauri event
// Still collect final result for structured return
```

This enables:
- Progress indicators in the HIVE UI while Claude Code works
- Early cancellation if the user sees something wrong
- Real-time streaming to Discord/Telegram (rate-limited)

#### 4.2 — Codex JSONL Streaming

Same pattern — `codex exec --json` produces JSONL events. Parse `turn.started`, `turn.completed`, `item.*` events for progress tracking.

### Phase 5: PTY Tier Improvements (Aider, Shell)

**Goal:** Replace silence-based detection with pattern-based for remaining PTY agents.

#### 5.1 — Prompt Pattern Detection

For agents that stay in PTY mode (Aider interactive, shell), detect completion by matching prompt patterns instead of silence:

```rust
/// Per-agent prompt patterns that indicate the agent is waiting for input
struct AgentPromptPattern {
    /// Regex that matches the agent's input prompt
    /// e.g., aider: r"^aider> " or r"^\(aider\) "
    /// e.g., bash: r"^\$ " or r"^user@host:"
    prompt_regex: Regex,

    /// How long after seeing the prompt to wait before declaring "done"
    /// (handles multi-line prompts that arrive in chunks)
    settle_time: Duration, // typically 500ms-1s
}
```

This is **massively more reliable** than the current 12-second silence threshold because:
- The prompt appears *immediately* when the agent is done
- No false positives from thinking pauses
- No false negatives from rapid-fire output
- Configurable per agent

#### 5.2 — AgentConfig Extension

```typescript
// types.ts — extend AgentConfig
export interface AgentConfig {
  id: string;
  name: string;
  command: string;
  args: string[];
  color: string;
  bridgeToChat?: boolean;
  // NEW:
  tier: 'subprocess' | 'pty';           // Which bridge to use for programmatic I/O
  promptPattern?: string;                // Regex for prompt detection (PTY tier)
  supportsStructuredOutput?: boolean;    // Can use delegate_to_agent
  sessionResumeFlag?: string;            // e.g., "--resume" for claude, "resume" for codex
}
```

#### 5.3 — Deprecate Silence-Based Bridge

Once prompt-pattern detection is stable, the silence-based `bridge_monitor_loop` becomes dead code. Remove it and the associated constants (`BRIDGE_SILENCE_THRESHOLD`, `BRIDGE_MIN_DELIVERY_INTERVAL`, etc.).

---

## 6. Critical Design Decisions

### Decision 1: Two Processes vs. One for Claude Code

**Option A:** Single PTY process serves both terminal pane AND programmatic bridge.
**Option B:** Two separate processes — PTY for terminal, subprocess for programmatic.

**Choice: Option B (two processes).**

Rationale:
- The PTY process runs interactive Claude Code (with TUI, thinking animations, etc.)
- The subprocess runs `claude -p` (headless, JSON output, no TUI)
- They share the same session via `--session-id` / `--resume` if needed
- Trying to parse structured data from a TUI process is the root cause of current fragility
- Cost: Claude Code subscription is unlimited — running two processes costs nothing

This means when a user opens a Claude Code terminal pane AND the orchestrator delegates a task to Claude Code, these are **separate processes**. The terminal pane is for watching. The subprocess is for doing. If the user wants to see what the subprocess is doing, we can stream its progress into the terminal pane via `term.write()`.

### Decision 2: Synchronous vs. Async Tool Execution

**Option A:** `delegate_to_agent` blocks until the subprocess completes (synchronous).
**Option B:** `delegate_to_agent` returns immediately, model polls for result (async).

**Choice: Option A (synchronous).**

Rationale:
- The worker system already handles async background tasks. Workers can call `delegate_to_agent` and they naturally handle long-running operations.
- For direct chat usage, the user expects to see the result when it's done.
- Synchronous is simpler (P3) and removes the need for a polling mechanism.
- If the task takes too long, the orchestrator can use `worker_spawn` to delegate to a worker that calls `delegate_to_agent`.

Timeout: Configurable per-agent, default 10 minutes (same as worker wall-clock timeout).

### Decision 3: Session Isolation for Remote Users

**Option A:** All Discord/Telegram users share one Claude Code session.
**Option B:** Each user gets their own session.
**Option C:** Each user+channel combination gets a session.

**Choice: Option C (user+channel).**

Rationale:
- User A in #general and User A in #code-review might have different contexts
- Sessions are lightweight (just a UUID tracking conversation history)
- Prevents cross-contamination between users
- Aligns with P6 (Secrets Stay Secret) — users don't see each other's conversations

### Decision 4: Where to Run Subprocesses (CWD)

**Option A:** Always HIVE's working directory.
**Option B:** Configurable per-agent or per-task.

**Choice: Option B (configurable).**

Rationale:
- Claude Code's effectiveness depends heavily on being in the right project directory
- Different tasks target different repos
- The `delegate_to_agent` tool accepts an optional `cwd` parameter
- Default: HIVE's configured project directory (from settings)

### Decision 5: How to Handle Multi-Turn for Aider

**Option A:** Treat Aider as single-shot only (subprocess tier).
**Option B:** Keep Aider in PTY tier with improved prompt detection.
**Option C:** Both — single-shot for simple tasks, PTY for complex multi-turn.

**Choice: Option C (both).**

Rationale:
- Simple Aider tasks (e.g., "fix the linting error in auth.py") work perfectly with `--message --yes --no-stream`
- Complex Aider sessions (e.g., ongoing refactoring with back-and-forth) need interactive PTY
- The model can choose based on task complexity
- `delegate_to_agent` with `agent: "aider"` → single-shot subprocess
- `send_to_agent` to a running Aider PTY → interactive mode

---

## 7. Risk Assessment

### Risk 1: Claude Code CLI Changes

**Risk:** Anthropic changes `claude -p` flags, JSON output format, or session management.
**Likelihood:** Medium (CLI is actively developed).
**Mitigation:**
- Pin to known-working Claude Code version in HIVE settings
- Version detection on startup: `claude --version` → parse, warn if unsupported
- JSON parsing should be defensive (unknown fields ignored, missing optional fields defaulted)
- Tests should exercise the actual CLI (integration tests, not mocks)

### Risk 2: Anthropic Breaks Non-Interactive Mode

**Risk:** Anthropic adds rate limiting, CAPTCHA, or blocks non-interactive usage.
**Likelihood:** Low (headless mode is an official feature for CI/CD).
**Mitigation:**
- HIVE uses the exact same CLI flags documented for CI/CD pipelines
- If headless mode is restricted, fall back to PTY tier (existing infrastructure)
- Keep PTY infrastructure maintained as a fallback, not dead code

### Risk 3: Codex CLI Instability

**Risk:** Codex CLI is newer than Claude Code; API surface may change.
**Likelihood:** Medium-High.
**Mitigation:**
- Same version detection + defensive parsing strategy
- Codex is lower priority than Claude Code — can fall back to PTY if needed
- Watch Codex GitHub issues for breaking changes (already seeing JSON format discrepancies)

### Risk 4: Process Accumulation

**Risk:** Subprocess bridge spawns processes that don't get cleaned up (zombies, leaked file handles).
**Likelihood:** Medium (existing pattern with `spawned_pids` mitigates this).
**Mitigation:**
- Track all subprocess PIDs in `spawned_pids` (existing pattern)
- Timeout kills after configurable wall-clock limit
- `perform_full_cleanup()` already iterates `spawned_pids` on shutdown
- Add process reaping on session expiry

### Risk 5: Cost Blindness

**Risk:** With per-user sessions, accumulated cost isn't visible.
**Likelihood:** Low (Claude Code Max is unlimited, Codex has its own billing).
**Mitigation:**
- Track and display accumulated cost per user from JSON metadata
- Expose via `integration_status` tool
- Optional per-user or per-session cost caps in settings

---

## 8. Migration Strategy

### Phase 1 → Phase 2 Can Ship Independently

Each phase is independently valuable:

| Phase | Ships | Value |
|-------|-------|-------|
| Phase 1 (Subprocess Engine) | Core `subprocess_bridge.rs` module | Foundation for everything |
| Phase 2 (HiveTool Integration) | `delegate_to_agent` tool | Orchestrator can reliably delegate tasks |
| Phase 3 (Session Management) | Per-user session registry | Multi-user Discord/Telegram support |
| Phase 4 (Streaming) | Real-time progress | UX improvement, not blocking |
| Phase 5 (PTY Improvements) | Pattern-based prompt detection | Reliability for PTY-tier agents |

### Backward Compatibility

- **No existing tools removed.** `send_to_agent`, `read_agent_output`, `list_agents` remain.
- **No existing PTY infrastructure removed.** Terminal panes work exactly as before.
- **New capability added alongside existing.** `delegate_to_agent` is a new tool, not a replacement.
- **Silence-based bridge deprecated last** (Phase 5), only after pattern-based detection is proven stable.

### Testing Strategy

1. **Unit tests** for JSON parsing (Claude Code response format, Codex JSONL events)
2. **Integration tests** that spawn actual `claude -p` and `codex exec` processes (gated on agent availability)
3. **Mock tests** for session registry lifecycle
4. **Regression tests** — ensure existing PTY tests still pass after each phase

### File Map (New + Modified)

```
NEW FILES:
  src-tauri/src/subprocess_bridge.rs     — Core subprocess engine
  src-tauri/src/session_registry.rs      — Per-user session tracking
  src-tauri/src/tools/delegate_tools.rs  — delegate_to_agent HiveTool

MODIFIED FILES:
  src-tauri/src/main.rs                  — Register new Tauri commands
  src-tauri/src/tools/mod.rs             — Register delegate_to_agent in ToolRegistry
  src/types.ts                           — Extend AgentConfig with tier/promptPattern
  src/lib/api.ts                         — Add delegate_to_agent to DANGEROUS_TOOLS
  src-tauri/src/content_security.rs      — Add delegate_to_agent to is_dangerous_tool()
  src-tauri/src/tools/worker_tools.rs    — Add delegate_to_agent to WORKER_BLOCKED_TOOLS
  src-tauri/src/pty_manager.rs           — Phase 5: add prompt pattern detection, deprecate silence bridge

UNCHANGED:
  src-tauri/src/tools/agent_tools.rs     — send_to_agent, read_agent_output, list_agents stay
  src/components/TerminalPane.tsx         — Visual terminal pane unchanged
  src/components/PaneHeader.tsx           — Agent selector unchanged
```

---

## Appendix A: Claude Code JSON Output Format (Reference)

From `claude -p "task" --output-format json`:

```json
{
  "result": "The text response from Claude Code",
  "session_id": "abc123-uuid",
  "cost_usd": 0.042,
  "duration_ms": 15230,
  "num_turns": 3
}
```

With `--json-schema`:
```json
{
  "result": "...",
  "session_id": "...",
  "cost_usd": 0.042,
  "duration_ms": 15230,
  "num_turns": 3,
  "structured_output": { /* matches provided schema */ }
}
```

## Appendix B: Codex JSONL Event Format (Reference)

From `codex exec "task" --json`:

```jsonl
{"type": "thread.started", ...}
{"type": "turn.started", ...}
{"type": "item", "item": {"type": "agent_message", "content": "..."}}
{"type": "item", "item": {"type": "command_execution", "command": "...", "output": "..."}}
{"type": "turn.completed", "usage": {"total_tokens": 1234, "cached_tokens": 500}}
```

## Appendix C: Aider Non-Interactive Mode (Reference)

```bash
aider --message "Fix the bug in auth.py" --yes --no-stream
# Output: plain text to stdout
# Exit code: 0 on success
# No session management — each invocation is independent
```

---

## Summary

The current PTY-based bridge works but is fundamentally the wrong abstraction for programmatic orchestration. CLI agents (Claude Code, Codex) already provide structured output modes designed for exactly this use case. By splitting into a two-tier architecture — subprocess for structured agents, PTY for interactive — HIVE gets reliable task delegation, multi-turn session management, cost tracking, and completion detection, all without any SDK dependency or API billing.

The terminal pane stays. The user can watch Claude Code work. But the brains of the operation — task delegation, result parsing, session tracking — move to the subprocess tier where they belong.

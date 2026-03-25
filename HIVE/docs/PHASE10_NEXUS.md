# Phase 10: NEXUS — Universal Agent Interface

**Status:** Phase 10 COMPLETE. All sub-phases (10.1–10.5.4) implemented.
**Last updated:** February 28, 2026
**Codename:** The Skeleton Key

> This document is the **single source of truth** for Phase 10 implementation.
> Any new session should read this FIRST before writing any NEXUS code.

---

## What Phase 10 IS

**HIVE becomes the one access point for every AI agent you subscribe to.**

Claude Code, Codex, Aider, Continue, Cursor's CLI — any CLI-based coding agent — runs **inside HIVE** as a terminal pane. Your local models, your cloud chat models, your CLI agents, all in one window. The user never leaves HIVE. The framework is permanent; the agents are swappable. That's P2 and P7 applied to coding agents, not just chat models.

This is NOT a terminal emulator. It's an **orchestration layer** that happens to embed terminals. The real value isn't "run claude in a tab" — it's:

1. **HIVE memory sees everything.** PTY output flows through Tauri, gets logged to HIVE's memory system. Kimi can search "what did Claude Code fix yesterday?"
2. **Cross-agent messaging.** Kimi (chat pane) calls a tool to send a prompt to Claude Code (terminal pane). Claude Code's output gets captured and piped back.
3. **MCP bridge.** HIVE starts its MCP server, Claude Code connects to it. Now Claude Code has access to HIVE's memory, tools, and any active chat model — zero extra work on Claude Code's side.
4. **Subscription sharing via Discord.** Your girlfriend messages on Discord → HIVE routes to whichever agent is configured → response goes back. She never installs anything. She uses your subs, your hardware.

```
┌─────────────────────────────────────────────────────────────────────┐
│  HIVE Desktop — The Skeleton Key                                     │
│                                                                      │
│  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐  │
│  │ Kimi (Cloud) │ │ Claude Code │ │ Local Llama  │ │ Codex (PTY) │  │
│  │  API stream  │ │   PTY/sub   │ │  llama.cpp   │ │  PTY/sub    │  │
│  │             │ │             │ │             │ │             │  │
│  │ > help me   │ │ > fix the   │ │ > summarize │ │ > review    │  │
│  │   design    │ │   auth bug  │ │   this file │ │   my PR     │  │
│  │   the UI    │ │             │ │             │ │             │  │
│  │ < sure,     │ │ < Reading   │ │ < The file  │ │ < LGTM,     │  │
│  │   here's    │ │   src/...   │ │   contains  │ │   2 nits    │  │
│  │   my take   │ │   Done.     │ │   3 classes │ │             │  │
│  └─────────────┘ └─────────────┘ └─────────────┘ └─────────────┘  │
│        │                │                │                │         │
│  ┌─────▼────────────────▼────────────────▼────────────────▼──────┐ │
│  │  Tauri Rust Backend                                            │ │
│  │  ├── Provider chat (existing)                                  │ │
│  │  ├── PTY manager: spawn/read/write/resize/kill                │ │
│  │  ├── Memory: all PTY output → searchable logs                 │ │
│  │  ├── MCP server: Claude Code ↔ HIVE tools bridge              │ │
│  │  └── Cross-agent tools: route prompts between panes           │ │
│  └────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────┘
```

---

## What's Already Built (Foundation)

The multi-pane chat infrastructure was completed in the preceding session. **This is the scaffold NEXUS builds on.**

### Completed Components

| Component | File | Lines | What It Does |
|-----------|------|-------|-------------|
| MultiPaneChat | `src/components/MultiPaneChat.tsx` | 228 | Manages N panes in resizable `react-resizable-panels` layout, add/remove/persist panes |
| ChatPane | `src/components/ChatPane.tsx` | 173 | Self-contained pane: own `useChat()` + `useConversationManager()`, remote channel bridge |
| PaneHeader | `src/components/PaneHeader.tsx` | ~80 | Per-pane model indicator, provider color, add/remove buttons |
| useLogs | `src/hooks/useLogs.ts` | ~50 | Console capture + log state (extracted from App.tsx) |
| useHuggingFace | `src/hooks/useHuggingFace.ts` | ~110 | HF search, recommendations, downloads (extracted from App.tsx) |
| useRemoteChannels | `src/hooks/useRemoteChannels.ts` | ~110 | Telegram/Discord/Worker/Routine listeners (extracted from App.tsx) |
| useConversationManager | `src/hooks/useConversationManager.ts` | ~120 | Conversation CRUD, auto-save, memory flush (extracted from App.tsx) |
| ChatPaneConfig type | `src/types.ts` | ~20 | Per-pane config: id, modelType, provider, modelId, modelDisplayName, port |
| StreamTokenPayload | `src-tauri/src/providers.rs` | ~10 | `{ token, stream_id }` — isolates concurrent streaming per pane |
| stream_id support | `src-tauri/src/provider_stream.rs` | 6 functions | All 6 streaming functions (OpenAI, Anthropic, Ollama, OpenRouter, DashScope, OpenAI-compat) accept and emit `stream_id` |
| stream_id TypeScript | `src/lib/api.ts` | ~15 | `chatWithProviderStream()` + `onCloudChatToken()` + `onCloudThinkingToken()` accept `streamId` |

### App.tsx Reduction

App.tsx was reduced from **1245 lines → 740 lines** (40% reduction) via hook extraction. The remaining 740 lines are genuine orchestration (global state, tab routing, model lifecycle) that belongs there.

### What This Foundation Enables

- **N independent chat panes** — each with its own model, conversation, streaming
- **Concurrent streaming isolation** — stream_id prevents cross-contamination
- **Remote channel routing** — Telegram/Discord messages appear in the active pane
- **Resizable layout** — react-resizable-panels with drag dividers
- **Layout persistence** — pane configs saved to localStorage
- **Default single-pane** — backwards compatible (P8: Low Floor)

### Phase 10.1–10.3: COMPLETED (PTY + Terminal UI + Pane Type System)

Implemented in Session 2 (commit `0d13efe`). Full end-to-end terminal pane pipeline:

| Component | File | What It Does |
|-----------|------|-------------|
| PTY Manager | `src-tauri/src/pty_manager.rs` (250 lines) | 5 Tauri commands: spawn/write/resize/kill/list. Dedicated OS threads for reader loops. Events: pty-output, pty-exit |
| TerminalPane | `src/components/TerminalPane.tsx` (200 lines) | Self-contained xterm.js v6 terminal. HIVE zinc/amber theme. Auto-fit + resize observer. WebLinksAddon. Full PTY lifecycle |
| Pane Type System | `src/types.ts` | PaneType ('chat' \| 'terminal'), AgentConfig, BUILTIN_AGENTS (Shell, Claude Code, Codex, Aider) |
| Multi-Pane Routing | `src/components/MultiPaneChat.tsx` | addTerminalPane(), pane type routing, PTY session tracking |
| Header UI | `src/components/PaneHeader.tsx` | Terminal icon, kill button, "Add" dropdown (Chat / Shell / Claude Code / Codex / Aider) |
| API Wrappers | `src/lib/api.ts` | 7 PTY functions: ptySpawn, ptyWrite, ptyResize, ptyKill, ptyList, onPtyOutput, onPtyExit |

**Dependencies added:** `portable-pty 0.9` + `uuid 1` (Rust), `@xterm/xterm 6` + `@xterm/addon-fit 0.11` + `@xterm/addon-web-links 0.12` (npm)

### Phase 10.4: COMPLETED (Agent Registry + Settings UI)

| Component | File | What It Does |
|-----------|------|-------------|
| Agent Registry UI | `src/components/SettingsTab.tsx` | AgentRegistrySection: list builtin + custom agents, availability check, add/edit/remove custom agents |
| Agent Availability | `src-tauri/src/pty_manager.rs` | `check_agent_available()` — runs `which`/`where` to verify CLI agent is installed |
| Custom Agent Storage | `src/lib/api.ts` | `getCustomAgents()` / `saveCustomAgents()` — localStorage persistence for user-defined agents |
| PaneHeader Integration | `src/components/PaneHeader.tsx` | "Add" dropdown includes custom agents alongside builtins |

### Phase 10.5.1: COMPLETED (PTY Output Memory Logging)

| Component | File | What It Does |
|-----------|------|-------------|
| ANSI Stripping | `src-tauri/src/pty_manager.rs` | `strip_ansi_escapes()` — regex-free ESC sequence removal for clean log text |
| Log Buffer | `src-tauri/src/pty_manager.rs` | Line accumulation in reader loop with 5s flush interval / 8KB max buffer |
| pty-log Event | `src-tauri/src/pty_manager.rs` | `flush_log_buffer()` emits `pty-log` events for optional memory storage |
| Frontend Listener | `src/lib/api.ts` | `onPtyLog()` — TypeScript wrapper for `pty-log` event subscription |

### Phase 10.5.3: COMPLETED (Cross-Agent Tools: send_to_agent + list_agents)

| Component | File | What It Does |
|-----------|------|-------------|
| SendToAgentTool | `src-tauri/src/tools/agent_tools.rs` | HiveTool: write input to a running PTY session's stdin |
| ListAgentsTool | `src-tauri/src/tools/agent_tools.rs` | HiveTool: list all active PTY sessions (id, command, start time) |
| Global Sessions | `src-tauri/src/pty_manager.rs` | Refactored to global `OnceLock<Mutex<HashMap>>` (same pattern as worker_tools) for cross-module access |
| Public API | `src-tauri/src/pty_manager.rs` | `write_to_session()` + `list_sessions_info()` — public functions for tool access |

### Phase 10.5.2: COMPLETED (MCP Auto-Bridge)

| Component | File | What It Does |
|-----------|------|-------------|
| setup_mcp_bridge | `src-tauri/src/pty_manager.rs` | Tauri command: injects HIVE MCP server entry into `~/.claude.json` for Claude Code |
| setupMcpBridge API | `src/lib/api.ts` | TypeScript wrapper for the Tauri command |
| MCP Link Button | `src/components/PaneHeader.tsx` | "Connect HIVE tools via MCP" button on Claude Code terminal panes, with status feedback |

### Phase 10.5.4: COMPLETED (Remote Channel → Agent Routing)

| Component | File | What It Does |
|-----------|------|-------------|
| ChannelRoutingConfig | `src/lib/api.ts` | Interface + localStorage persistence for per-channel routing config |
| ChannelRoutingSection | `src/components/SettingsTab.tsx` | Settings UI: dropdown per channel (Telegram/Discord) → chat pane or terminal agent |
| routeToAgent helper | `src/hooks/useRemoteChannels.ts` | Finds matching PTY session by command, writes message to stdin, falls back to chat |
| Telegram routing | `src/hooks/useRemoteChannels.ts` | Reads routing config, routes to agent or chat pane |
| Discord routing | `src/hooks/useRemoteChannels.ts` | Same pattern as Telegram |

---

## Post-Implementation Hardening (Feb 28 audit)

After Phase 10 was feature-complete, a codebase audit identified 5 improvements. All implemented:

| Fix | What Changed | Why |
|-----|-------------|-----|
| **Session cleanup** | PTY sessions transition to `exited: true` on natural process exit. OS handles (writer, master, child) freed. Metadata stays in HashMap for scrollback. User removes via kill/close. | Previously, exited sessions leaked file descriptors. Also, removing sessions immediately would destroy context the user might need (7.1 — idle session resumption). |
| **Command matching** | `normalizeCommand()` extracts basename from full paths, strips `.exe`/`.cmd`/`.bat`, lowercases. `routeToAgent` uses normalized matching + skips exited sessions. | `"claude"` didn't match `"C:\Users\...\claude.exe"` on Windows. Forward/backslash handling. |
| **strip_thinking fix** | Process `<think>` tags FIRST, then `/think` tags on cleaned output. | The `/think` regex was matching the `/think` inside `</think>` closing tags, causing cross-contamination between multiple thinking blocks. |
| **useChat.ts type fix** | `const { tool_calls }` → `let { tool_calls }` (line 1123). | `const` destructuring was reassigned on lines 1183/1187 — TypeScript TS2588 error. |
| **Test suite** | 11 new Rust tests (strip_ansi_escapes, PtySessionInfo), 7 new vitest tests (normalizeCommand). Fixed 3 stale assertions. | 92 Rust / 44 vitest / 0 tsc — all green. See `docs/TEST_HEALTH.md`. |

---

## What Needs to Be Built

### ~~Phase 10.1: PTY Infrastructure (Rust Backend)~~ COMPLETED

**Goal:** Spawn CLI processes with proper terminal emulation, read/write I/O through Tauri events.

**Dependencies to add:**
- `portable-pty` — Rust crate for cross-platform PTY (pseudo-terminal) allocation. Used by Alacritty, Wezterm, Zed. MIT license.
  - On Windows: uses ConPTY (Windows 10+)
  - On Linux/macOS: uses standard PTY via `openpty()`

**New files:**

| File | Purpose | Est. Lines |
|------|---------|-----------|
| `src-tauri/src/pty_manager.rs` | PTY session manager: spawn, read, write, resize, kill | ~250 |

**New Rust module: `pty_manager.rs`**

```
pty_manager.rs
├── struct PtySession {
│     id: String,
│     process: Box<dyn Child>,
│     writer: Box<dyn Write>,
│     reader: thread::JoinHandle<()>,  // background thread reading PTY output
│     command: String,                  // "claude", "codex", etc.
│     started_at: DateTime,
│   }
│
├── struct PtyManager {
│     sessions: HashMap<String, PtySession>,
│   }
│
├── Tauri commands:
│     pty_spawn(command: String, args: Vec<String>, cols: u16, rows: u16) → String (session_id)
│     pty_write(session_id: String, data: String)
│     pty_resize(session_id: String, cols: u16, rows: u16)
│     pty_kill(session_id: String)
│     pty_list() → Vec<PtySessionInfo>
│
├── Tauri events (emitted from background reader thread):
│     "pty-output" → { session_id: String, data: String }
│     "pty-exit"   → { session_id: String, exit_code: Option<i32> }
│
└── Data flow:
      User keystroke → invoke("pty_write", { session_id, data })
                    → PtySession.writer.write(data)
                    → CLI process receives input
                    → CLI process writes output
                    → Background reader thread reads from PTY
                    → app.emit("pty-output", { session_id, data })
                    → Frontend xterm.js renders the output
```

**Key design decisions:**
1. **Background reader thread per PTY** — reads from the PTY file descriptor in a loop, emits Tauri events. This is how every terminal emulator works (Alacritty, Wezterm, Hyper).
2. **PtyManager is Tauri managed state** — stored in `app.manage(Mutex<PtyManager>)`, same pattern as `AppState`.
3. **Session IDs** — UUID strings, not sequential. Allows reconnection after frontend reload.
4. **No auth handling** — HIVE doesn't touch the CLI agent's login. `claude` handles its own auth, `codex` handles its own. HIVE just provides the terminal window.

**Changes to existing files:**

| File | Change | Risk |
|------|--------|------|
| `src-tauri/Cargo.toml` | Add `portable-pty` dependency | None |
| `src-tauri/src/main.rs` | Add `mod pty_manager;`, register 5 new Tauri commands, add `PtyManager` to managed state | Low |

---

### ~~Phase 10.2: Terminal UI (React Frontend)~~ COMPLETED

**Goal:** Render xterm.js terminal in a chat pane, connected to PTY via Tauri events.

**Dependencies to add:**
- `@xterm/xterm` (v5+) — Terminal emulator component for the web. Used by VS Code, Hyper, Theia, Eclipse Che. MIT license. ~200KB.
- `@xterm/addon-fit` — Auto-resize terminal to container dimensions.
- `@xterm/addon-web-links` — Clickable URLs in terminal output.

**New files:**

| File | Purpose | Est. Lines |
|------|---------|-----------|
| `src/components/TerminalPane.tsx` | xterm.js terminal component with Tauri PTY bridge | ~150 |

**TerminalPane.tsx architecture:**

```tsx
TerminalPane.tsx
├── Props:
│     sessionId: string        // PTY session ID from Rust
│     command: string           // Display name ("Claude Code", "Codex")
│     isActive: boolean         // Focus management
│     onExit: (code) => void   // Cleanup when process exits
│
├── xterm.js setup:
│     const term = new Terminal({ theme: hiveTheme, fontFamily: 'monospace' })
│     const fitAddon = new FitAddon()
│     term.loadAddon(fitAddon)
│     term.loadAddon(new WebLinksAddon())
│     term.open(containerRef.current)
│     fitAddon.fit()
│
├── Input bridge (keystrokes → PTY):
│     term.onData((data) => {
│       invoke("pty_write", { sessionId, data })
│     })
│
├── Output bridge (PTY → xterm.js):
│     listen("pty-output", (event) => {
│       if (event.payload.session_id === sessionId) {
│         term.write(event.payload.data)
│       }
│     })
│
├── Exit handler:
│     listen("pty-exit", (event) => {
│       if (event.payload.session_id === sessionId) {
│         term.write("\r\n[Process exited]\r\n")
│         onExit(event.payload.exit_code)
│       }
│     })
│
├── Resize observer:
│     ResizeObserver on container → fitAddon.fit() → invoke("pty_resize", { sessionId, cols, rows })
│
└── Cleanup:
      useEffect return → unlisten events, term.dispose()
```

**Key design decisions:**
1. **xterm.js v5 (`@xterm/xterm`)** — the v5 package namespace. Not the old `xterm` package.
2. **Session-scoped event filtering** — same pattern as stream_id for chat tokens. Each terminal only processes events for its own session_id.
3. **ResizeObserver → pty_resize** — when the user drags the panel divider, the terminal resizes properly. `FitAddon.fit()` handles the xterm side, `pty_resize` handles the PTY side.
4. **Theme** — reuse HIVE's zinc/amber color scheme for the terminal theme.

---

### ~~Phase 10.3: Pane Type System (Multi-Pane Integration)~~ COMPLETED

**Goal:** MultiPaneChat supports two pane types — `chat` (existing) and `terminal` (new). Each pane in the layout can be either type.

**Changes to existing files:**

| File | Change | Risk |
|------|--------|------|
| `src/types.ts` | Extend `ChatPaneConfig` to support `paneType: 'chat' \| 'terminal'`, add `AgentConfig` type | Low |
| `src/components/MultiPaneChat.tsx` | Render ChatPane or TerminalPane based on pane type, add "Add Terminal" button | Low |
| `src/components/PaneHeader.tsx` | Show terminal-specific header (agent name, running status, kill button) | Low |

**Type changes:**

```typescript
// types.ts additions

/** Configuration for a CLI agent that runs in a terminal pane */
interface AgentConfig {
  id: string;
  name: string;          // Display name: "Claude Code", "Codex", "Aider"
  command: string;        // CLI command: "claude", "codex", "aider"
  args: string[];         // Default arguments
  icon?: string;          // Lucide icon name
  color?: string;         // Brand color for pane header
}

/** Built-in agent presets — user can add custom ones */
const BUILTIN_AGENTS: AgentConfig[] = [
  { id: 'claude-code', name: 'Claude Code', command: 'claude', args: [], icon: 'terminal', color: '#D97706' },
  { id: 'codex',       name: 'Codex',       command: 'codex',  args: [], icon: 'terminal', color: '#10B981' },
  { id: 'aider',       name: 'Aider',       command: 'aider',  args: [], icon: 'terminal', color: '#6366F1' },
  { id: 'shell',       name: 'Shell',       command: process.platform === 'win32' ? 'cmd' : 'bash', args: [], icon: 'terminal-square', color: '#71717A' },
];

/** Extended pane config — a pane is either a chat or a terminal */
type PaneType = 'chat' | 'terminal';

interface ChatPaneConfig {
  id: string;
  paneType: PaneType;          // NEW — 'chat' | 'terminal'
  // For chat panes:
  modelType?: PaneModelType;
  provider?: string;
  modelId?: string;
  modelDisplayName?: string;
  port?: number;
  // For terminal panes:
  agentId?: string;            // NEW — references AgentConfig.id
  ptySessionId?: string;       // NEW — set after PTY spawn
}
```

**MultiPaneChat rendering logic:**

```tsx
// In MultiPaneChat.tsx — the pane renderer switches on paneType
{pane.paneType === 'terminal' ? (
  <TerminalPane
    sessionId={pane.ptySessionId!}
    command={agent.name}
    isActive={activePaneId === pane.id}
    onExit={() => handleTerminalExit(pane.id)}
  />
) : (
  <ChatPane
    pane={pane}
    // ... existing props
  />
)}
```

---

### ~~Phase 10.4: Agent Registry & Settings UI~~ COMPLETED

**Goal:** Users can configure which CLI agents are available, add custom ones, and launch them from the pane header.

**Changes:**

| File | Change | Risk |
|------|--------|------|
| `src/components/SettingsTab.tsx` | New "CLI Agents" section — list agents, add custom, edit command/args | Low |
| `src/lib/api.ts` | `getAgentConfigs()` / `saveAgentConfigs()` — localStorage persistence | Low |
| `src/components/PaneHeader.tsx` | "Add Terminal Pane" dropdown showing available agents | Low |

**Settings UI:**

```
┌─ CLI Agents ──────────────────────────────────────────────────┐
│                                                                │
│  ┌────┐  Claude Code    claude              [Edit] [Remove]   │
│  │ ▶  │  Ready — /usr/bin/claude found                        │
│  └────┘                                                        │
│                                                                │
│  ┌────┐  Codex           codex              [Edit] [Remove]   │
│  │ ▶  │  Not found — install with npm i -g @openai/codex      │
│  └────┘                                                        │
│                                                                │
│  ┌────┐  Aider           aider              [Edit] [Remove]   │
│  │ ▶  │  Ready — /usr/bin/aider found                         │
│  └────┘                                                        │
│                                                                │
│  [+ Add Custom Agent]                                          │
│                                                                │
│  Custom agent: Name [________] Command [________] Args [____] │
└────────────────────────────────────────────────────────────────┘
```

**Agent availability check:** On app start, run `which <command>` (or `where` on Windows) for each configured agent. Show "Ready" or "Not found — install with [instructions]". This is a Tauri command (`check_agent_available(command: String) → bool`).

---

### Phase 10.5: HIVE Integration Layer (The Real Value)

**Goal:** Terminal panes aren't just terminals — they're connected to HIVE's brain.

**Sub-features:**

#### ~~10.5.1: PTY Output Logging to Memory~~ COMPLETED

Every line of output from a terminal pane gets optionally logged to HIVE's memory system. Not raw bytes — parsed, timestamped, associated with the agent name.

```rust
// In pty_manager.rs — the background reader thread
loop {
    let bytes_read = reader.read(&mut buf)?;
    if bytes_read == 0 { break; }
    let text = String::from_utf8_lossy(&buf[..bytes_read]);

    // Emit to frontend
    app.emit("pty-output", PtyOutputPayload { session_id: id.clone(), data: text.to_string() });

    // OPTIONAL: Log to memory (if enabled in settings)
    if log_to_memory {
        // Accumulate lines, flush every N seconds or on significant output
        line_buffer.push_str(&text);
        if should_flush(&line_buffer) {
            memory_save(app, &format!("[{}] {}", agent_name, line_buffer));
            line_buffer.clear();
        }
    }
}
```

**Design:** Not every byte gets saved — a line accumulator flushes every 5 seconds or on certain triggers (tool execution markers, error keywords, `Done.` patterns). This prevents memory spam from streaming output while capturing meaningful events.

| File | Change |
|------|--------|
| `src-tauri/src/pty_manager.rs` | Add optional memory logging in reader thread |
| Settings | New toggle: "Log terminal output to HIVE memory" (default: off) |

#### ~~10.5.2: MCP Auto-Bridge~~ COMPLETED

When a user launches Claude Code in a terminal pane, HIVE can optionally inject MCP configuration so Claude Code discovers HIVE's tools automatically.

**How it works:**
1. Claude Code reads `~/.claude.json` for MCP server configs
2. HIVE adds itself: `{ "mcpServers": { "hive": { "command": "hive-desktop", "args": ["--mcp"] } } }`
3. Now Claude Code can call HIVE's memory, web search, integrations, etc.

This is a **one-time setup instruction**, not runtime code. HIVE shows a "Connect to HIVE via MCP" button in the terminal pane header that:
- Checks if `~/.claude.json` exists
- If so, adds the HIVE MCP server entry (if not already present)
- If not, creates it with the entry

| File | Change |
|------|--------|
| `src-tauri/src/pty_manager.rs` | `setup_mcp_bridge(agent: String)` command — writes MCP config |
| `src/components/PaneHeader.tsx` | "Connect MCP" button for terminal panes |

#### ~~10.5.3: Cross-Agent Tool (send_to_agent)~~ COMPLETED

A new HiveTool that any chat model can use to send a prompt to a running CLI agent.

```json
{
  "name": "send_to_agent",
  "description": "Send a prompt or command to a running CLI agent (Claude Code, Codex, etc.) in a terminal pane",
  "parameters": {
    "agent_id": "string — the agent's session ID or name",
    "input": "string — the text to send (written to PTY stdin)"
  }
}
```

**How it works:**
1. Kimi (in a chat pane) calls `send_to_agent("claude-code", "fix the auth bug in src/auth.ts")`
2. HIVE writes the text to Claude Code's PTY stdin via `pty_write`
3. Claude Code receives it as if the user typed it
4. Output flows back through the PTY output event stream

For the return value, HIVE can optionally **wait for Claude Code to finish** by watching for the prompt to reappear (signaling idle), then return the captured output.

| File | Change |
|------|--------|
| `src-tauri/src/tools/agent_tools.rs` | **New** — `SendToAgentTool` implementing `HiveTool` |
| `src-tauri/src/tools/mod.rs` | Register `send_to_agent` in ToolRegistry |

#### ~~10.5.4: Remote Channel → Agent Routing~~ COMPLETED

Discord/Telegram messages can be routed to a specific terminal pane instead of a chat pane. Config in settings:

```
Discord messages → Route to: [Chat (Kimi) ▾] or [Claude Code ▾] or [Active Pane ▾]
Telegram messages → Route to: [Chat (Kimi) ▾] or [Claude Code ▾] or [Active Pane ▾]
```

When routed to a terminal pane, the message text is written to the agent's PTY stdin.

| File | Change |
|------|--------|
| `src/hooks/useRemoteChannels.ts` | Route to terminal pane via `pty_write` instead of `sendMessageRef` |
| `src/components/SettingsTab.tsx` | Channel routing dropdown |

---

## Implementation Order (Session Planning)

Each session should aim to complete ONE sub-phase. This is designed to survive context compaction.

```
Session 1+2 — Phase 10.1 + 10.2 + 10.3 ✅ COMPLETE
├── Added portable-pty + uuid to Cargo.toml
├── Created pty_manager.rs (spawn, read, write, resize, kill)
├── Registered 5 commands in main.rs + PtyManagerState + AppHandle
├── npm installed @xterm/xterm, @xterm/addon-fit, @xterm/addon-web-links
├── Created TerminalPane.tsx (self-contained, PaneHeader, xterm.js v6)
├── Extended types.ts: PaneType, AgentConfig, BUILTIN_AGENTS, ChatPaneConfig
├── Updated MultiPaneChat: pane type routing, addTerminalPane, PTY tracking
├── Updated PaneHeader: terminal icon, kill button, Add dropdown
├── Added 7 PTY API wrappers in api.ts
├── Rust + TypeScript compile clean
└── Committed 0d13efe, pushed

Session 3 — Phase 10.4 + 10.5.1 + 10.5.3 ✅ COMPLETE
├── Agent availability check: check_agent_available (which/where)
├── AgentRegistrySection in SettingsTab (list, add custom, edit, remove)
├── Custom agent persistence (localStorage via api.ts)
├── PTY memory logging: ANSI stripping, line accumulation, pty-log events
├── send_to_agent + list_agents HiveTools in agent_tools.rs
├── Refactored pty_manager.rs to global OnceLock sessions (cross-module)
├── Rust compile clean
└── Committed 9449134, pushed

Session 4 — Phase 10.5.2 + 10.5.4 ✅ COMPLETE
├── setup_mcp_bridge Tauri command (inject HIVE into ~/.claude.json)
├── "Connect MCP" button in PaneHeader for Claude Code terminal panes
├── ChannelRoutingSection in SettingsTab (per-channel dropdown)
├── routeToAgent helper in useRemoteChannels (pty_list + pty_write)
├── Graceful fallback: no matching session → falls back to chat pane
├── Rust + TypeScript compile clean
└── Committed + pushed
```

---

## Technical Research: Libraries & Prior Art

### portable-pty (Rust)

**Crate:** `portable-pty` — https://crates.io/crates/portable-pty
**Author:** Wez Furlong (creator of Wezterm)
**License:** MIT
**Used by:** Wezterm, Zed editor, Nushell

```rust
use portable_pty::{CommandBuilder, PtySize, native_pty_system};

let pty_system = native_pty_system();
let pair = pty_system.openpty(PtySize { rows: 24, cols: 80, .. })?;

let mut cmd = CommandBuilder::new("claude");
let child = pair.slave.spawn_command(cmd)?;
drop(pair.slave); // Release slave side

let mut reader = pair.master.try_clone_reader()?;
let mut writer = pair.master.take_writer()?;

// Read from PTY (in background thread):
let mut buf = [0u8; 4096];
loop {
    let n = reader.read(&mut buf)?;
    if n == 0 { break; }
    // emit to frontend
}

// Write to PTY:
writer.write_all(b"hello\n")?;

// Resize:
pair.master.resize(PtySize { rows: 40, cols: 120, .. })?;
```

**Key insight:** `portable-pty` handles ConPTY on Windows and standard PTY on Unix. We don't need platform-specific code. It just works.

### xterm.js (TypeScript/React)

**Package:** `@xterm/xterm` (v5)
**License:** MIT
**Used by:** VS Code (integrated terminal), Hyper, Theia, Eclipse Che, CoderPad, Replit

```tsx
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import { WebLinksAddon } from '@xterm/addon-web-links';
import '@xterm/xterm/css/xterm.css';

const term = new Terminal({
  theme: {
    background: '#18181b',  // zinc-900
    foreground: '#fafafa',  // zinc-50
    cursor: '#f59e0b',      // amber-500
    selectionBackground: '#f59e0b33',
  },
  fontFamily: '"JetBrains Mono", "Fira Code", monospace',
  fontSize: 14,
  cursorBlink: true,
});

const fitAddon = new FitAddon();
term.loadAddon(fitAddon);
term.loadAddon(new WebLinksAddon());
term.open(containerElement);
fitAddon.fit();
```

**Key insight:** xterm.js is the **de facto standard** for web-based terminal emulation. It supports full ANSI escape codes, mouse events, true color, ligatures, accessibility. It's what VS Code uses. No reinventing the wheel (P3).

### react-resizable-panels (already installed)

Already in the project. Terminal panes and chat panes coexist in the same `Group` layout. No additional dependency needed.

---

## Architecture Diagrams

### Data Flow: User Keystroke → CLI Agent → Screen

```
User types in xterm.js (TerminalPane.tsx)
       │
       ▼
term.onData(data) callback fires
       │
       ▼
invoke("pty_write", { session_id: "abc-123", data: "fix the bug\n" })
       │
       ▼
Rust: pty_manager.rs → sessions["abc-123"].writer.write_all(data)
       │
       ▼
CLI process (e.g. `claude`) receives "fix the bug\n" on stdin
       │
       ▼
CLI process does its thing (reads files, edits code, prints output)
       │
       ▼
CLI process writes to stdout/stderr
       │
       ▼
Rust: background reader thread reads from PTY master
       │
       ▼
app.emit("pty-output", { session_id: "abc-123", data: "<output bytes>" })
       │
       ▼
TerminalPane.tsx: listen("pty-output") → filter by session_id → term.write(data)
       │
       ▼
xterm.js renders the output (with full ANSI color, cursor movement, etc.)
```

### Data Flow: Cross-Agent Messaging

```
User asks Kimi (chat pane): "Tell Claude Code to fix the auth bug"
       │
       ▼
Kimi generates tool call: send_to_agent("claude-code", "fix the auth bug in src/auth.ts")
       │
       ▼
Rust: agent_tools.rs → finds PTY session for "claude-code"
       │
       ▼
pty_manager.rs → sessions["claude-session"].writer.write_all("fix the auth bug...\n")
       │
       ▼
Claude Code receives the prompt, starts working
       │
       ▼
Output flows through PTY → xterm.js (user sees Claude Code working in its pane)
       │
       ▼
Tool returns: "Sent to Claude Code. Output visible in terminal pane."
       │
       ▼
Kimi: "I've sent the task to Claude Code. You can see it working in the terminal pane."
```

### Data Flow: Discord → Terminal Agent

```
Girlfriend messages on Discord: "hey can you review my PR #42?"
       │
       ▼
discord_daemon.rs → emits "discord-incoming" event
       │
       ▼
useRemoteChannels.ts → checks routing config
       │
       ├── Route = "Chat (Kimi)" → sendMessageRef.current(prompt)
       │     └── Kimi responds via Discord
       │
       └── Route = "Claude Code" → invoke("pty_write", { session_id, data: prompt })
             └── Claude Code reviews the PR
             └── Output captured → sent back via discord_send tool
```

---

## Lattice Principle Compliance

| Principle | Status | Notes |
|-----------|--------|-------|
| P1: Bridges & Modularity | **PASS** | Terminal panes are independent modules. PTY manager is a self-contained Rust module. xterm.js is a drop-in component. |
| P2: Provider Agnosticism | **PASS** | Any CLI agent is just `{ command, args }`. Claude Code, Codex, Aider, a plain shell — all the same interface. The agents are replaceable; the framework survives. |
| P3: Simplicity Wins | **PASS** | portable-pty (battle-tested by Wezterm), xterm.js (battle-tested by VS Code). We write the glue, not the terminal emulator. |
| P4: Errors Are Answers | **PASS** | PTY exit events report exit codes. Agent availability check tells users what to install. "Not found" is actionable. |
| P5: Fix The Pattern | **PASS** | Same event-scoping pattern as streaming (session_id ≈ stream_id). Same self-contained component pattern as MemoryPanel/McpTab. |
| P6: Secrets Stay Secret | **PASS** | HIVE never touches agent auth. Claude Code handles its own login. Codex handles its own. No secrets cross boundaries. |
| P7: Framework Survives | **PASS** | When the next CLI agent appears, add it to BUILTIN_AGENTS. One line. Framework unchanged. |
| P8: Low Floor, High Ceiling | **PASS** | Default: single chat pane (existing behavior). Power user: split into chat + terminal + another terminal. Configure agents in settings. |

---

## Risk Assessment

| Risk | Severity | Mitigation |
|------|----------|------------|
| portable-pty doesn't compile on target | Low | Battle-tested crate, used by Wezterm. Fallback: `std::process::Command` with manual pipe handling (no resize, but functional). |
| xterm.js bundle size (~200KB) | Low | Tree-shaken, loaded only when a terminal pane is opened. Chat-only users never load it. |
| ConPTY not available (Windows < 10 1809) | Very Low | Windows 10 1809+ is required. HIVE already requires Windows 10 for WSL2. |
| PTY output flooding memory | Medium | Line accumulator with flush interval. Memory logging is opt-in. Settings toggle. |
| CLI agent expects interactive TTY features | Low | xterm.js + portable-pty provide a full PTY. ANSI escape codes, mouse events, alternate screen buffer — all supported. |
| Two CLI agents fighting over the same files | User's problem | HIVE doesn't lock files. Same as running two terminals. Maybe future: warning when two agents edit the same file. |

---

## Files Changed/Created (Complete List)

### New Files

| File | Phase | Purpose |
|------|-------|---------|
| `src-tauri/src/pty_manager.rs` | 10.1 | PTY session management |
| `src/components/TerminalPane.tsx` | 10.2 | xterm.js terminal component |
| `src-tauri/src/tools/agent_tools.rs` | 10.5.3 | `send_to_agent` HiveTool |
| `HIVE/docs/PHASE10_NEXUS.md` | — | This document |

### Modified Files

| File | Phase | Change |
|------|-------|--------|
| `src-tauri/Cargo.toml` | 10.1 | Add `portable-pty` dependency |
| `src-tauri/src/main.rs` | 10.1 | Add `mod pty_manager`, register commands, managed state |
| `HIVE/desktop/package.json` | 10.2 | Add `@xterm/xterm`, `@xterm/addon-fit`, `@xterm/addon-web-links` |
| `src/types.ts` | 10.3 | Extend `ChatPaneConfig`, add `AgentConfig`, `PaneType` |
| `src/components/MultiPaneChat.tsx` | 10.3 | Render terminal panes, "Add Terminal" button |
| `src/components/PaneHeader.tsx` | 10.3 | Terminal-specific header (kill, restart, MCP connect) |
| `src/components/SettingsTab.tsx` | 10.4 | "CLI Agents" settings section |
| `src/lib/api.ts` | 10.4 | Agent config persistence, PTY Tauri command wrappers |
| `src-tauri/src/tools/mod.rs` | 10.5.3 | Register `send_to_agent` tool |
| `src/hooks/useRemoteChannels.ts` | 10.5.4 | Route to terminal pane option |

---

## How To Continue (For New Sessions)

1. **Read this document first** — understand the vision, the phases, what's built
2. Read `CLAUDE.md` for coding standards and principle lattice
3. Check `ROADMAP.md` for overall HIVE context
4. `git log --oneline -10` to see recent commits
5. Check which sub-phase is next by reading "Implementation Order" above
6. `npx tsc --noEmit` to verify TypeScript compiles before you start
7. Pick up the next sub-phase, implement it, compile check, commit, push
8. Update the "Status" line at the top of this document

**The key insight:** HIVE doesn't reinvent the terminal. It uses battle-tested libraries (portable-pty, xterm.js) and provides the **connective tissue** — memory logging, MCP bridge, cross-agent tools, remote channel routing. That connective tissue is what makes HIVE the skeleton key, not just another terminal emulator.

---

*Phase 10 codename: NEXUS — because it's the node that connects every agent, every model, every channel into one interface.*

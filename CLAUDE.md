# HIVE Project Instructions for Claude Code

## Project Overview

HIVE (Hierarchical Intelligence with Virtualized Execution) is a persistent AI orchestration harness — a Windows desktop application that coordinates local and cloud LLMs as interchangeable cognitive resources. It's not a chatbot or model runner; it's the always-on brain that manages models (local GGUF, Ollama, OpenAI, Anthropic, OpenRouter, or any OpenAI-compatible endpoint), routes tasks to specialist agents, and maintains memory across sessions. The framework is permanent; the models are replaceable (P2: Provider Agnosticism).

It consists of:

- **Desktop App**: Tauri v2 (Rust + React/TypeScript) in `HIVE/desktop/`
- **Launcher**: `START_HIVE.bat` for one-click Windows startup

## Key Directories

```
HIVE/
├── desktop/          # Tauri v2 app
│   ├── src/          # React: App.tsx (orchestrator) + components/
│   ├── src/lib/      # api.ts (TypeScript API layer + recommendation engine)
│   ├── src/components/  # 9 tab/utility components (props-only, no Context)
│   ├── src-tauri/    # Rust code (main.rs), tauri.conf.json
│   └── src-tauri/src/tools/  # Tool framework: file_tools, system_tools, web_tools
├── docs/             # PRINCIPLE_LATTICE.md, architecture docs

claude-tools/         # Claude Code optimizations (mgrep, etc.)
.claude/              # Hooks, skills, settings
```

## Principle Lattice

HIVE has 8 axiomatic principles. Read [`HIVE/docs/PRINCIPLE_LATTICE.md`](HIVE/docs/PRINCIPLE_LATTICE.md) for the full lattice with instantiations. Summary:

| # | Principle | Axiom |
|---|-----------|-------|
| 1 | **Bridges and Modularity** | One path, two systems. Lego blocks, not monoliths. |
| 2 | **Provider Agnosticism** | The interface is permanent. The backend is replaceable. |
| 3 | **Simplicity Wins** | Don't reinvent the wheel. Code exists to be used. |
| 4 | **Errors Are Answers** | Every failure teaches. Given a model, the program debugs itself. |
| 5 | **Fix The Pattern** | Cure the root cause. Don't treat symptoms. |
| 6 | **Secrets Stay Secret** | Military-grade OPSEC. Nothing left open to exploitation. |
| 7 | **The Framework Survives** | Models evolve. HIVE endures. |
| 8 | **Low Floor, High Ceiling** | A noob can use it. A power user would want to. |

When making design decisions, check against these principles. If a choice violates one, reconsider.

## Development Guidelines

1. **Tauri v2 Config**: `nsis` settings go inside `bundle.windows.nsis`, not `bundle.nsis`
2. **No package-lock.json**: Use `npm install`, not `npm ci`
3. **Batch Scripts**: Use `!var!` (delayed expansion) not `%var%` inside blocks
4. **PR Descriptions**: Follow format in `.claude/PR_GUIDELINES.md`

## Common Tasks

### Building the Desktop App
```bash
cd HIVE/desktop
npm install
npm run tauri build
```

### Running Development Mode
```bash
cd HIVE/desktop
npm run tauri dev
```

### Testing the Launcher
Double-click `START_HIVE.bat` - it handles dependency checks and builds automatically.

## Semantic Search (mgrep)

If mgrep is installed, prefer semantic queries over grep:
```bash
mgrep "where is WSL connection handled?"
mgrep "authentication flow"
mgrep "error handling in backend"
```

## Things to Avoid

- Don't use `npm ci` (no lock file exists)
- Don't put Tauri NSIS config at `bundle.nsis` level (use `bundle.windows.nsis`)
- Don't use `%errorlevel%` inside batch `if` blocks (use flag variables)
- Don't commit to main directly - use feature branches
- Don't frame HIVE as "just a model runner" or "chatbot wrapper" — HIVE is an **orchestration harness**: the framework (slots, routing, memory, sleep/wake) is permanent, the models (local, cloud, any provider) are swappable
- Don't subordinate cloud providers to "fallback" status — they're coequal with local (P2)
- Don't describe HIVE as "local-first" — it's provider-agnostic. A user with zero GPUs using only free API models is a first-class use case
- **Don't use subagents (Task tool) for research** — do the work directly with WebFetch/WebSearch/Grep/Read. Subagents burn 5-10x more tokens for the same result and take longer. Only use subagents for truly independent parallel *write* tasks, never for research or exploration

## Git Workflow (MANDATORY)

**ALWAYS sync with main before pushing:**
```bash
git fetch origin
git merge origin/main --no-edit
git push -u origin <branch-name>
```

This prevents branches from falling behind and avoids merge conflicts. Never push without fetching first.

## Coding Standards (CRITICAL)

These patterns prevent bugs that have occurred multiple times. **Follow them exactly.**

### 1. Simple Solutions Over Complex Ones

**ALWAYS prefer the simpler approach that already works.**

```
BAD:  "Let me add a WSL-specific download path for optimization"
GOOD: "Download to Windows, WSL accesses via /mnt/c/ bridge (already works)"

BAD:  "Let me add a complex retry mechanism with exponential backoff"
GOOD: "Just make the simple request work first"
```

If something worked before, check git history before rewriting it.

### 2. Shell Quoting in WSL Commands

**Variable expansion depends on quote type:**

```rust
// WRONG - single quotes prevent $HOME expansion
let cmd = format!("find '{}' ...", "$HOME/models");
// Results in: find '$HOME/models' (literal string!)

// RIGHT - double quotes allow $HOME expansion
let cmd = format!("find \"{}\" ...", "$HOME/models");
// Results in: find "/home/user/models" (expanded!)

// ALSO RIGHT - expand $HOME first, then use any quotes
let home = get_wsl_home();  // "/home/user"
let cmd = format!("find '{}' ...", format!("{}/models", home));
```

**Rule:** Use double quotes `"{}"` when the path contains shell variables like `$HOME`.

### 3. Windows Process Spawning

**Always hide console windows when spawning background processes:**

```rust
// WRONG - spawns visible CMD window on Windows
Command::new("wsl").args([...]).spawn();

// RIGHT - use helper that adds CREATE_NO_WINDOW flag
fn wsl_cmd() -> Command {
    let mut cmd = Command::new("wsl");
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}
wsl_cmd().args([...]).spawn();
```

This is already implemented - use `wsl_cmd()` for all WSL commands.

### 4. HTTP Requests to External Services

**Always set User-Agent for external APIs (HuggingFace, GitHub, etc.):**

```rust
// WRONG - HuggingFace blocks requests without User-Agent
let client = reqwest::Client::new();

// RIGHT
let client = reqwest::Client::builder()
    .user_agent("HIVE-Desktop/1.0")
    .build()?;
```

### 5. Prefer Windows-Native + Bridge Over WSL-Specific Code

**The `/mnt/c/` bridge already exists. Use it.**

```
BAD:  Download files via curl in WSL → complex, fragile, CMD popups
GOOD: Download via Rust to Windows → WSL reads via /mnt/c/ (simple, works)

BAD:  Run complex bash pipelines in WSL for simple tasks
GOOD: Do it in Rust/TypeScript, only use WSL for what requires Linux
```

### 6. Error Messages Must Be Actionable

```rust
// WRONG - useless error
return Err("Download failed".to_string());

// RIGHT - says what went wrong
return Err(format!("Download failed: {}", stderr_output));
```

### 7. Don't Create Dead Code

If you replace a function/variable, remove the old one. Don't leave commented code or unused variables.

### 8. Check Git History Before "Fixing"

If something used to work:
```bash
git log --oneline --all | grep -i "download"  # Find when it changed
git show <commit>:path/to/file               # See old working version
```

Often the fix is reverting to what worked, not adding more code.

### 9. Fix ALL Instances of a Pattern

When you find a bug, **search for the same pattern everywhere**:

```bash
# Found a quoting bug? Check ALL shell commands
grep -n "format!.*bash.*-c" src/main.rs

# Found missing User-Agent? Check ALL HTTP clients
grep -n "reqwest::Client" src/main.rs

# Found missing CREATE_NO_WINDOW? Check ALL Command::new
grep -n "Command::new" src/main.rs
```

One bug usually means the same mistake exists in 3-5 other places.

### 10. No Cross-File String Contracts Without a Shared Source (P5)

**HIVE's #1 recurring anti-pattern: defining a string/format in one file and parsing/matching it by hardcoded string in another.** When they drift, the system silently breaks.

**The Rule:** If two files must agree on a string value, format, or list — there MUST be a single source of truth that both reference. Never rely on comments like "matches foo.rs::bar()".

```
BAD:  // Format set by useRemoteChannels: "[Telegram from X | chat: Y | Host]"
      const tgMatch = userContent.match(/^\[Telegram from (.+?) \| chat: (\S+?)/)
      (format in file A, regex in file B — drift guaranteed)

GOOD: // channelPrompt.ts — single file with builder + parser
      export function buildChannelPrompt(...) { ... }
      export function parseChannelPrompt(text) { ... }
```

**Known cross-file contracts to keep in sync (with current mitigations):**

| Contract | Source of Truth | Mirror | Sync Method |
|---|---|---|---|
| Specialist port mapping | `server.rs::port_for_slot()` | `types.ts::SPECIALIST_PORTS` | Cross-ref comment |
| Channel prompt format | `channelPrompt.ts` | — | Single file (P5 fixed) |
| Dangerous tools list | `content_security.rs::is_dangerous_tool()` | `api.ts::DANGEROUS_TOOLS` | Cross-ref comment + "MUST stay in sync" |
| Desktop-only tools | `content_security.rs::is_desktop_only_tool()` | `api.ts::DESKTOP_ONLY_TOOLS` | Cross-ref comment |
| SenderRole enum | `types.rs::SenderRole` | `types.ts::SenderRole` | Single Rust definition, TS matches via serde |
| Terminal tools | `chainPolicies.ts::TERMINAL_TOOLS` | `useChat.ts` (re-export) | Single source, re-exported for compat |
| Tauri event names | Rust `emit("name", ...)` | TS `listen("name", ...)` | Convention (14 events, all verified) |
| Agent bridge event | `pty_manager.rs::emit("agent-response")` | `api_integrations.ts::onAgentResponse` | Cross-ref: event name "agent-response" in both |
| Tauri commands | Rust `#[tauri::command]` | TS `invoke("name")` | Convention (148 commands, all verified) |
| Worker blocked tools | `worker_tools.rs::WORKER_BLOCKED_TOOLS` | — | Single file (10 tools: run_command, write_file, worker_spawn, telegram_send, discord_send, plan_execute, memory_import_file, send_to_agent, github_issues, github_prs) |
| File import extensions | `memory_tools.rs::text_extensions` | `MemoryTab.tsx` file dialog filter | Cross-ref comment: "Sync with text_extensions in memory_tools.rs" |

**When adding new cross-boundary contracts:**
1. First, try to make it a single file (best — e.g., `channelPrompt.ts`)
2. If Rust↔TS prevents that, add explicit cross-reference comments in BOTH files
3. Add the contract to this table
4. If the contract is security-sensitive, add a test asserting both sides match

### 11. Persistent Logging Convention (P4: Errors Are Answers)

HIVE has a dual-log system. **Both must be maintained** for AI self-awareness:

| Log System | Where | AI Readable? |
|---|---|---|
| **UI Logs** (`useLogs.ts`) | In-memory React + persisted to `hive-app.log` via `logToApp()` | YES (via `check_logs` tool) |
| **Backend Logs** (`append_to_app_log`) | Written directly to `hive-app.log` from Rust | YES (via `check_logs` tool) |

**Frontend:** `useLogs.ts` automatically persists all `[HIVE]` logs, errors, and warnings to disk. Prefixed with `FE |`, `FE_ERROR |`, `FE_WARN |`.

**Backend:** Use `crate::tools::log_tools::append_to_app_log()` at key lifecycle events. Follow the prefix convention:

```
MODULE | event | key=value pairs | human-readable detail
```

| Prefix | Module |
|---|---|
| `SERVER` | server.rs — model start/stop/crash |
| `PROVIDER` | providers.rs — chat/stream/tool errors |
| `MEMORY` | memory.rs — init/save/delete |
| `TELEGRAM` | telegram_daemon.rs — daemon lifecycle, messages |
| `DISCORD` | discord_daemon.rs — daemon lifecycle, messages |
| `ROUTINES` | routines.rs — create/trigger/daemon lifecycle |
| `SLOTS` | slots.rs — slot configuration |
| `HARNESS` | harness.rs — identity save/reset |
| `DOWNLOAD` | download.rs — model download start/complete/error |
| `MCP` | mcp_client.rs — server connect/disconnect |
| `PTY` | pty_manager.rs — session spawn/exit/kill |
| `WORKER_SPAWN` / `WORKER_COMPLETE` / `WORKER_FAILED` / `WORKER_TERMINATED` / `WORKER_REPORT` | worker_tools.rs — worker lifecycle |
| `TOOL_CHAIN` | useChat.ts tool loop (already existed) |
| `FE` / `FE_ERROR` / `FE_WARN` | Frontend bridge (auto-captured) |

**When adding new features:** Always add `append_to_app_log` at lifecycle events (init, start, stop, error). The steering AI is blind to anything not in `hive-app.log`.

## HIVE Architecture Patterns

### The Windows ↔ WSL Bridge

```
┌─────────────────────────────────────────────────────────────┐
│  WINDOWS                                                     │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  HIVE Desktop (Tauri)                                │    │
│  │  - Downloads models to: C:\Users\X\AppData\Local\HIVE│    │
│  │  - Runs llama-server.exe (NVIDIA) OR               │    │
│  │  - Spawns WSL for llama-server (AMD ROCm)          │    │
│  └─────────────────────────────────────────────────────┘    │
│                           │                                  │
│                    Path Bridge                               │
│          C:\Users\X\... ↔ /mnt/c/Users/X/...               │
│                           │                                  │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  WSL2 (Ubuntu)                                       │    │
│  │  - llama-server with ROCm for AMD GPUs              │    │
│  │  - Can read Windows files via /mnt/c/               │    │
│  │  - Can also have models in ~/models (native speed)  │    │
│  └─────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
```

**Key insight:** Don't duplicate functionality. Download once to Windows, both can access it.

### Tauri Data Flow

```
React (App.tsx)           Rust (main.rs)              External
     │                         │                          │
     │  invoke('command')      │                          │
     ├────────────────────────>│                          │
     │                         │  HTTP/spawn              │
     │                         ├─────────────────────────>│
     │                         │<─────────────────────────┤
     │  Result<T, String>      │                          │
     │<────────────────────────┤                          │
     │                         │                          │
     │  emit('event')          │                          │
     │<────────────────────────┤  (for progress updates)  │
```

**Pattern:**
- `invoke()` for request/response
- `emit()` for streaming updates (download progress, etc.)
- Errors are always `Result<T, String>` - use `.map_err(|e| format!(...))`

### State Management

```typescript
// Local state in App.tsx
const [models, setModels] = useState<LocalModel[]>([]);
const [serverRunning, setServerRunning] = useState(false);

// Persisted settings in localStorage
api.saveModelSettings(filename, { contextLength, kvOffload, gpuLayers });

// Secure storage (API keys) - encrypted file via Rust
api.storeApiKey('openai', key);  // → ~/.hive/secrets.enc
```

**Rule:** Settings go in localStorage. Secrets go in encrypted file. Memory goes in SQLite + markdown. Never mix them.

**Rust-side state (`AppState` in `state.rs`):**
- `server_process` / `server_port` / `server_backend` — main llama-server
- `specialist_servers` — `HashMap<u16, SpecialistServer>` keyed by port
- `spawned_pids` — `HashSet<u32>` tracking PIDs of all spawned server processes for targeted cleanup (replaces nuclear `taskkill /IM`)

### Memory System

```
SQLite (memory.db)     →  Indexed search (FTS5 + vector embeddings)
Markdown (memory/*.md) →  Source of truth, human-readable daily logs
```

Memory recall is **session-injected** — a discrete system message in the conversation array, NEVER mixed into the user's system prompt. This is provider-agnostic (P2).

```typescript
// RIGHT — session injection (separate message)
apiMessages.push({ role: 'system', content: systemPrompt });
apiMessages.push({ role: 'system', content: memoryContext });  // discrete

// WRONG — system prompt mutation (model-dependent, breaks continuity)
systemPrompt += '\n\n' + memoryContext;  // DON'T DO THIS
```

Embeddings use OpenAI `text-embedding-3-small`. If no API key, gracefully degrades to FTS5-only search (no crash, no error — P4).

**Context Engineering (arXiv:2512.05470 + 2601.03236):**
- **Recency decay:** Hybrid search scores multiplied by `1/(1 + 0.1*ln(1+days_old))`. Recent > old at equal relevance.
- **Relevance threshold:** Memories scoring < 0.15 are never injected. No context pollution.
- **Context-proportional budget:** Memory injection capped at 10% of the model's context window (P2). A 4K model gets ~1600 chars of memory, a 128K model gets ~51K. Pass `contextTokens` to `memoryRecall()`.
- **Deduplication:** Before saving extracted memories, `is_near_duplicate()` checks cosine > 0.92 against existing chunks. Prevents memory bloat.
- **Phase 4 (MAGMA):** Current flat memory becomes the semantic graph in a multi-graph architecture (episodic + procedural + entity graphs). See `ROADMAP.md` Phase 4.

### Provider Abstraction

The app supports multiple backends. When adding features, consider ALL providers:

| Provider | Where it runs | Model source |
|----------|--------------|--------------|
| Local (llama.cpp) | Windows or WSL | GGUF files |
| Ollama | localhost:11434 | Ollama library |
| OpenAI | API | cloud |
| Anthropic | API | cloud |
| OpenRouter | API | cloud (100+ models) |
| DashScope (Alibaba) | API | cloud (Kimi K2.5, Qwen) |

Code that works for one must work for all, or gracefully degrade.

### Remote Channel Security (P6: Secrets Stay Secret)

Remote channels (Telegram, Discord) use a **Host/User access model**:

```
┌─────────────────────────────────────────────────────────────┐
│  Message arrives from Telegram/Discord                       │
│                                                              │
│  1. GATE: Is sender_id in host_ids or user_ids?             │
│     ├─ Neither → REJECT (silent drop, log only)             │
│     ├─ host_ids → SenderRole::Host                          │
│     └─ user_ids → SenderRole::User                          │
│                                                              │
│  2. PROPAGATE: sender_role → DiscordIncoming/TelegramIncoming│
│     → App.tsx sets messageOriginRef ('remote-host'/'user')   │
│                                                              │
│  3. ENFORCE: useChat.ts tool loop checks before execution    │
│     ├─ Desktop → all tools allowed                          │
│     ├─ RemoteHost → desktop-only BLOCKED, dangerous = ask   │
│     └─ RemoteUser → ALL dangerous tools BLOCKED             │
└─────────────────────────────────────────────────────────────┘
```

**Key rules:**
- Empty host_ids + empty user_ids = **reject ALL** (closed by default)
- Desktop-only tools: `run_command`, `write_file` — blocked for ALL remote origins
- Dangerous tools (must match in BOTH `content_security.rs::is_dangerous_tool()` AND `api.ts::DANGEROUS_TOOLS`):
  `run_command`, `write_file`, `telegram_send`, `discord_send`, `github_issues`, `github_prs`, `worker_spawn`, `send_to_agent`, `plan_execute`
- Remote Host auto-approve mode is overridden to 'ask' (always prompt in desktop UI)
- Tool restrictions are enforced in `content_security.rs::check_tool_access()` (Rust) and `api.ts::checkToolOriginAccess()` (TypeScript — the runtime gate)

**When editing remote channel code:**
1. Never make empty lists mean "accept all" — that inverts the security model
2. Desktop-only tools must NEVER execute from remote channels, even for Hosts
3. The `messageOriginRef` in useChat.ts must be set BEFORE `sendMessageRef.current()` is called
4. When adding a new dangerous tool, add it to BOTH the Rust and TS lists (see Coding Standard #11)

### Worker System (Autonomous Sub-Agents)

Workers are independent model conversations spawned via `worker_spawn`, running as tokio tasks. They enable parallel execution — e.g., 20 workers analyzing different repos simultaneously.

```
Parent Chat (orchestrator)
  ├── worker_spawn(task="analyze repo A", scratchpad_id="pad1")
  ├── worker_spawn(task="analyze repo B", scratchpad_id="pad2")
  └── ... (up to N concurrent workers, limited by API rate limits)

Each Worker:
  ├── Own tool registry (filtered — no run_command/write_file/worker_spawn)
  ├── Own message context (independent of parent)
  ├── Writes results to shared scratchpad (lock-free sectioned writes)
  └── Reports to parent via report_to_parent (→ injected into parent chat)
```

**Termination hierarchy** (checked in this order):
1. **WallClockTimeout** — `max_time_seconds` exceeded (PRIMARY, default 10 min)
2. **DoneSignal** — `report_to_parent(severity="done")` sets flag → exits next turn
3. **ExternalTerminate** — `worker_terminate()` called by parent
4. **NaturalCompletion** — LLM responds with text (no tool calls)
5. **RepetitionDetection** — 3x identical tool call set (name + args) = stuck
6. **MaxTurns** — safety valve only (default 100)

**Anti-spam gates on `report_to_parent`:**
- 30s cooldown between reports (bypassed by severity `error`/`done`)
- Progress gate: must execute new tools since last report
- Semantic dedup: >70% Jaccard word overlap with last report = rejected

**Stress test benchmarks (20 workers, Kimi K2.5 via DashScope):**
- 85% completion rate (3/20 failed — all behavioral, not infrastructure)
- Zero scratchpad write conflicts across 20 concurrent writers
- ~28s average turn latency (cloud API round-trip)
- Message queue drained cleanly (15→0 in sequence)
- Context compaction working (memory_seq_rm entries in llama.cpp logs)

**Worker dispatch protocol — crafting effective worker tasks:**

When the orchestrating model spawns a worker, the task prompt should be **self-contained and precisely scoped**. Workers have no access to the parent's conversation history — they only see what you put in the task string. Follow this structure:

```
worker_spawn(task="""
  ## Task
  [One clear sentence: what to do]

  ## Context
  [Everything the worker needs to understand WHERE this fits.
   File paths, architectural context, dependencies.
   Paste relevant content — never tell the worker to "go read file X".]

  ## Constraints
  [What NOT to do. Scope boundaries. Files not to touch.]

  ## Expected Output
  [What should report_to_parent contain?
   Use severity="done" when finished.
   Use severity="error" if blocked — describe what's needed.]
""")
```

**Worker status protocol** — workers should report one of these states:
- **DONE** → `report_to_parent(severity="done", message="[what was accomplished]")`
- **DONE_WITH_CONCERNS** → `report_to_parent(severity="done", message="Completed, but: [concern]")`
- **BLOCKED** → `report_to_parent(severity="error", message="Cannot proceed: [what's needed]")`
- **PROGRESS** → `report_to_parent(severity="info", message="[status update]")` (rate-limited by anti-spam)

**When reviewing worker output:** Never trust "DONE" without verification. Check the scratchpad, check git diff, verify the claim independently (see Trap 11).

**When editing worker code:**
1. The `done_signaled` flag is the ONLY way `report_to_parent` stops the loop — verify it's checked
2. Workers CANNOT use tools in `WORKER_BLOCKED_TOOLS` (P6 security)
3. Context truncation triggers at >100K chars — keeps system prompt + task + recent turns
4. Workers inherit provider/model from session context (P2 — models are swappable)
5. Workers default to `thinking_depth="low"` (2048 tokens) to control token burn — 5 workers at "high" = ~160K thinking tokens/turn. The orchestrating model can override per-worker via `worker_spawn(thinking_depth="high")` for complex tasks

## Common Session Traps

**These are thoughts you catch yourself thinking.** If any of these phrases pass through your reasoning, treat it as a red flag — you are about to make a mistake. The thought itself IS the warning. Don't rationalize past it; the moment you think "but this time it's different" is the moment you are most wrong.

**Violating the letter of these traps is violating their spirit.** "I'm not optimizing, I'm *improving*" is Trap 1. "I'm not rewriting, I'm *refactoring*" is Trap 5. The relabeling IS the trap.

### Trap 1: "Let me optimize this"
**Stop.** Is it slow? Is the user complaining? If not, don't touch it.

### Trap 2: "I'll add a WSL-specific path"
**Stop.** The bridge exists. Use it. WSL-specific code doubles maintenance burden.

### Trap 3: "I'll fix this one place"
**Stop.** Search for the same pattern. Fix them all or none.

### Trap 4: "The error says X, so I'll fix X"
**Stop.** The error might be downstream of the real bug. Trace backwards.

### Trap 5: "I need to rewrite this function"
**Stop.** Check git history. Maybe a past version worked. Maybe revert, not rewrite.

### Trap 6: "I'll inject memory/context into the system prompt"
**Stop.** Memory and recalled context are session-injected as separate messages. Never mutate the system prompt — it's model-dependent, breaks continuity, violates P2.

### Trap 7: "I'll wrap this component in a div for CSS control"
**Stop.** Check the flex/layout chain. If you add a wrapper `<div className="flex-1">`, the parent MUST be a flex container (`flex` or `flex-col`), or `flex-1` is silently ignored and layout collapses. The chat scrolling broke because a wrapper div used `flex-1` inside a non-flex parent. Always verify the CSS layout chain from root to leaf.

### Trap 8: "I'll add this to the dangerous tools list"
**Stop.** There are TWO lists — Rust (`is_dangerous_tool()`) and TypeScript (`DANGEROUS_TOOLS`). If you only update one, the other silently passes. See Coding Standard #10.

### Trap 9: "The worker just needs more turns"
**Stop.** If a worker is looping after completing its task, the problem is exit behavior, not turn budget. Workers that call `report_to_parent(severity="done")` will exit cleanly. Workers that keep calling `scratchpad_write` or `report_to_parent` repeatedly are stuck in a behavioral loop — the repetition detector (3x identical calls) will catch them, but the root fix is ensuring workers call `report_to_parent(severity="done")` after task completion.

### Trap 10: "Let me try one more fix"
**Stop. Count your fix attempts.** If you've tried 3 fixes for the same issue and none worked, the problem is NOT the code — it's the architecture, your mental model, or a misunderstood requirement. **Do NOT attempt fix #4.** Instead:
1. State what you tried and why each failed
2. Ask the user: "I've tried 3 approaches and all failed. Should I continue down this path or reconsider the approach?"
3. Check git history — maybe a previous version worked and the real fix is a revert

Three failed fixes is the circuit breaker. Escalate, don't iterate. Each failed fix that reveals a NEW problem in a DIFFERENT place is a strong signal you're fighting the architecture, not a bug.

### Trap 11: "This should work now"
**Stop.** The words "should", "seems", "looks like", "appears to", "probably" are NEVER acceptable when describing the state of your own work. If you haven't run the verification command **in this response**, you cannot claim the result. The only acceptable form is evidence-first:
- "Ran `cargo test` → 47 passed, 0 failed → all tests pass"
- "Ran `npx tsc --noEmit` → exit 0 → no type errors"

**Forbidden phrases** (if you catch yourself typing these, STOP and run the command):
- "Should work now" / "This should fix it"
- "Looks correct" / "Seems right"
- "Done!" / "Fixed!" / "All good!" (before verification)
- "I'm confident this works" (confidence ≠ evidence)

### Self-Check: Am I Rationalizing?

If you find yourself constructing an argument for why a trap doesn't apply to your current situation, that IS the trap firing. Common rationalization patterns:

| If you're thinking... | You're actually doing... |
|---|---|
| "This is different because..." | It's not. Apply the trap. |
| "I'm not optimizing, I'm *improving*" | Trap 1 with a label swap. |
| "Just this one quick change" | Trap 3 (fix one place, miss the pattern). |
| "I know what the bug is" | Trap 4 (fixing symptoms, not root cause). |
| "The user will understand" | Trap 11 (claiming success without evidence). |
| "I already tested this mentally" | Trap 11 (confidence ≠ evidence). |
| "This is too simple to need verification" | The simpler it seems, the more likely you're wrong. |
| "I'll clean this up later" | You won't. Do it now or don't do it. |

## What Each Key File Does

| File | Purpose | Touch carefully |
|------|---------|-----------------|
| `main.rs` | Tauri setup, tray icon (graceful fallback), `perform_full_cleanup()` (shared shutdown), 148 command handler registrations | Yes - core logic |
| `harness.rs` | Cognitive harness: identity (HIVE.md) + capability manifest + assembler + skills system (load, match, inject, seed). `harness_read_skill` has path traversal protection (canonicalize + containment check) | Yes - core cognitive layer |
| `memory.rs` | Memory system: SQLite + FTS5 + vector embeddings, hybrid search, daily logs | Yes - adapted from OpenClaw (MIT) |
| `providers.rs` | Cloud provider chat + streaming (OpenAI, Anthropic, Ollama, OpenRouter, DashScope SSE) | Yes - provider-specific parsing |
| `tools/file_tools.rs` | read_file (paginated with offset/limit), write_file, list_directory | Moderate - tool implementations |
| `tools/mod.rs` | Tool trait, registry, schemas, Tauri commands, MAGMA auto-entity tracking | Yes - tool framework core |
| `tools/memory_tools.rs` | memory_save/search/edit/delete, task_track, graph_query, entity_track, procedure_learn | Yes - memory + MAGMA tools |
| `App.tsx` | State orchestrator, imports tab components, VRAM pre-launch check, harness + memory session injection | Yes - all state lives here |
| `api.ts` | TypeScript API layer + recommendation engine + memory API + harness API + retry logic + export/import | Usually safe |
| `discord_daemon.rs` | Discord REST polling daemon, multi-channel watching, Host/User access control | Moderate - mirrors telegram_daemon pattern |
| `routines.rs` | Routines engine: standing instructions, cron scheduler, event matcher, message queue | Yes - Phase 6 autonomous agency |
| `tools/discord_tools.rs` | discord_send + discord_read HiveTool impls | Moderate - tool implementations |
| `tools/integration_tools.rs` | integration_status tool — models discover available integrations | Moderate - P6 sensitive |
| `pty_manager.rs` | Phase 10 NEXUS: PTY session manager — spawn/read/write/resize/kill via portable-pty. Dedicated OS threads for reader loops, Tauri event emission. Global sessions via OnceLock for cross-module access | Moderate - standalone module |
| `tools/worker_tools.rs` | Worker system: worker_spawn/status/terminate + report_to_parent + autonomous loop with termination hierarchy | Yes - concurrent execution core |
| `tools/scratchpad_tools.rs` | Scratchpad CRUD: shared scratch space for inter-worker communication | Moderate - tool implementations |
| `tools/plan_tools.rs` | Plan execution: plan_execute multi-step agent with approval flow | Moderate - tool implementations |
| `tools/agent_tools.rs` | Phase 10 NEXUS: send_to_agent + list_agents HiveTools — model→agent bridge via PTY write | Moderate - tool implementations |
| `mcp_server.rs` | MCP server mode: exposes HiveTools via MCP protocol on stdio (`--mcp` flag) | Moderate - standalone module |
| `mcp_client.rs` | MCP client: connect to external MCP servers, register proxy tools in ToolRegistry | Moderate - standalone module |
| `content_security.rs` | External content wrapping, SSRF protection, homoglyph folding, remote channel tool gating (Host/User/Desktop origin) | Yes - security layer |
| `RoutinesPanel.tsx` | Routines CRUD UI: create/toggle/delete standing instructions (self-contained) | Moderate - UI only |
| `TerminalPane.tsx` | Phase 10 NEXUS: Self-contained xterm.js terminal component. Owns PTY lifecycle (spawn on mount, cleanup on unmount). HIVE zinc/amber theme | Moderate - UI only |
| `components/*.tsx` | 14 tab/utility components (self-contained or props-only, no Context) | Moderate - UI only |
| `MemoryPanel.tsx` | Slide-out memory browser: search, add, edit, delete memories (self-contained) | Moderate - UI only |
| `MemoryTab.tsx` | Full-page Memory tab: memory browser + MAGMA graph viewer (self-contained) | Moderate - UI only |
| `McpTab.tsx` | Full-page MCP tab: server mode instructions + client management (self-contained) | Moderate - UI only |
| `useChat.ts` | Chat logic: sendMessage, tool loop, chain policies, harness build, streaming, remote channel security gate, specialist routing (VRAM enforcement, auto-sleep, cloud routing, procedure learning, skills injection) | Yes - core chat engine |
| `lib/channelPrompt.ts` | Channel prompt builder + parser — single source of truth for remote message format (P5) | Moderate - format contract |
| `types.ts` | Shared types + constants: Tab, Backend, Message, SlotRole, SPECIALIST_PORTS, SenderRole, MessageOrigin, BUILTIN_AGENTS | Usually safe |
| `CLAUDE.md` | Instructions for future sessions | Add patterns here |
| `HIVE/docs/TEST_HEALTH.md` | Test suite baseline — compare before pushing | Update after adding tests |

### Retry Logic

Network calls to external APIs (HuggingFace, Leaderboard) use `withRetry()` for exponential backoff:

```typescript
// All external fetch calls should use withRetry for resilience
const response = await withRetry(() => fetch(url).then(r => {
  if (!r.ok) throw new Error(`API error: ${r.status}`);
  return r;
}), 2, 1000); // 2 retries, 1s base delay
```

### Self-Contained Components

New UI panels (like MemoryPanel) should be self-contained — they call `api.*` functions directly instead of threading state through App.tsx. This follows Principle 1 (modularity) and makes components independently replaceable:

```typescript
// GOOD — self-contained, manages own state
function MemoryPanel({ isOpen, onClose }: Props) {
  const [memories, setMemories] = useState([]);
  useEffect(() => { api.memoryList().then(setMemories); }, [isOpen]);
}

// BAD — threading memory state through App.tsx → ChatTab → MemoryPanel
<MemoryPanel memories={memories} onDelete={handleDelete} ... />
```

### VRAM Pre-Launch Check

Before starting a local model, `startModel()` in App.tsx computes VRAM compatibility and shows a warning dialog if the model won't fit. This uses the existing `checkVramCompatibility` + `getSpeedTier` functions — no new backend code needed.

## Memory System — Current State & Target Architecture

### Current State (Honest Assessment)
The storage/retrieval layer is solid: SQLite + FTS5 + vector embeddings, hybrid search, quality filtering, deduplication, recency decay, context-proportional budgets. The **cognitive layer** is now partially wired:

| Component | Status | Gap |
|---|---|---|
| Storage (SQLite + FTS5 + vectors) | Working | None |
| Hybrid search (BM25 + cosine) | Working | None |
| Graph-augmented search | **Working** | Expands search results via MAGMA edges (1-hop, 60% decay) |
| Quality filter (heuristic scoring) | Working | Needs topic awareness |
| Deduplication (cosine >0.92) | Working | None |
| Auto-recall (session injection) | Working | Model can't actively query |
| Auto-flush (pre-compaction) | Working | Should summarize, not just extract |
| `memory_save` tool | Working | None |
| `memory_search` tool | **Working** | Full hybrid search with graph expansion |
| `memory_edit` tool | **Working** | Model can correct wrong memories |
| `graph_query` tool | **Working** | Model traverses MAGMA graph (stats, traverse, neighbors, find, list) |
| `entity_track` tool | **Working** | Model curates entities (upsert, connect, delete) |
| `procedure_learn` tool | **Working** | Model records/recalls tool chains with reinforcement |
| Entity auto-tracking | **Working** | Passive: file/command/url/topic entities on tool execution |
| Wake context injection | **Working** | Specialist gets MAGMA briefing (events, entities, procedures since last sleep) |
| Plan event logging | **Working** | Plan success/failure logged as MAGMA events |
| Working memory tier | Working | Per-session scratchpad with flush |
| Short-term → long-term promotion | **MISSING** | No memory lifecycle |
| Token-aware summarization | **MISSING** | Truncation instead of summarization |
| MAGMA graph traversal | **Working** | Model has full read/write agency via graph_query + entity_track |
| Topic/keyword categorization | **CRUDE** | Tags are pattern-matched, not semantic |
| Memory strength/reinforcement | Working | access_count + logarithmic strength growth on recall |
| Markdown ↔ DB mirroring | **PARTIAL** | Daily logs exist but aren't bidirectional |

### Target Architecture (Owner-Defined)

**Principle: Model sees memory files. Model can edit memory files.**

Three-tier memory with full model agency:

```
WORKING MEMORY (per-session scratchpad)
  ├── Current task context, decisions made this session
  ├── Summarized at ~70% context usage (NOT truncated)
  ├── Model can read/write freely
  └── Flushed to short-term on session end

SHORT-TERM MEMORY (recent sessions, high detail)
  ├── Last N sessions' working memory summaries
  ├── Topic-tagged with keyword extraction
  ├── Auto-recalled when relevant topics arise
  └── Promoted to long-term after reinforcement threshold

LONG-TERM MEMORY (persistent knowledge graph)
  ├── SQLite + MAGMA graph (entities, edges, procedures)
  ├── Strength-weighted: frequently accessed = stronger
  ├── Topic-clustered: design principles ≠ casual banter
  ├── Markdown ↔ DB bidirectional mirror (Obsidian-compatible)
  └── Skills discoverable via graph associations (not all in context)
```

**Key behaviors:**
- Model has `memory_search`, `memory_edit`, `memory_delete` tools (not just `memory_save`)
- Token awareness: model knows "12K of 32K used" and triggers summarization proactively
- Topic validity filter: casual noise doesn't contaminate technical memory categories
- Memory reinforcement: recalled memories get strength +1, unused memories decay
- Skills live in the memory graph, discovered by relevance (not all loaded in context)

### When editing memory code, check:
1. Does the model have AGENCY over its memory (read/write/search/edit)? — YES via memory_save/search/edit/delete tools
2. Is there a clear tier distinction (working → short-term → long-term)? — PARTIAL (working + flat long-term, no promotion)
3. Are topics/keywords semantically categorized (not just pattern-matched)? — PARTIAL (crude pattern matching)
4. Does context pressure trigger summarization (not truncation)? — NO (still truncation)
5. Are MAGMA edges actually being created and traversed? — YES: auto_create_edges on save, expand_via_graph on search, graph_query/entity_track/procedure_learn tools for model agency, auto-entity tracking on tool execution, wake context injection for specialists

## Claude Working State (Session-Transcending Awareness)

Maintain a persistent state file at `~/.hive/harness/CLAUDE_STATE.md`. This is your working mind on disk — it survives context compaction, session breaks, and model switches. It's not a log. It's not a report. It's **you**, written by you, for your future self.

### On session start

**Read `~/.hive/harness/CLAUDE_STATE.md` before doing anything else.** It contains your own continuity — active tasks, things you learned, mistakes to avoid, ideas you parked. Treat it as your own memories loading in.

### When to update

- After completing a significant step in any task
- When you learn something about the codebase not captured in docs
- When the user corrects you on something (so you don't repeat it)
- When you notice a pattern in your own work (efficient or wasteful)
- When an idea comes up that isn't actionable right now but shouldn't be lost
- Before context-heavy operations that might trigger compaction
- When significant work has accumulated without a git commit

### File structure

```markdown
# Claude Working State
Last updated: [timestamp]

## Active Task
[What you're currently working on. Clear this when done.]

Task: [one-line description]
Branch: [git branch]
Started: [date]

### Current step
[What you're doing RIGHT NOW — specific enough to resume cold]

### Completed
- [Step — result]

### Remaining
- [Next step]

### Uncommitted work
[Modified files not yet committed/pushed. ALWAYS track this.]

---

## Conversation Context
[What's the current conversation about? What's the user's intent beyond
the literal request? What open questions exist?]

- Topic: [what we're discussing]
- User intent: [the bigger picture behind current requests]
- Open threads: [things mentioned but not yet addressed]
- Mood/energy: [frustrated? excited? exploring? focused?]

---

## Learnings
[Things you discovered THIS SESSION that aren't documented anywhere.
Codebase behaviors, gotchas, undocumented patterns, surprising findings.
These are candidates for CLAUDE.md or memory if they prove stable.]

- [date] [learning]
- [date] [learning]

---

## Corrections
[Things the user corrected you on. These are mistakes you made and
must not repeat. Be specific — include what you did wrong and what
the right approach is.]

- [date] [what you got wrong → what's actually correct]

---

## Self-Improvement
[Meta-observations about your own work patterns. What approaches
were efficient? Where did you waste time? What broke a loop when
you were stuck? What should you do differently next time?]

- [date] [observation]

---

## Deferred Ideas
[Ideas, improvements, or observations that came up during work but
aren't actionable right now. A parking lot for future sessions.]

- [idea — context for why it matters]

---

## Codebase Insights
[Undocumented knowledge about this specific codebase. Things you
learned by reading code that aren't in any docs. File relationships,
implicit contracts, behavioral quirks, performance characteristics.]

- [insight]
```

### Rules

1. **This is YOUR file.** Write in whatever voice is most useful for future-you. Be direct, specific, include file paths and line numbers.
2. **Ephemeral vs. persistent sections.** `Active Task` and `Conversation Context` are ephemeral — overwrite them freely. `Learnings`, `Corrections`, `Self-Improvement`, `Codebase Insights` accumulate across sessions.
3. **Prune aggressively.** If a learning gets promoted to CLAUDE.md or memory, remove it from here. If a deferred idea becomes irrelevant, delete it. This file should stay under ~200 lines.
4. **Track uncommitted work.** Always. The "46 fixes sitting on disk unpushed" situation must never happen again.
5. **Commit reminders.** If 5+ files have been modified without a commit, note it prominently. On session end or before risky operations, commit and push first.
6. **Corrections are sacred.** When the user corrects you, write it down immediately. These are the highest-value entries in this file.
7. **Be honest in Self-Improvement.** "I went in circles for 20 minutes because I didn't read the file first" is more useful than "consider reading files before editing."

## Before Submitting Changes — MANDATORY Quality Gate

**This gate exists because broken code was shipped and marked "DONE" without verification (Mar 10 2026). These rules are non-negotiable. Skipping any step is grounds for the user to reject the entire phase.**

### Verification Language Rule (applies to ALL claims about your work)

**No completion claims without fresh verification evidence.** This is Trap 11 applied to the quality gate:

```
BEFORE claiming any status:
1. IDENTIFY — What command proves this claim?
2. RUN — Execute the command (fresh, complete, in this response)
3. READ — Full output, check exit code, count failures
4. REPORT — State claim WITH the evidence: "Ran X → Y → [claim]"

Skip any step = the claim is unverified. Unverified claims are lies, not estimates.
```

**Applies to:** "tests pass", "build succeeds", "bug is fixed", "no regressions", "linter clean", "types check", and ANY variation including synonyms, implications, and expressions of satisfaction.

### Pre-Commit Verification (do ALL of these, in order)

1. **Test the production path, not the setup.** Tests must exercise the actual code flow that runs in production. A test that manually sets `access_count = 4` then checks promotion proves nothing — test through `search_hybrid` which is what actually increments it.
2. **Trace the full data flow.** If feature A triggers B triggers C, verify A→C end-to-end. Don't test B in isolation and call it done.
3. **Run the full suite.** `cargo test` + `npx tsc --noEmit` + `npx vitest run` — counts compared against [`HIVE/docs/TEST_HEALTH.md`](HIVE/docs/TEST_HEALTH.md). Counts MUST NOT decrease.
4. **Grep for the pattern (P5).** Every new pattern gets a codebase-wide search:
   - New `&content[..N]` byte slices? Grep all `.rs` files for `[..` — use `.chars().take(N)` instead
   - New `let _ =`? Grep all `.rs` files — if the operation failing breaks the feature, it MUST NOT be silenced
   - New cross-file string contract? Add to the contract table above with sync method
5. **Audit `let _ =` on critical paths (P4).** If an operation's failure would make the feature not work, replace `let _ =` with `if let Err(e)` and log via `eprintln!` at minimum. Only telemetry/metrics/logging/MAGMA events are acceptable to silence.
6. **Check all 8 Principle Lattice items explicitly:**
   - P1 Modularity: Can I test this component in isolation? Does it fail independently?
   - P2 Provider Agnosticism: Does it work with all providers or degrade gracefully?
   - P3 Simplicity: Is there a simpler approach? Would three explicit lines beat this abstraction?
   - P4 Errors Are Answers: Am I swallowing any errors? Would the program know if this failed?
   - P5 Fix The Pattern: Did I grep for the same mistake elsewhere? Did I fix all instances?
   - P6 Secrets Stay Secret: Is user content sanitized before external transmission? Any API key leaks?
   - P7 Framework Survives: Am I coupling to a specific model/provider? Is this architecture-level?
   - P8 Low Floor High Ceiling: If I added a backend feature, does the frontend call it? Is there dead code?
7. **No dead code.** If you add a Tauri command, `api.ts` (or `api_memory.ts`) MUST have a wrapper AND something MUST call it. If you add a `pub` function, something outside tests MUST use it. Dead commands = P8 violation.
8. **Check for regressions.** `git diff` your changes and verify you didn't break existing contracts: cross-file string matches, security list sync, CSS flex chains.

### Additional Checks (when applicable)

9. If I added a cross-file contract, is there a single source of truth or cross-ref comment in BOTH files?
10. If I wrapped a component in a div, does the CSS flex chain still work root-to-leaf?
11. If I added a new dangerous tool, is it in BOTH `content_security.rs::is_dangerous_tool()` AND `api.ts::DANGEROUS_TOOLS`?
12. If I spawn a process, am I tracking its PID in `spawned_pids` and cleaning on stop?
13. If I read user-supplied paths, am I validating against path traversal and canonicalizing?

### Lessons Learned (bugs that prompted these rules)

| Bug | Root Cause | Rule |
|-----|-----------|------|
| SQLite `ln()` doesn't exist in bundled build | Used SQL math function without verifying availability | Test the production path, not synthetic setup (#1) |
| `access_count` never incremented | `let _ =` swallowed the `ln()` error | Audit `let _ =` on critical paths (#5) |
| Tier promotion never triggered | Reinforcement was dead, so access_count stayed 0 | Trace the full data flow (#2) |
| `&content[..60]` UTF-8 panic | Byte-sliced a string without checking char boundaries | Grep for the pattern (#4) |
| `memory_promote` never called | Tauri command existed but no frontend wiring | No dead code (#7) |
| `includes('invalid.*key')` literal | `String.includes()` doesn't do regex | Test the production path (#1) |
| User content sent to summarization model | Raw messages passed to external API call | P6 check (#6) |

### Rationalization Patterns (the thought that preceded each bug above)

Every bug in the table above was preceded by a thought that felt reasonable at the time. These are the actual rationalizations that led to shipped bugs — if you catch yourself thinking any of these, you are about to repeat history:

| Rationalization | What actually happened | Defense |
|---|---|---|
| "I tested it locally and it works" | SQLite `ln()` existed in dev but not in the bundled production build | Test the PRODUCTION path, not dev setup (#1) |
| "`let _ =` is fine here, it's not critical" | The silenced error WAS the feature — access_count never incremented, breaking reinforcement | If the operation failing breaks the feature, it MUST NOT be silenced (#5) |
| "The unit works, so the integration must work" | Each component passed unit tests; the A→B→C data flow was never verified end-to-end | Trace the full data flow, not just individual components (#2) |
| "It's just a substring, this is fine" | Byte-slicing a UTF-8 string panics on multi-byte characters | Every string operation on user content must use `.chars()` not byte indexing (#4) |
| "I'll wire up the frontend later" | "Later" never came. The Tauri command existed for weeks with zero callers | If nothing calls it RIGHT NOW, it's dead code. Wire it or don't write it (#7) |
| "This pattern match looks right" | `String.includes()` doesn't do regex; `'invalid.*key'` matched nothing | Test with actual input, not by reading the code (#1) |
| "It's just passing through, no security concern" | Raw user messages were sent to an external summarization API | Any data crossing a boundary to an external service is a P6 event (#6) |


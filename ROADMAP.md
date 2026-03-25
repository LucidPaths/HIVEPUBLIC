# HIVE Roadmap

**Last Updated:** March 10, 2026
**Purpose:** Single source of truth for planning and progress tracking

---

## The Vision

**HIVE is a program you slap into any PC and it gives you a capable AI assistant.**

Not a chatbot. Not a model manager. A full assistant — like Jarvis, but it runs on your hardware. It chats, it codes, it browses the web, it uses tools, it remembers everything, it delegates between models when one isn't enough. Local models for free, cloud models when you need the big guns. One interface, any task.

The desktop app (Phase 1) is the nervous system — hardware detection, model management, provider abstraction. The tool/task layer (Phase 2) is the hands — file ops, web crawling, terminal, APIs. The memory substrate (Phase 3) is the hippocampus — persistent awareness across sessions and model swaps (built ON TOP of the tool layer, because memory operations are tool calls). The brain (Phase 4) is the prefrontal cortex — an always-on orchestrator that receives intent, picks the right specialist, delegates, and synthesizes results.

The user never thinks about which model. They talk to HIVE. HIVE figures out the rest.

```
┌─────────────────────────────────────────────────────────────────────┐
│  HIVE — "Double-click, done"                                        │
│                                                                     │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │  The Brain (always-on orchestrator)                            │  │
│  │  Receives user intent → decomposes → delegates → synthesizes  │  │
│  └──────────┬──────────────────┬──────────────────┬──────────────┘  │
│             │                  │                  │                  │
│       ┌─────▼─────┐    ┌──────▼──────┐    ┌──────▼──────┐         │
│       │  Local     │    │   Local     │    │   Cloud     │         │
│       │  Coder     │    │   Reasoner  │    │   Claude /  │         │
│       │  (GGUF)    │    │   (GGUF)    │    │   GPT / etc │         │
│       └───────────┘    └────────────┘    └────────────┘         │
│                                                                     │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │  Memory Substrate (shared across all models)         DONE      │  │
│  │  SQLite + embeddings · hybrid search · cross-session recall    │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                                                                     │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │  Task Layer (the hands)                              DONE      │  │
│  │  File ops · terminal · web crawling · API calls · tools        │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                                                                     │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │  Desktop Shell (Tauri v2 — the body)                 DONE      │  │
│  │  Hardware detection · model management · provider abstraction  │  │
│  │  Secure storage · VRAM calculator · recommendation engine      │  │
│  └───────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
```

**What makes this different from OpenClaw, LM Studio, GPT4All, etc.:**

| Feature | OpenClaw | LM Studio | GPT4All | Claude Code | **HIVE** |
|---------|----------|-----------|---------|------------|----------|
| Local models | Via Ollama | Yes | Yes | No | **Yes (llama.cpp native)** |
| Cloud models | Yes (10+ providers) | No | No | Anthropic only | **Yes (OpenAI, Anthropic, Ollama, OpenRouter, DashScope)** |
| Brain / delegation | Per-agent routing | No | No | No | **Smart router + specialist orchestrator (working)** |
| Memory persistence | SQLite + vectors | No | No | None | **SQLite + FTS5 + vectors + MAGMA graph (working)** |
| Procedure learning | No | No | No | No | **Auto-extract tool chains, reinforcement (working)** |
| Tool execution | Yes (skills) | No | No | Yes (filesystem) | **Yes (45 tools: file, terminal, web, system, memory, agents)** |
| Skills system | 53 skills | No | No | No | **Working (drop .md in ~/.hive/skills/, keyword matching)** |
| Web crawling | Yes | No | No | WebFetch | **Yes (Jina Reader + DuckDuckGo)** |
| Desktop GUI | No (headless daemon) | Yes | Yes | No (CLI) | **Yes (Tauri native)** |
| GitHub integration | Via gh skill | No | No | Built-in | **Working (issues, PRs, repos tools)** |
| Telegram/messaging | 14+ channels | No | No | No | **Working (Telegram + Discord daemons)** |
| Scheduled tasks | Cron system | No | No | No | **Working (cron + event triggers + routines engine)** |
| Model hot-swap | No (restart required) | Manual | Manual | N/A | **Automated with context preservation (planned)** |
| Hardware-aware | No | Partial | Partial | No | **Full (GPU/RAM/WSL auto-detect + VRAM budget enforcement)** |
| Identity persistence | Per-channel | None | None | Per-session | **Cross-session, cross-model (MAGMA + harness)** |
| MCP protocol | No | No | No | Built-in | **Bidirectional (server + client, 45 tools exposed)** |
| Agent bridge | No | No | No | N/A | **Working (PTY terminals, send_to_agent, list_agents)** |
| One-click setup | `npm install -g` | Installer | Installer | `npm install -g` | **`START_HIVE.bat` (planned installer)** |

**The bet:** OpenClaw proved 176k people want a personal AI assistant. But most people don't want to run a Node.js daemon from a terminal. They want to double-click something and have it work. That's HIVE.

---

## Current State Assessment

### What Exists (Working)

**Desktop Shell (Phase 1 — COMPLETE)**
- Tauri v2 + React/TypeScript, 9 modular UI components (+ WorkerPanel, MemoryPanel slide-outs)
- Hardware detection: GPU, VRAM, CPU, RAM, WSL2 status (NVIDIA + AMD ROCm)
- Model management: list local + WSL models, download from HuggingFace, GGUF metadata parsing
- VRAM calculator: accurate estimation from GGUF headers, color-coded compatibility badges
- Chat with streaming: local llama.cpp (Windows native + WSL/ROCm) + cloud providers
- Provider support: Local (llama.cpp), OpenAI, Anthropic, Ollama, OpenRouter, DashScope — all with streaming
- Secure API key storage: AES-256-GCM encrypted file
- Per-model settings: context length, GPU layers, KV cache offload, system prompts (persistent)
- Conversation persistence: optional save/restore with sidebar UI
- Model recommendations: 3-tier GPU-utilization engine + Open LLM Leaderboard benchmarks
- Model description tags: pipeline type + domain tags from HuggingFace metadata
- Rust backend modularized: 28 core + 16 tool modules (44 files, 22K lines, 148 Tauri commands)

**Tool Framework (Phase 2 — COMPLETE, 45 tools)**
- MCP-compatible tool schema with registration system
- Permission model: risk-based (low/medium/high/critical), user approves high-risk actions
- Core tools: `read_file`, `write_file`, `list_directory`, `run_command`, `system_info`, `web_fetch`, `web_search`, `web_extract`, `read_pdf`
- Integration tools: `telegram_send`, `telegram_get_updates`, `telegram_bot_info`, `discord_send`, `discord_read`, `github_issues`, `github_prs`, `github_repos`, `integration_status`
- Memory tools: `memory_save`, `memory_search`, `memory_edit`, `memory_delete`, `task_track`
- Autonomous research tools (Phase 8): `repo_clone`, `file_tree`, `code_search`, `scratchpad_create`, `scratchpad_write`, `scratchpad_read`, `worker_spawn`, `worker_status`, `worker_terminate`, `check_logs`
- Cognitive Bus: `route_to_specialist` (with HIVE identity injection), `read_agent_context` (cross-agent visibility)
- Orchestration: `plan_execute` (multi-step tool chaining)
- Tool call parsing: OpenAI format, Anthropic format, Hermes-native for local models, truncated tag recovery
- Agentic loop: up to 10 iterations, tool call → approve → execute → feed results → loop
- Tool calls + results visible in chat UI (collapsible blocks)
- Tool feedback markers: TOOL_OK/TOOL_ERROR/TOOL_DENIED prefixes for model awareness
- Local model compatibility: Hermes-native tool harness (Nanbeige/Qwen compatible), compact schemas for ≤16K context
- Thinking token separation: reasoning tokens stripped from content across all providers (DeepSeek R1, Anthropic, Kimi K2.5, OpenAI)

**Memory System (Phase 3 — COMPLETE)**
- SQLite + FTS5 full-text search via `rusqlite`
- Vector embeddings via OpenAI `text-embedding-3-small` (graceful degradation to FTS5-only if no key)
- Hybrid search: vector similarity + BM25 scoring
- Two-tier memory: daily markdown logs + chunked indexed database
- Session injection: discrete system message in conversation array (never system prompt mutation)
- Memory recall: formatted context retrieved and injected per-conversation
- Auto-flush: conversations extracted to memory before context compaction
- Adapted from OpenClaw architecture (MIT), built in Rust

### What's Next

1. **Model hot-swap** — swap specialists with context preservation via memory flush
2. **Browser automation (Phase 4.5.3)** — CDP browser control, AI-readable DOM snapshots
3. **UI polish** — responsive layout, visual VRAM budget bar, token/speed display
4. **Extended integrations (Phase 5)** — email, calendar, markets, custom APIs

### Recently Completed (Mar 10, latest)
- **Cognitive Bus (Phase 11) — DONE:**
  - Unified identity: all specialists (local + cloud) and workers receive HIVE identity via `read_identity()` / cached harness
  - `read_agent_context` tool: cross-agent visibility (MAGMA events, scratchpads, working memory, worker status)
  - Context bus: shared observable state via scratchpad convention (8h TTL, FIFO per agent, bus summary in volatile context)
  - Cross-model spawning: workers get HIVE identity, write to bus on completion, `slot_role` parameter resolves slot config
- **Testing & Hardening — DONE:**
  - 214 Rust tests + 96 vitest tests (up from 92 + 52 at baseline)
  - CI pipeline (GitHub Actions: cargo test + tsc --noEmit + vitest run)
  - useChat.ts decomposition (1917→1622 lines, chainPolicies.ts extracted)
  - SQLite WAL consistency (all 8 sites standardized)
  - Memory tier system (tier column, scoring, atomic insert, standalone promotion)
  - Context summarization (model-based + fallback, P6 sanitization)
- **Deep Audit — critical bugs found and fixed:**
  - SQLite `ln()` doesn't exist in bundled build — entire reinforcement pipeline was dead
  - UTF-8 byte-slice panics on multi-byte chars
  - Truncate-before-dedup ordering lost unique search results
  - Orphaned MAGMA edges on memory delete
  - Dead Tauri commands (`memory_promote`) wired to frontend
- **Security audit (`fix/audit-findings` branch):**
  - Path traversal, dead updater removal, tunnel hardening, PID-tracked process kill
  - Memory import refactor, procedure upsert, frontend fixes

### Previously Completed (Feb 28)
- **Phase 4 Brain — fully operational:**
  - Wake briefings, VRAM budget enforcement, auto-sleep timer
  - Cloud slot routing, routing indicator, procedure learning
- **Skills system (Phase 4.5.5) — DONE:**
  - `~/.hive/skills/` with 4 seed skills, keyword matching, per-turn injection
- **Document ingestion (Phase 9.3) — DONE:**
  - Batch import with native file dialog (multi-select PDF, markdown, code files)
  - Source file tracking: each imported memory knows its origin file
  - Progress bar UI in MemoryTab during bulk import
  - `memory_import_file` tool available to the model for on-demand ingestion
- **Procedure-aware recall (Phase 9.2) — DONE:**
  - `recall_matching_procedures()` looks up MAGMA procedures relevant to current task
  - Procedure reinforcement: success/fail counters updated on re-use
  - Auto-entity tracking: files, commands, URLs, topics extracted on tool execution
- **Quality audit**: Eliminated 6 lazy shortcuts in Brain/Skills code — proper Tauri commands instead of hacks, state-driven UI instead of string matching, persistent logging for AI observability, correct feature gating
- Test suite: 92 Rust + 52 vitest (was 44) + 0 tsc errors

### Previously Completed (Feb 28, earlier)
- **Persistent logging across all backend modules (P4)** — 11 Rust modules now log lifecycle events to `hive-app.log` via `append_to_app_log()`. Frontend bridge (`useLogs.ts`) auto-persists `[HIVE]` logs. The steering AI can read its own operational state via `check_logs` tool — server start/stop, provider errors, memory operations, daemon lifecycle, slot changes, download progress, MCP connections, PTY sessions, VRAM eviction, auto-sleep events.
- **Phase 10: NEXUS — Universal Agent Interface** — HIVE becomes the single access point for all CLI coding agents (Claude Code, Codex, Aider, Shell):
  - PTY backend (`portable-pty` + `uuid`) with 5 Tauri commands: spawn/write/resize/kill/list
  - xterm.js v6 terminal panes with HIVE zinc/amber theme, resize observer, web links
  - Pane type system: chat and terminal panes coexist in resizable multi-pane layout
  - Agent registry with settings UI, custom agents, availability detection (`which`/`where`)
  - PTY output memory logging: ANSI stripping, line accumulation, `pty-log` events
  - Cross-agent tools: `send_to_agent` + `list_agents` HiveTools (model→agent bridge)
  - MCP auto-bridge: one-click inject HIVE MCP server into `~/.claude.json`
  - Remote channel routing: Discord/Telegram messages → terminal agent with fallback
  - See `HIVE/docs/PHASE10_NEXUS.md` for full architecture and data flow diagrams
- **Phase 10 hardening** (post-audit):
  - Session cleanup: exited PTY sessions free OS resources but keep metadata for scrollback
  - Command matching: normalized path/extension handling for Windows and Unix agent routing
  - Pre-existing bug fixes: `strip_thinking` regex order (XML before slash to prevent cross-matching), `sanitize_api_error` test alignment, harness test alignment, `useChat.ts` type error
  - See `HIVE/docs/TEST_HEALTH.md` for test suite baseline tracking

### Recently Completed (Feb 25)
- **Comprehensive audit** — Full codebase audit (29K lines): architecture, capabilities, competitor comparison, code analysis, principle lattice compliance. Identified 6 improvement areas (see Phase 9 below)
- **useChat refactoring** — Extracted 5 chain policies from the agentic tool loop into pure functions: `detectRepetition()` (with ping-pong A-B-A-B detection), `classifyToolCalls()`, `isChainComplete()`, `executePlanSteps()`, module-level `TERMINAL_TOOLS` constant
- **Documentation overhaul** — Updated STATE_OF_HIVE.md (metrics, self-assessment), README_HIVE_V2.md (complete rewrite), ROADMAP.md (audit findings, new phases)

### Recently Completed (Feb 24)
- **Phase 8: Autonomous Research System** — 10 new tools (1,700+ lines Rust, 170 lines TSX):
  - **Workspace tools** (`workspace_tools.rs`): `repo_clone` (shallow git clone to isolated workspaces), `file_tree` (recursive directory listing with depth/pattern filtering), `code_search` (regex search across files with context lines)
  - **Scratchpad tools** (`scratchpad_tools.rs`): `scratchpad_create` (named key-value stores with TTL), `scratchpad_write` (append to sections), `scratchpad_read` (full/summary format)
  - **Worker tools** (`worker_tools.rs`): `worker_spawn` (autonomous background tokio tasks with own tool registry), `worker_status` (progress, turns, elapsed), `worker_terminate` (graceful shutdown). Workers sandboxed — no shell/write/messaging access
  - **Log tools** (`log_tools.rs`): `check_logs` (model self-debugging via app/server log reading with filtering)
  - **Worker status panel** (`WorkerPanel.tsx`): slide-out UI showing active workers with progress bars, stall warnings, turn counts. Polls every 3s. Wired into ChatTab status bar
  - **Persistent audit log**: `audit_log_tool_call` now writes to `hive-app.log` file (not just stderr), enabling model self-debugging via `check_logs` tool
  - **Tauri command**: `get_worker_statuses` for frontend polling
- **Phase 7: Plan execution** — `plan_execute` tool for structured multi-step tool chaining with variable substitution and conditional steps
- **Phase 6: Routines engine** — standing instructions with cron scheduler, event matching, message queue

### Previously Completed (Feb 21-23)
- **DashScope (Alibaba)** as 6th coequal provider — Kimi K2.5, Qwen models, OpenAI-compatible dispatch
- **Thinking token separation** — reasoning tokens stripped from content across all providers (DeepSeek R1, Anthropic, Kimi K2.5, OpenAI). Collapsible UI for power users
- **Provider chat dedup** — 3 identical OpenAI-compatible chat/stream functions unified into `chat_openai_compatible()` / `stream_openai_compatible()` (-139 net lines)
- **integration_status tool** — models discover available integrations at session start (Telegram, Discord, GitHub, embeddings)
- **Memory quality filter** — heuristic scoring (0.0-1.0) replaces crude char-length threshold. Filters greetings, code artifacts, generic preambles. Q+A pair detection saves user question + assistant answer as units
- **Harness stable/volatile split** — stable prompt (identity, tools, memory) cached by llama.cpp KV prefix; volatile state (turn count, VRAM, GPU) injected as separate message
- **Tool feedback markers** — TOOL_OK/TOOL_ERROR/TOOL_EXCEPTION/TOOL_DENIED prefixes so models distinguish intent vs execution
- **Compact tool schemas** — 7K→300 tokens for local models ≤16K context
- **Performance** — parallel pre-chat work (350ms→200ms), batched streaming (200→60 renders/sec), parallel Discord polls, SSE buffer fix for split-across-TCP-chunk JSON
- **Memory flush on window close** — beforeunload handler prevents memory loss when closing browser tab
- **Context window fix** — cloud models now use provider-reported context length (was hardcoded 4K)
- **UTF-8 safety** — 7 locations using byte slicing fixed to char iteration
- **Anthropic memory fix** — system messages beyond the first were being dropped
- **CSP enabled** — Content Security Policy activated in tauri.conf.json
- **Telegram plain text default** — prevents HTTP 400 from formatted messages, HTML opt-in with auto-retry fallback
- **Dead code removal** — deprecated `generateCompletion()` removed (-48 lines)

### Previously Completed (Feb 17-20)
- **OpenRouter** as 5th coequal provider (full streaming, status checks, model listing)
- **Smart model router** — benchmark-driven auto-routing replaces fixed specialist slots
- **Tool approval system** — 3 modes (ask/session/auto), per-tool risk overrides, disable individual tools
- **Discord integration** — daemon (REST polling), discord_send/discord_read tools, encrypted bot token
- **Telegram daemon** — background polling, allowlist management, parse_mode enum fix
- **Memory tab** — full-page MAGMA viewer + memory browser (search, add, edit, delete)
- **Content security rework** — neutral boundary markers, separate wrappers for external content vs authenticated user messages

---

## The Architecture Evolution

### What We Learned from OpenClaw

OpenClaw (176k stars, MIT) is the closest existing project to HIVE's vision. We studied their architecture to learn what works in production:

| OpenClaw Pattern | Proven? | HIVE Adaptation |
|-----------------|---------|-----------------|
| SQLite + vector embeddings for memory | Yes | **Adopted**: `memory.rs` with `rusqlite` + vector similarity |
| Two-tier memory (daily logs + long-term) | Yes | **Adopted**: markdown daily logs + chunked SQLite |
| Hybrid search (0.7 vector + 0.3 BM25) | Yes | **Adopted**: same ratio, proven effective |
| Memory flush before context compaction | Yes | **Adopted**: auto-extract before truncation |
| Per-agent model assignment | Yes | Adapt: per-specialist model pools (Phase 4) |
| Auth rotation + provider fallback chains | Yes | Partial: providers work, fallback chains planned |
| Node.js daemon + WebSocket gateway | Yes, but... | **Reject**: Wrong form factor for desktop app |
| Messaging platform adapters | Yes, but... | **Defer**: Phase 5 (Telegram/WhatsApp bridges) |

**Key decision: Study the patterns, build in Rust.** OpenClaw's architecture is proven, but their runtime (Node.js) and use case (messaging bot) don't fit. We take the ideas, not the code.

### What Changed from Original MAGMA Plan

The original roadmap referenced forking `FredJiang0324/MAMGA` — an academic project with 4 graph types (Semantic, Temporal, Causal, Entity). After studying OpenClaw's production system, we simplified:

| Original MAGMA Plan | What We Built |
|---------------------|--------------|
| Fork academic Python repo | Built in Rust (native to HIVE) |
| 4 separate graph databases | Single SQLite with FTS5 + vectors |
| Graph traversal algorithms | Hybrid search (vector + BM25) |
| Complex entity extraction | Two-tier memory (session + long-term) |
| NumPy embeddings | OpenAI `text-embedding-3-small` (or FTS5-only fallback) |

The 4-graph concept is still valid as a mental model, but the implementation is a single SQLite database with different query strategies — not four separate databases. Simpler. Faster. Same result.

---

## Unified Implementation Phases

### Phase 1: Foundation Layer — COMPLETE

```
┌─────────────────────────────────────────────────────────┐
│  PHASE 1: FOUNDATION                            DONE    │
│                                                          │
│  1.1 Context Management                                  │
│      ├── [x] Character-based truncation                  │
│      ├── [x] Memory-backed context recovery              │
│      └── [ ] Token-aware counting (chars/4 heuristic)    │
│                                                          │
│  1.2 Conversation Persistence                            │
│      ├── [x] Save/load chat history (localStorage)       │
│      ├── [x] Multiple conversation threads + sidebar UI  │
│      └── [ ] Export/import conversations                 │
│                                                          │
│  1.3 Provider Robustness                                 │
│      ├── [x] Error handling (actionable messages)        │
│      ├── [x] Pre-send health checks                     │
│      ├── [x] Abort recovery (stop button works)          │
│      └── [ ] Retry logic with backoff                    │
│                                                          │
│  1.4 Developer Infrastructure                            │
│      ├── [x] Split App.tsx into 8 components             │
│      ├── [x] Model recommendation engine (3-tier)        │
│      ├── [x] Model description tags (pipeline + domain)  │
│      ├── [ ] Capture llama-server stdout/stderr          │
│      ├── [ ] Token/speed display in chat UI              │
│      └── [ ] VRAM pre-launch check                       │
└─────────────────────────────────────────────────────────┘
```

---

### Phase 1.5: Structural Foundation — COMPLETE

```
┌─────────────────────────────────────────────────────────┐
│  PHASE 1.5: TIGHTEN THE FOUNDATION              DONE    │
│                                                          │
│  1.5.1 Split main.rs into Rust modules                  │
│      ├── [x] hardware.rs — GPU/CPU/RAM/WSL detection    │
│      ├── [x] models.rs — Local model listing, GGUF      │
│      ├── [x] server.rs — llama-server lifecycle         │
│      ├── [x] providers.rs — Chat providers (local/cloud)│
│      ├── [x] download.rs — HuggingFace downloads        │
│      ├── [x] security.rs — AES-256-GCM key storage     │
│      ├── [x] settings.rs — App config persistence       │
│      ├── [x] wsl.rs — WSL bridge helpers                │
│      ├── [x] gguf.rs — GGUF metadata parsing            │
│      ├── [x] vram.rs — VRAM estimation                  │
│      ├── [x] memory.rs — Memory system                  │
│      ├── [x] tools/ — Tool framework (mod, file, sys,   │
│      │       web tools)                                  │
│      └── [x] main.rs — 87 lines, setup + registration   │
│                                                          │
│  1.5.2 Capture server output                            │
│      └── [ ] Pipe llama-server stdout/stderr to Logs    │
│                                                          │
│  1.5.3 Token/speed display                              │
│      ├── [ ] Parse timing data from SSE stream          │
│      └── [ ] Show tokens/sec in chat UI                 │
└─────────────────────────────────────────────────────────┘
```

---

### Phase 2: Tool & Task Framework — COMPLETE

```
┌─────────────────────────────────────────────────────────┐
│  PHASE 2: TOOL & TASK FRAMEWORK                  DONE   │
│                                                          │
│  2.1 Tool Interface                                      │
│      ├── [x] Standard tool schema (MCP-compatible)       │
│      ├── [x] Tool registration system                    │
│      ├── [x] Permission model (risk-based approval)      │
│      ├── [x] Result formatting for model consumption    │
│      └── [x] tools/ module in Rust backend              │
│                                                          │
│  2.2 Core Tools                                          │
│      ├── [x] read_file (paginated) / write_file / list  │
│      ├── [x] run_command (sandboxed, Windows + WSL)     │
│      ├── [x] web_fetch (Jina Reader for clean text)     │
│      ├── [x] web_search (DuckDuckGo)                    │
│      ├── [x] system_info (hardware, disk, processes)    │
│      └── [ ] memory_query (search past as tool call)    │
│                                                          │
│  2.3 Tool Execution Engine                               │
│      ├── [x] Parse tool calls from model output          │
│      │   ├── [x] OpenAI format                          │
│      │   ├── [x] Anthropic format                       │
│      │   └── [x] Hermes-native (local models)           │
│      ├── [x] Execute with timeout + resource limits     │
│      ├── [x] Feed results back to model (agentic loop)  │
│      ├── [x] UI: collapsible tool call + result blocks  │
│      └── [x] Tool approval modal for high-risk actions  │
│                                                          │
│  2.4 Web Crawling                                        │
│      ├── [x] Fetch URL → extract text (Jina Reader)     │
│      ├── [x] Search queries (DuckDuckGo)                │
│      ├── [ ] Follow links for research tasks            │
│      └── [ ] Parse structured data (tables, APIs)       │
└─────────────────────────────────────────────────────────┘
```

---

### Phase 3: Memory Substrate — COMPLETE

```
┌─────────────────────────────────────────────────────────┐
│  PHASE 3: MEMORY SUBSTRATE                       DONE   │
│  (OpenClaw-inspired, built in Rust)                      │
│                                                          │
│  3.1 Storage Layer                                       │
│      ├── [x] SQLite via rusqlite                         │
│      ├── [x] Vector similarity for semantic search       │
│      ├── [x] BM25 full-text search (FTS5)               │
│      └── [x] Hybrid search (vector + BM25)              │
│                                                          │
│  3.2 Session Memory                                      │
│      ├── [x] Markdown daily logs (source of truth)      │
│      ├── [x] Chunk into ~400-token segments for indexing │
│      ├── [x] Embed chunks via OpenAI text-embedding-3   │
│      │       (graceful degradation to FTS5-only)         │
│      └── [x] Index session content for cross-session    │
│                                                          │
│  3.3 Long-Term Memory                                    │
│      ├── [x] Auto-save conversations to memory DB       │
│      ├── [x] Session-injected recall (discrete message) │
│      ├── [x] Memory flush before context compaction     │
│      └── [x] Cross-model memory persistence             │
│                                                          │
│  3.4 Context Management                                  │
│      ├── [x] Memory flush before compaction              │
│      ├── [x] Smart retrieval: query memory for relevant │
│      └── [x] Context injection via separate system msg  │
│                                                          │
│  3.5 Integration                                         │
│      ├── [x] Chat tab gets memory-augmented context      │
│      ├── [x] All providers benefit (local + cloud)      │
│      ├── [x] Memory persists across model swaps          │
│      └── [x] memory.rs module in Rust backend           │
└─────────────────────────────────────────────────────────┘
```

---

### Phase 3.5: Memory Rework — Cognitive Memory Architecture — HIGH PRIORITY
**Goal**: The model has full agency over its own memory. Three-tier cognitive architecture replaces flat storage.

**Why this matters now:** The current memory system is a solid filing cabinet, but the model can't open the drawers. It can save memories but can't search, edit, or delete them. There's no working memory, no session continuity, and the MAGMA graph is dead schema. Without fixing this, Phase 4 (The Brain) will be a brain with amnesia.

**Design principle (owner-defined):** Model sees memory files. Model can edit memory files. Simple.

**Inspiration:** neo4j + markdown mirroring pattern (bidirectional sync between human-readable files and graph DB). Skills discovered via memory graph associations, not all loaded in context.

```
┌─────────────────────────────────────────────────────────────┐
│  PHASE 3.5: COGNITIVE MEMORY                      PLANNED    │
│                                                              │
│  3.5.1 Model Memory Agency (CRITICAL — unblocks everything) │
│      ├── [x] memory_search tool — model can query its own   │
│      │       memory actively (not just auto-injected)        │
│      ├── [x] memory_edit tool — model can correct/update    │
│      │       existing memories (true update, re-chunks)      │
│      ├── [x] memory_delete tool — model can prune outdated  │
│      │       memories                                        │
│      └── [x] Memory tool discovery in harness manifest      │
│                                                              │
│  3.5.2 Three-Tier Memory Architecture                        │
│      ├── [x] WORKING MEMORY — per-session scratchpad        │
│      │   ├── Model reads/writes freely during conversation  │
│      │   ├── Summarized at ~70% context usage (not dropped) │
│      │   ├── Preserved across tool loops within session     │
│      │   └── Flushed to short-term on session end           │
│      ├── [~] SHORT-TERM MEMORY — recent session summaries   │
│      │   ├── Working memory flushed as tagged records       │
│      │   ├── Topic-tagged with keyword extraction           │
│      │   ├── Auto-recalled when relevant topics arise       │
│      │   └── Promoted to long-term after reinforcement      │
│      └── [~] LONG-TERM MEMORY — persistent knowledge graph  │
│          ├── Existing SQLite + vectors + MAGMA tables       │
│          ├── [x] Strength-weighted (access count → stronger)│
│          ├── [x] Topic keywords extracted on save           │
│          └── [ ] Markdown ↔ DB bidirectional mirror         │
│                                                              │
│  3.5.3 Token-Aware Context Management                        │
│      ├── [x] Token usage tracking per turn (model knows     │
│      │       "12K of 32K used")                              │
│      ├── [x] At ~70% context: summarize to working memory   │
│      │       instead of truncating                           │
│      ├── [x] Key insights from summary strengthen related   │
│      │       long-term memories (flush → reinforce feedback) │
│      └── [x] Harness volatile context reports token pressure │
│                                                              │
│  3.5.4 Topic Validity & Categorization                       │
│      ├── [x] Keyword extraction for memory categorization   │
│      │       (TF-based, stopword-filtered)                   │
│      ├── [x] Topic clusters: auto-classify memories into     │
│      │       topic:technical/project/personal/general        │
│      ├── [x] Memory reinforcement: recalled memories get    │
│      │       strength +1, logarithmic growth                 │
│      └── [x] Deduplication across tiers (working → short →  │
│              long: flush checks is_near_duplicate before save)│
│                                                              │
│  3.5.5 MAGMA Graph Activation                                │
│      ├── [x] Auto-create edges on memory save (keyword      │
│      │       overlap → related_to edges)                     │
│      ├── [x] Graph traversal for related memory discovery   │
│      │       (existing magma_traverse already works)         │
│      ├── [x] Skills as graph nodes — tools registered as     │
│      │       MAGMA entities with keyword edges               │
│      ├── [x] Markdown ↔ Graph bidirectional sync            │
│      │       (reimport_markdown command + daily log writes)  │
│      └── [ ] Visualization in MemoryTab (already partial)   │
│                                                              │
│  3.5.6 Session Continuity                                    │
│      ├── [x] Session handoff notes (AI writes "next steps"  │
│      │       at session end, next session picks up)          │
│      ├── [x] Working memory persists as session artifact    │
│      └── [x] Cross-session task tracking (task_track tool   │
│              + MAGMA entities + task_upsert/list commands)   │
└─────────────────────────────────────────────────────────────┘
```

**Key insight from the "skills in memory graph" pattern:**
When you have 100+ skills, loading them all into context makes the model stupid (too much ambiguity). Instead, skills live as nodes in the memory graph, connected to relevant topics/entities. When a conversation touches "postgres backend," the PostgreSQL skill surfaces automatically via graph association. The model discovers the right skill at the right time, not all skills all the time.

**Success metrics:**
- [x] Model can search, edit, and delete its own memories via tool calls
- [x] Conversations summarize at ~70% context instead of truncating
- [x] Memory has working memory tier (short/long term distinction partial)
- [x] Keyword extraction provides topic categorization on save
- [x] MAGMA edges are auto-created on save (keyword overlap detection)
- [x] Session handoff notes enable continuity across conversations
- [x] Memory reinforcement — access_count + strength weighting in search
- [x] Topic clusters prevent cross-contamination (auto-classify + recall boosting)
- [x] Memory graph is editable (reimport_markdown syncs markdown → DB)
- [x] Skills as graph nodes (tools as MAGMA entities + keyword edges)

---

### Phase 4: The Brain — Multi-Model Orchestration — FUNCTIONAL
**Goal**: HIVE becomes an intelligent system, not a chatbot

```
┌─────────────────────────────────────────────────────────┐
│  PHASE 4: THE BRAIN                        FUNCTIONAL    │
│                                                          │
│  4.1 Model Pool                                   DONE   │
│      ├── [x] Registry of available models (local+cloud) │
│      ├── [x] Capability tags per model (benchmark       │
│      │       strengths: code, reasoning, writing, etc.) │
│      ├── [x] VRAM budget manager (track, enforce, evict)│
│      └── [x] Smart router in api.ts + slots.rs          │
│                                                          │
│  4.2 Side-Chat System (Worker Sub-Agents)         DONE   │
│      ├── [x] worker_spawn: background tokio tasks with  │
│      │   own tool registry + any provider (P2)           │
│      ├── [x] Parallel execution (multiple workers)      │
│      ├── [x] Scratchpad result aggregation              │
│      ├── [x] worker_status + worker_terminate           │
│      ├── [x] WorkerPanel.tsx (slide-out UI in ChatTab)  │
│      └── [x] Workers sandboxed (no shell/write/msg)     │
│                                                          │
│  4.3 Orchestrator (The Brain)                   PARTIAL  │
│      ├── [x] Consciousness model routes via tool call   │
│      ├── [x] route_to_specialist tool (local + cloud)   │
│      ├── [x] MAGMA wake briefing injection              │
│      ├── [x] VRAM enforcement + idle eviction           │
│      ├── [x] Auto-sleep (5min idle timeout)             │
│      ├── [x] Procedure learning (auto-extract chains)   │
│      └── [x] orchestrator.rs + useChat.ts wiring        │
│                                                          │
│  4.4 Model Hot-Swap with Context Preservation            │
│      ├── Stop current model (graceful shutdown)         │
│      ├── Flush conversation state to memory substrate   │
│      ├── Start new model (different specialist)         │
│      ├── Inject relevant context from memory            │
│      ├── Continue conversation seamlessly               │
│      └── User experiences ONE continuous intelligence   │
│                                                          │
│  4.5 Delegation Patterns                                 │
│      ├── Single-model: brain IS the model (simple case)│
│      ├── MoE-style: brain routes to local specialists  │
│      ├── Hybrid: local for speed, cloud for power      │
│      ├── Chain: model A produces → model B refines     │
│      └── Parallel: multiple models, best answer wins   │
└─────────────────────────────────────────────────────────┘
```

**This is the Jarvis moment.** The user says "build me a Flask API with auth." The brain decomposes: research best practices (web crawl) → generate code (coder model) → test it (terminal) → report results. The user didn't pick models. They just asked for something.

**Cloud + Local synergy**: The brain can decide "this task needs Claude-level reasoning, the local 7B won't cut it" and route to cloud. Or "this is a simple format task, save the API credits, use the local 3B." The user sets their preferences (local-first, cost ceiling, etc.) and HIVE respects them.

**Deliverable**: Multi-model delegation. Seamless specialist swapping. One continuous experience regardless of which model(s) are doing the work.

---

### Phase 4.5: Security Hardening + Integration Layer — HIGH PRIORITY
**Goal**: HIVE can connect to the world safely

This phase runs in parallel with Phase 4. Every integration follows the **"doors and keys"** pattern:
HIVE ships the door (the integration code). The user provides the key (API token, credentials).
No key = door stays closed. With key = tool registers in capability manifest.

See [THE_VISION.md](HIVE/docs/vision/THE_VISION.md) for the full integration architecture.

```
┌─────────────────────────────────────────────────────────────┐
│  PHASE 4.5: SECURITY + INTEGRATIONS              PLANNED    │
│                                                              │
│  4.5.0 Security Hardening (CRITICAL — before any new tools) │
│      ├── [x] External content wrapping (prompt injection     │
│      │       defense for ALL tool results with external data)│
│      ├── [x] SSRF protection for web_tools.rs               │
│      │       (block private IPs, limit redirects)            │
│      ├── [x] Dangerous tools registry (centralized risk DB) │
│      ├── [x] Unicode homoglyph folding in content wrapping  │
│      ├── [ ] Audit logging for all tool executions          │
│      └── [ ] Suspicious pattern detection (monitoring)      │
│      Adapted from: OpenClaw src/security/ (MIT)             │
│                                                              │
│  4.5.1 GitHub Integration                            DONE    │
│      ├── [x] User provides: Personal Access Token           │
│      ├── [x] Tool: github_issues (list, search, filter)     │
│      ├── [x] Tool: github_prs (list, filter)                │
│      ├── [x] Tool: github_repos (browse, search)            │
│      ├── [x] Provider-agnostic: ANY model can use these     │
│      │       via existing agentic loop + tool framework      │
│      └── [ ] Skill: GITHUB.md prompt docs for models that   │
│              don't natively do tool calls (Hermes harness)   │
│                                                              │
│  4.5.2 Telegram Bot Integration                      DONE    │
│      ├── [x] User provides: Bot API Token (from @BotFather) │
│      ├── [x] Rust module: telegram_daemon.rs (Tokio task)   │
│      ├── [x] Receive messages → feed to consciousness       │
│      ├── [x] Send responses back via bot                    │
│      ├── [x] Command HIVE remotely from phone               │
│      ├── [ ] Push notifications (task complete, alerts)     │
│      ├── [x] Allowlist management in Settings               │
│      └── [x] Tools: telegram_send, telegram_get_updates,    │
│              telegram_bot_info                               │
│                                                              │
│  4.5.2b Discord Bot Integration                      DONE    │
│      ├── [x] User provides: Bot Token + Channel ID         │
│      ├── [x] Rust module: discord_daemon.rs (Tokio task)    │
│      ├── [x] Receive messages → feed to consciousness       │
│      ├── [x] Send responses back via bot                    │
│      ├── [x] Auto-discover channels on startup              │
│      ├── [x] Allowlist management in Settings               │
│      └── [x] Tools: discord_send, discord_read              │
│                                                              │
│  4.5.3 Browser Automation                                   │
│      ├── [ ] CDP-based browser control                      │
│      ├── [ ] AI-readable DOM snapshots (not raw HTML)       │
│      ├── [ ] Navigate, screenshot, interact, fill forms     │
│      ├── [ ] Headless mode for background research          │
│      └── [ ] Response caching for repeated fetches          │
│      Study: OpenClaw src/browser/ (Playwright patterns)     │
│                                                              │
│  4.5.4 Cron / Scheduled Tasks                        DONE    │
│      ├── [x] Schedule types: cron (5-field expressions),    │
│      │       event triggers (keyword/regex/channel)          │
│      ├── [x] Persistent schedule storage (routines.rs)      │
│      ├── [x] Wake events → trigger agent turns              │
│      ├── [x] UI: RoutinesPanel.tsx (CRUD, toggle, delete)   │
│      └── [x] Output routing to Telegram, Discord, or local │
│      Implemented as Phase 6: Routines Engine                │
│                                                              │
│  4.5.5 Skills System (SKILL.md)                        DONE  │
│      ├── [x] Skills are markdown files that teach the agent │
│      │       how to use external CLIs/tools                  │
│      ├── [x] Drop SKILL.md in ~/.hive/skills/ → agent learns│
│      ├── [x] Built-in skills: research.md, coding.md,      │
│      │       memory.md, github.md (4 seed skills)            │
│      ├── [x] Keyword-based relevance matching per turn      │
│      ├── [x] Per-turn injection (preserves KV cache)        │
│      ├── [x] Settings UI: list, refresh, open folder        │
│      ├── [ ] Skill creator meta-skill                       │
│      └── [ ] No code execution in skills — just prompt eng  │
│      Study: OpenClaw skills/ directory (53 skills)          │
└─────────────────────────────────────────────────────────────┘
```

**The "doors and keys" model:**
- GitHub door → user inserts PAT → `github_issues`, `github_prs`, `github_repos` tools activate
- Telegram door → user inserts bot token → `telegram_send`, `telegram_receive` tools activate
- Email door → user inserts IMAP/SMTP creds → `email_read`, `email_send` tools activate
- No key? No error. Tool just doesn't appear in capability manifest. Clean degradation.

**Security is non-negotiable:** External content wrapping (4.5.0) MUST land before any new integration goes live. Every piece of data from GitHub, Telegram, email, or the web gets wrapped in injection-protection boundaries before any model sees it. Adapted from OpenClaw's `external-content.ts` (MIT).

---

### Phase 5: Platform & Ecosystem (Future)
**Goal**: HIVE becomes an ecosystem, not just an app

```
┌─────────────────────────────────────────────────────────┐
│  PHASE 5: PLATFORM                            FUTURE    │
│                                                          │
│  5.1 Daemon Mode                                         │
│      ├── Persistent background operation                │
│      ├── Heartbeat loop, acts without UI open           │
│      ├── System tray integration                        │
│      └── Wake-on-event triggers                         │
│                                                          │
│  5.2 Extended Integrations                                │
│      ├── Email (IMAP/SMTP — read, compose, send, search)│
│      ├── Calendar (Google/Outlook — events, reminders)  │
│      ├── WhatsApp bridge                                │
│      ├── Markets (Polymarket, etc. — read, trade)       │
│      └── Custom REST/WebSocket/gRPC API connector       │
│                                                          │
│  5.3 Skill/Plugin System                                 │
│      ├── Downloadable skill packs                       │
│      ├── Community-contributed tools                     │
│      ├── MCP server marketplace                         │
│      └── Skill store in UI                              │
│                                                          │
│  5.4 Platform Integrations                               │
│      ├── Email (send/receive/manage)                    │
│      ├── Calendar (scheduling, reminders)               │
│      ├── File system monitoring (watch folders)         │
│      ├── Clipboard integration                          │
│      ├── Screenshot/OCR for vision models               │
│      └── Background agents (run while user works)       │
│                                                          │
│  5.5 Automation                                          │
│      ├── Scheduled tasks ("check X every morning")      │
│      ├── Event triggers ("when file changes, do Y")     │
│      ├── Workflow builder (visual task chains)          │
│      └── Autonomous multi-step flows                    │
│                                                          │
│  5.6 Sharing & Distribution                              │
│      ├── One-click installer (not just BAT file)        │
│      ├── Model pack presets ("coding setup", "research") │
│      ├── Export/import HIVE configurations               │
│      └── Community model recommendations               │
└─────────────────────────────────────────────────────────┘
```

---

### Phase 9: Audit-Driven Improvements — ACTIVE
**Goal**: Address weaknesses identified in the Feb 25 comprehensive audit.

```
┌─────────────────────────────────────────────────────────────┐
│  PHASE 9: AUDIT FIXES                             ACTIVE     │
│                                                              │
│  9.1 Security: Proper Key Derivation                  DONE    │
│      ├── [x] Replace derive_machine_key() DefaultHasher      │
│      │       with SHA-256 based HKDF (sha2 crate, already    │
│      │       a dependency)                                    │
│      ├── [x] Migration: re-encrypt existing secrets.enc      │
│      │       with new key on first run                        │
│      └── [x] Audit all other crypto paths for weak KDF        │
│                                                              │
│  9.2 MAGMA Retrieval Wiring                           DONE    │
│      ├── [x] Wire magma_traverse() into memory_recall()      │
│      │       so graph relationships enhance search results    │
│      │       (find_graph_connected_memories in memory.rs)     │
│      ├── [x] Entity-aware recall: "what do I know about      │
│      │       this file/project/tool?" (graph_query tool)      │
│      └── [x] Procedure-aware recall: recall_matching_         │
│              procedures() + auto-extraction + reinforcement   │
│                                                              │
│  9.3 Document Ingestion (RAG Pipeline)                 DONE   │
│      ├── [x] memory_import_file tool — ingest PDF, markdown, │
│      │       code files into memory as chunked records        │
│      ├── [x] Batch import: native file dialog (multi-select) │
│      │       with progress bar in MemoryTab                   │
│      └── [x] Source tracking: source_file column in memories │
│              table, set on import for citation                │
│                                                              │
│  9.4 useChat Handler Registry                                 │
│      ├── [ ] Extract specialist routing into handler function │
│      ├── [ ] Extract normal tool execution into handler       │
│      └── [ ] Create handler registry pattern for extensible   │
│              tool-specific interceptors                       │
│                                                              │
│  9.5 Automated Tests                                 PARTIAL  │
│      ├── [x] Rust unit tests for security.rs (10 tests:      │
│      │       encrypt/decrypt roundtrip, KDF, migration)      │
│      ├── [x] Rust unit tests for memory.rs (16 tests:        │
│      │       save/search/recall, dedup, quality filter)      │
│      ├── [x] Rust unit tests for content_security.rs (11     │
│      │       tests: homoglyph, wrapping, SSRF, audit log)    │
│      ├── [x] Rust unit tests for pty_manager.rs (11 tests:   │
│      │       strip_ansi_escapes, session info serialization)  │
│      ├── [x] Rust unit tests for providers.rs (14 tests:     │
│      │       strip_thinking, sanitize, reasoning extraction)  │
│      ├── [x] TypeScript tests for useChat chain policies     │
│      │       (37 vitest: repetition, tool class, channels)   │
│      ├── [x] TypeScript tests for normalizeCommand (7 vitest:│
│      │       paths, extensions, case, separators)             │
│      ├── [x] CI: cargo test + tsc --noEmit + vitest run      │
│      └── [x] Test health tracking: docs/TEST_HEALTH.md       │
│                                                              │
│  9.6 Cross-Platform (Low Priority)                            │
│      ├── [ ] Audit Windows-specific code paths               │
│      ├── [ ] Abstract GPU detection behind platform trait    │
│      └── [ ] macOS/Linux feasibility assessment              │
└─────────────────────────────────────────────────────────────┘
```

---

### Phase 10: NEXUS — Universal Agent Interface — COMPLETE

**Goal**: HIVE becomes the single access point for every AI agent. CLI tools run inside terminal panes, connected to HIVE's memory, tools, and channels.

```
┌─────────────────────────────────────────────────────────────┐
│  PHASE 10: NEXUS (The Skeleton Key)             COMPLETE     │
│                                                              │
│  10.1 PTY Infrastructure (Rust Backend)             DONE     │
│      ├── portable-pty + uuid crate dependencies              │
│      ├── pty_manager.rs: spawn/write/resize/kill/list        │
│      ├── Dedicated OS threads for reader loops               │
│      └── Events: pty-output, pty-exit                        │
│                                                              │
│  10.2 Terminal UI (React Frontend)                  DONE     │
│      ├── @xterm/xterm v6 + addon-fit + addon-web-links      │
│      ├── TerminalPane.tsx: self-contained component          │
│      └── HIVE zinc/amber theme, auto-fit, web links         │
│                                                              │
│  10.3 Pane Type System                              DONE     │
│      ├── PaneType: 'chat' | 'terminal'                      │
│      ├── AgentConfig + BUILTIN_AGENTS (Shell, Claude Code,   │
│      │   Codex, Aider)                                       │
│      └── MultiPaneChat: type-based routing                   │
│                                                              │
│  10.4 Agent Registry + Settings                     DONE     │
│      ├── AgentRegistrySection in SettingsTab                 │
│      ├── check_agent_available (which/where)                 │
│      └── Custom agent persistence (localStorage)             │
│                                                              │
│  10.5 HIVE Integration Layer                        DONE     │
│      ├── 10.5.1: PTY output memory logging (ANSI strip,     │
│      │   line accumulation, pty-log events)                  │
│      ├── 10.5.2: MCP auto-bridge (inject HIVE into          │
│      │   ~/.claude.json for Claude Code)                     │
│      ├── 10.5.3: Cross-agent tools (send_to_agent,          │
│      │   list_agents HiveTools)                              │
│      └── 10.5.4: Remote channel → agent routing              │
│          (Discord/Telegram → terminal agent, with fallback)  │
└─────────────────────────────────────────────────────────────┘
```

---

### Phase 11: Cognitive Bus — Model Multiplexing & Unified Identity
**Goal**: HIVE becomes a system-level Mixture of Experts. Models share one identity, observe each other, and spawn sub-agents across providers.

**The Vision**: Strip individual model system prompts. Every model — local 9B, cloud reasoning engine, CLI agent in a terminal — speaks as HIVE. They share a context bus (observable state), can read each other's recent activity, and any model can spawn sub-agents via any other model. The user talks to HIVE. HIVE is all of them.

This is kernel-level model scheduling: HIVE allocates cognitive resources (models) to tasks the way an OS allocates CPU cores to processes. Fast local model for triage, cloud reasoner for hard problems, coding agent for implementation — all coordinated, all aware of each other.

```
┌─────────────────────────────────────────────────────────────────────┐
│  COGNITIVE BUS ARCHITECTURE                                          │
│                                                                      │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │  UNIFIED IDENTITY LAYER (harness_build → all models)          │  │
│  │  HIVE.md identity + capability manifest + memory recall        │  │
│  │  Every model speaks as HIVE — the framework IS the identity    │  │
│  └──────────┬──────────────────┬──────────────────┬──────────────┘  │
│             │                  │                  │                  │
│  ┌──────────▼──────┐  ┌───────▼───────┐  ┌───────▼───────┐        │
│  │  Slot: Local     │  │  Slot: Cloud   │  │  Slot: PTY     │        │
│  │  Qwen3.5 9B      │  │  Kimi K2.5     │  │  Claude Code   │        │
│  │  (fast triage)   │  │  (reasoning)   │  │  (coding)      │        │
│  └────────┬─────────┘  └───────┬────────┘  └───────┬────────┘        │
│           │                    │                    │                  │
│  ┌────────▼────────────────────▼────────────────────▼────────────┐  │
│  │  CONTEXT BUS (shared observable state)                         │  │
│  │  ┌─────────────┐  ┌──────────────┐  ┌────────────────────┐   │  │
│  │  │ Scratchpads │  │ MAGMA Events │  │ Working Memory     │   │  │
│  │  │ (per-agent)  │  │ (episodic)   │  │ (per-session)      │   │  │
│  │  └─────────────┘  └──────────────┘  └────────────────────┘   │  │
│  │  read_agent_context(agent_id) → summarized view of activity  │  │
│  └──────────────────────────────────────────────────────────────┘  │
│                                                                      │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │  CROSS-MODEL SPAWNING                                         │  │
│  │  Any model can: worker_spawn(provider="anthropic",            │  │
│  │    model="claude-sonnet-4-20250514") or slot_role="coder"              │  │
│  │  Workers get full HIVE identity + write to context bus        │  │
│  └──────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
```

**Existing plumbing (P3: don't reinvent):**

| Component | Where | What it does |
|-----------|-------|-------------|
| `assemble_prompt()` | `harness.rs:509` | Builds HarnessContext (identity + capabilities + volatile state) |
| `worker_spawn` | `worker_tools.rs:167` | Spawns workers with any provider/model, custom system_prompt |
| `route_to_specialist` | `specialist_tools.rs:17` | Routes to specialist with MAGMA wake briefing |
| `scratchpad_*` tools | `scratchpad_tools.rs` | Lock-free inter-agent coordination (create/write/read) |
| `magma_events_since()` | `magma.rs:58` | Episodic event retrieval (filtered by agent + time) |
| `build_wake_context()` | `orchestrator.rs:185` | MAGMA briefing assembly (events, entities, procedures) |
| Slot system | `slots.rs` | SlotRole, SlotState, VramBudget, assignment tracking |
| `send_to_agent` | `agent_tools.rs:21` | PTY bridge to terminal agents (Claude Code, etc.) |

**What was built (March 10, 2026 — `fix/audit-findings` branch):**

```
┌─────────────────────────────────────────────────────────────────┐
│  PHASE 11: COGNITIVE BUS                              DONE        │
│                                                                    │
│  11.1 Unified Identity Layer                                       │
│      ├── [x] Inject full harness_build() output for ALL models     │
│      │       Local specialists: read_identity() in Rust             │
│      │       Cloud specialists: cached harness in TypeScript        │
│      │       Workers: read_identity() when no custom system_prompt  │
│      ├── [x] Cloud specialists receive HIVE identity               │
│      ├── [x] Workers receive HIVE identity                         │
│      └── [x] Log harness injection for each model (P4)             │
│                                                                    │
│  11.2 read_agent_context Tool                                      │
│      ├── [x] New HiveTool: ReadAgentContextTool (45th tool)        │
│      │       Queries: MAGMA events, scratchpads, working memory,   │
│      │       worker status — filtered by agent_id + since_minutes   │
│      ├── [x] Register in ToolRegistry                               │
│      └── [x] Agent activity enrichment via MAGMA events             │
│                                                                    │
│  11.3 Context Bus — Shared Observable State                         │
│      ├── [x] context_bus scratchpad (auto-created on first write)  │
│      ├── [x] Tool loop writes chain summary after completion       │
│      ├── [x] Specialists write to bus after task completion         │
│      ├── [x] Bus summary in volatile context (separate msg, KV ok) │
│      └── [x] Formalized on existing scratchpads (P3: no new system)│
│                                                                    │
│  11.4 Cross-Model Agent Spawning Enhancement                        │
│      ├── [x] Workers get harness_build() identity                  │
│      ├── [x] Workers auto-write to context bus on completion       │
│      ├── [x] slot_role parameter resolves slot config to model     │
│      └── [x] Any model can spawn sub-agents via any other model    │
│                                                                    │
│  11.5 Architecture Prerequisites (completed in Phases 4A-4D)       │
│      ├── [x] useChat.ts decomposition → chainPolicies.ts (351 ln) │
│      ├── [x] SQLite WAL consistency (all 8 sites standardized)     │
│      ├── [x] Memory tier promotion (working → short → long)        │
│      └── [x] Context summarization (model-based + fallback)        │
└─────────────────────────────────────────────────────────────────────┘
```

**Success metrics (all met):**
- [x] All models (specialists, workers, cloud slots) receive full HIVE identity
- [x] `read_agent_context("coder")` returns recent activity summary
- [x] Context bus scratchpad exists and agents' activity is visible
- [x] Workers can be spawned with `slot_role="coder"` (resolves to slot's model)
- [x] Worker completions appear in the context bus
- [x] Two models can observe each other's recent activity in real-time

---

## Decision: What To Build vs. What To Steal

### Build Ourselves (Novel)
- [x] Desktop app (Tauri v2 + React/TypeScript)
- [x] Provider abstraction (Local, OpenAI, Anthropic, Ollama, OpenRouter, DashScope)
- [x] VRAM calculator with GGUF metadata parsing
- [x] Secure API key storage (AES-256-GCM)
- [x] KV cache offload wiring
- [x] Conversation persistence
- [x] Model recommendation engine (3-tier, benchmark-ranked)
- [x] Model description tags (pipeline + domain)
- [x] Tool framework (file, terminal, web, system tools)
- [x] Memory substrate (SQLite + FTS5 + vector embeddings, hybrid search)
- [x] Agentic loop (tool call → execute → feed back, up to 10 iterations)
- [x] Hermes-native tool harness for local models
- [x] DashScope (Alibaba) provider with Kimi K2.5 + Qwen models
- [x] Thinking token separation (reasoning vs content across all providers)
- [x] Memory quality filter (heuristic scoring, Q+A pair detection)
- [x] Harness stable/volatile split (KV cache prefix optimization)
- [x] Context management — truncation + memory-backed + model-based summarization
- [x] Memory rework (Phase 3.5) — three-tier cognitive memory, model agency, MAGMA activation
- [x] Brain / orchestrator (Phase 4) — slot system + routing tool + MAGMA + wake briefings + auto-sleep
- [x] Side-chat system (parallel cloud workers) — Phase 8 autonomous research workers
- [x] Cognitive Bus (Phase 11) — unified identity, context bus, read_agent_context, slot_role spawning
- [ ] Model hot-swap with context preservation

### Steal Patterns (Proven — OpenClaw MIT)
- [x] **Memory architecture** — SQLite + vectors + hybrid search + memory flush
- [x] **SOUL.md identity injection** → harness.rs (HIVE.md)
- [x] **Capability manifest auto-generation** → harness.rs
- [x] **Pre-compaction memory flush** → App.tsx
- [x] **External content security wrapping** — boundary markers + homoglyph folding
- [x] **SSRF protection** — URL validation, private IP blocking, redirect limits
- [x] **Dangerous tools registry** — centralized tool risk categorization
- [x] **Cron/scheduled tasks** — routines engine with cron triggers + event triggers
- [ ] **Browser automation** — CDP + AI-readable DOM snapshots
- [x] **Skills system** — SKILL.md prompt docs, user-extensible
- [x] **Telegram patterns** — allowlists, daemon lifecycle, plain text default
- [x] **Agent-to-agent messaging** — cross-session communication via PTY (Phase 10 NEXUS)
- [ ] **Fallback chains** — auth rotation + provider fallback pattern
- [x] **MCP Protocol** — tool schema compatibility
- [x] **llama.cpp function calling** — Hermes-native tool call support

### Integrate (Plug & Play)
- [x] **MCP Servers** — bidirectional MCP: HIVE as server (--mcp) + HIVE as client (connect external servers)
- [x] **llama.cpp** — inference engine (integrated)
- [x] **Ollama** — alternative local inference (integrated)

---

## Priority Matrix

| Task | Impact | Effort | Priority |
|------|--------|--------|----------|
| ~~App.tsx split~~ | ~~HIGH~~ | ~~LOW~~ | ~~DONE~~ |
| ~~Model recommendations~~ | ~~HIGH~~ | ~~MEDIUM~~ | ~~DONE~~ |
| ~~main.rs modularization~~ | ~~HIGH~~ | ~~MEDIUM~~ | ~~DONE~~ |
| ~~Tool framework~~ | ~~HIGH~~ | ~~MEDIUM~~ | ~~DONE~~ |
| ~~Memory substrate~~ | ~~HIGH~~ | ~~HIGH~~ | ~~DONE~~ |
| ~~OpenRouter provider~~ | ~~HIGH~~ | ~~MEDIUM~~ | ~~DONE~~ |
| ~~Smart model router~~ | ~~HIGH~~ | ~~HIGH~~ | ~~DONE~~ |
| ~~Tool approval system~~ | ~~HIGH~~ | ~~MEDIUM~~ | ~~DONE~~ |
| ~~Telegram integration~~ | ~~HIGH~~ | ~~LOW~~ | ~~DONE~~ |
| ~~Discord integration~~ | ~~MEDIUM~~ | ~~MEDIUM~~ | ~~DONE~~ |
| ~~Memory tab~~ | ~~MEDIUM~~ | ~~MEDIUM~~ | ~~DONE~~ |
| ~~Security hardening~~ | ~~CRITICAL~~ | ~~MEDIUM~~ | ~~DONE (4/6 items)~~ |
| ~~GitHub integration~~ | ~~HIGH~~ | ~~MEDIUM~~ | ~~DONE~~ |
| ~~**Memory rework (Phase 3.5)**~~ | ~~CRITICAL~~ | ~~HIGH~~ | ~~DONE~~ |
| ~~**Side-chat system (workers)**~~ | ~~HIGH~~ | ~~MEDIUM~~ | ~~DONE~~ |
| ~~**Routines / cron (Phase 6)**~~ | ~~HIGH~~ | ~~MEDIUM~~ | ~~DONE~~ |
| ~~**Plan execution (Phase 7)**~~ | ~~HIGH~~ | ~~MEDIUM~~ | ~~DONE~~ |
| ~~**Autonomous research (Phase 8)**~~ | ~~HIGH~~ | ~~HIGH~~ | ~~DONE~~ |
| ~~**Security: proper KDF (9.1)**~~ | ~~HIGH~~ | ~~LOW~~ | ~~DONE~~ |
| ~~**MAGMA retrieval wiring (9.2)**~~ | ~~HIGH~~ | ~~MEDIUM~~ | ~~DONE~~ |
| ~~**Document ingestion / RAG (9.3)**~~ | ~~HIGH~~ | ~~MEDIUM~~ | ~~DONE~~ |
| ~~**Skills system (SKILL.md)**~~ | ~~MEDIUM~~ | ~~LOW~~ | ~~DONE~~ |
| ~~**Automated tests (9.5)**~~ | ~~HIGH (214 Rust + 96 vitest)~~ | ~~MEDIUM~~ | ~~DONE~~ |
| ~~**useChat decomposition (Phase 11.5)**~~ | ~~HIGH~~ | ~~MEDIUM~~ | ~~DONE~~ |
| ~~**Cognitive Bus (Phase 11)**~~ | ~~CRITICAL (the vision)~~ | ~~HIGH~~ | ~~DONE~~ |
| **Model hot-swap** | HIGH (flagship feature) | HIGH | **P3** |
| **Browser automation** | HIGH (research capability) | HIGH | **P3** |
| Server output capture | MEDIUM | LOW | **P4** |
| Token/speed display | MEDIUM | LOW | **P4** |
| Daemon mode | MEDIUM | MEDIUM | **P4** |
| Email integration | MEDIUM | MEDIUM | **P4** |
| Cross-platform (9.6) | LOW (market expansion) | HIGH | **P5** |
| Calendar integration | MEDIUM | HIGH | **P5** |
| Markets / custom APIs | LOW | MEDIUM | **P5** |

---

## Immediate Next Steps

The Cognitive Bus (Phase 11) is complete. HIVE's core architecture — identity, memory, tools, specialists, workers, context bus — is fully operational. What remains is polish, expansion, and hardening.

### 1. Model Hot-Swap with Context Preservation
- Swap specialists mid-conversation with memory flush + wake briefing continuity
- Context preserved via working memory summarization (Phase 4D infrastructure in place)

### 2. Browser Automation (Phase 4.5.3)
- CDP browser control with AI-readable DOM snapshots
- Enables web research workflows beyond `web_fetch` + `web_search`

### 3. Extended Integrations (Phase 5)
- Email (IMAP/SMTP), calendar, markets, custom APIs
- Same daemon pattern as Telegram/Discord

### 4. UI Polish
- Responsive layout improvements, visual VRAM budget bar
- Token/speed display in chat UI
- Server output captured in Logs tab

### Recently Completed (March 10, 2026)
- **Cognitive Bus (Phase 11)** — Unified identity for all models, `read_agent_context` tool, shared context bus, cross-model spawning with `slot_role` parameter
- **Testing & Hardening** — 214 Rust tests + 96 vitest tests + CI pipeline (GitHub Actions)
- **Architecture Cleanup** — useChat.ts decomposition, SQLite WAL consistency, memory tier promotion, context summarization
- **Deep Audit** — Fixed dead reinforcement pipeline (`ln()`), UTF-8 panics, orphaned MAGMA edges, stale closures, dead Tauri commands
- **Security Audit** — Path traversal, updater removal, tunnel hardening, PID tracking

---

## Document Organization

### Active (current, maintained)
- `CLAUDE.md` — Development guidelines, coding standards (checked into repo root)
- `ROADMAP.md` — This file (single source of truth for planning)
- `CHANGELOG.md` — User-facing changelog of notable changes
- `HIVE/docs/STATE_OF_HIVE.md` — Periodic status reports (current: Report #4, Mar 2026)
- `HIVE/docs/PRINCIPLE_LATTICE.md` — **Non-negotiable axioms.** Do NOT modify casually.
- `HIVE/docs/PHASE4_IMPLEMENTATION.md` — Phase 4 orchestrator implementation guide
- `HIVE/docs/PHASE10_NEXUS.md` — Phase 10 NEXUS architecture + implementation log
- `HIVE/docs/TEST_HEALTH.md` — Test suite baseline tracking (P4: Errors Are Answers)

### Vision & Reference (aspirational, conceptual)
All in `HIVE/docs/vision/` — see [vision/README.md](HIVE/docs/vision/README.md):
- THE_VISION.md — North star: what HIVE is becoming
- ARCHITECTURE_PRINCIPLES.md — Provider-agnostic design philosophy (Jan 2026, Python pseudocode)
- MODEL_MODULARITY_GUIDE.md — How model swapping works (conceptual, Python pseudocode)
- Architecture Overview v2, Hot-Swap Mechanics, Implementation Theory, Technical Specification
- Research Findings, HIVE README v2

---

## Success Metrics

### Phase 1 Complete: YES
- [x] Chat works for 50+ exchanges without failure
- [x] Conversations persist across app restarts
- [x] User can switch between conversations
- [x] App.tsx split into components
- [x] Model recommendation engine (3-tier, benchmark-ranked)
- [ ] Server output captured for debugging

### Phase 1.5 Complete: YES (minus polish)
- [x] main.rs split into modules (each <400 lines) — 12 modules, 87-line main.rs
- [x] No functionality regressions — cargo check clean
- [ ] Server output captured and surfaced in Logs tab
- [ ] Token/speed displayed in chat UI

### Phase 2 Complete: YES
- [x] Model can invoke tools via standardized schema
- [x] Core tools working: read_file, write_file, run_command, web_fetch, web_search
- [x] Permission model: risk-based approval (low/medium/high/critical)
- [x] Tool calls + results visible in chat UI (collapsible blocks)
- [x] MCP-compatible tool interface
- [x] Hermes-native tool harness for local models

### Phase 3 Complete: YES
- [x] SQLite memory database persists across sessions
- [x] Model can recall past conversations via session injection
- [x] Memory flush happens before context truncation
- [x] Hybrid search returns relevant past context (vector + BM25)
- [x] Graceful degradation (FTS5-only if no embedding API key)

### Phase 4 Complete When:
- [x] Brain model receives intent and routes tasks via tool call
- [x] Side-chats spawn to cloud workers in parallel
- [ ] Two models can swap with context preserved (swap time < 15s)
- [x] Brain routes tasks to appropriate specialist (local + cloud)
- [x] Cloud fallback works when local can't handle task
- [x] Wake briefings inject MAGMA context on specialist wake
- [x] VRAM enforcement evicts idle specialists when tight
- [x] Auto-sleep frees resources for idle specialists
- [x] Procedure learning auto-extracts successful tool chains
- [ ] User experiences one continuous conversation (needs hot-swap)

### Phase 4.5 Complete When:
- [x] External content wrapping active on ALL tool results with external data
- [x] SSRF protection on web_tools.rs (blocks private IPs, limits redirects)
- [x] GitHub integration: user provides PAT → can list issues and PRs via tool calls
- [x] Telegram integration: user provides bot token → can command HIVE from phone
- [x] Discord integration: user provides bot token → can command HIVE from Discord
- [ ] Browser automation: CDP-based navigate + screenshot + AI-readable DOM
- [x] Cron system: routines engine with cron expressions + event triggers + output routing
- [x] Skills system: drop SKILL.md in `~/.hive/skills/` → agent learns new capability (4 seed skills, keyword matching, per-turn injection)

### Phase 5 Complete When:
- [ ] Daemon mode: HIVE runs persistently in background
- [ ] Email integration working (read + send via IMAP/SMTP)
- [ ] Scheduled task system operational
- [ ] One-click installer (not just BAT file)

### Phase 11 Complete: YES (March 10, 2026)
- [x] All models (specialists, workers, cloud slots) receive full HIVE identity via `harness_build()`
- [x] `read_agent_context("coder")` returns formatted activity summary (events + scratchpads + working memory)
- [x] Context bus scratchpad auto-created on first write, agents write status after tool loops
- [x] Workers spawnable with `slot_role="coder"` (resolves to slot's configured model)
- [x] Worker completions appear in context bus
- [x] Two models can observe each other's recent activity via `read_agent_context`
- [x] Bus summary included in harness volatile context

---

*For detailed change history, see [CHANGELOG.md](CHANGELOG.md).*

---

**End of Roadmap**

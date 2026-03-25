# HIVE — Hierarchical Intelligence with Virtualized Execution

A persistent AI orchestration harness — a Windows desktop app that coordinates local and cloud LLMs as interchangeable cognitive resources. HIVE is the always-on brain that manages models, routes tasks to specialists, remembers across sessions, and maintains identity. The framework is permanent; the models are replaceable.

## What HIVE Is

HIVE is not a chatbot. It's not a model runner. It's an **orchestration framework** that stays constant while models, providers, and inference backends change underneath.

- Zero GPUs? Use free API models — you're a first-class citizen
- One GPU? Run a local brain, route specialists to cloud
- Multiple GPUs? Run everything local
- Mix and match freely — HIVE is provider-agnostic

## What It Does

### 6 Providers, One Interface

Same streaming, same tool calling, same memory injection — regardless of backend.

| Provider | Where | Models |
|----------|-------|--------|
| Local (llama.cpp) | Your machine (NVIDIA/AMD) | Any GGUF file |
| Ollama | localhost:11434 | Ollama library |
| OpenAI | Cloud | GPT-4o, o1, etc. |
| Anthropic | Cloud | Claude Sonnet, Opus, etc. |
| OpenRouter | Cloud | 100+ models |
| DashScope | Cloud | Kimi K2.5, Qwen |

Adding a new OpenAI-compatible provider is a 1-line dispatch table entry.

### 43 MCP-Compatible Tools

Every tool self-describes via JSON Schema. Risk-based approval: read-only tools auto-execute, destructive tools always ask.

| Category | Tools |
|----------|-------|
| **File I/O** | `read_file` (paginated), `write_file`, `list_directory` |
| **System** | `run_command` (Windows + WSL), `system_info` |
| **Web** | `web_fetch`, `web_search`, `web_extract`, `read_pdf` |
| **GitHub** | `github_issues`, `github_prs`, `github_repos` |
| **Telegram** | `telegram_send`, `telegram_get_updates`, `telegram_bot_info` |
| **Discord** | `discord_send`, `discord_read` |
| **Memory** | `memory_save`, `memory_search`, `memory_edit`, `memory_delete` |
| **MAGMA** | `task_track`, `graph_query`, `entity_track`, `procedure_learn` |
| **Orchestration** | `route_to_specialist`, `plan_execute`, `integration_status`, `list_tools` |
| **Research** | `repo_clone`, `file_tree`, `code_search`, `check_logs` |
| **Workers** | `worker_spawn`, `worker_status`, `worker_terminate`, `worker_report` |
| **Scratchpads** | `scratchpad_create`, `scratchpad_write`, `scratchpad_read` |
| **Agents** | `send_to_agent`, `list_agents` |
| **Import** | `memory_import_file` |

Plus any external tools connected via MCP client — they appear in the same registry automatically.

### Persistent Memory (MAGMA Multi-Graph)

Four interconnected graphs over a SQLite + FTS5 + vector embedding foundation:

- **Episodic** — what happened (events, sessions, tool calls)
- **Entity** — what exists (files, models, projects, agents)
- **Procedural** — what works (learned tool chains with success/fail tracking)
- **Relationship** — how things connect (typed weighted edges across all graphs)

Hybrid search (BM25 + cosine similarity), recency decay, relevance thresholds, context-proportional budgets (10% of model context window), near-duplicate detection. Gracefully degrades to keyword-only search without an OpenAI key.

### Cognitive Harness

HIVE has an identity — a user-editable markdown file (`HIVE.md`) defining personality and principles, combined with an auto-generated capability manifest (loaded tools, active model, VRAM, memory stats). The model always knows what it is and what it can do. Swap from a local 7B to Claude — identity persists, capabilities auto-update.

### Specialist Slot System

Five specialist roles with independent model assignments, server instances, and VRAM budgeting:

| Slot | Role |
|------|------|
| **Consciousness** | General reasoning, triage |
| **Coder** | Code generation, review |
| **Terminal** | Shell commands, system ops |
| **WebCrawl** | Web research, scraping |
| **ToolCall** | Tool chain execution |

Task routing analyzes requests and delegates to the right specialist. Slots have lifecycle states (idle/loading/active/sleeping), VRAM tracking, and fallback chains.

### MCP Protocol (Bidirectional)

**HIVE as MCP Server** — launch with `--mcp` flag, any MCP client (Claude Code, Cursor, etc.) can use HIVE's tools:
```json
{ "mcpServers": { "hive": { "command": "hive-desktop", "args": ["--mcp"] } } }
```

**HIVE as MCP Client** — connect to any external MCP server, its tools appear in the registry automatically as `mcp_<server>_<tool>`.

### Routines Engine

Autonomous standing instructions that fire on schedule or events:

- **Cron triggers** — 5-field cron expressions ("every day at 9am")
- **Event triggers** — channel-based ("when a Telegram message arrives")
- **Output routing** — responses go to Telegram, Discord, or stay local
- Runs through the same agentic loop with full tool and memory access

### Autonomous Research Workers

Background agents that research independently:

- Clone repos, search codebases, scrape the web
- Write results to named scratchpads with TTL
- Sandboxed — no shell execution, no file writes, no messaging
- Worker status panel with progress bars and stall warnings

### NEXUS — Universal Agent Interface

CLI coding agents (Claude Code, Codex, Aider, or any CLI tool) run inside HIVE as terminal panes, connected to the full stack:

- **PTY backend** (portable-pty) — real pseudo-terminals, not just pipe wrappers
- **xterm.js v6** — full terminal emulation with ANSI colors, cursor movement, mouse events
- **Memory logging** — PTY output (ANSI-stripped) flows to HIVE's memory system
- **Cross-agent tools** — chat model calls `send_to_agent` to delegate to a running CLI agent
- **MCP bridge** — one click injects HIVE's MCP config into Claude Code's settings
- **Channel routing** — Discord/Telegram messages route directly to a terminal agent

Add any CLI agent: define `{ name, command, args }` in Settings. Framework unchanged (P7).

### Multi-Channel Awareness

Telegram and Discord daemons run as background tasks, receiving messages and responding through the full agentic pipeline. Content security boundaries protect against prompt injection. Adding new channels follows the same daemon pattern.

### Hardware-Aware Model Management

- Auto-detects GPU (NVIDIA/AMD/Intel), CPU, RAM, WSL2 status
- VRAM pre-launch checks with compatibility badges (good / tight / insufficient)
- Live resource monitoring (VRAM, RAM, GPU utilization) every chat turn
- HuggingFace model search with benchmark scores from Open LLM Leaderboard
- 3-tier recommendation engine: Fast (headroom), Quality (pushes GPU), Big Brain (RAM offload)
- GGUF metadata parsing (architecture, params, quantization, context length)
- Thinking token separation for reasoning models (DeepSeek R1, Kimi K2.5, Anthropic, OpenAI)

### Persistent Observability (P4: Errors Are Answers)

Dual-log system — frontend and backend both write to `hive-app.log`, readable by the AI via `check_logs`:

- **Frontend**: `useLogs.ts` auto-persists all `[HIVE]` logs, errors, and warnings (`FE |`, `FE_ERROR |`, `FE_WARN |`)
- **Backend**: 10 Rust modules log lifecycle events with structured prefixes (`SERVER |`, `PROVIDER |`, `MEMORY |`, `TELEGRAM |`, `DISCORD |`, `ROUTINES |`, `SLOTS |`, `HARNESS |`, `DOWNLOAD |`, `MCP |`, `PTY |`)
- The steering AI can read its own logs, diagnose errors, and self-correct — not just react to user reports

### Content Security

- External content wrapped in boundary markers with injection prevention
- Unicode homoglyph folding prevents boundary marker forgery
- Error sanitization strips API keys/tokens before they reach logs or memory
- AES-256-GCM encrypted API key storage
- Separate wrapping for untrusted web content vs. authenticated user messages

## Architecture

```
Windows 11
├── HIVE Desktop (Tauri v2: Rust + React/TypeScript)
│   ├── React UI ─── 18 components + 4 hooks + App.tsx orchestrator
│   │   ├── Setup, Models, Browse, Chat, Settings, Logs, Memory, MCP tabs
│   │   └── MemoryPanel, RoutinesPanel, WorkerPanel, VramPreview, SlotConfig slide-outs
│   ├── TypeScript API layer ─── api.ts + recommendation engine + memory + integrations
│   ├── useChat.ts ─── agentic tool loop, plan execution, streaming
│   └── Rust backend ─── 28 core modules + 16 tool modules (22K lines)
│       ├── Core: hardware, models, server, providers, memory, harness, security
│       ├── Orchestration: orchestrator, slots, routines, working_memory
│       ├── NEXUS: pty_manager, agent_tools (terminal pane PTY backend)
│       ├── Integrations: telegram_daemon, discord_daemon, mcp_server, mcp_client
│       ├── Memory: memory, magma, working_memory
│       └── Tools: file, system, web, github, telegram, discord, memory,
│           workspace, scratchpad, worker, log, plan, specialist, integration
│
├── Inference
│   ├── Windows native: llama-server.exe (NVIDIA CUDA)
│   ├── WSL2 Ubuntu: llama-server via ROCm (AMD GPUs)
│   └── Cloud: OpenAI, Anthropic, Ollama, OpenRouter, DashScope APIs
│
├── Memory
│   ├── SQLite + FTS5 + vector embeddings (hybrid search)
│   ├── MAGMA multi-graph (episodic, entity, procedural, relationship)
│   └── Daily markdown logs (source of truth)
│
└── Daemons
    ├── Telegram (long-polling)
    ├── Discord (REST polling)
    └── Routines (cron scheduler + event matcher)
```

**Key design:** Download models to Windows, WSL2 reads them via `/mnt/c/` bridge. No duplication.

## Project Structure

```
HiveMind/
├── README.md
├── ROADMAP.md               # What's done, what's next
├── CHANGELOG.md             # Detailed change history
├── CLAUDE.md                # Development guidelines & coding standards
├── HIVE/
│   ├── desktop/             # The app (Tauri v2)
│   │   ├── src/             # React frontend
│   │   │   ├── App.tsx      # State orchestrator (~1,240 lines)
│   │   │   ├── useChat.ts   # Agentic loop + streaming (~1,660 lines)
│   │   │   ├── types.ts     # Shared types (~350 lines)
│   │   │   ├── lib/
│   │   │   │   ├── api.ts              # Core API layer (~1,560 lines)
│   │   │   │   ├── api_memory.ts       # Memory API helpers
│   │   │   │   ├── api_recommendations.ts  # Recommendation engine
│   │   │   │   └── api_integrations.ts # Integration API helpers
│   │   │   ├── hooks/
│   │   │   │   ├── useLogs.ts             # Console capture + persistent log bridge
│   │   │   │   ├── useHuggingFace.ts      # HF search + recommendations
│   │   │   │   ├── useRemoteChannels.ts   # Telegram/Discord/Worker/Routine listeners
│   │   │   │   └── useConversationManager.ts # Conversation CRUD + auto-save
│   │   │   └── components/  # 18 tab/utility components
│   │   │       ├── SetupTab.tsx         # Hardware detection
│   │   │       ├── ModelsTab.tsx        # Local model management
│   │   │       ├── BrowseTab.tsx        # HuggingFace browser
│   │   │       ├── ChatTab.tsx          # Chat interface
│   │   │       ├── SettingsTab.tsx      # Config + API keys + harness editor
│   │   │       ├── LogsTab.tsx          # Application logs
│   │   │       ├── MemoryTab.tsx        # Memory browser + MAGMA graph viewer
│   │   │       ├── McpTab.tsx           # MCP server/client management
│   │   │       ├── MemoryPanel.tsx      # Slide-out memory search/edit
│   │   │       ├── RoutinesPanel.tsx    # Routines CRUD
│   │   │       ├── WorkerPanel.tsx      # Background worker status
│   │   │       ├── SlotConfigSection.tsx # Specialist slot config
│   │   │       ├── VramPreview.tsx      # VRAM compatibility preview
│   │   │       ├── ModelInfoPopup.tsx   # GGUF metadata popup
│   │   │       ├── ChatPane.tsx         # Individual chat pane (model + conversation)
│   │   │       ├── PaneHeader.tsx       # Pane header bar (model selector, controls)
│   │   │       ├── TerminalPane.tsx     # NEXUS: xterm.js terminal pane
│   │   │       └── MultiPaneChat.tsx    # Multi-pane orchestrator (chat + terminal)
│   │   └── src-tauri/       # Rust backend
│   │       └── src/
│   │           ├── main.rs             # Entry point + command registration
│   │           ├── hardware.rs         # GPU/CPU/RAM/WSL detection
│   │           ├── models.rs           # Local model listing, GGUF parsing
│   │           ├── server.rs           # llama-server lifecycle
│   │           ├── providers.rs        # Chat providers (6 backends)
│   │           ├── provider_stream.rs  # SSE streaming
│   │           ├── provider_tools.rs   # Tool call parsing (OpenAI/Anthropic/Hermes)
│   │           ├── memory.rs           # SQLite + FTS5 + embeddings
│   │           ├── magma.rs            # MAGMA multi-graph (4 graphs)
│   │           ├── working_memory.rs   # Per-session working memory
│   │           ├── harness.rs          # Cognitive harness (identity + capabilities)
│   │           ├── orchestrator.rs     # Task routing engine
│   │           ├── slots.rs            # Specialist slot management
│   │           ├── routines.rs         # Cron + event routines engine
│   │           ├── security.rs         # AES-256-GCM key storage
│   │           ├── content_security.rs # Injection prevention + sanitization
│   │           ├── download.rs         # HuggingFace downloads
│   │           ├── mcp_server.rs       # MCP server mode (--mcp)
│   │           ├── mcp_client.rs       # MCP client (external servers)
│   │           ├── telegram_daemon.rs  # Telegram background polling
│   │           ├── discord_daemon.rs   # Discord background polling
│   │           ├── http_client.rs      # Shared HTTP client config
│   │           ├── state.rs            # App state management
│   │           ├── types.rs            # Shared Rust types
│   │           ├── paths.rs            # Path resolution helpers
│   │           ├── gguf.rs             # GGUF metadata parsing
│   │           ├── wsl.rs              # WSL bridge helpers
│   │           ├── vram.rs             # VRAM estimation
│   │           ├── pty_manager.rs      # NEXUS: PTY session lifecycle
│   │           └── tools/              # 16 tool modules (42 tools)
│   │               ├── mod.rs          # Tool trait + registry
│   │               ├── file_tools.rs
│   │               ├── system_tools.rs
│   │               ├── web_tools.rs
│   │               ├── github_tools.rs
│   │               ├── telegram_tools.rs
│   │               ├── discord_tools.rs
│   │               ├── memory_tools.rs
│   │               ├── workspace_tools.rs
│   │               ├── scratchpad_tools.rs
│   │               ├── worker_tools.rs
│   │               ├── log_tools.rs
│   │               ├── plan_tools.rs
│   │               ├── specialist_tools.rs
│   │               ├── integration_tools.rs
│   │               └── agent_tools.rs     # NEXUS: send_to_agent, list_agents
│   └── docs/
│       ├── PRINCIPLE_LATTICE.md
│       ├── STATE_OF_HIVE.md
│       ├── PHASE4_IMPLEMENTATION.md
│       ├── PHASE10_NEXUS.md            # NEXUS terminal pane architecture
│       ├── TEST_HEALTH.md              # Test suite baseline tracking
│       └── archive/vision/             # Architecture vision docs
├── .claude/                            # Claude Code hooks + skills
└── claude-tools/                       # mgrep semantic search
```

## By the Numbers

| Metric | Value |
|--------|-------|
| Total lines of code | ~36,000 (22K Rust + 14K TypeScript) |
| Rust modules | 28 core + 16 tool modules |
| Tauri commands | 135 |
| MCP-compatible tools | 43 (extensible via MCP client) |
| Providers | 6 |
| Specialist slots | 5 |
| Memory graphs | 4 |
| Background daemons | 3 (Telegram, Discord, Routines) |
| Frontend components | 18 |
| Security layers | 4 (encryption, wrapping, homoglyphs, sanitization) |
| MCP directions | 2 (server + client) |

## System Requirements

- **OS:** Windows 11 (with WSL2 for AMD GPUs)
- **GPU:** Any NVIDIA (native) or AMD with ROCm support (via WSL2) — or none (cloud-only is first-class)
- **RAM:** 16GB minimum, 32GB recommended for local models
- **Software:** Node.js, Rust toolchain (for building from source)

## Building

```bash
cd HIVE/desktop
npm install
npm run tauri dev    # Development mode
npm run tauri build  # Production build
```

Or on Windows, double-click `START_HIVE.bat` for one-click build + launch.

## Design Principles

HIVE has [8 axiomatic principles](HIVE/docs/PRINCIPLE_LATTICE.md):

| # | Principle | Axiom |
|---|-----------|-------|
| 1 | Bridges and Modularity | One path, two systems. Lego blocks, not monoliths. |
| 2 | Provider Agnosticism | The interface is permanent. The backend is replaceable. |
| 3 | Simplicity Wins | Don't reinvent the wheel. Code exists to be used. |
| 4 | Errors Are Answers | Every failure teaches. Given a model, the program debugs itself. |
| 5 | Fix The Pattern | Cure the root cause. Don't treat symptoms. |
| 6 | Secrets Stay Secret | Military-grade OPSEC. Nothing left open to exploitation. |
| 7 | The Framework Survives | Models evolve. HIVE endures. |
| 8 | Low Floor, High Ceiling | A noob can use it. A power user would want to. |

## The Vision

HIVE's long-term goal is a full AI assistant — not a chatbot. A local brain model that decomposes tasks, delegates to specialists (local or cloud), uses tools, remembers everything, and presents one continuous intelligence to the user.

**The framework survives. The models evolve.**

## Documentation

| Document | Purpose |
|----------|---------|
| [CLAUDE.md](CLAUDE.md) | Development guidelines, coding standards, patterns |
| [ROADMAP.md](ROADMAP.md) | What's done, what's next, priority matrix |
| [CHANGELOG.md](CHANGELOG.md) | Detailed change history |
| [Principle Lattice](HIVE/docs/PRINCIPLE_LATTICE.md) | The 8 axiomatic principles |
| [State of HIVE](HIVE/docs/STATE_OF_HIVE.md) | Comprehensive feature report + self-assessment |
| [Phase 10 NEXUS](HIVE/docs/PHASE10_NEXUS.md) | Terminal pane architecture + CLI agent integration |
| [Test Health](HIVE/docs/TEST_HEALTH.md) | Test suite baseline (92 Rust, 44 vitest, 0 tsc errors) |
| [Architecture Principles](HIVE/docs/archive/vision/ARCHITECTURE_PRINCIPLES.md) | Provider agnosticism philosophy |
| [Model Modularity Guide](HIVE/docs/archive/vision/MODEL_MODULARITY_GUIDE.md) | How model swapping works |
| [Vision Docs](HIVE/docs/archive/vision/) | Future architecture (hot-swap, multi-agent, brain) |

## Acknowledgments

- Memory system architecture adapted from [OpenClaw](https://github.com/openclaw) (MIT License) — SQLite + vector embeddings + hybrid search patterns
- Documentation maintenance hooks adapted from [vincitamore/claude-org-template](https://github.com/vincitamore/claude-org-template) (MIT License)

## License

MIT

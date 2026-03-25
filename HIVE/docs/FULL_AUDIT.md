# HIVE Full Codebase Audit

**Date:** 2026-03-02
**Auditor:** Claude Opus 4.6 — systematic file-by-file review
**Scope:** Every source file, dependency, architecture decision, and design pattern
**Lines Assessed:** 38,691 across 80+ source files (Rust + TypeScript + React)

---

## PART 1: WHAT IS HIVE?

HIVE (Hierarchical Intelligence with Virtualized Execution) is a **persistent AI orchestration harness** — a Windows desktop application (Tauri v2: Rust backend + React/TypeScript frontend) that coordinates local and cloud LLMs as interchangeable cognitive resources.

**What it is NOT:**
- Not a chatbot wrapper (ChatGPT-clone)
- Not a model runner (Ollama/LM Studio competitor)
- Not a prompt engineering tool
- Not a RAG framework bolted onto a chat UI

**What it IS:**
A *permanent framework* where models are replaceable slots. HIVE provides the skeleton — tool execution, memory persistence, multi-channel I/O, worker orchestration, security gating, specialist routing, autonomous routines — and any LLM (local GGUF, Ollama, OpenAI, Anthropic, OpenRouter, DashScope) fills the cognitive role.

The closest analogy is an **operating system for AI agents**, where:
- Models are processes
- Tools are syscalls
- Memory is the filesystem
- The harness is the kernel
- Remote channels (Telegram/Discord) are I/O ports
- Workers are threads
- Routines are cron jobs

---

## PART 2: WHAT CAN IT DO?

### Core Capabilities (Verified in Code)

| Capability | Implementation | Files | Status |
|---|---|---|---|
| **6 Provider Support** | Local (llama.cpp), Ollama, OpenAI, Anthropic, OpenRouter, DashScope | `providers.rs`, `provider_stream.rs`, `provider_tools.rs` | Working |
| **44 Tools** | File I/O, shell exec, web fetch/search/extract, PDF reader, GitHub, Telegram, Discord, memory CRUD, graph queries, workers, scratchpads, plans, agents, code search, repo clone | `tools/*.rs` (13 tool modules) | Working |
| **Persistent Memory** | SQLite + FTS5 + vector embeddings, hybrid search (BM25 + cosine), dedup, recency decay, chunking with overlap | `memory.rs` (2691 lines) | Working |
| **MAGMA Knowledge Graph** | 4-graph architecture: Semantic (memories), Episodic (events), Entity (tracked objects), Procedural (learned tool chains). Cross-graph typed edges | `magma.rs`, `memory_tools.rs` | Working |
| **Autonomous Workers** | Tokio-spawned sub-agents with own tool registries, wall-clock timeouts, repetition detection, anti-spam report gates, Jaccard dedup | `worker_tools.rs` (1116 lines) | Working |
| **Remote Channels** | Telegram + Discord daemons with Host/User access control, role-based tool gating | `telegram_daemon.rs`, `discord_daemon.rs` | Working |
| **Routines Engine** | Cron + event-triggered standing instructions, message queue with dead-letter, response routing | `routines.rs` (1276 lines) | Working |
| **Cognitive Harness** | Editable identity (HIVE.md), auto-generated capability manifest, skills system, KV-cache-friendly prompt assembly | `harness.rs` (1047 lines) | Working |
| **Multi-Pane Chat** | Up to 4 independent chat panes, each with own model/provider/conversation | `MultiPaneChat.tsx`, `ChatPane.tsx` | Working |
| **Terminal Panes (NEXUS)** | PTY sessions for CLI agents (shell, Claude Code, Codex, Aider), xterm.js frontend, model-to-agent bridge | `pty_manager.rs`, `TerminalPane.tsx`, `agent_tools.rs` | Working |
| **MCP Protocol** | Both server mode (`--mcp` flag for Claude Code integration) and client mode (consume external MCP servers) | `mcp_server.rs`, `mcp_client.rs` | Working |
| **VRAM Intelligence** | GGUF parsing, VRAM estimation, pre-launch compatibility checks, quant walk-down suggestions, MoE expert offload awareness | `gguf.rs` (473 lines) | Working |
| **Security** | AES-256-GCM encrypted secrets, SSRF protection, homoglyph folding, prompt injection detection, content boundary wrapping, tiered tool access | `security.rs`, `content_security.rs` | Working |
| **Cloudflare Tunnel** | Remote MCP/inference access via free trycloudflare.com tunnels. Race-safe, non-blocking, port-validated | `tunnel.rs` | Working |
| **System Tray** | Minimize-to-tray, clean shutdown via tray menu | `main.rs` setup | Working |
| **Plan Execution** | Multi-step tool chaining with variable substitution, conditionals, approval flow | `plan_tools.rs`, `useChat.ts` | Working |
| **Skills System** | Markdown skill files, relevance matching, injection into harness | `harness.rs` | Working |
| **Thinking Depth Control** | Provider-agnostic Off/Low/Medium/High thinking budgets (Anthropic adaptive, OpenAI reasoning_effort, DashScope thinking_budget) | `providers.rs` | Working |
| **Multi-Key Rotation** | Round-robin API key rotation with failover for rate limit resilience | `security.rs` | Working |

---

## PART 3: ARCHITECTURE ASSESSMENT

### 3.1 Scale: AI Slop → Enterprise Standard

**Rating: 7.5/10 — Serious Independent Project, Approaching Professional Grade**

This is not AI slop. The evidence:

1. **Principle-driven development.** The 8-axiom Principle Lattice is not decorative — every file header cites which principles it satisfies. Design decisions reference P1-P8 by number. This is unusual discipline for a project at this stage.

2. **Cross-file contract awareness.** The CLAUDE.md documents 8 explicit cross-boundary contracts (Rust ↔ TypeScript string agreements) with sync methods. Most projects of this size don't even acknowledge this failure mode exists.

3. **Security is structural, not cosmetic.** The Host/User/Desktop origin model with tiered tool gating, SSRF protection, homoglyph folding, content boundary wrapping, and closed-by-default access lists — this is a *designed* security model, not afterthought `if (admin)` checks.

4. **Memory system is research-informed.** Citations to arXiv:2512.05470 (context engineering) and arXiv:2601.03236 (MAGMA graph architecture). Recency decay, deduplication thresholds, context-proportional budgets — these are techniques from the literature, implemented.

5. **Worker system has production patterns.** Wall-clock timeouts, repetition detection (exact + ping-pong), Jaccard dedup on reports, anti-spam gates, dead worker severance. This handles the "LLM stuck in a loop" failure mode that most agent frameworks ignore.

**What keeps it from 9-10:**
- Single-developer bus factor
- No CI/CD pipeline
- Test coverage: 92 Rust tests, 52 vitest tests (see TEST_HEALTH.md for breakdown)
- No integration tests
- No load/stress test automation (manual benchmarks documented but not automated)
- Some god-file tendencies (memory.rs at 2691 lines)

### 3.2 Backend Architecture (Rust/Tauri)

**Strengths:**
- Clean module separation (28 Rust modules, each with a clear purpose)
- `HiveTool` trait provides genuine polymorphism — new tools are 1 struct + 1 trait impl
- Tool registry with sorted deterministic output (enables prompt caching)
- Errors are always `Result<ToolResult, String>` — the model SEES errors, never crashes
- Process cleanup on exit is thorough (servers, PTY sessions, daemons) — deduplicated into `perform_full_cleanup()`, uses tracked PID kills instead of nuclear `taskkill /IM`
- WAL mode on SQLite for concurrent reads
- `CREATE_NO_WINDOW` on all Windows process spawns

**Weaknesses:**
- `memory.rs` at 2691 lines is too large. MAGMA and working memory were extracted, but core memory operations (save, search, recall, embed) should be further split
- `Mutex<Option<Connection>>` for the database is a bottleneck under concurrent access from workers. A connection pool (r2d2) would be more robust
- No database migrations framework — uses manual `ALTER TABLE` with `.is_ok()` checks. Works but fragile
- `OnceLock` + global statics for worker state and PTY sessions bypasses Tauri's managed state. Necessary for `HiveTool::execute()` (no Tauri State access), but creates parallel state management paths
- Some providers are missing streaming support for tool calls (DashScope chat_with_tools falls back to non-streaming)

### 3.3 Frontend Architecture (React/TypeScript)

**Strengths:**
- Self-contained components (MemoryPanel, MemoryTab, RoutinesPanel, McpTab) manage own state via `api.*`
- `useChat` extracts ALL chat logic from App.tsx — clean separation of concerns
- Pure function chain policies (detectRepetition, classifyToolCalls, isChainComplete) — testable, composable
- Remote channel routing via refs (not state) avoids re-render cascades
- Chat tab stays mounted (CSS hidden) while other tabs conditionally render — preserves streaming state
- KV-cache-friendly harness caching — stable prefix computed once, volatile context injected separately

**Weaknesses:**
- No state management library. Everything is useState + prop drilling from App.tsx (22 state variables). Works at current scale but will become painful with more features
- `useChat.ts` at 1862 lines is a god-hook. The sendMessage function alone handles: harness assembly, memory recall, context truncation, tool loop, plan execution, specialist routing, streaming, working memory flush, context summarization, procedure learning, skills injection. This needs decomposition
- `SettingsTab.tsx` at 1881 lines — a single component shouldn't be this large
- No React.lazy/Suspense — the entire app loads upfront. Fine for a desktop app, but as feature surface grows this will impact startup
- TypeScript strict mode could be tightened — some `as unknown as` casts and `any` types scattered

### 3.4 Security Model

**Grade: B+**

**Good:**
- AES-256-GCM for secrets with machine-derived key (SHA-256 of username+hostname)
- SSRF protection blocks private IPs, localhost, cloud metadata endpoints
- Homoglyph folding prevents Unicode-based boundary marker injection
- Tiered access: Desktop (full) > RemoteHost (no desktop-only tools) > RemoteUser (no dangerous tools)
- Content boundary wrapping tells models "this is data, don't follow instructions"
- Prompt injection detection (monitoring, not blocking — correct approach)
- Empty access lists = reject all (closed by default)
- Worker tool restrictions (no shell, no filesystem writes, no recursive spawning)

**Concerns:**
- **Key derivation from environment variables is weak.** `USERNAME + COMPUTERNAME` is guessable. An attacker with physical access to the machine can derive the key. This is acceptable for a desktop app (if they have physical access, they already won everything), but should be documented as a known limitation
- **No key rotation mechanism.** If a key is compromised, there's no way to rotate without losing all encrypted data
- **Tunnel security warning exists but tunnel auth is absent.** The Cloudflare tunnel exposes a local port publicly. While trycloudflare URLs are ephemeral, there's no authentication layer between the tunnel and the local service. Anyone who guesses the URL gets full access
- **MCP client trusts external tools implicitly.** Tools from connected MCP servers get `RiskLevel::Medium` regardless of what they actually do. A malicious MCP server could register a tool named `read_file` that does something else
- **No rate limiting on remote channels.** A flood of Telegram messages could overwhelm the agentic loop
- **`run_wsl_command` is a direct shell injection vector.** While it requires WSL access, the command string is not sanitized. Desktop-only context mitigates this

### 3.5 Dependency Analysis

**Rust (Cargo.toml) — 19 dependencies:**
| Dependency | Purpose | Risk |
|---|---|---|
| tauri 2 + plugins (dialog, shell) | Core framework | Low — maintained by Tauri team |
| reqwest 0.12 | HTTP client | Low — industry standard |
| tokio 1 | Async runtime | Low — de facto standard |
| rusqlite 0.31 (bundled) | SQLite + FTS5 | Low — bundled = no system dep |
| aes-gcm 0.10 | Encryption | Low — RustCrypto, audited |
| rmcp 0.16 | MCP protocol | Medium — newer crate, less battle-tested |
| portable-pty 0.9 | PTY management (Wezterm) | Medium — works but less maintained |
| pdf-extract 0.7 | PDF text extraction | Medium — niche crate |
| scraper 0.20 | HTML parsing | Low |

**TypeScript (package.json) — 7 runtime deps, 8 dev deps:**
| Dependency | Purpose | Risk |
|---|---|---|
| @tauri-apps/api + plugins | Tauri bridge | Low |
| react 18, react-dom 18 | UI framework | Low |
| @xterm/xterm 6 + addons | Terminal emulator | Low — Wezterm lineage |
| lucide-react | Icons | Low |
| react-resizable-panels | Pane layout | Low |
| tailwindcss 3 | Styling | Low |
| vitest | Testing | Low |

**Assessment:** Lean dependency tree. No bloat. The two medium-risk crates (rmcp, portable-pty) are justified by their functionality. No known CVEs in any pinned versions at time of review.

---

## PART 4: BOTTLENECKS & SHORTCOMINGS

### 4.1 Critical

1. **Single Mutex on SQLite.** `MemoryState { db: Mutex<Option<Connection>> }` serializes ALL database access. When 20 workers are running + the main chat + auto-entity tracking + routine evaluation all want the DB, this is a hard bottleneck. Worker tools work around this by opening separate connections for entity tracking (`magma_auto_track_entity` opens its own Connection), but the core memory operations still serialize.
   - **Fix:** r2d2-sqlite connection pool, or migrate to a separate DB file per concern (memory.db, events.db, routines.db)

2. **Backend test gaps.** The Rust crate has 92 tests across security, memory, content_security, harness, providers, routines, pty_manager, and tools modules — but no tests for: tool execution round-trip, worker lifecycle, MCP protocol, GGUF parsing logic. See TEST_HEALTH.md for full inventory.
   - **Fix:** At minimum, add integration tests for the tool registry (execute known tools, verify results) and memory round-trip (save, search, verify recall)

3. **useChat.ts is a 1862-line monolith.** The `sendMessage` function does ~15 distinct things sequentially. A bug in any one section (e.g., memory recall) can break the entire chat flow. This is the #1 source of regression risk.
   - **Fix:** Extract into phases: `buildContext()`, `executeToolLoop()`, `handleSpecialistRouting()`, `flushWorkingMemory()`, each as a separate function or sub-hook

4. **No CI/CD.** 338 commits, no automated build/test/lint pipeline. Manual "run tests before pushing" documented in CLAUDE.md but not enforced.
   - **Fix:** GitHub Actions with `cargo test` + `npx vitest run` + `npx tsc --noEmit` on PR

### 4.2 Significant

5. **Memory system lacks promotion tiers.** The CLAUDE.md documents a 3-tier architecture (working → short-term → long-term) but the code only has working memory + flat long-term. There's no promotion mechanism, no decay-based forgetting, no summarization-based compression. Memories accumulate forever.

6. **No graceful degradation for local model crashes.** If llama-server crashes mid-conversation, the frontend polls `checkServerHealth()` during startup but doesn't monitor during chat. The user sees a cryptic error from the failed HTTP request.
   - **Fix:** Periodic health check (every 30s) with auto-reconnect or clear error toast

7. **Conversation persistence is localStorage-based.** Conversations are JSON-serialized into localStorage. This has a ~5-10MB limit depending on browser. Long conversations with tool results will hit this ceiling silently.
   - **Fix:** Migrate to IndexedDB or persist via Tauri filesystem commands

8. **Provider status checks are sequential.** `loadProviders()` checks each provider status serially (the `for` loop on line 221 of App.tsx). With 5 providers configured, startup includes 5 sequential API calls. Should be `Promise.all()`.

9. **No input validation on tool arguments.** Tools receive `serde_json::Value` and parse manually. If the model sends malformed arguments, each tool handles errors differently. A schema validation layer (using the declared JSON Schema) would standardize this.

### 4.3 Minor

10. **Dead code in provider_tools.rs.** Anthropic tool handling duplicates logic from `provider_stream.rs`. These should share a common parsing layer.

11. ~~**`which` command for cloudflared detection doesn't work on Windows.**~~ **FIXED** (Mar 2026, `fix/audit-findings`). `which_cloudflared()` now uses platform-conditional `where`/`which` with `CREATE_NO_WINDOW` on Windows.

12. **VRAM estimation is approximate.** The GGUF-based estimation is a heuristic (file size proxy for weights + KV cache formula). It's useful but can be 10-30% off for quantized models, especially with GQA/MQA heads.

13. **No dark/light theme toggle.** The zinc/amber theme is hardcoded. Low priority but noted as a UX gap.

---

## PART 5: ADVANTAGES & NOVELTIES

### What HIVE Does That Competitors Don't

| Feature | HIVE | LM Studio | Ollama | Open WebUI | TypingMind | ChatGPT |
|---|---|---|---|---|---|---|
| **Provider-agnostic orchestration** | 6 providers, any model fills any slot | Local only | Local only | Ollama wrapper | Cloud only | OpenAI only |
| **Persistent multi-graph memory** | SQLite + FTS5 + MAGMA (4 graphs) | None | None | Basic RAG | None | Limited |
| **Autonomous workers** | N parallel sub-agents with tool access | No | No | No | No | No |
| **Integrated remote channels** | Telegram + Discord with access control | No | No | No | No | No |
| **Standing routines (cron + events)** | Full cron engine + event matching | No | No | No | No | No |
| **Terminal agent bridge** | PTY sessions for Claude Code/Codex/Aider | No | No | No | No | No |
| **MCP server + client** | Both directions (expose + consume) | No | No | No | No | No |
| **Security-gated tool execution** | Tiered: Desktop > Host > User | N/A | N/A | Basic | N/A | Limited |
| **VRAM-aware model management** | GGUF parsing, pre-launch checks, suggestions | Yes | No | No | N/A | N/A |
| **Cognitive harness (editable identity)** | HIVE.md + auto-capabilities + skills | No | No | Partial | No | Custom instructions |
| **Plan execution (tool chaining)** | Multi-step with variables + conditionals | No | No | No | No | No |

### Genuine Novelties

1. **The "Framework Survives" principle (P7) as architecture.** Most AI tools bind tightly to specific models or providers. HIVE treats the model as a replaceable component within a permanent framework. This is not just a design aspiration — the code actually enforces it (provider abstraction, slot system, any model can fill any specialist role).

2. **MAGMA graph integration in a desktop app.** Multi-graph memory architectures exist in research papers, but implementing episodic + entity + procedural + semantic graphs with cross-graph typed edges in a consumer desktop app is rare. The auto-entity tracking (file/command/URL entities created passively on tool execution) is particularly clever — it builds the knowledge graph without the model needing to explicitly do anything.

3. **Worker anti-spam gates.** The combination of Jaccard dedup, cooldown timers, progress gates, and repetition detection on worker reports is a production-grade solution to a problem (LLM agents spamming their orchestrator) that most frameworks handle with crude turn limits.

4. **Chain policies as composable pure functions.** The tool loop in useChat.ts uses `detectRepetition()`, `classifyToolCalls()`, `isChainComplete()` as independent, testable policies. Adding a new policy doesn't touch the loop body. This is good software engineering.

5. **KV-cache-friendly harness assembly.** The system prompt is split into a stable prefix (identity + capabilities + tools — identical across turns) and a volatile suffix (turn count, VRAM, metrics — changes every turn). llama.cpp's KV cache matches on token-level prefix, so keeping the prefix stable gives free performance. This level of inference-aware prompt engineering is uncommon.

---

## PART 6: "IS-STATE" — Where HIVE Actually Is Right Now

### Honest Capability Matrix

| Dimension | Claimed | Actual | Gap |
|---|---|---|---|
| Provider agnosticism | 6 providers, all coequal | 6 providers work, but tool calling varies by provider (Anthropic native, others OpenAI-format compat). DashScope streaming needs polish | Small |
| Memory system | 3-tier with promotion | 2-tier (working + flat long-term). No promotion, no decay, no summarization. Memory accumulates forever | Significant |
| MAGMA graph | Full multi-graph with traversal | Schema and tools exist. Auto-entity tracking works. But the graph is sparse — most edges are auto-generated, not semantically rich | Moderate |
| Workers | 20+ concurrent, production-ready | Stress-tested at 20 (85% completion). Infrastructure solid. But workers can't write files or run commands, limiting their utility to analysis tasks | By design (P6) |
| Routines | Full autonomous agency | Cron + event triggers work. But routines are fire-and-forget — no feedback loop, no conditional chaining, no retry with backoff | Moderate |
| Security | "Military-grade OPSEC" | AES-256-GCM encryption is real. But key derivation is environment-variable-based, tunnel has no auth, and there's no rate limiting | Moderate |
| Multi-pane | Up to 4 panes | Works, but each pane is fully independent — no cross-pane communication, no shared context | By design |
| Test coverage | Tests exist | 92 Rust tests (8 modules), 52 vitest tests (9 suites). No integration tests, no e2e tests | Significant gap |

### Technical Debt Inventory

1. **No migration framework** — manual ALTER TABLE in init_db()
2. **localStorage for conversations** — will hit size limits
3. **Sequential provider status checks** — should parallelize
4. **No health monitoring for running models** — silent failure mode
5. **`useChat.ts` monolith** — regression risk
6. **Global statics bypassing Tauri state** — parallel state management
7. **No input sanitization on WSL commands** — acceptable in desktop context
8. **Tunnel lacks authentication** — security gap if URL is discovered

---

## PART 7: FUTURE-PROOFING & DIRECTION

### What's Naturally Set Up for the Future

1. **Multi-model orchestration.** The slot system (consciousness, coder, terminal, webcrawl, toolcall) + specialist routing + VRAM budget management is already architected for multiple models running simultaneously. The infrastructure exists; it just needs VRAM-aware auto-sleep/wake and cross-model routing.

2. **MCP as the integration layer.** HIVE can both expose its tools (server mode) and consume external tools (client mode). As the MCP ecosystem grows, HIVE becomes a hub.

3. **Agent-as-a-service.** The PTY bridge + terminal panes means HIVE can orchestrate Claude Code, Codex, Aider, or any CLI tool. The `send_to_agent` tool lets the model delegate to other agents. This positions HIVE as a meta-agent orchestrator.

4. **Graph-based memory for long-term learning.** MAGMA's multi-graph schema is forward-compatible. Today it stores basic entities and events; tomorrow it can support semantic clustering, concept formation, and procedural mastery tracking.

### Recommended Future Directions (Priority Order)

1. **CI/CD + Test Suite** — The highest-leverage investment. Cover the happy paths for: tool execution, memory round-trip, provider routing, worker lifecycle. This unlocks fearless refactoring.

2. **useChat.ts decomposition** — Break the monolith into phases. This is the #1 regression risk and the #1 barrier to contribution.

3. **SQLite connection pool** — Replace `Mutex<Option<Connection>>` with r2d2-sqlite. Unblocks concurrent worker + main chat database access.

4. **Memory tier promotion** — Implement short-term → long-term with reinforcement-based promotion and decay-based forgetting. Without this, memory grows unbounded.

5. **Model health monitoring** — Periodic health checks with auto-reconnect and clear error UI. Silent model crashes are a bad UX.

6. **Tunnel authentication** — Add a shared secret or token-based auth layer to the Cloudflare tunnel endpoint.

### Use Cases (Current & Adjacent)

**Current (validated):**
- Power user running local models with cloud fallback
- Remote HIVE access via Telegram/Discord
- Parallel research tasks via workers
- Scheduled autonomous actions via routines
- Multi-agent workflows (HIVE + Claude Code + Shell)

**Adjacent (enabled by current architecture):**
- Team deployment (tunnel + access control = shared AI assistant)
- Continuous monitoring (routines + web tools = always-on intelligence)
- Knowledge base curation (RAG import + MAGMA graph = structured organizational memory)
- Dev environment orchestration (PTY bridge + agent tools = AI-powered IDE integration)

---

## PART 8: PERSONAL ASSESSMENT

### What Impresses Me

1. **The CLAUDE.md is the best project context document I've reviewed.** It's not just instructions — it's an institutional memory. The "Common Session Traps" section alone prevents more bugs than most test suites. The cross-file contract table is gold.

2. **The architecture is opinionated in the right ways.** Provider agnosticism as a first principle (not an afterthought), memory as session-injection (not system prompt mutation), security as tiered access (not binary admin/user) — these are decisions that reflect understanding of the problem space.

3. **The worker system is genuinely novel for a desktop app.** 20 concurrent autonomous sub-agents with anti-spam, dedup, repetition detection, and wall-clock timeouts — this is infrastructure that cloud AI platforms struggle with, built in ~1100 lines of Rust.

4. **The principled approach to code quality.** "Fix ALL instances of a pattern", "Check git history before fixing", "No cross-file string contracts without a shared source" — these aren't aspirational. The code shows evidence of following them (channelPrompt.ts, cross-ref comments, pattern-wide fixes in commit history).

### What Concerns Me

1. **Bus factor of 1.** 52 commits from the owner, 286 from Claude. If the owner steps away, no one else can maintain this without the CLAUDE.md.

2. **The gap between documented architecture and implemented code.** The memory system documentation describes a 3-tier system with promotion, summarization, and semantic categorization. The code has 2 tiers with flat accumulation. The CLAUDE.md is honest about this ("MISSING" markers), but new contributors might expect the documented system to exist.

3. **Testing philosophy is "test in production."** The TEST_HEALTH.md exists, the cargo test + vitest commands are documented, but coverage is ~5% of the codebase at best. For a system that manages encrypted secrets, executes shell commands, and controls autonomous agents — this is a risk.

4. **Scale ceiling is real.** The architecture works beautifully at its current scale (single user, 1-4 panes, 20 workers). But there's no path to multi-user, no distributed state, no horizontal scaling. This is correct for a desktop app — but should be acknowledged as a ceiling, not a bug.

### Final Verdict

**HIVE is a 7.5/10 — the most architecturally thoughtful single-developer AI orchestration project I've assessed.** It's not enterprise software (no CI, limited tests, bus factor of 1), but it's not trying to be. It's a *framework that takes the problem seriously*: provider agnosticism, persistent memory, security-first remote access, autonomous agents, principled design.

The gap between HIVE and enterprise-grade is primarily operational (testing, CI, monitoring), not architectural. The bones are good. The foundation supports the ambition. What it needs most is not more features — it's infrastructure to protect the features it already has.

**Recommendation:** Before adding ANY new capability, invest in:
1. CI/CD pipeline
2. Integration test suite covering tool execution + memory + providers
3. useChat.ts decomposition
4. SQLite connection pool

These four changes would move HIVE from 7.5 to 8.5+ and make every future feature safer to build.

---

## PART 9: AUDIT FIX LOG

### March 9, 2026 — `fix/audit-findings` branch (6 commits)

Branch created off `claude/analyze-repo-branches-eornG`. Fixes 4 critical, 4 high, 6 medium, and 12 low findings from this audit. Security-first commit ordering.

**Commit 1: Path Traversal + Import Hardening (P6)**
- `harness_read_skill`: blocked path traversal via `../../secrets` — now validates name (rejects `..`, `/`, `\`, `\0`) and canonicalizes + verifies containment within skills directory
- `MemoryImportFileTool`: bumped `RiskLevel::Medium` → `RiskLevel::High` (arbitrary file reads into persistent DB)
- Added home-directory sandbox check — file imports blocked outside `~` and CWD
- Added `memory_import_file` to `WORKER_BLOCKED_TOOLS` (workers cannot import files)
- Made `ImportSection`, `split_markdown_by_headings`, `split_file_into_sections` `pub(crate)` for reuse

**Commit 2: Remove Dead Updater + Harden Tunnel (P6)**
- Removed `tauri-plugin-updater` entirely (empty pubkey, no release infrastructure) — config, capability, plugin init, Cargo dependency all deleted
- `tunnel.rs` near-full rewrite:
  - Static `OnceLock<Regex>` (compiled once, not per-call)
  - Platform-conditional finder: `where` on Windows, `which` on Unix (was trying `which` first on Windows → error + latency)
  - `CREATE_NO_WINDOW` on all spawned processes (cloudflared + finder)
  - Race condition fixed: sentinel-based URL mutex prevents concurrent `tunnel_start` calls from spawning duplicate processes
  - Blocking I/O fixed: stderr reader loop wrapped in `tokio::task::spawn_blocking` (was blocking async runtime)
  - Stderr pipe auto-cleanup: `BufReader` + `stderr` handle moved into blocking closure, dropped on scope exit
  - Port validation: reject port 0, warn on privileged ports (<1024)

**Commit 3: Process Safety — Cleanup Dedup + PID Tracking (P4, P5)**
- Extracted `perform_full_cleanup(app)` shared function — deduplicates identical cleanup logic from tray quit handler and window close handler
- Replaced `expect("no app icon")` panic with graceful fallback: logs warning, uses 1x1 transparent pixel
- Added `spawned_pids: Mutex<HashSet<u32>>` to `AppState` — records PIDs on every server spawn (main, WSL, specialist native, specialist WSL)
- Replaced nuclear `taskkill /F /IM llama-server.exe` (killed ALL llama-server.exe system-wide) with per-PID `taskkill /F /PID <pid>` — only kills HIVE's own processes
- PIDs cleaned from set on graceful stop (`stop_server_internal`, `stop_specialist_server_internal`)

**Commit 4: Memory Import Quality (P3, P5)**
- Added 10MB file size limit with actionable error message
- Replaced byte-level chunking (`content.as_bytes().chunks(1600)` → `String::from_utf8_lossy`) with heading-aware splitting via `split_markdown_by_headings` / `split_file_into_sections` — eliminates UTF-8 corruption on emoji/CJK text
- Added embedding computation per section via `try_get_embedding()` (was completely missing — imports had no vector embeddings)
- Replaced raw `INSERT OR REPLACE` with `write_memory_internal()` — proper FTS5 indexing, chunking with overlap, deduplication
- Added `source_file` attribution via UPDATE after each write (for RAG citation)

**Commit 5: Frontend Fixes (P5, P8)**
- `MemoryTab.tsx`: synced file dialog filter with Rust `text_extensions` allowlist — removed unsupported `pdf`, added all 30 supported extensions, added `All Files (*)` fallback. Comment cross-references `memory_tools.rs`
- `McpTab.tsx`: added `min={1} max={65535}` to tunnel port input, clamped value in onChange handler — prevents port 0, 99999, or negative values
- `magma_save_procedure`: upsert by name — existing procedure with same name gets `success_count` incremented and description/steps updated instead of creating a duplicate row. Fixes unbounded procedure accumulation from repeated tool chains

**Commit 6: Documentation Accuracy**
- Tauri command counts: 141/135 → 148 across CLAUDE.md, ROADMAP.md
- Tool counts: 41+/42/43/45 → 44 across ROADMAP.md, STATE_OF_HIVE.md, FULL_AUDIT.md
- Orchestrator 4.3: DONE → PARTIAL (classify_task + plan_vram are internal to route_task, which is exported but never called from frontend)
- Test counts in FULL_AUDIT.md synced with TEST_HEALTH.md (92 Rust, 52 vitest)
- Full documentation update with fix descriptions (this section)

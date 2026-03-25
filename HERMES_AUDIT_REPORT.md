# HIVE/HiveMind Audit Report
## Branch: fix/audit-findings
## Auditor: Hermes Agent (Claude Opus 4) — March 23, 2026
## Scope: Full codebase — 46 Rust files (29,132 LOC), 43 TS/TSX files (15,819 LOC)

---

## CRITICAL — Fix Before Any Public Release

### SEC-1: Encryption Key Derivation is Trivially Breakable
**File:** `HIVE/desktop/src-tauri/src/security.rs`
**What:** Key derivation uses `USERNAME + HOSTNAME` only. Anyone with file access to `~/.hive/secrets.enc` can derive the decryption key in microseconds. This stores all API keys (OpenAI, Anthropic, OpenRouter, DashScope, Discord, Telegram tokens).
**Impact:** Total credential compromise if machine is shared, backed up to cloud, or accessed by malware.
**Fix:** Use Windows Credential Manager via `keyring` crate or `windows-sys` crate to store the master key. Fallback: prompt user for a passphrase on first run, derive key with Argon2id. Never derive from guessable machine metadata.
**Effort:** Medium (1-2 sessions). Requires migration path from old key to new key.

### BUG-1: Undefined Behavior — unsafe set_var()
**File:** `HIVE/desktop/src-tauri/src/memory.rs` line ~731
**What:** `unsafe { std::env::set_var("ORT_DYLIB_PATH", ...) }` is UB since Rust 1.66 when called after threads exist. Tauri spawns threads before this runs.
**Impact:** Potential crash, memory corruption. Undefined behavior means anything can happen.
**Fix:** Set the env var in `main()` before `tauri::Builder::default()`, or use `OnceLock<PathBuf>` and pass it to the ONNX runtime config directly instead of via env var.
**Effort:** Low (15 minutes).

### BUG-3: Garbled/Redacted Syntax in Committed Code
**Files:**
- `HIVE/desktop/src/useChat.ts` lines 554, 648
- `HIVE/desktop/src/components/ChatTab.tsx` lines 259, 291
- `HIVE/desktop/src/lib/api.ts` lines 1330, 1341, 1345, 1350, 1369
**What:** Multiple lines contain `=***` and `=estima...ens(` patterns. Appears to be a commit hook or tool that censored token estimation code. The committed code on this branch has broken syntax that would fail to compile/run.
**Impact:** This branch may not build. If it does build, these functions produce wrong results.
**Fix:** Check git history for the original code. If a pre-commit hook is censoring, fix the hook regex. Restore the actual `api.estimateTokens(...)` calls.
**Effort:** Low (30 minutes to trace and fix).

---

## HIGH — Fix Soon

### SEC-2: API Keys Not Zeroed in Memory
**File:** `HIVE/desktop/src-tauri/src/security.rs`
**What:** Decrypted API keys live in `HashMap<String, String>`. Rust's `String` is not zeroed on drop. Keys persist in process memory until the OS reclaims the page.
**Fix:** Use `secrecy::SecretString` from the `secrecy` crate. Implements `Zeroize` on drop.
**Effort:** Medium (touch every callsite that reads a key).

### SEC-3: Workers Can Poison Persistent Memory
**File:** `HIVE/desktop/src-tauri/src/tools/worker_tools.rs`
**What:** Workers can call `memory_save`. A hallucinating worker writing false facts to persistent memory corrupts the knowledge base permanently.
**Fix:** Either block `memory_save` for workers (add to BLOCKED_TOOLS), or add a `source: "worker"` tag to worker-created memories with lower trust weight in search ranking.
**Effort:** Low (add to blocked list) or Medium (trust tagging).

### BUG-2: TOCTOU Race on Secrets File
**File:** `HIVE/desktop/src-tauri/src/security.rs`
**What:** `load_secrets()` and `save_secrets()` have no file locking. Concurrent access (main thread + worker storing keys) can corrupt the file.
**Fix:** Use `fs2::FileExt::lock_exclusive()` around file operations, or use atomic write (write to temp file, then rename).
**Effort:** Low (30 minutes).

### BUG-4: Non-Atomic Memory Edit
**File:** `HIVE/desktop/src/hooks/useMemoryBrowser.ts`
**What:** `handleSaveEdit` does `memory_delete` then `memory_save` (two separate calls). If `memory_save` fails after `memory_delete` succeeds, the memory entry is lost.
**Fix:** Add a `memory_update` Tauri command that does both in a single SQLite transaction. Or at minimum, reverse the order: save new first, delete old second.
**Effort:** Low (1 hour).

### BUG-5: File Upload Memory Bomb
**File:** `HIVE/desktop/src/components/ChatTab.tsx`
**What:** `handleFilesUpload` reads entire file into `Uint8Array` then converts to `number[]`. This roughly doubles memory usage. 50MB limit means ~100MB+ RAM per upload.
**Fix:** Stream the file to the Rust backend via Tauri's `fs` plugin or chunked IPC. Don't convert `Uint8Array` to `number[]` — pass the typed array directly.
**Effort:** Medium (requires Tauri IPC change).

### PERF-1: Linear Vector Search
**File:** `HIVE/desktop/src-tauri/src/memory.rs`
**What:** `search_vector` scans ALL chunk embeddings sequentially. No approximate nearest neighbor index.
**Impact:** Latency grows linearly with memory count. At 10K+ memories, every search adds noticeable delay to every chat turn (since memory recall runs on each turn).
**Fix:** Add an in-memory HNSW index (use `instant-distance` or `hnsw_rs` crate). Rebuild on startup from SQLite, update incrementally on save. Or use SQLite's upcoming vector search extension.
**Effort:** High (2-3 sessions). Requires index lifecycle management.

### PERF-3: Single Mutex Bottleneck on Memory DB
**File:** `HIVE/desktop/src-tauri/src/memory.rs`
**What:** Global `Mutex<Option<Connection>>` serializes ALL memory operations. Workers + main chat + recall all contend on one lock.
**Fix:** Use `r2d2` connection pool with SQLite WAL mode, or at minimum use `RwLock` (multiple concurrent readers, exclusive writers). Better: move memory ops to a dedicated thread with an mpsc channel.
**Effort:** High (architectural change).

---

## MEDIUM — Should Fix

### PERF-2: N+1 SQL Updates in Memory Reinforcement
**File:** `HIVE/desktop/src-tauri/src/memory.rs`
**What:** `search_hybrid` does per-result SQL UPDATE for `access_count` and `strength`. N search results = N UPDATE statements.
**Fix:** Batch into a single `UPDATE memories SET access_count = access_count + 1, strength = MIN(1.0, strength + 0.05) WHERE id IN (?, ?, ?, ...)`.
**Effort:** Low (30 minutes).

### PERF-4: Skill/Specialist Vectors Never Update
**Files:** `harness.rs` (SKILL_VECTORS), `orchestrator.rs` (SPECIALIST_VECTORS)
**What:** Computed once via `OnceLock` on first access. Adding a new skill or changing specialist config requires app restart.
**Fix:** Replace `OnceLock` with `RwLock`. Add a `harness_reload_skills` command that recomputes vectors. Call it after skill file changes.
**Effort:** Low-Medium (1 hour).

### PERF-5: Sequential Provider Status Checks
**File:** `HIVE/desktop/src/App.tsx`
**What:** `loadProviders` loops through providers with `await checkProviderStatus()` sequentially. 6 providers × network timeout = slow startup.
**Fix:** Use `Promise.all()` or `Promise.allSettled()` to check all providers in parallel.
**Effort:** Low (15 minutes).

### PERF-6: localStorage as Database
**File:** `HIVE/desktop/src/lib/api.ts`
**What:** Conversations, settings, layouts, routing stored in localStorage. ~5MB browser limit. Large/many conversations will hit ceiling silently.
**Fix:** Move conversations to SQLite (you already have it for memory). Keep settings in localStorage (small). Or use Tauri's `store` plugin.
**Effort:** Medium (1-2 sessions).

### SEC-4: Unauthenticated Daemon Commands
**Files:** `telegram_daemon.rs`, `discord_daemon.rs`
**What:** Bot accepts commands from anyone who can message it. Access control is via allowed_users/allowed_groups lists, but if those are empty or misconfigured, the bot is open.
**Fix:** Default to deny-all if allowed_users is empty (fail-closed). Add a warning log when daemon starts with empty access lists.
**Effort:** Low (30 minutes).

### SEC-5: Cloudflare Tunnel No Auth
**File:** `HIVE/desktop/src/components/McpTab.tsx`
**What:** MCP tunnel exposes server without authentication.
**Fix:** Generate a random bearer token on tunnel creation, require it in MCP requests. Display token to user for client configuration.
**Effort:** Medium (1 session).

---

## LOW — Nice to Have

### ARCH-1: useChat.ts is 1,889 Lines
**File:** `HIVE/desktop/src/useChat.ts`
**What:** `sendMessage()` alone is ~1,300 lines with 7+ levels of nesting. The specialist cloud routing duplicates logic from the main tool loop.
**Fix:** Extract into focused functions:
- `buildApiMessages()` — context assembly
- `executeToolLoop()` — the agentic tool loop
- `handleStreamingResponse()` — SSE processing
- `handleChannelGuarantee()` — Telegram/Discord delivery
- `routeToSpecialist()` — specialist cloud path
Each can be a separate file in a `chat/` directory, imported by useChat.
**Effort:** High (2-3 sessions, risky refactor).

### ARCH-2: Prop Drilling (20+ Props Through 3 Levels)
**Files:** `App.tsx` → `MultiPaneChat` → `ChatPane` → `ChatTab`
**Fix:** Create React Contexts:
- `AppSettingsContext` (model, settings, provider statuses)
- `SystemInfoContext` (hardware, VRAM, dependencies)
- `ProviderContext` (available providers, cloud keys)
**Effort:** Medium (1-2 sessions).

### ARCH-3: No React Error Boundaries
**What:** Any component crash takes down the entire app.
**Fix:** Wrap each tab in an ErrorBoundary component with a "Something went wrong — click to retry" fallback. React's `componentDidCatch` or use `react-error-boundary` package.
**Effort:** Low (1 hour).

### ARCH-4: Result<T, String> Everywhere
**What:** All Tauri commands return `String` errors. No structured error types.
**Fix:** Create a `HiveError` enum with `thiserror`. Categories: `Io`, `Db`, `Provider`, `Security`, `Validation`, `NotFound`. Implement `Into<InvokeError>` for Tauri.
**Effort:** High (touches every command, but can be done incrementally).

### ARCH-5: Telegram/Discord Controls Duplication
**Files:** `TelegramDaemonControls.tsx` (270 lines), `DiscordDaemonControls.tsx` (270 lines)
**What:** Near-identical components.
**Fix:** Generic `DaemonControls<T>` component parameterized by platform. Pass platform-specific labels, API functions, and config shape as props.
**Effort:** Low (1-2 hours).

### ARCH-6: CapabilitySnapshot God Object
**File:** `HIVE/desktop/src-tauri/src/harness.rs`
**What:** 30+ fields with `#[serde(default)]`. Growing unbounded.
**Fix:** Split into sub-structs: `HardwareSnapshot`, `MemorySnapshot`, `ProviderSnapshot`, `ToolSnapshot`. CapabilitySnapshot composes them.
**Effort:** Low-Medium (1 hour + update all consumers).

### ARCH-7: Global State via OnceLock
**What:** WORKERS, SESSIONS, SKILL_VECTORS are global statics. Hard to test, hidden coupling.
**Fix:** Move to Tauri managed state (`app.manage()`). Already done for some state (AppState, MemoryState). Consistency pass to migrate the rest.
**Effort:** Medium (1-2 sessions).

---

## DEAD CODE — Remove

| Item | File | Action |
|------|------|--------|
| `extract_keywords_frequency` | memory.rs | Remove (YAKE replaced it) |
| `QueueStatus::as_str` | routines.rs | Remove |
| `SpecialistServer::slot_role` | state.rs | Remove |
| `openModelSelector` (no-op) | MultiPaneChat.tsx | Remove or implement |

---

## SUMMARY

| Severity | Count |
|----------|-------|
| Critical | 3 |
| High | 5 |
| Medium | 6 |
| Low | 7 |
| Dead Code | 4 |
| **Total** | **25** |

Estimated total fix effort: ~15-20 Claude Code sessions for everything.
Minimum viable fix (Critical + High only): ~6-8 sessions.

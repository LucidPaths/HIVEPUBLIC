# HIVE Codebase Audit Log

**Branch:** `fix/audit-findings` (consolidated from `claude/codebase-audit-2USJB` + 2 other session branches)
**Period:** 2026-03-10 → 2026-03-14
**Scope:** ~42K lines (26K Rust, 15K TypeScript), 50+ source files

---

## Merge Note for Other Sessions

This branch touches 3 files that the main working branch also modifies. When merging, expect conflicts in:

| File | What this branch changed | Resolution |
|------|--------------------------|------------|
| `CLAUDE.md` | Added quality gate (mandatory pre-commit checklist), cross-file contract table, coding standards 10/11, memory architecture docs, Claude Working State section, updated file table | Take whichever version has more entries — both branches add to the same tables. The quality gate and contract table from this branch are new sections, not modifications of existing ones |
| `HIVE/docs/STATE_OF_HIVE.md` | Rewrote to Report #4 covering Phase 5 cognitive bus, audit results, test tripling | Compare dates — keep the more recent report. If main also updated, merge the "What Changed" sections |
| `HIVE/docs/TEST_HEALTH.md` | Updated baselines to 225 Rust / 103 vitest / 0 tsc errors | Take the higher test counts. If main added new tests too, add both |

Everything else on this branch is **new files or modifications to files that main didn't touch** — no conflicts expected. The bulk of the work is in Rust (`src-tauri/src/`) and TypeScript (`src/`) source files: audit fixes, Phase 4C/4D memory tiers, Phase 5 cognitive bus, agent bridge, harness identity upgrade, and ~130 new tests.

**Also on this branch (2026-03-13):** `claude-starter-kit/` upgraded — new `/codebase-audit` skill, `WORKING_STATE` session-transcending memory pattern, upgraded quality gate with lessons-learned table, upgraded hooks (commit tracking, working state loading). All model-agnostic. 7 files changed, no conflicts with main.

---

## Audit & Fixes

### Audit #1 (2026-03-10)
- 6 parallel domain-specific agents, line-by-line analysis
- **92 findings**: 2 critical, 6 high, 30 medium, 38 low, 16 info
- Both criticals fixed: Google Fonts CDN leak (P6), `run_wsl_command` shell injection
- 62 findings fixed across 41 files in initial pass
- Follow-up: 22 deferred findings + P5 root-cause sweep

### Audit #2 (2026-03-11)
- 7 parallel agents, second full pass after fixes applied
- **100 findings** (after dedup + design review): 0 critical, 12 high, 38 medium, 30 low, 20 info
- 4 findings accepted by design (file tools sandbox, run_command, encryption key derivation, MCP ungated)
- Cross-file contracts: ALL 6 verified IN SYNC
- SettingsTab monolith split (1918 → 617 + 1302 lines in settings/)

### Audit #3 (2026-03-14)
- 8 parallel agents, third full pass after branch consolidation (3 branches cherry-picked into `fix/audit-findings`)
- **24 findings**: 0 critical, 0 high, 11 medium, 13 low
- All 11 MEDIUM fixed: github_tools.rs JSON parse errors (7x P5 sweep), memory.rs iterator unwrap, web_tools.rs JSON-LD + SearXNG retry, scratchpad metadata dead code, MemoryTab O(n²) lookup, McpTab setTimeout leak, CLAUDE.md contract table
- All 13 LOW fixed: React index keys, memoization, silenced ptyWrite catch, misleading test comment, timezone docs, empty section placeholders, TEST_HEALTH stale count, starter-kit hook divergence docs
- **Remaining 32 open findings from Audit #2 verified**: 29 already fixed by other sessions, 3 fixed in this pass (S7, B4, DOC2)
- P5 root-cause sweep: all 8 fix patterns confirmed eradicated across full codebase
- **Result: 0 open findings. All 32 resolved.**

### Code Quality Fixes (across all audits)
- SQLite PRAGMA standardized across all 8 connection sites (WAL mode, journal_size_limit, busy_timeout)
- useChat.ts decomposed — pure functions extracted to `chainPolicies.ts`
- Memory reinforcement pipeline fixed (SQLite bundled build lacks `ln()`)
- UTF-8 byte-slicing fixed everywhere (`.chars().take(N)` pattern)
- Dead code removal, React pattern fixes, silent error suppression replaced with logging
- Conversation persistence preserves all Message fields (thinking, toolCalls, toolCallId)
- PTY spawn validates command is simple program name (no paths/metacharacters)

---

## Features Built

### Phase 4C/4D — Memory Improvements
- **Memory tier system** (`memory.rs`): short-term → long-term promotion based on access_count thresholds
- **Context summarization** (`memory.rs`, `providers.rs`): model-based summarization before context truncation, replaces naive truncation
- Tier scoring with recency decay, quality filtering, dedup (cosine > 0.92)

### Phase 5 — Cognitive Bus
- **5A/5B**: Unified identity for specialists — all models share HIVE.md identity + `read_agent_context` tool for cross-model awareness
- **5C**: Context bus (`context_bus_write`/`context_bus_summary`) — shared activity stream across all agents
- **5D**: Workers inherit HIVE identity, can write to bus, `slot_role` param for using specialist model configs

### Phase 11 (partial) — Agent Bridge
- Silence-based response detection + automatic injection into orchestrator chat
- 5-gate delivery: content exists, 5s silence, 10s rate limit, 50-char minimum, 0.70 Jaccard dedup
- Full implementation: Rust (BridgeState, monitor thread, emit) + TS (listener, channelPrompt, tool access)
- PTY input newline translation fix

### Harness Identity Upgrade
- Analyzed 6 competitor AI agent system prompts (Devin, Cursor, Augment, Manus, Kiro, Junie)
- Integrated 8 behavioral patterns into DEFAULT_IDENTITY (scope discipline, parallel tools, anti-flattery, verify-before-claiming, circuit breaker, test integrity, read-before-write, status-over-silence)
- Token budget: ~280 → ~350 tokens (compressed for local model compatibility)
- Upgraded seed skills (`coding.md` with debugging patterns, new `troubleshooting.md`) — CLAUDE.md-derived behavioral wisdom moved to contextual skills instead of always-on identity

### Testing Infrastructure
- 84 new Rust tests (92→176→225)
- 44 new vitest tests (52→96→103)
- CI pipeline (GitHub Actions)
- E2E manual test plan (`E2E_TEST_PLAN.md`)

---

## Accepted By Design

These were flagged but are correct per the HIVE threat model:

| Finding | Reasoning | Principle |
|---------|-----------|-----------|
| File tools have no path sandbox | Desktop user IS the owner. Remote blocked by DESKTOP_ONLY_TOOLS | P8 + P6 |
| run_command allows arbitrary shell | High risk + approval prompt + DESKTOP_ONLY_TOOLS | P8 + P6 |
| Encryption key from USERNAME+COMPUTERNAME | No passphrase = P8 Low Floor. File ACLs are primary gate | P8 vs P6 |
| MCP server exposes all tools ungated | User explicitly runs `--mcp`. Trust boundary is parent process | P8 |

---

## Verified Positive Findings

- All 6 cross-file contracts IN SYNC (DANGEROUS_TOOLS, DESKTOP_ONLY_TOOLS, SPECIALIST_PORTS, SenderRole, ThinkingDepth, Events)
- Zero SQL injection — every query uses parameterized statements
- Zero XSS — no `dangerouslySetInnerHTML`
- SSRF protection consistent on web_fetch, web_extract, read_pdf, MCP HTTP
- All string truncation uses `.chars().take(N)` (workspace_tools.rs fixed — uses `strip_prefix`/`strip_suffix`)
- Closed-by-default security — empty host/user lists reject all remote messages
- Clean CSP — no unsafe-eval
- No credential exposure — API keys via OS keyring, never in localStorage or logs

---

## Open Findings (unfixed)

Priority order: security → crashes → silent failures → UX → races → dead code → React → quality → docs.

_All findings below were open as of Audit #2. Status updated Mar 14, 2026._

### All Clear

**All 32 open findings have been resolved.** See "Resolved Findings" below for the full list.

---

## Resolved Findings

### Security (7/7 fixed)

| ID | File | Issue | Resolution |
|----|------|-------|------------|
| S1 | `agent_tools.rs` | `send_to_agent` risk level Medium → High | Risk level set to High |
| S2 | `web_tools.rs` | `web_extract` risk level Low → Medium | Risk level set to Medium |
| S3 | `download.rs` | WSL download path traversal | `shell_escape()` on path and URL |
| S4 | `discord_tools.rs` + `discord_daemon.rs` | Discord channel_id unvalidated | `validate_snowflake()` in both locations |
| S5 | `memory.rs` | FTS5 query injection via double-quotes | `.replace('"', "")` before wrapping |
| S6 | `telegram_tools.rs` | Bot token in reqwest error messages | `sanitize_api_error()` on error output |
| S7 | `pty_manager.rs` | PTY spawn accepts arbitrary commands | Rejects path separators + shell metacharacters (Mar 14) |

### Bugs and Logic Errors (12/12 fixed)

| ID | File | Issue | Resolution |
|----|------|-------|------------|
| B1 | `workspace_tools.rs` | `matches_simple_glob` byte slicing | Safe string methods (`strip_prefix`/`strip_suffix`) |
| B2 | `tunnel.rs` | `__starting__` sentinel returned as URL | Sentinel check returns error |
| B3 | `tunnel.rs` | Mutex ordering deadlock risk | Documented + enforced lock ordering |
| B4 | `useConversationManager.ts` | Conversation strips thinking/toolCalls | `ChatMessage.thinking` added, all 3 load/save paths preserve fields (Mar 14) |
| B5 | `ChatTab.tsx` | Stale closure in streaming tok/s | Ref sync pattern |
| B6 | `gguf.rs` | `let _ =` on file seeks | Error check + break on failure |
| B7 | `gguf.rs` | Array count integer overflow | Count validated < 100M |
| B8 | `pty_manager.rs` | PTY UTF-8 split across chunks | `utf8_leftover` reassembly buffer |
| B9 | `scratchpad_tools.rs` | `is_expired` clock skew wrap | `.max(0)` clamp |
| B10 | `memory.rs` + `routines.rs` | `filter_map(ok())` silently drops rows | Error logged before drop |
| B11 | `models.rs` | Shell injection in stat command | `shell_escape()` on path |
| B12 | `SlotConfigSection.tsx` | Optimistic toggle drift | Backend-returned state as source of truth |

### Code Quality (3/3 fixed)

| ID | File | Issue | Resolution |
|----|------|-------|------------|
| Q2 | `SettingsTab.tsx` | 1918-line monolith | Split into `settings/` directory |
| Q4 | `discord_daemon.rs`, `telegram_daemon.rs`, `routines.rs` | Daemon start TOCTOU | `compare_exchange` atomic CAS |
| Q7 | `memory.rs` | `std::sync::Mutex` poison | `unwrap_or_else(\|e\| e.into_inner())` recovery |

### Dead Code (5/5 fixed)

| ID | File | Issue | Resolution |
|----|------|-------|------------|
| D1 | `models.rs` | Dead `save_model_file` | Removed |
| D2 | `orchestrator.rs` | Dead `WakeResult` struct | Removed |
| D3 | `MultiPaneChat.tsx` | Dead `_modelSelectorPaneId` | Removed |
| D4 | `gguf.rs` | Dead `re_patterns` iteration | Removed |
| D5 | `main.rs` | 4 dead Tauri commands | Removed |

### React Patterns (3/3 fixed)

| ID | Files | Issue | Resolution |
|----|-------|-------|------------|
| R1 | LogsTab, SetupTab | Array index keys | Stable keys (dep name, composite) |
| R2 | App.tsx, useConversationManager, ChatTab | Missing useEffect dependencies | Dependencies added |
| R3 | ChatTab, McpTab | Unmounted state update risk | Refs + cleanup |

### Documentation Drift (3/3 fixed)

| ID | File | Issue | Resolution |
|----|------|-------|------------|
| DOC1 | CLAUDE.md | WORKER_BLOCKED_TOOLS count wrong | Updated to 10 |
| DOC2 | CLAUDE.md | Tauri command count wrong | Updated to 148 (Mar 14) |
| DOC3 | tools/mod.rs | Tool count wrong | Updated to 44 |

---

## Intelligence Graduation (HIVE_ADVISORY.md execution)

Started: 2026-03-14

| Phase | Severity | What | File(s) | Status |
|-------|----------|------|---------|--------|
| 1 | CRITICAL | Cloud specialist tool gap — `chatWithProvider` → `chatWithTools` + tool execution loop | `useChat.ts:1250` | **DONE** |
| 2 | HIGH | Keyword extraction — frequency counting → YAKE (5-feature scoring, multi-word) | `memory.rs:843-887` | **DONE** |
| 3 | HIGH | Local embedding layer — `fastembed` v5.12.1 + ONNX Runtime v1.23.2 | `memory.rs`, `Cargo.toml` | **DONE** |
| 4 | HIGH | Semantic skills matching — Tool2Vec (40 synthetic queries, 5 skills) | `harness.rs:796-950` | **DONE** |
| 5 | HIGH | Semantic task routing — 3-layer tiered (keywords→embedding→fallback) | `orchestrator.rs:62-200` | **DONE** |
| 6 | HIGH | Semantic topic classification — centroid cascade (6 categories, 30 seeds) | `memory.rs:1147-1350` | **DONE** |
| 7 | MEDIUM | Progressive context summarization — 3-tier (65/80/95%) structured compression | `useChat.ts:866-960` | **DONE** |
| 8A+8B | MEDIUM | Power-law decay + archival (90-day, low strength) | `memory.rs` | **DONE** |
| 8C | MEDIUM | Memory consolidation — topic clustering (cosine > 0.7), cluster merge (3+), `absorbed` edges | `memory.rs` | **DONE** |
| 8D | MEDIUM | Active forgetting — supersession (cosine > 0.85 + same topic), `supersedes` edges | `memory.rs` | **DONE** |

Full roadmap: `HIVE_GRADUATION_ROADMAP.md` (790+ lines, competitor + paper research)

---

## Patterns Worth Noting

These recurring patterns were found across multiple files. All have been addressed (P5 sweep confirmed Mar 14):

1. **`let _ =` on critical paths** — 20+ instances across memory.rs, log_tools.rs, harness.rs, server.rs, provider_stream.rs. Only acceptable for telemetry/MAGMA events
2. **Byte slicing (`&str[..N]`)** — Fixed everywhere. Always use `.chars().take(N)`
3. **Missing SSRF validation** — download.rs uses `shell_escape()`, hardware.rs is local-only
4. **Shell injection in format strings** — models.rs fixed via `shell_escape()`
5. **Daemon TOCTOU races** — All 3 daemons use `compare_exchange` now
6. **`serde_json::from_str(&body).unwrap_or_default()`** — Eradicated from all HTTP response parsing (github_tools.rs 7 instances fixed Mar 14)
7. **Silent `.catch(() => {})` on user-facing IO** — ptyWrite now logs warnings; remaining instances are fire-and-forget telemetry/cleanup (acceptable)
8. **Conversation field stripping** — `thinking`, `toolCalls`, `toolCallId` preserved across all 3 load/save paths (Mar 14)

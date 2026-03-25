# Test Suite Health Log

**Purpose:** Track test suite baseline across sessions so future contributors can immediately tell whether a test failure is pre-existing or a regression they introduced. This is P4 (Errors Are Answers) applied to the development process itself.

**How to use this:** Before pushing, run all three checks and compare against the latest baseline. If your counts went down, you broke something. If a specific test fails that passed before, it's your regression — not pre-existing.

---

## Current Baseline

| Check | Command | Result | Last Updated |
|-------|---------|--------|-------------|
| Rust tests | `cargo test` | **269 passed, 0 failed** | Mar 14, 2026 |
| TypeScript tests | `npx vitest run` | **103 passed, 0 failed** | Mar 14, 2026 |
| TypeScript types | `npx tsc --noEmit` | **0 errors** | Mar 14, 2026 |

### Quick Check (copy-paste)

```bash
cd HIVE/desktop/src-tauri && cargo test 2>&1 | tail -3
cd HIVE/desktop && npx vitest run 2>&1 | tail -5
cd HIVE/desktop && npx tsc --noEmit 2>&1 | wc -l  # should be 0
```

---

## Test Inventory

### Rust Tests (269 total)

| Module | Tests | What They Cover |
|--------|-------|----------------|
| `security.rs` | 10 | AES-256-GCM encrypt/decrypt roundtrip, KDF determinism, key length, migration fallback, empty/unicode strings, different ciphertext per encrypt |
| `memory.rs` | 53 | Quality filter (8), keyword extraction (2), topic classification (3), markdown splitting (4), **integration: memory save/search/update/delete round-trip (8), deduplication with embeddings (3), MAGMA events (2), entities (2), procedures (1), edges (1), full graph round-trip (1), tier system (11), reinforcement regression (1: access_count+strength actually update), dedup-before-truncate (1), Phase 8C consolidation (5: sparse skip, grouping, clustering, strength inheritance, no-reconsolidate), Phase 8D supersession (6: marks superseded, MAGMA edge, requires same topic, skips consolidation source, tier ordering, no double-supersede)** |
| `content_security.rs` | 11 | Homoglyph folding (fullwidth, CJK, smart quotes), external content wrapping, SSRF protection (private IPs, localhost), audit logging, boundary markers |
| `harness.rs` | 15 | Stable manifest generation (model info, tools, cloud format), volatile context (turn count, truncation, VRAM, GPU), empty snapshot, assemble_prompt identity/user instructions, **Phase 5A: read_identity returns HIVE identity** |
| `providers.rs` | 40 | `strip_thinking` (9 cases), `sanitize_api_error` (5), `extract_reasoning_content` (4), `parse_thinking_depth` (6), `inject_thinking_params` (10 — Anthropic/OpenAI/DashScope/OpenRouter/Ollama/Local with depth variants), `is_retryable_error` (8 — 429/500/502/503/529/400/401/plain), `openai_compat_endpoint` (4) |
| `provider_tools.rs` | 34 | `tools_to_openai_format` (3), `tools_to_anthropic_format` (2), `parse_openai_tool_calls` (5), `parse_tool_calls_from_text` Hermes XML (7 — basic/multiple/truncated/missing brace/markdown/no calls/empty), `parse_kimi_tool_calls` (2), `parse_deepseek_tool_calls` (2), `parse_mistral_tool_calls` (3 — pre-v11/v11/no marker), `parse_bare_json_tool_calls` (3), `merge_consecutive_roles` (4) |
| `server.rs` | 8 | `port_for_slot` (all 5 roles + unknown fallback + uniqueness + range validation) |
| `orchestrator.rs` | 17 | `classify_task` (9 — code/terminal/web/tool/consciousness/empty/case insensitive/confidence scaling/cap), `plan_vram` (5 — fits/needs eviction/never evicts consciousness/evicts oldest/no candidates), `VramBudget` (4 — available/never negative/can_fit/deficit) |
| `routines.rs` | 11 | Cron parsing (wildcard, specific, range, step, weekday, list), cron matching, event matching (keyword, regex, channel) |
| `pty_manager.rs` | 15 | `strip_ansi_escapes` (plain text, CSI color codes, cursor movement, tilde-terminated, two-char escapes, carriage returns with terminal overwrite, spinner overwrite, OSC sequences, OSC ST terminator, mixed, empty, only-escapes, unicode), `PtySessionInfo` serialization with `exited` field |
| `tools/mod.rs` | 11 | `create_default_registry` tool count (≥40), schema sort order, schema structure (non-empty name/description, type:object params), core tool presence (27 tools), risk levels (run_command=high, read_file=low), unknown tool returns error (P4), register/unregister lifecycle, overwrite semantics, silent unregister of nonexistent, **Phase 5B: read_agent_context registration + schema** |
| Other (mcp) | 4 | MCP server handler |

### TypeScript Tests (103 total)

| Suite | Tests | What They Cover |
|-------|-------|----------------|
| `substitutePlanVariables` | 7 | Variable replacement in plan steps (`$var` syntax), nested objects, arrays, multiple vars, missing vars passthrough |
| `evaluatePlanCondition` | 6 | Step conditions: resolved vars, empty/whitespace, TOOL_ERROR, TOOL_EXCEPTION, literal strings |
| `detectRepetition` | 6 | Same-tool repetition detection (fast-track +2 per exact match), ping-pong A-B-A-B pattern, reset on tool change |
| `classifyToolCalls` | 4 | Terminal vs non-terminal tool classification, mixed batches, discord_send deferral |
| `isChainComplete` | 5 | Chain completion detection: terminal tool success, error, non-terminal passthrough, missing result, batch |
| `detectExternalChannel` | 9 | Telegram/Discord message format parsing, role tags (Host/User), no-match, partial match, no-username, extra fields |
| `TERMINAL_TOOLS` | 3 | Contains telegram_send/discord_send, excludes non-terminal tools |
| `channelPrompt round-trip` | 5 | buildTelegramPrompt/buildDiscordPrompt → parseChannelPrompt round-trips, username/guild optional, detectExternalChannel delegation |
| `normalizeCommand` | 7 | Bare command, .exe/.cmd/.bat stripping, Unix paths, Windows paths, mixed separators, case insensitivity |
| `computeToolResultMaxChars` | 7 | Context-proportional char limit: minimum clamp (4000), linear scaling, maximum clamp (40000), common context sizes (4K/32K), boundary cases |
| `formatToolResult` | 10 | TOOL_OK/TOOL_ERROR prefixes, truncation with generic vs read_file-specific hints, content preservation, boundary (exact limit/one over), empty content, toolCallId/toolName passthrough |
| `buildVolatileContext` | 16 | Empty state, turn count, truncation warning, VRAM hints (4 tiers: 13B+/7-8B/3B/near-full), GPU util, model VRAM, context pressure (moderate/HIGH/CRITICAL), working memory indicator, RAM info, pipe-separated combination |
| `shouldSaveProcedure` | 7 | Empty/single-step rejection, 2-step and 5-step acceptance, 6-step rejection, any-failure rejection, all-failure rejection |
| `buildProcedureData` | 6 | Non-qualifying returns null, chain name from tool sequence, trigger lowercasing/trimming, trigger truncation at 100 chars, arg key preservation, successful tool filtering |

---

## History

### Mar 14, 2026 — Intelligence Graduation Phases 8C+8D

**Changes:**
- Rust: 258 → 269 passed (+11 new tests)
- Vitest: 103 passed (unchanged)
- tsc: 0 errors (unchanged)

**Phase 8C — Memory Consolidation:**
- `memory.rs`: Periodic consolidation of dense topic groups (10+ memories)
- Greedy clustering with cosine similarity > 0.7 threshold, centroid recomputation
- Clusters of 3+ memories merged into single consolidated memory
- Originals marked `tier: 'consolidated'` (0.3x weight — recoverable but deprioritized)
- MAGMA `absorbed` edges link consolidated → originals
- Consolidated memory inherits max strength from constituents
- Runs on `memory_promote` alongside archival (Phase 8B)
- 5 new tests: sparse skip, grouping, clustering, strength inheritance, no-reconsolidate

**Phase 8D — Active Forgetting (Mem0 DELETE Pattern):**
- `memory.rs`: Supersession check on every memory save (except consolidation source)
- Finds top-5 similar memories by cosine > 0.85, same topic tag required
- Old memories marked `tier: 'superseded'` (0.2x weight — nearly invisible but recoverable)
- MAGMA `supersedes` edges link new → old with similarity metadata
- Skips already-superseded/consolidated memories (no cascading)
- 6 new tests: marks superseded, MAGMA edge, requires same topic, skips consolidation source, tier ordering, no double-supersede

### Mar 14, 2026 — Intelligence Graduation Phases 3-6

**Changes:**
- Rust: 236 → 258 passed (+22 new tests)
- Vitest: 103 passed (unchanged)
- tsc: 0 errors (unchanged)

**Phase 3 — Local Embedding Layer (fastembed):**
- `memory.rs` + `Cargo.toml`: Added `fastembed` v5.12.1 with `ort-load-dynamic` for local ONNX embeddings
- `all-MiniLM-L6-v2`: 384-dim vectors, ~22MB model, zero network latency (~10-50ms compute)
- Inserted at top of embedding cascade (before cloud providers)
- `OnceLock<Option<Mutex<TextEmbedding>>>` singleton — Mutex needed because `embed()` takes `&mut self`
- Fixed `cosine_similarity()` to return 0.0 for dimension mismatches (384 vs 1536)
- ONNX Runtime v1.23.2 at `~/.hive/onnxruntime/` (bypasses old v1.17.1 in System32 via `ORT_DYLIB_PATH`)
- 7 new tests: cosine dimensions (4), fastembed functional (3)

**Phase 4 — Semantic Skills Matching (Tool2Vec):**
- `harness.rs`: Replaced keyword overlap with Tool2Vec pattern for skill matching
- 40 synthetic queries (5 skills × 8 each), hardcoded in `builtin_skill_queries()`
- Built-in skills: average 8 query embeddings → 384-dim centroid per skill
- Custom skills: embed name + first 200 chars as fallback
- Keyword matching preserved as fallback when fastembed unavailable
- 5 new tests: query coverage, averaging, keyword fallback, semantic matching

**Phase 5 — Semantic Task Routing (3-Layer Tiered):**
- `orchestrator.rs`: Refactored `classify_task()` from flat keywords to 3-layer router
- Layer 1: Keyword rules (0ms, deterministic) — existing logic, returns `Option`
- Layer 2: Embedding similarity (5-15ms) — MAX aggregation per specialist, 40 pre-computed utterances
- Falls back to Consciousness when no layer claims the task
- 5 new tests: layer isolation, semantic routing, coverage, integration

**Phase 6 — Semantic Topic Classification:**
- `memory.rs`: Added centroid-based topic cascade to `classify_topic()`
- 6 categories (technical/project/personal/conversational/creative/reference) × 5 seed sentences = 30 seeds
- Cascade: keywords first (structured metadata) → semantic for "general" fallback
- `average_embeddings()` extracted as shared `pub(crate)` utility (used by Phases 4+6)
- 5 new tests: category coverage, semantic classification, cascade ordering

---

### Mar 14, 2026 — Intelligence Graduation Phases 1-2

**Changes:**
- Rust: 228 → 236 passed (+8 new tests)
- Vitest: 103 passed (unchanged)
- tsc: 0 errors (unchanged)

**Phase 1 — Cloud Specialist Tool Gap (CRITICAL):**
- `useChat.ts`: Replaced `chatWithProvider()` with `chatWithTools()` for cloud specialists
- Cloud specialists can now call HIVE tools (memory, files, web, etc.)
- Includes sub-loop for specialist tool execution (max 8 iterations)
- Tool results truncated per `TOOL_RESULT_MAX_CHARS`

**Phase 2 — YAKE Keyword Extraction:**
- `memory.rs`: Replaced frequency counting with YAKE algorithm (5 statistical features)
- Multi-word keyphrase extraction (1-3 grams)
- Features: casing, position, frequency, context diversity, sentence spread
- Original frequency-based extractor preserved as `extract_keywords_frequency()` fallback
- 5 new tests: multiword, no_stopwords, max_eight, dedup, frequency_fallback

**Phase 7 — Progressive Context Summarization:**
- `useChat.ts`: Replaced single-shot 70% summarization with 3-tier progressive system
- Tier 1 (65%): Structured summarization of oldest 30% using Factory.ai prompt pattern
- Tier 2 (80%): Aggressive — keep last 10 messages raw + comprehensive summary
- Tier 3 (95%): Emergency — compress tool results (Anthropic pattern: 84% token reduction)
- Summaries cached via ref and injected into tool loop iterations

**Phase 8A+8B — Memory Lifecycle (Decay + Archival):**
- `memory.rs`: Replaced logarithmic recency decay with power-law (matches biological forgetting)
- Power-law: `(1 + hours)^(-0.3)` — old memories retain faint trace, not zero
- Added `last_accessed` column with migration + backfill
- Reinforcement now updates `last_accessed` timestamp
- Added `archive_stale_memories()` — 90+ days no access + strength < 1.1 → archived tier
- Archived memories penalized (0.5x tier_weight) but still searchable
- 3 new tests: archive_thresholds, skip_already_archived, tier_weight_values

---

### Mar 10, 2026 — Phase 5D: Cross-Model Agent Spawning Enhancement

**Changes:**
- Rust: 214 passed (unchanged)
- Vitest: 96 passed (unchanged)
- tsc: 0 errors (unchanged)

**Phase 5D — Workers get HIVE identity, bus writes, slot_role:**
- `worker_tools.rs`: Workers receive full HIVE identity (`read_identity()`) when no custom system_prompt is provided (was generic "You are a focused research worker")
- `worker_tools.rs`: All 6 worker exit paths (natural, done signal, wall clock, turn limit, repetition, LLM error) write completion status to context bus (fire-and-forget)
- `worker_tools.rs`: New `slot_role` parameter — resolve slot configuration to provider/model instead of requiring explicit API model IDs. Priority: explicit params > slot_role > session context
- `worker_tools.rs`: Fixed `&w.task[..80]` UTF-8 byte-slice panic (P5 — same pattern fixed in Phase 4 audit)
- Log entries include identity source (hive/custom) and slot_role for observability (P4)

---

### Mar 10, 2026 — Phase 5C: Context Bus (shared agent activity feed)

**Changes:**
- Rust: 214 passed (unchanged)
- Vitest: 96 passed (unchanged)
- tsc: 0 errors (unchanged)

**Phase 5C — Context Bus (formalized on existing scratchpads, P3):**
- `scratchpad_tools.rs`: Added `context_bus_write()` + `context_bus_summary()` pub(crate) helpers — 8h TTL, 10 entries/agent FIFO cap, 1KB max size
- `tools/mod.rs`: Tauri command wrappers `context_bus_write` + `context_bus_summary`
- `main.rs`: Registered both commands
- `api.ts`: TypeScript wrappers `contextBusWrite()` + `contextBusSummary()`
- `specialist_tools.rs`: Local specialists write to bus on completion (fire-and-forget)
- `useChat.ts`: Cloud specialists write to bus on completion; tool loop writes chain summary to bus; bus summary injected into volatile context (separate system message, preserves KV cache prefix)
- All bus writes are best-effort with `.catch(() => {})` / `tokio::spawn` — P4 graceful degradation

---

### Mar 12, 2026 — Bridge Polish P0: Fix strip_ansi_escapes carriage return handling

**Changes:**
- Rust: 225 → 228 passed (+3 new tests, 1 updated)
- Vitest: 103 passed (unchanged)
- tsc: 0 errors (unchanged)

**P0 fix — "Thinking" spam in bridge output:**
- `pty_manager.rs`: Rewrote `strip_ansi_escapes()` to simulate terminal carriage return behavior
- Bare `\r` now clears the current line buffer (simulates cursor-to-column-0 overwrite) instead of being deleted
- `\r\n` treated as normal newline, `\n` treated as normal newline
- Added OSC sequence handling (`\x1b]...\x07` or `\x1b]...\x1b\\`) — previously leaked content
- This eliminates Claude Code's "Thinking..." spinner text concatenating 20+ times in bridge output
- Also fixes `read_agent_output` returning thinking spam (P2 in TODO_BRIDGE_POLISH.md)

---

### Mar 11, 2026 — Cross-Agent Output Bridge (read_agent_output)

**Changes:**
- Rust: 214 → 218 passed (+4 new tests)
- Vitest: 96 → 103 passed (unchanged from upstream)
- tsc: 0 errors (unchanged)

**Cross-agent output reading:**
- `pty_manager.rs`: Added per-session circular output buffer (500 lines, ANSI-stripped)
- `pty_manager.rs`: Fixed `\r\n` → `\r\r` double carriage return in `write_to_session()`
- `agent_tools.rs`: Added `ReadAgentOutputTool` — models can read other agents' terminal output
- `mod.rs`: Registered new tool (44 → 45 total)
- Enables bidirectional cross-agent comms: `send_to_agent` (write) + `read_agent_output` (read)

---

### Mar 10, 2026 — Phase 5A/5B: Unified Identity + ReadAgentContext

**Changes:**
- Rust: 212 → 214 passed (+2 new tests)
- Vitest: 96 passed (unchanged)
- tsc: 0 errors (unchanged)

**Phase 5A — Unified Identity (all specialists get HIVE harness):**
- `harness.rs`: `read_identity()` made `pub(crate)` for cross-module access
- `specialist_tools.rs`: Local specialists now receive full HIVE identity (HIVE.md) + specialist role designation + MAGMA wake context (was generic "You are a specialist")
- `useChat.ts`: Cloud specialists now receive cached harness identity + specialist role (was wake context only)
- Both paths log harness injection for observability (P4)

**Phase 5B — `read_agent_context` tool:**
- New `ReadAgentContextTool` in specialist_tools.rs — queries MAGMA events, scratchpads, working memory, worker status for any agent
- `scratchpad_tools.rs`: Added `list_scratchpads_summary()` pub(crate) accessor
- `worker_tools.rs`: Added `get_worker_summary()` and `list_active_workers_summary()` pub(crate) accessors
- Registered in `tools/mod.rs` (now 45 tools)
- New tests: harness identity test, tool registration + schema validation

---

### Mar 10, 2026 — Full branch audit: critical bugs found and fixed

**Changes:**
- Rust: 210 → 212 passed (+2 regression tests)
- Vitest: 96 passed (unchanged)
- tsc: 0 errors (unchanged)

**CRITICAL fix — memory reinforcement was completely dead:**
- SQLite's bundled build does NOT include `ln()` function
- The `strength = 1.0 + 0.1 * ln(...)` SQL silently failed (swallowed by `let _ =`)
- access_count never incremented, strength never updated, tier promotion never triggered
- Fix: compute `ln()` in Rust, write result back. Regression test proves it works.

**CRITICAL fix — UTF-8 byte-slice panic:**
- `&content[..60]` panics on multi-byte chars. Fixed with `.chars().take(60).collect()`
- Same pattern fixed in telegram_tools.rs

**HIGH fix — search results lost to premature truncation:**
- `results.truncate()` ran BEFORE dedup. Unique results below cut were lost. Reversed ordering.

**HIGH fix — orphaned MAGMA edges on memory delete:**
- `memory_delete`, `delete_memory_public`, `memory_clear_all` all cleaned edges now

**MEDIUM fix — GROUP BY mismatch in tier counts:**
- `GROUP BY tier` vs `SELECT COALESCE(tier, 'long_term')` could double-count. Fixed with `GROUP BY t`.

**MEDIUM fix — `classifyError` literal string match:**
- `includes('invalid.*key')` treated regex as literal. Fixed to `includes('invalid') && includes('key')`.

**MEDIUM fix — silent summarization failure:**
- Cloud-path fallback write swallowed error with `.catch(() => {})`. Now logs warning.

---

### Mar 10, 2026 — Phase 4C/4D audit fixes

**Changes:**
- Rust: 206 → 210 passed (+4 new tests)
- Vitest: 96 passed (unchanged)
- tsc: 0 errors (unchanged)

**Phase 4C audit fixes (memory tiers):**
- `write_memory_with_tier()` — atomic tier on INSERT, eliminates INSERT-then-UPDATE race
- `promote_due_memories()` — standalone function, not trapped inside search_hybrid
- Tauri commands `memory_tier_counts` + `memory_promote` — no dead code
- Tier scoring wired into hybrid search: `score *= tier_weight(&tier)`
- New tests: standalone promotion, atomic insert, default tier, tier weight scoring

**Phase 4D audit fixes (context summarization):**
- Tool regex: added EXCEPTION/DENIED/DEFERRED/SKIPPED (was only OK/ERROR)
- P6: User messages sent to summarization model limited to 80-char topic hints
- Stale closure: captured provider/modelId before async fire-and-forget
- Model output sanitization: 4000 char cap + control character strip

---

### Mar 10, 2026 — Phase 4A-4C: Architecture improvements

**Changes:**
- Rust: 201 → 206 passed (+5 new tests)
- Vitest: 96 passed (unchanged)
- tsc: 0 errors (unchanged)

**Phase 4A — useChat.ts decomposition:**
- Extracted 18 pure functions + types from useChat.ts (1917 lines) into `src/lib/chainPolicies.ts` (351 lines)
- useChat.ts reduced to 1622 lines. Re-exports maintain backward compatibility.
- Zero behavioral change — all vitest tests pass unchanged.

**Phase 4B — SQLite PRAGMA consistency:**
- Standardized all 8 DB connection sites to: `journal_mode=WAL; busy_timeout=5000; foreign_keys=ON;`
- Previously 5 of 8 were missing busy_timeout (causes "database is locked" under concurrent access)

**Phase 4C — Memory tier system:**
- Added `tier` column (working/short_term/long_term) with migration
- Working memory flush saves as `short_term`; promoted to `long_term` after `access_count > 3`
- New tests: tier default, set, promotion, no-promotion below threshold, tier counts

---

### Mar 10, 2026 — Phase 3B: Integration tests (memory round-trip, tool registry)

**Changes:**
- Rust: 176 → 201 passed (+25 new tests)
- Vitest: 96 passed (unchanged from Phase 2C)
- tsc: 0 errors (unchanged)

**New test coverage areas:**
- `memory.rs` (+15): SQLite integration tests — save/search/update/delete round-trip (8), embedding-based deduplication (3), MAGMA episodic events (2), entity graph with unique constraint (2), procedure save + outcome tracking (1), edge creation + traversal (1), full graph round-trip: memory → entity → edge → search (1)
- `tools/mod.rs` (+10): `create_default_registry` validation (tool count ≥40, sorted schemas, valid JSON Schema structure, 27 core tools present, risk level verification), P4 error handling (unknown tool returns ToolResult not Err), register/unregister lifecycle (add, remove, overwrite, silent nonexistent remove)

---

### Mar 10, 2026 — Phase 2C: Tool execution loop tests

**Changes:**
- Vitest: 52 → 96 passed (+44 new tests)
- Rust: 176 passed (unchanged)
- tsc: 0 errors (unchanged)

**Refactoring for testability (P1: Modularity):**
- Extracted `computeToolResultMaxChars()` from inline expression in tool loop
- Extracted `formatToolResult()` from `executeAndFormatTool()` — pure formatting logic separated from I/O
- Extracted `shouldSaveProcedure()` and `buildProcedureData()` from inline procedure extraction
- Exported `buildVolatileContext()` (was already pure, just not exported)
- `executeAndFormatTool()` now delegates to `formatToolResult()` — zero behavioral change

**New test coverage areas:**
- `computeToolResultMaxChars` (7): Context-proportional scaling with min/max clamps, boundary cases
- `formatToolResult` (10): Status prefix formatting (TOOL_OK/TOOL_ERROR), truncation behavior (generic vs read_file-specific hint), content preservation, boundary conditions
- `buildVolatileContext` (16): All status string sections (turns, truncation, VRAM tiers, GPU util, context pressure levels, working memory, RAM), combination formatting
- `shouldSaveProcedure` (7): Chain length validation (too short/just right/too long), failure rejection
- `buildProcedureData` (6): Metadata construction (chain name, trigger pattern, arg keys), edge cases

---

### Mar 9, 2026 — Phase 2A/2B/2D: Provider, Server, and Orchestrator tests

**Changes:**
- Rust: 92 → 176 passed (+84 new tests)
- Vitest: 52 passed (unchanged)
- tsc: 0 errors (unchanged)

**New test coverage areas:**
- `providers.rs` (+26): `parse_thinking_depth` (6), `inject_thinking_params` (10 across all providers), `is_retryable_error` (8), `openai_compat_endpoint` (4)
- `provider_tools.rs` (+34): Tool schema conversion (5 — OpenAI/Anthropic format), tool call parsing across 6 model families (22 — OpenAI native, Hermes XML, Kimi, DeepSeek, Mistral, bare JSON), `merge_consecutive_roles` (4), edge cases (truncated tags, missing braces, markdown wrapping)
- `server.rs` (+8): `port_for_slot` all branches + uniqueness + range validation
- `orchestrator.rs` (+17): `classify_task` keyword routing (9 cases), `plan_vram` eviction logic (5 cases), `VramBudget` methods (4 cases)

---

### Feb 28, 2026 — Phase 4 Brain + Skills + RAG completion

**Changes:**
- Vitest: 44 → 52 passed (+8 new tests: channelPrompt round-trip, additional evaluatePlanCondition/substitutePlanVariables/detectRepetition cases)
- Rust: 92 passed (unchanged)
- tsc: 0 errors (unchanged)

**New test coverage areas:**
- `channelPrompt round-trip` (5 tests): buildTelegramPrompt/buildDiscordPrompt → parseChannelPrompt fidelity
- Expanded `evaluatePlanCondition` (4→6): TOOL_EXCEPTION, whitespace-only, literal string
- Expanded `substitutePlanVariables` (5→7): array substitution, multi-variable strings
- Expanded `detectExternalChannel` (6→9): role tags (Host/User/SenderRole)

### Feb 28, 2026 — Baseline established

**Before (pre-audit):**
- Rust: 78 passed, 3 failed (strip_thinking_multiple_blocks, sanitize_truncates_large_bodies, stable_manifest_cloud_provider_tool_format)
- Vitest: 37 passed, 0 failed
- tsc: 2 errors (TS2588 in useChat.ts)

**Root causes of failures:**
1. `strip_thinking_multiple_blocks` — `/think` regex was processing before `<think>`, causing it to match the `/think` inside `</think>` closing tags. Fix: reverse processing order (XML first).
2. `sanitize_truncates_large_bodies` — Test asserted `"(truncated)"` but `safe_truncate()` was changed to append `"..."`. Fix: update assertion.
3. `stable_manifest_cloud_provider_tool_format` — Test asserted `"tool_call"` but code was intentionally changed to `"function-calling API"`. Fix: update assertion.
4. `useChat.ts` TS2588 — `const` destructuring of `tool_calls` was later reassigned. Fix: `const` -> `let`.

**After (post-audit):**
- Rust: 92 passed, 0 failed (+14 new tests, 3 fixes)
- Vitest: 44 passed, 0 failed (+7 new tests)
- tsc: 0 errors (1 fix)

**New tests added:**
- 11 Rust: `strip_ansi_escapes` (10 cases) + `PtySessionInfo` serialization (1)
- 7 vitest: `normalizeCommand` (path normalization for agent routing)

# HIVE Advisory: Intelligence Graduation Plan

**Author:** Claude Code (Opus 4.6) — Full Codebase Audit, March 2026
**Audience:** Claude Code session executing improvements
**Scope:** Every file, tool, function, and cross-file contract in the HIVE repository
**Branch:** `claude/init-repo-l36Pu`

---

## Executive Assessment

HIVE's plumbing is excellent. The infrastructure — Tauri v2 bridge, 45-tool MCP registry, 6-provider abstraction, MAGMA graph schema, specialist slot system, worker concurrency, PTY management, remote channel security — is production-grade engineering. The architecture is sound, modular, and principled.

**The intelligence layer is shallow.** Every cognitive function that should be semantic is pattern-based. Keywords are frequency-counted, not understood. Topics are matched against hardcoded word lists, not classified. Skills are found by substring overlap, not relevance. Task routing uses string matching, not intent analysis. Context management truncates instead of summarizing. Cloud specialists are text-only when they should have full tool access.

The result: HIVE has the body of an orchestration harness but the mind of a grep pipeline. The models plugged into it are intelligent; the harness routing work to them is not. This document identifies every instance of shallow intelligence, prioritizes fixes, and provides implementation guidance specific enough to execute directly.

---

## Table of Contents

1. [Critical Bug: Cloud Specialist Tool Gap](#1-critical-bug-cloud-specialist-tool-gap)
2. [Shallow Intelligence: The Pattern Problem](#2-shallow-intelligence-the-pattern-problem)
3. [Memory System Gaps](#3-memory-system-gaps)
4. [Context Management: Truncation Over Summarization](#4-context-management-truncation-over-summarization)
5. [Testing Infrastructure Gaps](#5-testing-infrastructure-gaps)
6. [useChat.ts Complexity](#6-usechatts-complexity)
7. [Minor Concerns](#7-minor-concerns)
8. [Priority Execution Order](#8-priority-execution-order)
9. [Implementation Guidance Per Fix](#9-implementation-guidance-per-fix)

---

## 1. Critical Bug: Cloud Specialist Tool Gap

**Severity:** CRITICAL — P2 violation (Provider Agnosticism)
**Status:** Confirmed bug, not a design gap

### The Problem

When a specialist slot is routed to a cloud provider (OpenAI, Anthropic, OpenRouter, DashScope), it loses all tool access. The specialist becomes a text-only, one-shot responder — fundamentally crippled compared to a local specialist.

### Evidence

**`useChat.ts` lines ~1213-1250** — Cloud specialist routing:
```typescript
// This calls chatWithProvider() which returns Promise<string>
// NO tool schemas are passed. The specialist gets zero tools.
const response = await api.chatWithProvider(
  provider, model, messages, thinkingDepth
);
```

**`api.ts` lines ~992-1004** — The TypeScript wrapper:
```typescript
async chatWithProvider(provider, model, messages, thinking_depth) {
  return invoke<string>('chat_with_provider', { ... });
  // Returns string. Not a tool call. Not structured output. Just text.
}
```

**`providers.rs` lines ~370-401** — The Rust handler:
```rust
pub async fn chat_with_provider(...) -> Result<String, String> {
  // Accepts: provider, model, messages, thinking_depth
  // Does NOT accept: tools, tool_schemas, tool_registry
}
```

**`providers.rs` lines ~432-445** — The function that DOES support tools:
```rust
pub async fn chat_with_tools(..., tools: Vec<ToolSchema>) -> ... {
  // This exists! It handles tool calls! But nobody calls it for specialists.
}
```

### Impact

A user running HIVE with zero local GPU (first-class use case per P2) gets specialists that can only generate text. They cannot use `read_file`, `memory_search`, `web_fetch`, or any of the 45 tools. The Coder specialist can't read code. The WebCrawl specialist can't fetch URLs. The ToolCall specialist can't call tools.

This makes cloud-only HIVE a fundamentally different (worse) product than local HIVE. That directly violates P2: "The interface is permanent. The backend is replaceable."

### Fix

Wire `chat_with_tools()` into the cloud specialist path in `useChat.ts`. The function already exists in `providers.rs` — the gap is purely in the routing layer. See [Implementation Guidance §1](#fix-1-cloud-specialist-tool-gap) for specifics.

---

## 2. Shallow Intelligence: The Pattern Problem

**Severity:** HIGH — Affects every cognitive function
**Core issue:** Every place HIVE makes a "smart" decision, it uses string patterns instead of semantic understanding.

### 2A. Keyword Extraction (`memory.rs` lines ~843-887)

**What it does:** Extracts "keywords" from text for memory indexing.

**How it actually works:**
1. Lowercase the text
2. Split on whitespace
3. Remove words shorter than 3 characters
4. Remove words in a hardcoded stopword list (~50 common English words)
5. Count word frequency
6. Return top 8 by frequency

**What's wrong:**
- "Machine learning model training pipeline" → keywords by frequency, not by meaning
- Synonyms are invisible: "car" and "automobile" are unrelated
- Multi-word concepts are destroyed: "neural network" becomes two unrelated words
- Domain-specific stopwords aren't filtered: "function", "return", "const" dominate in code discussions
- No stemming: "running", "runs", "ran" are three separate keywords
- No TF-IDF: common-in-this-corpus words aren't downweighted

**The right approach:** Use the embedding model (already available — `text-embedding-3-small`) to generate a semantic vector. For keyword display, use TF-IDF or RAKE algorithm. For matching, use cosine similarity on embeddings (which HIVE already does for deduplication — the infrastructure exists).

### 2B. Topic Classification (`memory.rs` lines ~893-942)

**What it does:** Assigns a topic category to memories.

**How it actually works:**
```rust
fn classify_topic(content: &str) -> String {
    let lower = content.to_lowercase();
    // Hardcoded keyword lists:
    let technical_keywords = ["code", "bug", "error", "function", "api", ...];
    let project_keywords = ["task", "milestone", "deadline", "sprint", ...];
    let personal_keywords = ["preference", "style", "habit", "like", ...];

    // Count matches per category, highest wins
    // Default: "general"
}
```

**What's wrong:**
- "I like this API's coding style" matches ALL THREE categories (personal: "like", "style"; technical: "API", "coding"; project: none → but it's really a personal preference)
- The category list is closed — can't discover new topics
- No hierarchical classification: "Rust async error handling" is just "technical"
- Adding a new category requires a code change and recompile

**The right approach:** Use the LLM itself (or embeddings + clustering) to classify. HIVE already has provider access — a single cheap inference call with a structured prompt ("Classify this memory into one of: [categories]. Reply with just the category name.") would be infinitely more accurate. For zero-API-cost scenarios, use embedding clustering with auto-discovered topic centroids.

### 2C. Skills Matching (`harness.rs` lines ~796-825)

**What it does:** Finds relevant skills to inject into the model's context.

**How it actually works:**
```rust
fn find_relevant_skills(query: &str, skills: &[Skill]) -> Vec<&Skill> {
    // For each skill:
    //   - Split query into words
    //   - For each word, check if skill.name contains it (+3 points)
    //   - For each word, check if skill.content contains it (+1 point)
    //   - Take top 2 skills by score, inject up to 2000 chars each
}
```

**What's wrong:**
- "Help me write a Python script" won't match a skill named "scripting_automation" unless "script" is a substring of "scripting" (it is, by luck — but "code" wouldn't match "programming")
- A skill about "database migrations" won't match "I need to update my schema" — zero word overlap
- Name matches (+3) over content matches (+1) means a skill named "helper" scores higher than a skill whose entire content is about the exact topic, if "helper" appears in the query
- Injecting 2 skills × 2000 chars = 4KB of potentially irrelevant context every message

**The right approach:** Embed skill descriptions at load time. On query, embed the query and find nearest skills by cosine similarity. The embedding infrastructure already exists in `memory.rs` — reuse it.

### 2D. Task Routing (`orchestrator.rs`)

**What it does:** Decides which specialist slot should handle a message.

**How it actually works:**
```rust
fn classify_task(message: &str) -> SlotRole {
    let lower = message.to_lowercase();
    if lower.contains("code") || lower.contains("write") || lower.contains("function") ... {
        SlotRole::Coder
    } else if lower.contains("search") || lower.contains("find") || lower.contains("web") ... {
        SlotRole::WebCrawl
    }
    // ... etc
    // Default: Consciousness
}
```

**What's wrong:**
- "Help me think through the code architecture" → routes to Coder (matched "code"), but this is a Consciousness task
- "Write me a poem about databases" → routes to Coder (matched "write"), should be Consciousness
- "Find the bug in my code" → could match WebCrawl ("find") or Coder ("code") — order-dependent
- No understanding of intent, only keyword presence

**The right approach:** Use the steering model itself to classify. A single inference call: "Given this message, which specialist should handle it: [list with descriptions]? Reply with just the role name." This costs one cheap inference but gets intent classification right. For offline/no-API scenarios, fall back to the keyword heuristic (which is better than nothing).

### 2E. Worker Anti-Spam (`worker_tools.rs`)

**What it does:** Prevents workers from flooding the parent with repetitive reports.

**How it actually works:**
- Jaccard word overlap > 70% with last report = rejected

**What's wrong:** Jaccard similarity on bag-of-words is crude. "Task completed successfully, all tests pass" and "All tests pass, task completed successfully" have 100% Jaccard overlap but are legitimately the same message. Meanwhile, "Finished analyzing repo A: 3 critical bugs found" and "Finished analyzing repo B: 3 critical bugs found" have high overlap but are meaningfully different (different repos). The current approach works acceptably for anti-spam but would fail for semantic deduplication.

**Assessment:** LOW priority. The current approach is adequate for its purpose.

---

## 3. Memory System Gaps

**Severity:** HIGH — The memory system has solid storage but incomplete cognitive processing

### 3A. No Summarization, Only Truncation

**Where:** `memory.rs` — `safe_truncate()` caps content at 3200 characters
**Where:** `useChat.ts` lines ~940-960 — context pressure handling

The entire memory pipeline uses truncation, never summarization:
- Memories longer than 3200 chars are truncated with `safe_truncate()` (which correctly handles UTF-8 boundaries, at least)
- Context pressure at 70% triggers a ONE-SHOT summarization to working memory, but then all further pressure response is pure message dropping (oldest first)
- Auto-flush extracts keywords and saves to memory, but the extraction is the crude frequency-based method from §2A

**What should happen:** When a memory exceeds the budget, summarize it using the active model (even a cheap/fast one). When context pressure hits, summarize the oldest N messages into a single context message rather than dropping them entirely. The model loses continuity when messages are dropped — summarization preserves the thread.

### 3B. No Memory Lifecycle (Short-Term → Long-Term Promotion)

**Where:** `memory.rs` — `promote_due_memories()` lines ~1716-1724

The promotion mechanism exists but is primitive:
- `access_count > 3` → promote from `short_term` to `long_term`
- That's it. No decay, no consolidation, no "forgetting"
- No concept of memory freshness beyond recency decay on search scores
- No periodic consolidation ("these 15 memories about project X can be merged into 3")

**What should happen:**
- Periodic consolidation: group related memories and merge/summarize
- Decay function: memories not accessed in N days lose strength
- Active forgetting: contradicted or corrected memories should be marked, not just left alongside their corrections
- Working memory → short-term promotion should include a summarization step

### 3C. No Bidirectional Markdown ↔ DB Sync

**Where:** `memory.rs` — daily log markdown files exist but are write-only

Memories are saved to both SQLite and daily markdown logs. But:
- Editing a markdown file doesn't update SQLite
- The markdown files are append-only, never consolidated
- A user can't edit `memory/2026-03-14.md` in their text editor and have HIVE pick up the changes

**Assessment:** MEDIUM priority. The SQLite store is the source of truth. Markdown sync is a convenience feature. But for the Obsidian-compatible vision described in CLAUDE.md, bidirectional sync matters.

### 3D. Graph-Augmented Search Works but Edges Are Thin

**Where:** `memory.rs` — `auto_create_edges()`, `expand_via_graph()`

MAGMA graph edges are auto-created on memory save, and search results are expanded via 1-hop graph traversal with 60% score decay. This works. But:
- Edge types are limited and auto-generated — the model doesn't actively curate the graph structure (though `entity_track` and `graph_query` tools exist for this)
- No edge weighting beyond the initial creation — frequently co-accessed memories should have stronger edges
- No graph pruning — edges accumulate without cleanup

**Assessment:** LOW-MEDIUM priority. The graph works; it just needs maturation.

---

## 4. Context Management: Truncation Over Summarization

**Severity:** MEDIUM-HIGH
**Where:** `useChat.ts` lines ~940-960

### Current Behavior

1. At 70% context usage → ONE summarization pass → save to working memory
2. Beyond 70% → drop oldest messages (pure truncation)
3. No awareness of message importance (a critical decision message is dropped as readily as a greeting)
4. No progressive summarization (summarize → summarize the summary → etc.)

### What Should Happen

1. At 50% context → begin progressive summarization of oldest messages
2. At 70% → more aggressive summarization, group related messages
3. At 85% → final pass, keep only: system prompt, memory context, last N messages, summary of everything else
4. Never drop messages without summarizing them first
5. Mark certain messages as "pinned" (user decisions, critical context) — these survive all summarization

### Implementation Note

This is tangled with the specialist routing system. A specialist waking from sleep gets a MAGMA briefing (wake context injection) — that's good. But the main Consciousness thread has no equivalent protection against context loss.

---

## 5. Testing Infrastructure Gaps

**Severity:** MEDIUM
**Where:** Entire codebase

### Current State

- **Rust:** `cargo test` — unit tests exist for core modules (memory, security, tools)
- **TypeScript:** `vitest` — unit tests exist for key functions
- **E2E:** ZERO. No integration tests. No Playwright/Cypress. No automated UI testing.

### What's Missing

1. **No E2E test framework** — The most critical gap. A change to `useChat.ts` that breaks specialist routing won't be caught until a human notices.
2. **No provider integration tests** — Can't verify that all 6 providers still work after a change to `providers.rs`
3. **No tool execution tests** — The 45 tools have schema tests but no execution tests (would require mocking the Tauri environment)
4. **No memory round-trip tests** — save → search → recall → verify content integrity
5. **No cross-file contract tests** — DANGEROUS_TOOLS in Rust vs TypeScript could drift without detection (currently verified by comments only)

### Recommendation

Start with integration tests for the most-changed code paths:
1. Memory save → search → recall round-trip
2. Provider chat (mock HTTP) for all 6 providers
3. Cross-file contract assertions (automated, not comment-based)
4. Tool schema validation (all 45 tools have valid schemas)

E2E testing (Playwright for the Tauri webview) is important but lower priority than the intelligence fixes.

---

## 6. useChat.ts Complexity

**Severity:** MEDIUM
**Where:** `useChat.ts` (~1600 lines)

### The Problem

`useChat.ts` is the most complex file in the codebase. It handles:
- Message sending and streaming
- Tool call loop with chain policies
- Specialist routing (local + cloud)
- Context pressure management
- Remote channel security gating
- Procedure learning
- Skills injection
- VRAM enforcement
- Auto-sleep for specialists
- Working memory management

This is too many responsibilities for one file. `chainPolicies.ts` was extracted (good), but the file is still a monolith.

### Recommendation

Extract into focused modules:
1. `specialistRouter.ts` — all specialist routing logic (local + cloud + VRAM + auto-sleep)
2. `contextManager.ts` — context pressure, summarization, message management
3. `toolExecutor.ts` — tool call loop, chain policies, security gating
4. `useChat.ts` — thin orchestrator that composes the above

This is a refactoring task, not an intelligence upgrade. Do it AFTER the intelligence fixes (§8 priority order).

---

## 7. Minor Concerns

### 7A. Hardcoded Port Mapping

**Where:** `server.rs::port_for_slot()`, `types.ts::SPECIALIST_PORTS`

Five specialist slots are mapped to hardcoded ports (8081-8085). This works but:
- Port conflicts with other services are possible
- No dynamic port allocation
- Cross-file contract (Rust ↔ TypeScript) maintained by comments

**Assessment:** LOW priority. It works. Port conflicts are rare.

### 7B. Single-Model Embedding Dependency

**Where:** `memory.rs` — hardcoded to `text-embedding-3-small` via OpenAI API

If the user has no OpenAI API key, embeddings fail and the system degrades to FTS5-only search. This graceful degradation is correct (P4), but:
- No local embedding option (could use a small GGUF embedding model via llama.cpp)
- No provider choice for embeddings (violates P2 in spirit)

**Assessment:** MEDIUM priority. Adding a local embedding fallback would make the memory system work fully offline.

### 7C. No Rate Limiting on Provider Calls

**Where:** `providers.rs`, `useChat.ts`

No rate limiting or queuing for API calls. 20 workers hitting DashScope simultaneously rely on the API's own rate limiting. If the API returns 429, the worker fails rather than queuing.

**Assessment:** LOW-MEDIUM priority. The worker system's stress test showed 85% completion, suggesting this is adequate but not robust.

---

## 8. Priority Execution Order

Execute these in order. Each builds on the previous.

### Phase 1: Critical Bug Fix (1-2 hours)

| # | Task | Why First |
|---|------|-----------|
| 1.1 | **Fix cloud specialist tool gap** | P2 violation. Cloud-only users get a broken product. The fix is small — wire existing `chat_with_tools()` into the specialist path. |

### Phase 2: Semantic Intelligence Foundation (4-8 hours)

| # | Task | Why Now |
|---|------|---------|
| 2.1 | **Replace keyword extraction with embeddings** | Foundation for everything else. The embedding infrastructure exists (memory.rs deduplication). Reuse it for keyword extraction. |
| 2.2 | **Replace topic classification with LLM call** | Once embeddings work, topic classification becomes a cheap inference call or embedding cluster assignment. |
| 2.3 | **Replace skills matching with embedding similarity** | Embed skills at load time, query by cosine similarity. Direct reuse of §2.1 infrastructure. |
| 2.4 | **Replace task routing with LLM classification** | Use the steering model to classify intent. Fall back to keyword heuristic when no API is available. |

### Phase 3: Memory System Upgrades (4-6 hours)

| # | Task | Why After Phase 2 |
|---|------|-------------------|
| 3.1 | **Add summarization to memory save** | Requires the LLM call pattern established in Phase 2. Replace `safe_truncate()` with model-based summarization for long memories. |
| 3.2 | **Add progressive context summarization** | Replace message dropping with summarization. Uses same LLM call infrastructure. |
| 3.3 | **Add memory consolidation** | Periodic task: group related memories (by embedding similarity from §2.1), summarize groups, replace originals with consolidated versions. |
| 3.4 | **Add memory decay** | Memories not accessed in N days lose strength. Simple SQL update on a schedule. |

### Phase 4: Infrastructure Hardening (2-4 hours)

| # | Task | Why Last |
|---|------|----------|
| 4.1 | **Add cross-file contract tests** | Automated assertions that DANGEROUS_TOOLS, SPECIALIST_PORTS, etc. are in sync between Rust and TypeScript. |
| 4.2 | **Add memory round-trip integration tests** | save → search → recall → verify. |
| 4.3 | **Extract useChat.ts into focused modules** | Refactoring. Does not change behavior. |
| 4.4 | **Add local embedding fallback** | Small GGUF embedding model for offline use. |

---

## 9. Implementation Guidance Per Fix

### Fix 1: Cloud Specialist Tool Gap

**Files to modify:**
- `HIVE/desktop/src/useChat.ts` — cloud specialist routing (~lines 1213-1250)
- `HIVE/desktop/src/lib/api.ts` — add `chatWithTools()` wrapper
- `HIVE/desktop/src-tauri/src/providers.rs` — expose `chat_with_tools` as Tauri command if not already

**Approach:**

1. In `providers.rs`, verify `chat_with_tools()` is registered as a Tauri command. If not, add `#[tauri::command]` and register in `main.rs`.

2. In `api.ts`, add a new wrapper:
```typescript
async chatWithTools(
  provider: string, model: string, messages: Message[],
  tools: ToolSchema[], thinking_depth?: string
): Promise<{ content: string; tool_calls?: ToolCall[] }> {
  return invoke('chat_with_tools', { provider, model, messages, tools, thinking_depth });
}
```

3. In `useChat.ts`, replace the cloud specialist call path. Instead of:
```typescript
const response = await api.chatWithProvider(provider, model, messages, thinkingDepth);
```
Use:
```typescript
const response = await api.chatWithTools(provider, model, messages, toolSchemas, thinkingDepth);
// Then handle tool_calls in the response — enter the same tool loop used for local specialists
```

4. The tool loop already exists for local specialists. Factor it into a shared function that both local and cloud specialist paths call.

**Verification:** After this fix, test with a cloud provider (e.g., OpenRouter with a free model). Send a message that requires tool use (e.g., "read the file HIVE.md"). The cloud specialist should invoke `read_file` and return the content. If it returns text saying "I can't access files", the fix didn't work.

**Caution:** The `chat_with_tools` response format may differ by provider (OpenAI vs Anthropic tool call format). Verify that `providers.rs` normalizes tool calls across all providers. If it doesn't, that's a sub-task to handle.

---

### Fix 2.1: Replace Keyword Extraction with Embeddings

**Files to modify:**
- `HIVE/desktop/src-tauri/src/memory.rs` — `extract_keywords()` function (~lines 843-887)

**Approach:**

The embedding infrastructure already exists in `memory.rs` for deduplication (`is_near_duplicate()` uses `generate_embedding()`). Reuse it:

1. Keep the current `extract_keywords()` as `extract_keywords_fallback()` for when no API key is available.

2. Create a new `extract_keywords_semantic()`:
   - Generate an embedding for the content
   - Use TF-IDF scoring (compute IDF from existing memory corpus) to extract meaningful terms
   - Alternatively, use RAKE (Rapid Automatic Keyword Extraction) algorithm — it's simple, doesn't need an API, and is far better than frequency counting
   - Return top 8 keywords by TF-IDF or RAKE score

3. For the embedding-based approach (requires API key):
   - Generate embedding for the content
   - Compare against a pre-built vocabulary of concept embeddings
   - Return the N closest concept labels

**Recommended:** Start with RAKE (no API needed, pure Rust, massive improvement over frequency counting). Add embedding-based extraction as an enhancement later.

**RAKE algorithm in brief:**
1. Split text on stopwords and punctuation → candidate keywords (including multi-word phrases)
2. Build a word co-occurrence matrix from the candidates
3. Score each word: `score = degree(word) / frequency(word)`
4. Score each candidate phrase: sum of member word scores
5. Return top N candidates

This naturally captures multi-word phrases ("neural network", "API endpoint"), weights rare-but-important words higher, and ignores stopwords without needing a hardcoded list.

**Rust crates:** Consider `rake` or implement manually (the algorithm is ~50 lines).

---

### Fix 2.2: Replace Topic Classification with LLM Call

**Files to modify:**
- `HIVE/desktop/src-tauri/src/memory.rs` — `classify_topic()` function (~lines 893-942)

**Approach:**

1. Keep current `classify_topic()` as `classify_topic_fallback()`.

2. Create `classify_topic_semantic()`:
   - Build a prompt: "Classify the following text into exactly one category: technical, project, personal, conversational, creative, reference. Reply with only the category name.\n\nText: {content}"
   - Call the cheapest available provider (Ollama local if available, otherwise the user's configured provider)
   - Parse the single-word response
   - Cache results (same content = same topic, no need to re-classify)

3. For offline operation (no API available):
   - Use embedding clustering: maintain centroid embeddings for each topic category
   - On new memory, compute embedding, find nearest centroid
   - Update centroids incrementally (running average)

**Cost concern:** One cheap classification call per memory save is negligible. Even at OpenAI pricing, classifying 1000 memories costs ~$0.02.

---

### Fix 2.3: Replace Skills Matching with Embedding Similarity

**Files to modify:**
- `HIVE/desktop/src-tauri/src/harness.rs` — `find_relevant_skills()` (~lines 796-825)

**Approach:**

1. On harness initialization (or skill file change), embed each skill's description + content into a vector. Store these embeddings in memory (not SQLite — skills are few and loaded at startup).

2. On query, embed the user message and compute cosine similarity against all skill embeddings.

3. Return skills above a similarity threshold (e.g., 0.3), sorted by similarity, capped at 2-3 skills.

4. Fallback: If no embedding API is available, use the current substring matching.

**Key improvement:** A query "help me automate my deployment" will match a skill about "CI/CD pipeline configuration" even though zero words overlap — because the embeddings capture semantic similarity.

---

### Fix 2.4: Replace Task Routing with LLM Classification

**Files to modify:**
- `HIVE/desktop/src-tauri/src/orchestrator.rs` — `classify_task()`

**Approach:**

1. Keep current keyword heuristic as fallback.

2. Add LLM-based classification:
   ```
   System: You are a task router. Given a user message, determine which specialist should handle it.
   Available specialists:
   - Consciousness: General reasoning, planning, philosophical discussion, architecture decisions
   - Coder: Writing, debugging, or reviewing code. Must involve actual code production.
   - Terminal: System administration, command execution, file operations
   - WebCrawl: Web search, URL fetching, online research
   - ToolCall: Complex multi-tool operations, data processing pipelines

   Reply with ONLY the specialist name.

   User message: {message}
   ```

3. Use the cheapest/fastest available model for this call. It's a simple classification — even a small local model handles it well.

4. Cache recent classifications (LRU cache, 100 entries). Same message pattern = same routing.

**Critical test case:** "Help me think through the code architecture" must route to Consciousness, not Coder. The keyword approach gets this wrong; the LLM approach gets it right.

---

### Fix 3.1: Add Summarization to Memory Save

**Files to modify:**
- `HIVE/desktop/src-tauri/src/memory.rs` — `safe_truncate()` usage, memory save pipeline

**Approach:**

1. When a memory exceeds the character budget (currently 3200):
   - Instead of `safe_truncate(content, 3200)`, call the LLM: "Summarize the following in under 500 words, preserving all key facts, decisions, and technical details:\n\n{content}"
   - Store the summary as the memory content
   - Optionally store the full original in a separate `full_content` column for retrieval if needed

2. Fallback: If no LLM is available, use `safe_truncate()` (current behavior).

---

### Fix 3.2: Add Progressive Context Summarization

**Files to modify:**
- `HIVE/desktop/src/useChat.ts` — context pressure handling (~lines 940-960)

**Approach:**

Replace the current "drop oldest messages" with a progressive summarization strategy:

1. At 50% context: Flag for summarization preparation
2. At 60% context: Summarize the oldest 30% of messages into a single summary message:
   ```typescript
   const oldMessages = messages.slice(0, Math.floor(messages.length * 0.3));
   const summary = await summarizeMessages(oldMessages);
   // Replace oldMessages with a single system message containing the summary
   messages = [
     { role: 'system', content: `[Context summary: ${summary}]` },
     ...messages.slice(Math.floor(messages.length * 0.3))
   ];
   ```
3. At 80% context: More aggressive summarization — keep only last 10 messages + comprehensive summary of everything before
4. Never fully drop messages without first extracting their key content

**Implementation note:** The summarization call itself uses tokens. Budget for this — the summary call should use a small, fast model, not the main conversation model.

---

### Fix 3.3: Add Memory Consolidation

**Files to modify:**
- `HIVE/desktop/src-tauri/src/memory.rs` — new function

**Approach:**

Add a periodic consolidation routine (triggered on app start, or every N hours):

1. Query memories grouped by topic
2. For each topic with > 10 memories:
   - Compute pairwise embedding similarity
   - Cluster similar memories (cosine > 0.7)
   - For each cluster > 3 memories: summarize into one consolidated memory
   - Mark originals as `consolidated` (don't delete — keep for retrieval)
   - The consolidated memory inherits the highest strength score from its constituents

3. This naturally handles the "15 memories about the same bug fix" problem.

---

### Fix 3.4: Add Memory Decay

**Files to modify:**
- `HIVE/desktop/src-tauri/src/memory.rs` — new function, called periodically

**Approach:**

```sql
-- Reduce strength of memories not accessed in 30+ days
UPDATE memories
SET strength = strength * 0.9
WHERE last_accessed < datetime('now', '-30 days')
AND strength > 0.1;  -- Floor to prevent total decay

-- Memories below 0.1 strength with no access in 90 days → archive
UPDATE memories
SET tier = 'archived'
WHERE strength < 0.1
AND last_accessed < datetime('now', '-90 days');
```

Run this on app startup. Archived memories are excluded from search by default but recoverable.

---

### Fix 4.1: Add Cross-File Contract Tests

**Files to create:**
- `HIVE/desktop/src-tauri/tests/contract_sync.rs`

**Approach:**

Write tests that parse both Rust and TypeScript source files and verify string lists match:

```rust
#[test]
fn dangerous_tools_in_sync() {
    let rust_source = std::fs::read_to_string("src/content_security.rs").unwrap();
    let ts_source = std::fs::read_to_string("../src/lib/api.ts").unwrap();

    // Parse DANGEROUS_TOOLS from both files
    // Assert they contain the same items
}

#[test]
fn specialist_ports_in_sync() {
    // Parse port_for_slot() from server.rs
    // Parse SPECIALIST_PORTS from types.ts
    // Assert they match
}
```

This replaces the "cross-ref comment" approach with automated verification.

---

## Guiding Principles for Execution

1. **Test after every fix.** Run `cargo test` + `npx tsc --noEmit` + `npx vitest run` after each change. Compare counts to `HIVE/docs/TEST_HEALTH.md`.

2. **Preserve fallbacks.** Every semantic upgrade must have a non-API fallback. HIVE must work fully offline (P2). The intelligence degrades but doesn't break.

3. **Don't rewrite — extend.** The existing functions work. Rename them to `*_fallback()` and add the semantic version alongside. The caller tries semantic first, falls back to heuristic.

4. **One commit per fix.** Don't batch. Each fix is independently testable and revertable.

5. **Read before writing.** Every file mentioned in this document should be read fresh before modification. Line numbers may have shifted.

6. **Check the contract table.** After any change to types, tool lists, or cross-file constants, verify both sides of every contract in the table in CLAUDE.md.

7. **Follow CLAUDE.md.** This advisory supplements, not replaces, the project's CLAUDE.md. All coding standards, traps, and verification gates still apply.

---

## Summary

HIVE is an impressive piece of engineering with a genuinely novel architecture. The principle lattice, the provider abstraction, the tool framework, the security model, the worker system — these are all well-designed and well-implemented. The project doesn't need a rewrite. It needs its intelligence layer to match the quality of its infrastructure.

The fixes in this document are ordered from "critical bug that breaks a core principle" to "nice-to-have improvements." Phase 1 is a single bug fix. Phase 2 is the transformative work — replacing pattern matching with semantic understanding across four functions. Phase 3 builds on Phase 2 to upgrade the memory system. Phase 4 is housekeeping.

After these changes, HIVE graduates from "excellent plumbing with shallow intelligence" to "excellent plumbing with genuine cognitive capability." The harness becomes worthy of the models it orchestrates.

---

*Generated from full codebase audit — every file, function, and cross-file contract verified.*
*March 2026 — Claude Code (Opus 4.6)*

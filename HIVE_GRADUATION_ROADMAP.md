# HIVE Intelligence Graduation — Execution Roadmap

**Author:** Claude Code (Opus 4.6) — Full Codebase Audit + Competitor Research + Paper Survey
**Date:** March 14, 2026
**Based on:** HIVE_ADVISORY.md + 5 parallel research agents covering 9 frameworks, 12 papers, 6 production systems
**Branch:** `fix/audit-findings`

---

## Research Sources Summary

### Frameworks Analyzed
- **LangChain/LangGraph** — Conditional edge router, ConversationSummaryBufferMemory
- **CrewAI** — Manager-based delegation with `allow_delegation` + `allowed_agents`
- **AutoGen/AG2** — GroupChat selector (LLM-based, being merged into Microsoft Agent Framework)
- **Semantic Kernel** — Auto function calling (deprecated planners in favor of native function calling)
- **MemGPT/Letta** — OS-inspired virtual memory (main context / recall / archival tiers)
- **Mem0** — 6-stage memory pipeline with ADD/UPDATE/DELETE/NOOP operations
- **DSPy** — Programmatic prompt optimization (BootstrapFewShot, GEPA)
- **LlamaIndex** — ToolRetrieverRouterQueryEngine, RAKEKeywordTableIndex
- **OpenAI Assistants/Responses API** — Function calling, structured output

### Key Papers (2023-2026)
- **Tool2Vec** (Sep 2024) — Embed synthetic usage queries, not descriptions. 25-30% recall improvement.
- **RouterEval** (EMNLP 2025) — Benchmark: simple cosine similarity matches complex clustering.
- **Rerouting LLM Routers** (COLM 2025) — LLM-based routing most robust against adversarial attacks.
- **MixLLM** (NAACL 2025) — Tiered routing validated (fast methods first, escalate when needed).
- **AnyMAC** (EMNLP 2025) — Next-Agent Prediction confirms structured output approach.
- **LLMLingua-2** (ACL 2024) — Token classification for 3-6x prompt compression, BERT-base model.
- **SimpleMem** (2025) — 26.4% F1 improvement over Mem0, 30x fewer tokens, affinity-based consolidation.
- **MemoryBank** (2023) — Ebbinghaus forgetting curve: R = e^(-t/S), strength grows on recall.
- **Everything is Context** (arXiv:2512.05470) — File-system abstraction for context engineering.
- **Mem0 Paper** (arXiv:2504.19413) — 26% improvement over OpenAI, 91% lower p95 latency.
- **Cognitive Memory Survey** (arXiv:2504.02441) — Power-law decay matches biological forgetting.
- **Agent Memory Survey** (ACM 2025) — All modern systems use embeddings for retrieval.

### Production Systems Referenced
- **Factory.ai** — Structured summarization (3.70/5.0 quality, beats Anthropic 3.44 and OpenAI 3.35)
- **Anthropic** — Server-side compaction, context editing, tool result clearing (84% token reduction)
- **JetBrains Research** — Observation masking, summary costs >7% of total, trajectory elongation risk
- **vLLM Semantic Router** — Rust-based, uses ModernBERT for routing (not an LLM)
- **Portkey MCP Tool Filter** — 1000+ tools to 10-20 in <10ms using precomputed embeddings
- **Speakeasy** — Dynamic toolset architecture, up to 160x token reduction

---

## Key Research Findings That Change The Plan

### 1. YAKE > RAKE for keyword extraction
Both available in Rust. YAKE uses 5 features (casing, position, frequency, context, sentence distribution) vs RAKE's 2 (degree/frequency). YAKE has built-in Levenshtein dedup. Benchmarks show YAKE achieves best results among all statistical methods and outperforms RAKE on precision.

**Rust crates:**
- `rake` (v0.3.6, MIT/Apache-2.0) — standalone RAKE, 11K downloads/month
- `keyword_extraction` (v1.5.0, LGPL-3.0) — RAKE+YAKE+TF-IDF+TextRank in one crate. **LGPL license is restrictive** — may need manual YAKE implementation instead (~80 lines Rust)
- `stop-words` (v0.8, MIT) — stopword lists by language

### 2. Tool2Vec is the breakthrough for skills matching
Don't embed skill descriptions directly — embed *synthetic example queries* per skill. "What would a user ask that requires this skill?" Average those query embeddings = skill vector. 25-30% recall improvement over description embedding (ToolBank dataset benchmark).

### 3. 3-layer tiered router is industry consensus
Every production system converges on: Keywords (0ms) → Embedding similarity (5-15ms, offline) → LLM structured output (200-800ms). Don't over-engineer — RouterEval (EMNLP 2025) showed complex clustering gives "marginal" gains over simple cosine similarity.

### 4. `fastembed-rs` eliminates API dependency for embeddings
Apache-2.0, ONNX-based, runs `all-MiniLM-L6-v2` locally (22MB model, 384 dims). No Python, no Ollama needed. 3-5x faster than Python equivalents, 60-80% less memory. Downloads model on first use, cached forever. This makes everything work offline.

**Alternative:** HIVE already has Ollama integration — `nomic-embed-text` via `/api/embeddings` is zero new code. Use fastembed as fallback when Ollama isn't running.

### 5. Power-law decay > exponential for memory
Research shows `(1 + t)^(-β)` matches biological forgetting curves — old memories retain a faint trace instead of vanishing completely. Power-law with strength multiplier: `relevance = (1 + hours)^(-0.3) * (1 + ln(1 + access_count) * 0.5)`.

### 6. Structured summarization > freeform
Factory.ai evaluation (36,611 production messages): structured prompts with dedicated sections (Current State, Key Decisions, Pending Work, Critical Context) score 3.70/5.0 vs Anthropic's freeform at 3.44. Freeform loses technical details over multiple compression cycles.

### 7. Mem0's 4-operation pattern is the gold standard for consolidation
For each new fact extracted from conversation:
1. Embed the fact
2. Retrieve top-10 similar existing memories by cosine similarity
3. LLM chooses: **ADD** (new), **UPDATE** (merge with existing), **DELETE** (contradicts existing), **NOOP** (already known)
4. Results: 26% improvement over OpenAI memory, 91% lower p95 latency, 90% token cost reduction

### 8. Specialist tool scoping is a security requirement
Every framework (CrewAI, LangChain, OWASP) recommends filtered tool subsets per agent, not full access. For HIVE cloud specialists, filter tools by relevance to the specialist role.

---

## Dependency Decision

| Crate | License | Purpose | Verdict |
|-------|---------|---------|---------|
| `rake` (v0.3.6) | MIT/Apache-2.0 | RAKE keyword extraction | **SAFE** — use if YAKE not needed separately |
| `keyword_extraction` (v1.5.0) | LGPL-3.0 | RAKE+YAKE+TF-IDF+TextRank | **SKIP** — LGPL restrictive for desktop app |
| `fastembed` (v5.12) | Apache-2.0 | Local ONNX embeddings (all-MiniLM-L6-v2) | **APPROVED** — user approved Mar 14, 2026 |
| `stop-words` (v0.8) | MIT | Stopword lists by language | **SAFE** |
| `intent-classifier` | MIT | Few-shot TF-IDF+ML classification | **SAFE** — alternative to embedding router |

**APPROVED by user (Mar 14, 2026).** `fastembed` crate is the linchpin for offline semantic everything (Phases 3-6).

---

## Execution Status

| Phase | Status | Commit |
|-------|--------|--------|
| 1 — Cloud Specialist Tool Gap | **DONE** | `dc01418` |
| 2 — YAKE Keyword Extraction | **DONE** | `dc01418` |
| 3 — Local Embedding Layer (fastembed) | **DONE** | `4dea479` |
| 4 — Semantic Skills Matching (Tool2Vec) | **DONE** | `4dea479` |
| 5 — Semantic Task Routing (3-layer) | **DONE** | `4dea479` |
| 6 — Semantic Topic Classification | **DONE** | `4dea479` |
| 7 — Progressive Context Summarization | **DONE** | `dc01418` |
| 8A+8B — Power-law Decay + Archival | **DONE** | `dc01418` |
| 8C — Memory Consolidation | **DONE** | pending commit |
| 8D — Active Forgetting | **DONE** | pending commit |

---

## Execution Order (8 Phases)

### Phase 1: CRITICAL Bug Fix — Cloud Specialist Tool Gap [DONE]

**Severity:** CRITICAL — P2 violation (Provider Agnosticism)
**Effort:** 1-2 hours
**Dependencies:** None

**What:** Wire `chatWithTools()` into cloud specialist path. The function already exists in `providers.rs` — the gap is purely in the routing layer.

**Where:**
- `HIVE/desktop/src/useChat.ts` line 1250 — cloud specialist routing
- `HIVE/desktop/src/lib/api.ts` line 1141 — `chatWithTools()` wrapper (already exists)
- `HIVE/desktop/src-tauri/src/providers.rs` line 437 — `chat_with_tools` Tauri command (already registered in main.rs:228)

**The Bug (exact code):**
```typescript
// useChat.ts:1250-1254 — THIS IS THE BUG
const cloudResult = await api.chatWithProvider(
  provider as api.ProviderType,
  model,
  specialistMessages,
);
// chatWithProvider returns Promise<string> — NO TOOLS
```

**The Fix:**
```typescript
// Replace with chatWithTools + handle response type
const toolResponse = await api.chatWithTools(
  provider as api.ProviderType,
  model,
  specialistMessages,
  loopTools,  // Pass available tools (already in scope)
);
// chatWithTools returns Promise<ChatResponse> with { type, content, tool_calls }
```

**Complications:**
1. `chatWithTools()` returns `ChatResponse` (not `string`). Must handle the response type.
2. If the cloud specialist returns `tool_calls`, need to enter the tool execution loop (same loop used for main model). Factor the tool loop into a shared function.
3. Tool schema token cost: 44 tools × ~100 tokens each = ~4400 tokens per specialist call. Consider filtering tools per specialist role (security + efficiency).
4. Provider format differences: `providers.rs` already normalizes across all providers — no changes needed in Rust.

**Tool filtering per specialist (recommended):**
| Specialist | Tools to include | Rationale |
|---|---|---|
| Coder | read_file, write_file, list_directory, code_search, file_tree, run_command, memory_search | Code operations |
| Terminal | run_command, list_directory, file_tree, system_info, check_logs | System operations |
| WebCrawl | web_fetch, web_search, web_extract, read_pdf, memory_save | Web operations |
| ToolCall | ALL tools | General-purpose multi-tool |
| Consciousness | ALL tools minus run_command, write_file | Reasoning + safe tools |

**Verification:** Send a cloud specialist a message requiring tool use (e.g., "read the file HIVE.md"). Should invoke `read_file` and return content. If it returns text saying "I can't access files", the fix didn't work.

---

### Phase 2: Keyword Extraction Upgrade (YAKE) [DONE]

**Severity:** HIGH — Foundation for Phases 4-6
**Effort:** 3-4 hours
**Dependencies:** None (pure Rust implementation)

**What:** Replace frequency counting with YAKE algorithm. YAKE extracts multi-word keyphrases, weighs by position/casing/context, deduplicates automatically.

**Where:** `memory.rs:843-887` — `extract_keywords()`

**Algorithm (YAKE — 5 features per word):**

1. **Preprocessing:** Tokenize text, identify sentences
2. **Feature 1 — Casing (TCase):** Uppercase ratio. Words with unusual casing (acronyms, proper nouns) score higher.
3. **Feature 2 — Word Position (TPos):** Earlier in document = more important. `TPos = ln(ln(3 + median_sentence_position))`
4. **Feature 3 — Word Frequency (TFreq):** Normalized frequency. BUT high frequency = less discriminative (opposite of naive counting).
5. **Feature 4 — Word Relatedness (TRel):** Co-occurrence strength with surrounding words. Words that appear in diverse contexts score higher.
6. **Feature 5 — Word DifSentence (TDif):** How many different sentences contain this word. Words spread across many sentences are more important.
7. **Inverse score per word:** `H = (TRel × TPos) / (TCase + (TFreq / TRel) + (TDif / TRel))`
8. **N-gram candidate scoring:** Generate 1-3 word candidates, score = product of member word scores × additional factors
9. **Deduplication:** Levenshtein distance — remove near-identical candidates

**Implementation approach:**
- Implement YAKE in pure Rust (~80-100 lines) to avoid LGPL dependency
- OR use `rake` crate (MIT) for RAKE as stepping stone, add YAKE features on top
- Rename current `extract_keywords()` to `extract_keywords_frequency()` (keep as fallback)
- New `extract_keywords()` uses YAKE, returns top 8 keyphrases
- Add domain-specific stopwords for code discussions: "function", "class", "return", "const", "let", "var", "import", etc.

**Test cases:**
- "Machine learning model training pipeline" → should extract "machine learning", "training pipeline" (not just "machine", "learning", "model")
- "Rust async error handling patterns" → should extract "async error handling", "rust" (not "error", "handling" as separate words)
- "I prefer dark mode for coding at night" → should extract "dark mode", "coding" (personal preference, not technical)

---

### Phase 3: Local Embedding Layer (`fastembed`) [DONE]

**Severity:** HIGH — Enables Phases 4-6 to work offline
**Effort:** 2-3 hours
**Dependencies:** `fastembed` crate (Apache-2.0) — **APPROVED by user Mar 14, 2026**

**Implementation notes (Mar 14, 2026):**
- `fastembed` v5.12.1 with `ort-load-dynamic` + `online` features (x86_64-pc-windows-gnu has no prebuilt ort-sys binaries)
- ONNX Runtime v1.23.2 DLL placed at `~/.hive/onnxruntime/onnxruntime.dll` (bypasses old v1.17.1 in System32 via `ORT_DYLIB_PATH`)
- `LOCAL_EMBEDDER: OnceLock<Option<Mutex<TextEmbedding>>>` — singleton, `Mutex` needed because `embed()` takes `&mut self`
- `get_local_embedding()` is sync, called from async `get_embedding()` via `tokio::task::spawn_blocking`
- `cosine_similarity()` now returns 0.0 for dimension mismatches (384-dim fastembed vs 1536-dim OpenAI) — old memories gracefully degrade to FTS5-only matching
- `memory_has_embeddings_provider()` returns true when fastembed available (bundled dep, always works unless init failed)
- Tests: 236 → 243 Rust (+4 cosine dimension tests, +3 fastembed functional tests)

**What:** Add `fastembed-rs` as local embedding provider. Downloads `all-MiniLM-L6-v2` (22MB ONNX model) on first use, cached forever after.

**Where:** `memory.rs` — `get_embedding()` cascade (lines ~570-690)

**Current cascade:** OpenAI → DashScope → OpenRouter → Ollama → graceful degrade to FTS5-only

**New cascade:** fastembed (local ONNX, 0ms network, ~10-50ms compute) → OpenAI → DashScope → OpenRouter → Ollama → empty vec

**Implementation:**
```rust
// In memory.rs, add at top of get_embedding cascade:
use fastembed::{TextEmbedding, InitOptions, EmbeddingModel};
use std::sync::OnceLock;

static FASTEMBED: OnceLock<Option<TextEmbedding>> = OnceLock::new();

fn get_local_embedding(text: &str) -> Result<Vec<f64>, String> {
    let model = FASTEMBED.get_or_init(|| {
        TextEmbedding::try_new(InitOptions {
            model_name: EmbeddingModel::AllMiniLML6V2,
            show_download_progress: true,
            ..Default::default()
        }).ok()
    });
    match model {
        Some(m) => {
            let embeddings = m.embed(vec![text], None)
                .map_err(|e| format!("fastembed error: {}", e))?;
            Ok(embeddings[0].iter().map(|&x| x as f64).collect())
        }
        None => Err("fastembed not available".to_string())
    }
}
```

**Key considerations:**
- `all-MiniLM-L6-v2` produces 384-dim vectors vs OpenAI's 1536-dim
- Must NOT mix dimensions in same DB — either normalize or keep separate columns
- Solution: store embedding model name alongside vector, skip cosine when dimensions mismatch
- Model downloaded to `~/.cache/fastembed/` (or custom path)
- First-use download ~22MB, then fully offline forever
- Thread-safe via `OnceLock` — single instance shared across all threads

**Fallback behavior:** If `fastembed` init fails (e.g., missing ONNX runtime), cascade continues to cloud providers. If all fail, empty vec → FTS5-only search (existing behavior).

---

### Phase 4: Semantic Skills Matching (Tool2Vec Pattern) [DONE]

**Severity:** HIGH
**Effort:** 4-6 hours
**Dependencies:** Phase 3 (fastembed for offline embeddings)

**Implementation notes (Mar 14, 2026):**
- `builtin_skill_queries()`: 5 skills × 8 synthetic queries each (40 total, hardcoded)
- `SKILL_VECTORS: OnceLock<HashMap<String, Vec<f64>>>` — computed once, cached for process lifetime
- Built-in skills: average 8 query embeddings → single 384-dim Tool2Vec centroid per skill
- Custom skills (user-added): embed `name + first 200 chars` as fallback
- `find_relevant_skills()` now: embed query → cosine vs skill vectors → threshold 0.3 → sort by similarity
- Keyword fallback preserved: if fastembed unavailable, uses word overlap (original behavior)
- `cosine_similarity` and `get_local_embedding` made `pub(crate)` for cross-module access
- Tests: 243 → 248 Rust (+2 averaging, +1 keyword fallback, +1 query coverage, +1 semantic matching)

**What:** Replace substring matching with embedding similarity using Tool2Vec pattern.

**Where:** `harness.rs:796-825` — `find_relevant_skills()`

**Tool2Vec Algorithm (from the paper, adapted for HIVE):**

1. **One-time precomputation (on skill load or change):**
   - For each skill file in `~/.hive/skills/`:
     - Generate 5-8 synthetic example queries: "What would a user say that needs this skill?"
     - Examples for `coding.md`: "write a function", "fix this bug", "refactor the code", "implement an interface", "debug this error"
     - Examples for `research.md`: "search for information", "find documentation", "look up this topic", "what does the research say"
     - Embed each query using fastembed/cloud
     - Average all query embeddings → skill vector (single 384-dim vector per skill)
   - Cache skill vectors in memory + persist to SQLite (table: skill_vectors)

2. **Per-query matching:**
   - Embed user message (~10ms with fastembed)
   - Cosine similarity against each skill vector
   - Return skills above threshold (0.3), sorted by similarity, cap at 2-3
   - Truncate content to `max_chars` (existing behavior)

3. **Fallback:** If no embedding model available, use current substring matching

**Synthetic query generation:**
- If cloud LLM available: ask model to generate queries (one-time cost, ~$0.01)
- If offline: use hardcoded example queries per built-in skill (5 skills × 8 queries = 40 queries, manually curated)
- Store in skill metadata (YAML frontmatter or separate JSON)

**Where to store skill vectors:**
```sql
CREATE TABLE IF NOT EXISTS skill_vectors (
    skill_name TEXT PRIMARY KEY,
    embedding TEXT NOT NULL,  -- JSON array of f64
    model_name TEXT NOT NULL, -- e.g., "all-MiniLM-L6-v2"
    updated_at TEXT NOT NULL
);
```

**Why Tool2Vec > direct description embedding:**
- Developer-written descriptions live in different semantic space than user queries
- "CI/CD pipeline configuration" (description) ≠ "help me deploy my app" (user query)
- Synthetic queries bridge this gap — 25-30% recall improvement (ToolBank benchmark)

---

### Phase 5: Semantic Task Routing (3-Layer Tiered Router) [DONE]

**Severity:** HIGH
**Effort:** 4-6 hours
**Dependencies:** Phase 3 (fastembed)

**Implementation notes (Mar 14, 2026):**
- `classify_task()` refactored into 3-layer tiered router (was flat keyword matching)
- Layer 1 (`classify_by_keywords`): Existing keyword lists, returns `Option` — `None` escalates to Layer 2
- Layer 2 (`classify_by_embedding`): Pre-computed utterance embeddings per specialist, MAX aggregation, threshold 0.45, cosine → confidence mapping
- `SPECIALIST_VECTORS: OnceLock<HashMap<SlotRole, Vec<Vec<f64>>>>` — stores ALL utterance embeddings (not averaged), computed once
- `specialist_utterances()`: 5 specialists × 8 utterances each = 40 synthetic queries (matching roadmap spec)
- Layer 3 (LLM structured output): deferred — requires async, `classify_task` is sync. Consciousness fallback serves the same purpose
- Tests: 248 → 253 Rust (+2 layer tests, +1 integration, +1 coverage, +1 semantic routing)

**What:** Replace keyword matching with 3-layer tiered router.

**Where:** `orchestrator.rs:62-129` — `classify_task()`

**Architecture:**

```
User Message
     │
     ▼
┌──────────────────────────┐
│ Layer 1: Keyword Rules   │ ◄── 0ms, offline, deterministic
│ Contains "code"+"write"  │     Current classify_task() logic
│ → Coder (high confidence)│     Only for UNAMBIGUOUS cases
│ No match? → Layer 2      │
└──────────┬───────────────┘
           │
           ▼
┌──────────────────────────┐
│ Layer 2: Embedding Sim   │ ◄── 5-15ms, offline after model load
│ cosine(msg, utterances)  │     fastembed all-MiniLM-L6-v2
│ MAX aggregation per role │     Pre-computed utterance vectors
│ Best > 0.82? → route     │     8-10 examples per specialist
│ Below? → Layer 3         │
└──────────┬───────────────┘
           │
           ▼
┌──────────────────────────┐
│ Layer 3: LLM Classify    │ ◄── 200-800ms, requires API
│ Structured output prompt │     Cheapest available model
│ enum: specialist name    │     Cached in LRU(100)
│ Always returns an answer │
└──────────────────────────┘
```

**Layer 2 — Pre-computed Utterance Vectors:**

```
Consciousness (general reasoning, planning, discussion):
  - "explain this concept to me"
  - "what do you think about this approach"
  - "help me understand how this works"
  - "analyze this situation"
  - "let's discuss the architecture"
  - "summarize what we've done"
  - "give me your opinion on this design"
  - "help me plan the implementation"

Coder (actual code production — writing, debugging, reviewing):
  - "write a function that does X"
  - "fix this bug in the code"
  - "refactor this function"
  - "implement this interface"
  - "add error handling to this"
  - "review this code for issues"
  - "create a class for X"
  - "debug why this test fails"

Terminal (system admin, command execution, file ops):
  - "run this command"
  - "execute the build script"
  - "check the server logs"
  - "install this package"
  - "list the files in this directory"
  - "start the development server"
  - "kill the process on port 8080"
  - "check disk usage"

WebCrawl (web search, URL fetching, online research):
  - "search the web for information about"
  - "find documentation for this library"
  - "look up the latest release notes"
  - "what's the current status of this project"
  - "research best practices for"
  - "fetch this URL and extract the content"
  - "find articles about this topic"
  - "check if this API endpoint is documented"

ToolCall (complex multi-tool operations, pipelines):
  - "use the tool to create a file"
  - "call the API and process the response"
  - "save this information to memory"
  - "send a message to the telegram channel"
  - "check the integration status"
  - "execute a multi-step workflow"
  - "automate this sequence of operations"
  - "run the pipeline end to end"
```

**Layer 3 — LLM Structured Output Prompt:**
```
System: You are a task router. Given a user message, determine which specialist should handle it.
Available specialists:
- Consciousness: General reasoning, planning, philosophical discussion, architecture decisions, explaining concepts
- Coder: Writing, debugging, reviewing, or refactoring actual code. Must involve code production.
- Terminal: System administration, running commands, file operations, process management
- WebCrawl: Web search, URL fetching, online research, documentation lookup
- ToolCall: Complex multi-tool operations, data processing pipelines, automation

Reply with ONLY the specialist name (Consciousness, Coder, Terminal, WebCrawl, or ToolCall).

User message: {message}
```

**LRU Cache:** Cache recent classifications (message hash → (role, confidence, timestamp)). Evict after 100 entries or 1 hour.

**Critical test cases:**
- "Help me think through the code architecture" → Consciousness (NOT Coder)
- "Write me a poem about databases" → Consciousness (NOT Coder)
- "Find the bug in my code" → Coder (code production, NOT WebCrawl)
- "Search for the React documentation" → WebCrawl
- "Run cargo test" → Terminal
- "Save this to memory and send it to Telegram" → ToolCall

---

### Phase 6: Semantic Topic Classification [DONE]

**Severity:** HIGH
**Effort:** 2-3 hours
**Dependencies:** Phase 3 (fastembed for offline centroids)

**Implementation notes (Mar 14, 2026):**
- `topic_seed_examples()`: 6 categories × 5 seed sentences each (30 total) — technical, project, personal, conversational, creative, reference
- `TOPIC_CENTROIDS: OnceLock<Vec<(String, Vec<f64>)>>` — averaged seed embeddings per category
- `classify_topic_semantic()`: embed first 300 chars → cosine vs centroids → nearest above 0.3 threshold
- Cascade: keywords first (uses structured metadata: extracted keyphrases + tags) → semantic for "general" fallback
- `average_embeddings()` extracted to `memory.rs` as `pub(crate)` — shared by Phase 4 (Tool2Vec) and Phase 6 (topic centroids)
- Expanded from 3 categories (technical/project/personal) to 6 (+ conversational/creative/reference)
- Tests: 253 → 258 Rust (+1 coverage, +3 semantic classification, +1 cascade ordering)

**What:** Replace hardcoded keyword lists with semantic classification cascade.

**Where:** `memory.rs:893-959` — `classify_topic()`

**Cascade:**

1. **LLM available:** Single cheap inference call
   ```
   Classify this text into exactly one category:
   technical, project, personal, conversational, creative, reference
   Reply with ONLY the category name.

   Text: {first 200 chars of content}
   ```
   Cost: ~$0.001 per classification. Cache by content hash (same content = same topic).

2. **Embedding model available, no LLM:** Centroid-based classification
   - Maintain centroid embeddings for each topic category
   - On new memory: embed content, find nearest centroid by cosine similarity
   - Update centroids incrementally: `new_centroid = (old_centroid * count + new_embedding) / (count + 1)`
   - Initialize centroids from seed examples (10 per category)

3. **No model available:** Current keyword-based classification (fallback)

**Category centroids — seed examples:**
```
technical: "debugging a null pointer exception", "implementing the REST API", "the function returns incorrect values", "optimizing the database query performance", "refactoring the authentication module"
project: "the sprint deadline is Friday", "we decided to use PostgreSQL", "the roadmap for Q2 includes migration", "blocking issue on the deployment pipeline", "milestone 3 is 80% complete"
personal: "I prefer dark mode", "my coding style uses snake_case", "I find tabs more readable than spaces", "I work best in the morning", "my favorite language is Rust"
conversational: "hello how are you", "thanks for the help", "that makes sense", "interesting point", "let me think about that"
creative: "write me a poem about recursion", "generate a story about AI", "create a metaphor for distributed systems", "describe this concept as if explaining to a child"
reference: "the documentation says to use v3", "according to the RFC specification", "the GitHub issue mentions this fix", "the error code 404 means not found"
```

**Cache implementation:**
```rust
use std::collections::HashMap;
use std::sync::Mutex;

static TOPIC_CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

fn classify_topic_cached(content: &str, ...) -> String {
    let hash = hash_text(&content[..content.len().min(500)]);
    let cache = TOPIC_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(guard) = cache.lock() {
        if let Some(topic) = guard.get(&hash) {
            return topic.clone();
        }
    }
    let result = classify_topic_semantic(content, ...);
    if let Ok(mut guard) = cache.lock().or_else(|e| Ok::<_, ()>(e.into_inner())) {
        guard.insert(hash, result.clone());
    }
    result
}
```

---

### Phase 7: Progressive Context Summarization [DONE]

**Severity:** MEDIUM-HIGH
**Effort:** 4-6 hours
**Dependencies:** None (uses existing `chatWithProvider`)

**What:** Replace message dropping with progressive summarization.

**Where:** `useChat.ts:900-960` — context pressure handling

**Current behavior:**
1. At ~70% context: ONE summarization attempt (model-based if cloud, fallback topic extraction)
2. After that: `truncateMessagesToFit()` drops oldest messages (pure truncation)
3. No progressive summarization, no importance scoring, no pinning

**New behavior:**

```
50% context: Flag for preparation. No action yet.

65% context: Structured summarization of oldest 30% of messages.
  - Use cheapest available model (Sonnet, local 7B)
  - Structured prompt (Factory.ai pattern):
    "Summarize this conversation for continuity. Include:
     ## Current State - What task is being worked on, what's accomplished
     ## Key Decisions - Technical decisions and rationale
     ## Modified Files - Exact file paths changed
     ## Pending Work - Next steps, blockers
     ## Critical Context - Preferences, error patterns, anything costly to rediscover
     Be specific. Include file paths, function names, error messages.
     Limit: 2000 tokens."
  - Replace old messages with single system message containing summary
  - Keep most recent 70% of messages raw (high fidelity)

80% context: Aggressive summarization.
  - Keep only last 10 messages raw
  - Everything else → comprehensive summary
  - Re-summarize: old summary + newly evicted messages → new summary (MemGPT pattern)

95% context: Emergency compression.
  - Summarize everything except system prompt + last 5 messages
  - Clear old tool results (Anthropic pattern: 84% token reduction)
  - This is the "last resort" before actual message dropping
```

**Message importance scoring (optional enhancement):**
- Messages containing tool results: LOW importance (can be re-executed)
- Messages containing user decisions: HIGH importance (pin)
- Messages containing error information: MEDIUM importance
- Greeting/acknowledgment messages: LOWEST importance (summarize first)

**Cost:** 1 LLM call per summarization cycle. Use the cheapest available model — summarization is a well-solved task even for small models.

---

### Phase 8: Memory Lifecycle (Decay + Consolidation) [ALL DONE]

**Severity:** MEDIUM
**Effort:** 4-6 hours
**Dependencies:** Phase 3 (fastembed for clustering)

**What:** Add memory decay, periodic consolidation, active forgetting.

**Where:** `memory.rs` — new functions

#### 8A: Memory Decay (Power-Law)

**Function:**
```rust
fn decay_weight(hours_since_last_access: f64, access_count: i64) -> f64 {
    let time_decay = (1.0 + hours_since_last_access).powf(-0.3);
    let strength_factor = 1.0 + (1.0 + access_count as f64).ln() * 0.5;
    time_decay * strength_factor
}
```

**Effect:**
| Time since access | Access count = 0 | Access count = 10 | Access count = 100 |
|---|---|---|---|
| 1 hour | 1.00 | 1.70 | 2.31 |
| 1 day (24h) | 0.38 | 0.65 | 0.88 |
| 1 week (168h) | 0.22 | 0.37 | 0.50 |
| 1 month (720h) | 0.14 | 0.24 | 0.32 |
| 1 year (8760h) | 0.07 | 0.13 | 0.17 |

Apply during search scoring (multiply with existing relevance score), NOT during storage. Never delete based on decay alone.

#### 8B: Memory Archival

```sql
-- Run on app startup
-- Archive memories with very low effective strength and no access in 90+ days
UPDATE memories
SET tier = 'archived'
WHERE tier IN ('short_term', 'long_term')
AND strength < 0.1
AND last_accessed < datetime('now', '-90 days');
```

Archived memories are excluded from default search but recoverable via explicit tier filter.

#### 8C: Memory Consolidation (Periodic) [DONE]

**Implementation notes (Mar 14, 2026):**
- `consolidate_memories(conn)` → groups by topic tag → clusters with cosine > 0.7 → merges clusters of 3+
- Greedy clustering: best-match centroid assignment, centroid recomputed after each addition via `average_embeddings_ref()`
- Merged content: concatenated with `---` separators, truncated to 4800 chars (3 chunks max)
- `tier: 'consolidated'` for originals (0.3x tier_weight — recoverable but deprioritized)
- MAGMA `absorbed` edges: consolidated →absorbed→ each original (with cluster_size metadata)
- Consolidated memory: `source: "consolidation"`, `tier: long_term`, inherits max strength
- Runs on `memory_promote` alongside archival (Phase 8B) — triggered on app startup
- LLM consolidation deferred — structured merge works offline. Can upgrade later with LLM summarization.
- Tests: 258 → 263 (+5: sparse skip, grouping, clustering, strength, no-reconsolidate)

Triggered on `memory_promote` (app startup or periodic):

1. Query memories grouped by topic tag
2. For each topic with > 10 memories:
   - Compute pairwise embedding similarity (using stored chunk embeddings)
   - Cluster similar memories (cosine > 0.7)
   - For clusters > 3 memories:
     - Merge content with structured separators
     - Save consolidated memory with `source: "consolidation"`
     - Mark originals as `tier: 'consolidated'` (not deleted — recoverable)
     - Consolidated memory inherits highest strength from constituents

3. Log consolidation results to app log

#### 8D: Active Forgetting (Mem0 DELETE Pattern) [DONE]

**Implementation notes (Mar 14, 2026):**
- `check_supersession(conn, new_id, embeddings, tags)` — runs after every `write_memory_with_tier` (except consolidation source)
- Finds top-5 similar memories by cosine > 0.85 threshold
- Same topic tag required for supersession (different topics = different domain, no conflict)
- `tier: 'superseded'` for old memory (0.2x tier_weight — nearly invisible but recoverable)
- MAGMA `supersedes` edge: new →supersedes→ old (with similarity + auto_superseded metadata)
- Skips already-superseded/consolidated memories (prevents cascading)
- Offline-only: no LLM needed — same topic + high similarity is the heuristic
- Tests: 263 → 269 (+6: marks superseded, MAGMA edge, requires same topic, skips consolidation, tier ordering, no double-supersede)

When saving a new memory:
1. Embed new memory content
2. Retrieve top-5 similar existing memories by cosine similarity (> 0.85)
3. If same topic tag AND similarity > 0.85:
   - Mark old memory as `tier: 'superseded'`
   - Add edge: new_memory →supersedes→ old_memory
   - Log supersession event

**Heuristic:** Same topic tag + cosine > 0.85 = "same domain, newer version". Works offline without LLM. Future enhancement: LLM contradiction detection for subtle conflicts.

---

## Architecture After All Phases

```
User Message
     │
     ├──► Keyword Extraction (YAKE, offline, <1ms)
     │         └──► Memory indexing, MAGMA graph edges, display labels
     │
     ├──► Task Routing (3-layer tiered)
     │         ├── L1: Keywords (0ms) ──► unambiguous → route immediately
     │         ├── L2: Embedding sim (5ms) ──► confident (>0.82) → route
     │         └── L3: LLM classify (300ms) ──► ambiguous → route
     │
     ├──► Skills Injection (Tool2Vec pattern)
     │         └── Embed query → cosine vs skill vectors → top 2-3 skills
     │
     └──► Specialist Execution
              ├── Local: route_to_specialist tool (existing, unchanged)
              └── Cloud: chatWithTools() + tool loop (Phase 1 fix)

Memory Save Pipeline:
     │
     ├──► YAKE keywords (always, <1ms, offline)
     ├──► Embedding generation (fastembed → cloud cascade, ~10-50ms)
     ├──► Topic classification (LLM → centroid → keyword cascade)
     ├──► Near-duplicate check (cosine > 0.92, existing)
     ├──► Graph edges (FTS5 keyword overlap, existing)
     └──► Store: keywords + embedding + topic + tier + strength

Memory Lifecycle:
     │
     ├──► Reinforcement on recall (existing, logarithmic strength growth)
     ├──► Promotion: short_term → long_term (access_count > 3, existing)
     ├──► Decay: power-law scoring multiplier (Phase 8A)
     ├──► Consolidation: periodic clustering + LLM merge (Phase 8C)
     ├──► Active forgetting: contradiction → supersede (Phase 8D)
     └──► Archival: strength < 0.1 + 90 days → archived (Phase 8B)

Context Management:
     │
     ├── 50%: prepare (flag, no action)
     ├── 65%: structured summarization (oldest 30%)
     ├── 80%: aggressive summarization (keep last 10 + summary)
     └── 95%: emergency compression (tool result clearing + minimal messages)
```

---

## Embedding Model Dimensions Compatibility

**Problem:** `all-MiniLM-L6-v2` (fastembed) produces 384-dim vectors. OpenAI `text-embedding-3-small` produces 1536-dim. These CANNOT be compared by cosine similarity.

**Solution options:**
1. **Separate columns:** `embedding_384` and `embedding_1536` in chunks table. Compare only within same dimension.
2. **Model name tracking:** Store `embedding_model` alongside each vector. Skip cosine when models differ.
3. **One model per installation:** Use fastembed if available, else cloud. Don't mix.
4. **Dimensionality reduction:** OpenAI supports `dimensions: 384` parameter to output 384-dim vectors.

**Recommended:** Option 4. When using OpenAI, set `dimensions: 384` to match fastembed. All vectors are 384-dim, all comparable. Ollama's `nomic-embed-text` outputs 768-dim — would need its own handling.

**Practical approach:** Track model name per embedding. Only compare embeddings from same model. On model change, existing embeddings become FTS5-only (text search still works, just no vector component until re-embedded).

---

## Guiding Principles

1. **Rename, don't replace.** Current functions become `*_fallback()`. New semantic versions try first, fall back gracefully.
2. **Offline-first.** `fastembed` makes everything work without API keys. Degrade to keywords, never break.
3. **One commit per fix.** Each phase is independently testable and revertable.
4. **Test after every change.** Baseline: 228 Rust / 103 vitest / 0 tsc errors.
5. **No new crates without approval.** Supply chain security per user policy.
6. **P2: Provider Agnosticism.** Local and cloud must be interchangeable. Every semantic upgrade must have a non-API fallback.
7. **P4: Errors Are Answers.** Log failures, never silently swallow them. Fallback paths must log which tier they're using.
8. **P5: Fix The Pattern.** When changing a pattern (e.g., embedding model), grep for ALL instances codebase-wide.

---

## Quick Reference: Files Modified Per Phase

| Phase | Files | Nature |
|---|---|---|
| 1 | useChat.ts | Bug fix (cloud specialist tool gap) |
| 2 | memory.rs | Feature (YAKE keyword extraction) |
| 3 | memory.rs, Cargo.toml | Feature (fastembed local embeddings) |
| 4 | harness.rs, memory.rs | Feature (Tool2Vec skills matching) |
| 5 | orchestrator.rs | Feature (3-layer tiered router) |
| 6 | memory.rs | Feature (semantic topic classification) |
| 7 | useChat.ts | Enhancement (progressive summarization) |
| 8 | memory.rs | Feature (decay, consolidation, forgetting) |

---

## Open Questions for User

1. ~~Approve `fastembed` crate?~~ **APPROVED** (Mar 14, 2026)
2. ~~YAKE: implement manually or use `rake` crate?~~ **Implemented manually** (~150 lines pure Rust, no new deps)
3. **Specialist tool filtering?** Should cloud specialists get ALL 44 tools or filtered subsets per role?
4. **Embedding dimension strategy?** Use OpenAI `dimensions: 384` to match fastembed? Or track model per embedding?

---

*Generated from full codebase audit (3 prior audits, 32 resolved findings) + 5 parallel research agents + competitor analysis of 9 frameworks + survey of 12 research papers.*
*March 14, 2026 — Claude Code (Opus 4.6)*

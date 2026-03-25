# HIVE Architecture Overview v2

**HIVE: Hierarchical Intelligence with Virtualized Execution**

## Project Vision

PRISM (Personal Reasoning Intelligence Swarm Model) - formerly conceptualized - now evolves into **HIVE**: a local, VRAM-efficient cognitive architecture implementing hot-swappable specialist AI agents coordinated by a persistent reasoning layer.

**Core Metaphor:** Human brain with specialized regions + executive consciousness

---

## What We're Building

### The Problem with Monolithic Models

**Traditional Approach:**
```
User → Single Large Model (14B-70B) → Response
```

**Issues:**
- Model good at everything, great at nothing
- 10-50GB VRAM locked permanently
- Can't specialize for specific domains
- No task parallelization
- Expensive inference for simple tasks

### HIVE Approach

```
User → [Consciousness Layer] → Route to Specialist → Execute → Return
           ↓                                                    ↑
        [MAGMA Memory Substrate] ←──────────────────────────────┘
```

**Advantages:**
- Specialists optimized for specific tasks
- VRAM used only when needed (hot-swapping)
- Multiple small experts > single generalist
- Persistent memory across swaps
- Scales with task complexity

---

## System Architecture

### Layer 1: Consciousness (Always Active)

**Component:** Reasoning Agent
**Model:** LFM2-2.6B-Transcript (or similar 2-3B reasoning model)
**VRAM:** ~3GB (Q4_K_M quantization)
**Role:** Executive function, orchestration, planning

**Responsibilities:**
- Analyze incoming user requests
- Determine which specialist(s) needed
- Coordinate multi-step tasks
- Maintain conversational state
- Monitor system health

**Always loaded because:**
- Fast response for planning
- Minimal VRAM footprint
- Acts as "consciousness" - continuous awareness

---

### Layer 2: Specialist Agents (Hot-Swappable)

#### Specialist 1: CODER

**Purpose:** Code generation, architecture, debugging, refactoring

**Model Options:**
- **Primary:** NousCoder-14B (Q5_K_M, ~10GB)
  - Best for: Competitive programming, algorithms, complex logic
  - Trained via RL on 24K coding problems
  - LiveCodeBench: 67.87% Pass@1
  
- **Alternative:** Qwen2.5-Coder-14B-Instruct (Q5_K_M, ~10GB)
  - Best for: Real-world applications, multi-file projects, APIs
  - Trained on 5.5T tokens of code + text-code grounding
  - HumanEval: ~87%, excellent at software engineering

- **Lightweight:** Qwen2.5-Coder-7B-Instruct (Q5_K_M, ~6GB)
  - Best for: VRAM efficiency, quick edits, simpler tasks
  - 80%+ of 14B capability, half the VRAM

**Load When:**
- Building new features
- Debugging code
- Refactoring architecture
- Code review
- Documentation generation

**Sleep After:** Task completion, context saved to MAGMA

---

#### Specialist 2: TERMINAL

**Purpose:** Safe system command execution, file operations

**Model:** SETA-RL-Qwen3-8B (Q5_K_M, ~6GB)
- Reinforcement learning trained for safe terminal use
- Understands system state, file structures
- Prevents dangerous operations

**Load When:**
- Running shell commands
- File system operations
- System monitoring
- Package installation
- Git operations

**Sleep After:** Command execution complete

---

#### Specialist 3: WEBCRAWL

**Purpose:** Web scraping, data gathering, research

**Architecture:** Hybrid
- **Code-based:** BeautifulSoup/Scrapy for actual scraping
- **LLM layer:** Qwen2.5-3B-Instruct (Q5_K_M, ~3GB) for extraction/summarization

**Load When:**
- Researching topics
- Gathering documentation
- Monitoring web sources
- Extracting structured data

**Sleep After:** Research compiled

---

#### Specialist 4: TOOLCALL

**Purpose:** API interaction, tool selection, function calling

**Model:** Fine-tuned OPT-350M (Q8_0, <1GB)
- Minimal VRAM footprint
- Fast inference
- Custom-trained for tool orchestration

**Load When:**
- Calling external APIs
- Selecting appropriate tools
- Orchestrating tool chains

**Note:** May keep persistently loaded due to tiny size

---

### Layer 3: Memory Substrate (MAGMA)

**Location:** System RAM (not VRAM)
**Storage:** 32GB available (your specs)

**MAGMA: Multi-Graph Memory Architecture**

#### Graph Types

**1. Semantic Graph**
- Knowledge representation
- Concept relationships
- Code dependencies
- Technology stack mappings

**2. Temporal Graph**
- Event sequences
- Agent wake/sleep timestamps
- File modification history
- Task progression timeline

**3. Causal Graph**
- Cause → Effect relationships
- Decision reasoning chains
- Error → Fix mappings
- Feature → Implementation paths

**4. Entity Graph**
- Files, functions, variables
- Agents and their states
- Projects and components
- User preferences

#### Persistence Format

```
magma/
├── graphs/
│   ├── semantic.db       # SQLite + embeddings
│   ├── temporal.db       # Event log
│   ├── causal.db         # Reasoning chains
│   └── entity.db         # Object registry
├── agent_states/
│   ├── coder_state.json
│   ├── terminal_state.json
│   ├── webcrawl_state.json
│   └── toolcall_state.json
└── embeddings/
    └── semantic_vectors.npy  # For similarity search
```

---

## Operational Flow

### Example: Building a Flask API

**Step 1: User Request**
```
User: "Build a Flask API with user authentication and rate limiting"
```

**Step 2: Consciousness Analysis**
```
[LFM2 Reasoning Layer]
Task decomposition:
1. Design API structure
2. Implement authentication
3. Add rate limiting
4. Write tests

Required specialists: CODER
Complexity: High
Estimated time: 30-60 minutes
```

**Step 3: Wake Coder**
```
[HIVE Orchestrator]
- Check VRAM: 3GB used (reasoning), 13GB available ✓
- Load NousCoder-14B (10GB)
- Inject context from MAGMA:
  * User's preferred frameworks (Flask, SQLAlchemy)
  * Previous auth implementations
  * Code style preferences
- Total VRAM: 13GB / 16GB
```

**Step 4: Execute**
```
[NousCoder Agent]
- Generate API structure
- Implement authentication (JWT-based, per user history)
- Add rate limiting (Redis-backed, configurable)
- Create tests
- Output: Complete codebase
```

**Step 5: Sleep Coder**
```
[HIVE Orchestrator]
- Extract state:
  * Files created: app.py, auth.py, rate_limit.py, tests/
  * Context: "Implemented Flask API with JWT auth and Redis rate limiting"
  * Next steps: "Could add logging, deployment config"
- Save to MAGMA
- Unload NousCoder (free 10GB VRAM)
- VRAM: 3GB (back to baseline)
```

**Step 6: Testing (Optional)**
```
[Consciousness decides: Need terminal for testing]
- Wake SETA Terminal Agent (6GB)
- Run: pytest tests/
- Parse results
- Sleep Terminal Agent
```

---

## Multi-Agent Collaboration Example

**Task:** "Find best practices for React hooks and implement a custom hook"

**Workflow:**

```
1. [Consciousness] Decomposes:
   - Research React hooks best practices
   - Design custom hook
   - Implement hook
   
2. [Wake WebCrawl Agent]
   - Scrape React docs
   - Gather articles on hook patterns
   - Compile findings
   [Sleep WebCrawl]
   
3. [MAGMA] Stores research in semantic graph

4. [Wake Coder Agent]
   - Retrieves research from MAGMA
   - Designs custom hook based on best practices
   - Implements with tests
   [Sleep Coder]
   
5. [Consciousness] Returns complete solution
```

**Total VRAM peak:** 3GB (reasoning) + 3GB (webcrawl) or 10GB (coder) = 13GB max

---

## VRAM Management Strategy

### Baseline State (Idle)
```
Consciousness (LFM2-2.6B): 3GB
Available: 13GB
```

### Active States

**Light Task (Web Research)**
```
Consciousness: 3GB
WebCrawl: 3GB
Total: 6GB
Available: 10GB
```

**Heavy Task (Code Generation)**
```
Consciousness: 3GB
NousCoder-14B: 10GB
Total: 13GB
Available: 3GB (safety buffer)
```

**Medium Task (Terminal)**
```
Consciousness: 3GB
SETA-8B: 6GB
Total: 9GB
Available: 7GB
```

**Multi-Agent (WebCrawl + ToolCall)**
```
Consciousness: 3GB
WebCrawl: 3GB
ToolCall: <1GB
Total: 7GB
Available: 9GB
```

---

## Context Injection Protocol

### Sleep Event

**What to save:**
- Current conversation history (compressed)
- Files being worked on + their states
- Intermediate reasoning/plans
- Task progress percentage
- Next intended actions

**Format:**
```json
{
  "agent": "coder",
  "timestamp": "2026-01-20T16:30:00Z",
  "context": {
    "task": "Implementing user auth",
    "files": ["auth.py", "models.py", "tests/test_auth.py"],
    "progress": 0.75,
    "state": "Implemented JWT, need to add refresh tokens",
    "next_steps": ["Add refresh token logic", "Write tests"]
  },
  "conversation_history": [...],
  "vram_freed": 10.0
}
```

### Wake Event

**What to inject:**
```
Good morning, Coder Agent.

SLEEP SUMMARY:
- Last active: 1h 23m ago
- You were: Implementing JWT authentication
- Progress: 75% complete

CHANGES WHILE ASLEEP:
- Reasoning layer decided: Use Redis for token storage
- Files modified: config.py (added Redis connection)
- Other agents active: None

FILES YOU WERE WORKING ON:
- auth.py (JWT implementation, 75% done)
- models.py (User model, complete)
- tests/test_auth.py (stub created)

CURRENT TASK:
Complete JWT authentication by:
1. Add refresh token logic
2. Write comprehensive tests
3. Add error handling

You have full context. Resume work.
```

**Compression:** If wake context >2K tokens, compress via:
- Summarize conversation history
- Link to files instead of including content
- Provide diffs instead of full file states

---

## Technical Implementation

> **CRITICAL (January 2026):** HIVE is provider-agnostic. The stack below shows
> the CURRENT implementation, but the architecture MUST support swapping ANY
> component. See `ARCHITECTURE_PRINCIPLES.md` for the full philosophy.

### Stack

**Base Infrastructure:**
- **OS:** WSL2 Ubuntu 24.04 on Windows 11
- **GPU Compute:** ROCm 6.4.2.1 (AMD Radeon RX 9060 XT)
- **Inference Engine:** llama.cpp (GGUF models) — **swappable**
- **Python Interface:** llama-cpp-python — **swappable**
- **Memory Backend:** SQLite + NumPy (MAGMA graphs)

**Provider Abstraction Layer (NEW):**
```
┌─────────────────────────────────────────────────────────────┐
│                  HIVE Orchestration Layer                   │
│         (Talks to Provider Interface, not backends)         │
└─────────────────────────┬───────────────────────────────────┘
                          │
              ┌───────────┴───────────┐
              │   Provider Interface   │
              │   (Universal Protocol) │
              └───────────┬───────────┘
       ┌──────────────────┼──────────────────┐
       ▼                  ▼                  ▼
┌─────────────┐    ┌─────────────┐    ┌─────────────┐
│    LOCAL    │    │    CLOUD    │    │   CUSTOM    │
│ ─────────── │    │ ─────────── │    │ ─────────── │
│ llama.cpp   │    │ Claude API  │    │ Self-hosted │
│ Ollama      │    │ OpenAI API  │    │ Fine-tuned  │
│ vLLM        │    │ Gemini API  │    │ Enterprise  │
│ ExLlamaV2   │    │ Azure       │    │ RunPod/etc  │
└─────────────┘    └─────────────┘    └─────────────┘
```

**Orchestration:**
- **Framework:** Custom Python orchestrator
- **State Management:** MAGMA memory substrate
- **Model Loading:** Via Provider Interface (not direct llama-cpp)
- **Context Injection:** Templated prompts + MAGMA retrieval
- **Provider Selection:** Hardware-aware, cost-aware, fallback chains

**UI (To Be Built):**
- **Backend:** Flask or FastAPI
- **Frontend:** React (to be generated by NousCoder)
- **Communication:** WebSocket for streaming responses
- **Monitoring:** Real-time VRAM/performance dashboard
- **Model Selection:** Per-slot provider/model configuration UI

---

## Model Registry

> **NOTE (January 2026):** This registry shows LOCAL GGUF models as primary options.
> However, ANY slot can also be filled by cloud APIs. See the Extended Registry below.

### Primary Models (Local GGUF)

| Agent | Model | Parameters | Quant | VRAM | Purpose |
|-------|-------|------------|-------|------|---------|
| Consciousness | LFM2-2.6B | 2.6B | Q4_K_M | ~3GB | Reasoning, orchestration |
| Coder (Option 1) | NousCoder-14B | 14B | Q5_K_M | ~10GB | Competitive programming |
| Coder (Option 2) | Qwen2.5-Coder-14B | 14B | Q5_K_M | ~10GB | Software engineering |
| Coder (Lightweight) | Qwen2.5-Coder-7B | 7B | Q5_K_M | ~6GB | Efficient coding |
| Terminal | SETA-RL-Qwen3-8B | 8B | Q5_K_M | ~6GB | Safe terminal use |
| WebCrawl | Qwen2.5-3B-Instruct | 3B | Q5_K_M | ~3GB | Summarization |
| ToolCall | OPT-350M (fine-tuned) | 350M | Q8_0 | <1GB | API/tool calling |

### API Providers (Cloud Alternatives)

| Agent | Provider | Model | Cost/1K tokens | Best For |
|-------|----------|-------|----------------|----------|
| Consciousness | Claude API | claude-3-haiku | ~$0.00025 | Fast, cheap reasoning |
| Consciousness | OpenAI | gpt-4o-mini | ~$0.00015 | Alternative fast reasoning |
| Coder | Claude API | claude-3-5-sonnet | ~$0.003/$0.015 | Best code quality |
| Coder | OpenAI | gpt-4-turbo | ~$0.01/$0.03 | Alternative high quality |
| Terminal | Claude API | claude-3-5-sonnet | ~$0.003/$0.015 | Tool use capabilities |
| WebCrawl | Claude API | claude-3-haiku | ~$0.00025 | Fast summarization |
| ToolCall | OpenAI | gpt-4o-mini | ~$0.00015 | Function calling |

**When to use APIs:**
- Hardware limited (laptop, low VRAM)
- Complex task requiring frontier capabilities
- Local model quality insufficient
- User explicitly prefers API

### Alternative Models (Future Consideration)

**Vision (GUI Analysis):**
- STEP3-VL-10B: 10B vision-language model
- Use case: Screenshot debugging, UI analysis
- Not needed for v1

**Larger Coders (If VRAM allows):**
- Qwen3-Coder-480B-A35B: 480B MoE, 35B active
- Would require Q2/Q3 quantization to fit
- Test after v1 stable

---

## Performance Expectations

### Inference Speed (Per Model)

**LFM2-2.6B (Consciousness):**
- Tokens/sec: 40-60 t/s
- Planning latency: <1 second
- Always responsive

**NousCoder-14B:**
- Tokens/sec: 15-25 t/s
- Code generation: 2-5 sec per function
- Quality: High

**SETA-8B:**
- Tokens/sec: 20-30 t/s
- Command interpretation: <1 sec

**Qwen2.5-3B:**
- Tokens/sec: 50-80 t/s
- Summarization: Very fast

### Load/Unload Times

- Small models (<3B): 2-4 seconds
- Medium models (7-8B): 5-8 seconds
- Large models (14B): 8-12 seconds

**Total swap overhead:** ~10-15 seconds for full hot-swap

---

## Design Principles

### 1. VRAM is Precious
- Only load what's needed, when it's needed
- Aggressive unloading after task completion
- Monitor usage constantly

### 2. Memory is Cheap (RAM)
- Store everything in MAGMA
- Disk persistence for long-term memory
- Embeddings for semantic search

### 3. Consciousness is Constant
- Reasoning layer never sleeps
- Always available for quick decisions
- Minimal VRAM footprint

### 4. Specialists are Focused
- Each model does ONE thing well
- No generalist bloat
- Optimized training for domain

### 5. Context is King
- Perfect context injection = seamless experience
- MAGMA ensures no information loss
- Wake briefings are comprehensive

### 6. Provider Agnosticism (Added January 2026)
- **No model or provider is permanent** — all are swappable
- **The framework survives, the models evolve**
- **Local-first, cloud-capable** — user chooses
- **Fallback chains** — if primary fails, alternatives activate
- **Hardware-aware selection** — recommend based on available resources
- **Cost-aware selection** — respect user's budget constraints

> See `ARCHITECTURE_PRINCIPLES.md` for the complete provider-agnostic philosophy.
> This principle ensures HIVE remains relevant regardless of which models or
> providers dominate in 2027, 2028, and beyond.

---

## Evolution Path

### v1: Basic Hot-Swapping
- Manual specialist selection
- Single specialist at a time
- Simple context injection
- File-based MAGMA

### v2: Intelligent Orchestration
- Automatic specialist selection (reasoning layer decides)
- Multi-specialist collaboration
- Compressed context injection
- SQLite-backed MAGMA with embeddings

### v3: Optimization
- Parallel specialist loading (if VRAM allows)
- Predictive pre-loading
- Context caching
- Agent performance monitoring

### v4: Advanced Features
- Test-time training (TTT-E2E)
- Agent drift monitoring
- Self-improvement loops
- Vision integration (STEP3-VL)

---

## Success Criteria

**v1 Complete When:**
- ✓ Consciousness layer active and responsive
- ✓ At least 2 specialists hot-swappable
- ✓ Context preserved across swaps
- ✓ MAGMA persisting state to disk
- ✓ Can complete multi-step task requiring specialist swap

**Production Ready When:**
- ✓ All 4 core specialists operational
- ✓ Sub-10-second swap times
- ✓ <5% context loss across swaps
- ✓ UI for monitoring and interaction
- ✓ 24-hour uptime without crashes

---

## References

See IMPLEMENTATION_THEORY.md for academic foundations and research backing.

---

**End of Architecture Overview v2**

# HIVE Architecture Principles

**Foundational Design Philosophy - January 2026**

> **Note (Feb 2026):** This document's principles are authoritative. The Python code
> examples are conceptual -- the actual codebase uses Rust (12 modules) and TypeScript
> (App.tsx + 8 components). The philosophy applies regardless of implementation language.
>
> **Implementation status:**
> - Provider abstraction: **DONE** (Local llama.cpp, OpenAI, Anthropic, Ollama — all with streaming)
> - Memory substrate: **DONE** (`memory.rs`): SQLite + FTS5 + vector embeddings, hybrid search, markdown daily logs, session-injected recall. Adapted from OpenClaw (MIT)
> - Tool framework: **DONE** (`tools/`): file ops (paginated read_file with offset/limit), terminal, web fetch/search, risk-based approval, agentic loop
> - Slot-based orchestration: **Planned** (Phase 4 — The Brain)

---

## Executive Summary

This document establishes the **non-negotiable architectural principles** that guide all HIVE development. Any future Claude session, developer, or AI agent reading this repository MUST understand and adhere to these principles.

**Core Thesis:** HIVE is a **harness**, not an implementation. It provides the orchestration framework that remains constant while the models, providers, and inference backends change.

---

## Principle 1: Absolute Provider Agnosticism

### The Rule

**NO model, provider, or inference backend shall be hardcoded as a permanent fixture of any role.**

HIVE must treat ALL of the following as interchangeable components:

| Provider Type | Examples |
|--------------|----------|
| **Local Inference (GGUF)** | llama.cpp, Ollama, LM Studio |
| **Local Inference (Other)** | vLLM, TGI, ExLlamaV2 |
| **Cloud APIs** | Claude API, OpenAI API, Google Gemini API |
| **Enterprise APIs** | Azure OpenAI, AWS Bedrock, Vertex AI |
| **Custom Endpoints** | Self-hosted APIs, fine-tuned model servers |

### Why This Matters

**Reality Check (January 2026):**
- Today's "best" local 14B model will be obsolete in 6-12 months
- Today's API pricing and capabilities will change quarterly
- New inference backends will emerge (just as llama.cpp router mode did in Dec 2025)
- Hardware will evolve (AMD ROCm, Intel Arc, Apple MLX, Qualcomm NPU)

**The 2027 Test:** One year from now (January 2027):
- Will NousCoder-14B still be the best coding model? **No.**
- Will llama.cpp be the only inference option? **No.**
- Will Claude/GPT APIs have the same pricing/capabilities? **No.**
- Will GGUF be the dominant format? **Unknown.**

**Therefore:** HIVE must survive ALL of these changes with **zero architecture modifications**.

### Implementation Requirements

```
WRONG:
  def load_coder():
      return Llama(model_path="nouscoder-14b.gguf")  # Hardcoded!

RIGHT:
  def load_coder(config: SlotConfig):
      provider = get_provider(config.provider_type)  # Could be "llama.cpp", "ollama", "claude_api", etc.
      return provider.load(config.model_identifier)
```

**The abstraction layer MUST support:**
1. Local GGUF models via llama.cpp
2. Local models via Ollama
3. Local models via any OpenAI-compatible endpoint
4. Cloud models via official APIs (Claude, GPT, Gemini)
5. Custom endpoints with configurable authentication

---

## Principle 2: Role-Based Architecture (The Slot System)

### The Rule

**HIVE defines ROLES (slots), not models. Any compatible model can fill any slot.**

### The Five Core Slots

| Slot | Purpose | Requirements | Example Fillers |
|------|---------|--------------|-----------------|
| **Consciousness** | Orchestration, routing, planning | Fast inference, good reasoning | Local 3B, Claude Haiku, GPT-4-mini |
| **Coder** | Code generation, debugging | Strong coding benchmarks | Local 14B coder, Claude Sonnet, GPT-4 |
| **Terminal** | Safe command execution | Safety-trained, shell understanding | Local 8B RL model, Claude with tools |
| **WebCrawl** | Summarization, extraction | Fast, good at summarization | Local 3B, Claude Haiku |
| **ToolCall** | API/function calling | Structured output, function calling | Local 350M, any function-calling model |

### Slot Requirements vs. Model Selection

```yaml
# Example: Coder Slot Configuration
coder_slot:
  requirements:
    min_humaneval_score: 70
    capabilities:
      - code_generation
      - code_explanation
      - multi_language
    max_latency_ms: 5000

  # ANY of these can fill the slot:
  options:
    - provider: "llama.cpp"
      model: "nouscoder-14b-q5.gguf"
      vram_gb: 10

    - provider: "ollama"
      model: "qwen2.5-coder:14b"
      vram_gb: 10

    - provider: "claude_api"
      model: "claude-3-5-sonnet-20241022"
      cost_per_1k_tokens: 0.003

    - provider: "openai_api"
      model: "gpt-4-turbo"
      cost_per_1k_tokens: 0.01
```

### The User Decides

HIVE provides the **harness**. The user configures:
1. Which provider to use per slot
2. Which specific model within that provider
3. Fallback preferences if primary is unavailable
4. Cost/speed/quality tradeoffs

---

## Principle 3: Hardware-Aware Model Selection

### The Rule

**HIVE must recommend models based on available hardware, not assume hardware fits models.**

### Hardware Detection Requirements

HIVE must detect and utilize:

| Hardware Type | Detection Method | Relevant Metrics |
|--------------|------------------|------------------|
| **NVIDIA GPU** | `nvidia-smi`, `pynvml` | VRAM, CUDA cores, compute capability |
| **AMD GPU** | `rocm-smi`, ROCm APIs | VRAM, CUs, ROCm version |
| **Intel Arc** | `intel_gpu_top`, Level Zero | VRAM, Xe cores |
| **Apple Silicon** | `system_profiler`, Metal APIs | Unified memory, Neural Engine |
| **CPU Only** | `psutil`, `cpuinfo` | RAM, cores, AVX support |

### Adaptive Model Recommendations

```
User has: RTX 4070 (12GB VRAM)
Consciousness slot: Local 3B model fits ✓
Coder slot options:
  - Local 14B Q4: 8GB ✓ (fits with consciousness)
  - Local 14B Q5: 10GB ✓ (tight fit)
  - Local 14B Q8: 14GB ✗ (won't fit)
  - Claude API: $$ (always available)

Recommendation: "Local 14B Q5 for best quality that fits your hardware.
                 Claude API available as fallback for complex tasks."

User has: MacBook Air M2 (8GB unified)
Consciousness slot: Local 2B model or API
Coder slot options:
  - Local 7B Q4: 4.5GB ✓ (fits)
  - Local 14B: ✗ (won't fit)
  - Claude API: $$ (recommended for heavy tasks)

Recommendation: "Local 7B for quick tasks, Claude Sonnet for complex coding.
                 Your hardware limits local model size."
```

### Future-Proofing Hardware Support

The hardware detection layer must be **extensible**:
- New GPU vendors (Qualcomm, future AMD/Intel)
- NPU/TPU support as it becomes available
- Hybrid configurations (GPU + NPU)
- Cloud GPU providers (RunPod, Lambda, etc.)

---

## Principle 4: Backend Abstraction Layer

### The Rule

**HIVE talks to an abstraction layer, never directly to inference engines.**

### The Provider Interface

Every provider (local or API) must implement:

```python
class ModelProvider(Protocol):
    """Universal interface for ANY model provider."""

    def load(self, model_id: str, config: dict) -> Model:
        """Load/connect to a model."""
        ...

    def unload(self, model_id: str) -> None:
        """Unload/disconnect from a model."""
        ...

    def generate(self, prompt: str, params: GenerationParams) -> str:
        """Generate completion."""
        ...

    def generate_stream(self, prompt: str, params: GenerationParams) -> Iterator[str]:
        """Stream completion."""
        ...

    def get_capabilities(self) -> ProviderCapabilities:
        """Report what this provider can do."""
        ...

    def health_check(self) -> bool:
        """Check if provider is available."""
        ...
```

### Implemented Providers (Target)

| Provider | Type | Hot-Swap | Cost | Latency |
|----------|------|----------|------|---------|
| `LlamaCppProvider` | Local GGUF | Yes | Free | Low |
| `OllamaProvider` | Local (any) | Yes | Free | Low |
| `VLLMProvider` | Local (any) | Yes | Free | Very Low |
| `ClaudeProvider` | Cloud API | N/A | $$ | Medium |
| `OpenAIProvider` | Cloud API | N/A | $$$ | Medium |
| `AzureOpenAIProvider` | Enterprise | N/A | $$$ | Medium |
| `CustomEndpointProvider` | Any | Configurable | Varies | Varies |

### Provider Selection Logic

```python
def select_provider_for_slot(slot: str, user_prefs: UserPreferences) -> ModelProvider:
    """
    Select best provider based on:
    1. User's explicit preference (if any)
    2. Hardware constraints
    3. Cost constraints
    4. Latency requirements
    5. Availability
    """
    candidates = get_compatible_providers(slot)

    # Filter by hardware
    if user_prefs.local_only:
        candidates = [p for p in candidates if p.is_local]

    # Filter by cost
    if user_prefs.max_cost_per_1k_tokens:
        candidates = [p for p in candidates if p.cost <= user_prefs.max_cost_per_1k_tokens]

    # Filter by availability
    candidates = [p for p in candidates if p.health_check()]

    # Return best match
    return rank_by_preference(candidates, user_prefs)[0]
```

---

## Principle 5: Future-Proof Design Patterns

### The Rule

**Design for paradigm shifts, not just model updates.**

### What Could Change

| Current State (2026) | Possible Future (2027+) |
|---------------------|------------------------|
| GGUF is dominant format | New format emerges (GGML v4, ONNX-LLM, etc.) |
| llama.cpp is primary local engine | vLLM/TGI become consumer-friendly |
| Hot-swapping = unload/load | Speculative decoding, KV-cache sharing |
| Single-GPU focus | Multi-GPU, CPU+GPU hybrid common |
| Text-only specialists | Multimodal specialists standard |
| 14B is "large local" | 70B+ runs on consumer hardware |

### Design Patterns for Resilience

**1. Interface-Based Design**
```
Code to interfaces, not implementations.
Never: llama_cpp.Llama()
Always: provider.load()
```

**2. Configuration Over Code**
```
Model selection in YAML/JSON, not Python.
Never: hardcoded model paths
Always: config.specialists.coder.model
```

**3. Graceful Degradation**
```
If preferred provider fails, fallback chain activates.
Never: crash if model unavailable
Always: try_next_option() or ask_user()
```

**4. Capability Detection**
```
Query what a model CAN do, don't assume.
Never: assume all coders support streaming
Always: if provider.supports('streaming'): stream()
```

**5. Version Negotiation**
```
Handle breaking changes in backends.
Never: assume llama.cpp API is stable
Always: check_backend_version() and adapt
```

---

## Principle 6: UI/UX for Model Selection

### The Rule

**Users must be able to visualize and configure the slot system intuitively.**

### Required UI Components

**1. Slot Visualization**
```
┌─────────────────────────────────────────────────────────┐
│  HIVE Architecture                                      │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  ┌─────────────────────────────────────────────────┐   │
│  │  CONSCIOUSNESS (Always Active)                   │   │
│  │  ├─ Current: Claude Haiku                       │   │
│  │  ├─ Status: ● Online                            │   │
│  │  └─ [Change Model ▼]                            │   │
│  └─────────────────────────────────────────────────┘   │
│           │                                             │
│           ▼                                             │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐  │
│  │  CODER   │ │ TERMINAL │ │ WEBCRAWL │ │ TOOLCALL │  │
│  │──────────│ │──────────│ │──────────│ │──────────│  │
│  │ Local 14B│ │ Local 8B │ │ Local 3B │ │ Local 350M│  │
│  │ ○ Asleep │ │ ● Active │ │ ○ Asleep │ │ ● Active │  │
│  │ [Config] │ │ [Config] │ │ [Config] │ │ [Config] │  │
│  └──────────┘ └──────────┘ └──────────┘ └──────────┘  │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

**2. Model Selection Modal**
```
┌─────────────────────────────────────────────────────────┐
│  Configure CODER Slot                                   │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  Provider Type:  ○ Local (GGUF)                        │
│                  ○ Local (Ollama)                       │
│                  ● Cloud (Claude API)                   │
│                  ○ Cloud (OpenAI API)                   │
│                  ○ Custom Endpoint                      │
│                                                         │
│  Model:          [claude-3-5-sonnet-20241022    ▼]     │
│                                                         │
│  ┌─────────────────────────────────────────────────┐   │
│  │  Model Info:                                     │   │
│  │  • HumanEval: 92%                               │   │
│  │  • Cost: $0.003/1K input, $0.015/1K output      │   │
│  │  • Latency: ~500ms                              │   │
│  │  • Context: 200K tokens                         │   │
│  └─────────────────────────────────────────────────┘   │
│                                                         │
│  Fallback: [Claude Haiku → Local 7B → None     ▼]      │
│                                                         │
│              [Cancel]                    [Apply]        │
└─────────────────────────────────────────────────────────┘
```

**3. Hardware-Aware Recommendations**
```
┌─────────────────────────────────────────────────────────┐
│  Hardware Detected: AMD RX 9060 XT (16GB VRAM)          │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  Recommended Configuration:                             │
│                                                         │
│  ● Budget-Friendly (All Local)                         │
│    └─ Consciousness: LFM2-2.6B    [3GB]                │
│    └─ Coder: NousCoder-14B Q5     [10GB]               │
│    └─ Total: 13GB / 16GB available                     │
│                                                         │
│  ○ Balanced (Local + API Fallback)                     │
│    └─ Consciousness: LFM2-2.6B    [3GB]                │
│    └─ Coder: Local 7B + Claude API backup              │
│                                                         │
│  ○ Performance (API-First)                             │
│    └─ Consciousness: Claude Haiku                      │
│    └─ Coder: Claude Sonnet                             │
│    └─ Estimated cost: ~$0.50/hour heavy use            │
│                                                         │
│                              [Apply Recommendation]     │
└─────────────────────────────────────────────────────────┘
```

---

## Principle 7: Configuration Schema

### The Rule

**All model/provider configuration must be declarative and version-controlled.**

### Master Configuration Structure

```yaml
# hive_config.yaml - The single source of truth

version: "1.0"
last_modified: "2026-01-25"

# Hardware profile (auto-detected or manual)
hardware:
  gpu:
    vendor: "AMD"
    model: "RX 9060 XT"
    vram_gb: 16
  ram_gb: 32
  platform: "WSL2/Windows 11"

# Provider configurations
providers:
  llama_cpp:
    enabled: true
    binary_path: "/usr/local/bin/llama-server"
    model_dir: "~/models"
    default_context: 8192

  ollama:
    enabled: true
    base_url: "http://localhost:11434"

  claude_api:
    enabled: true
    api_key_env: "ANTHROPIC_API_KEY"  # Never store keys in config!

  openai_api:
    enabled: false
    api_key_env: "OPENAI_API_KEY"

# Slot configurations
slots:
  consciousness:
    provider: "llama_cpp"
    model: "lfm2-2.6b-q4_k_m.gguf"
    always_loaded: true
    fallback:
      - provider: "claude_api"
        model: "claude-3-haiku-20240307"

  coder:
    provider: "llama_cpp"
    model: "nouscoder-14b-q5_k_m.gguf"
    vram_required_gb: 10
    fallback:
      - provider: "ollama"
        model: "qwen2.5-coder:14b"
      - provider: "claude_api"
        model: "claude-3-5-sonnet-20241022"

  terminal:
    provider: "llama_cpp"
    model: "seta-rl-qwen3-8b-q5_k_m.gguf"
    vram_required_gb: 6

  webcrawl:
    provider: "llama_cpp"
    model: "qwen2.5-3b-q5_k_m.gguf"
    vram_required_gb: 3

  toolcall:
    provider: "llama_cpp"
    model: "opt-350m-toolcall-q8_0.gguf"
    vram_required_gb: 1
    always_loaded: true  # Tiny, keep it ready

# User preferences
preferences:
  prefer_local: true
  max_api_cost_per_session_usd: 5.00
  fallback_to_api: true
  quality_vs_speed: "balanced"  # "quality" | "balanced" | "speed"
```

---

## Anti-Patterns to Avoid

### NEVER Do These

| Anti-Pattern | Why It's Bad | Correct Approach |
|--------------|--------------|------------------|
| Hardcode model names in code | Breaks when models change | Use config references |
| Assume GGUF format | Other formats will emerge | Use provider abstraction |
| Assume llama.cpp API | API will change | Version-check and adapt |
| Reject cloud APIs entirely | Limits user choice | Offer as option |
| Assume specific VRAM | Hardware varies wildly | Detect and adapt |
| Couple specialists to models | Prevents swapping | Use slot interface |
| Store API keys in config | Security risk | Use environment variables |

---

## Verification Checklist

Before any PR is merged, verify:

- [ ] No model names hardcoded in orchestration logic
- [ ] All model access goes through provider abstraction
- [ ] Configuration is declarative (YAML/JSON)
- [ ] Fallback chain is defined for each slot
- [ ] Hardware detection is used for recommendations
- [ ] API keys use environment variables
- [ ] New providers implement the standard interface
- [ ] UI allows model selection per slot

---

## Summary

**HIVE = Orchestration Harness + Swappable Components**

```
┌────────────────────────────────────────────────────────────────┐
│                     HIVE HARNESS (Fixed)                       │
│  ┌──────────────────────────────────────────────────────────┐ │
│  │  Orchestrator │ MAGMA Memory │ Sleep/Wake │ UI/API       │ │
│  └──────────────────────────────────────────────────────────┘ │
│                              │                                  │
│                    ┌─────────┴─────────┐                       │
│                    │ Provider Abstract │                       │
│                    │      Layer        │                       │
│                    └─────────┬─────────┘                       │
│           ┌──────────────────┼──────────────────┐              │
│           ▼                  ▼                  ▼              │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐        │
│  │  Local      │    │  Cloud      │    │  Custom     │        │
│  │  Providers  │    │  APIs       │    │  Endpoints  │        │
│  │ ─────────── │    │ ─────────── │    │ ─────────── │        │
│  │ llama.cpp   │    │ Claude      │    │ Self-hosted │        │
│  │ Ollama      │    │ OpenAI      │    │ Fine-tuned  │        │
│  │ vLLM        │    │ Gemini      │    │ Enterprise  │        │
│  └─────────────┘    └─────────────┘    └─────────────┘        │
│                              │                                  │
│                    ┌─────────┴─────────┐                       │
│                    │   ANY MODEL       │                       │
│                    │   FILLS ANY SLOT  │                       │
│                    └───────────────────┘                       │
└────────────────────────────────────────────────────────────────┘
```

**This is the HIVE promise: The framework survives. The models evolve.**

---

**Document Version:** 1.0
**Established:** January 25, 2026
**Status:** FOUNDATIONAL - All development must comply

---

**End of Architecture Principles**

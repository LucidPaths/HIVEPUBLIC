# HIVE Model Modularity Guide

**Models are LEGO Blocks - Swap Freely**

---

## Core Principle: Model-Agnostic Architecture

**CRITICAL UNDERSTANDING:**

HIVE doesn't care WHICH model fills a role - it only cares that the role is FILLED.

```
WRONG Thinking:
"HIVE needs NousCoder-14B specifically"

RIGHT Thinking:
"HIVE needs a [CODER SLOT] - could be NousCoder, Qwen, DeepSeek, or whatever"
```

---

## Provider Abstraction: Beyond Local Models

> **CRITICAL UPDATE (January 2026):** HIVE is provider-agnostic, not just model-agnostic.
> See `ARCHITECTURE_PRINCIPLES.md` for the complete philosophy.

### The Provider Hierarchy

HIVE supports multiple provider types for EVERY slot:

```
┌─────────────────────────────────────────────────────────────┐
│                     SLOT (e.g., Coder)                      │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  Provider Options (User Configurable):                      │
│                                                             │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐         │
│  │   LOCAL     │  │   CLOUD     │  │   CUSTOM    │         │
│  │ ─────────── │  │ ─────────── │  │ ─────────── │         │
│  │ llama.cpp   │  │ Claude API  │  │ Self-hosted │         │
│  │ Ollama      │  │ OpenAI API  │  │ Fine-tuned  │         │
│  │ vLLM        │  │ Gemini API  │  │ Enterprise  │         │
│  │ ExLlamaV2   │  │ Azure       │  │ RunPod      │         │
│  └─────────────┘  └─────────────┘  └─────────────┘         │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### Provider Types Explained

| Provider | Type | Cost | Latency | Privacy | Best For |
|----------|------|------|---------|---------|----------|
| **llama.cpp** | Local GGUF | Free | Low | Full | Default local inference |
| **Ollama** | Local (any) | Free | Low | Full | Easy model management |
| **vLLM** | Local (any) | Free | Very Low | Full | High-throughput |
| **Claude API** | Cloud | $$ | Medium | Partial | Best reasoning quality |
| **OpenAI API** | Cloud | $$$ | Medium | Partial | GPT-4 capabilities |
| **Azure OpenAI** | Enterprise | $$$ | Medium | Compliant | Enterprise requirements |
| **Custom** | Self-hosted | Varies | Varies | Full | Fine-tuned models |

### Slot Configuration with Providers

```yaml
# Example: Coder slot with multiple provider options
coder:
  # Primary: Local model (default)
  primary:
    provider: "llama_cpp"
    model: "nouscoder-14b-q5_k_m.gguf"
    vram_required: 10

  # Fallback chain (if primary unavailable or insufficient)
  fallback:
    - provider: "ollama"
      model: "qwen2.5-coder:14b"

    - provider: "claude_api"
      model: "claude-3-5-sonnet-20241022"
      # Used when: local VRAM exhausted, complex task, user preference

    - provider: "openai_api"
      model: "gpt-4-turbo"
      # Used when: Claude unavailable, specific capability needed
```

### When to Use Each Provider Type

**Use LOCAL (llama.cpp, Ollama, vLLM) when:**
- Privacy is critical (data never leaves machine)
- Running many requests (no per-token cost)
- Latency-sensitive (no network round-trip)
- Offline operation needed
- Hardware can handle model size

**Use CLOUD APIs (Claude, OpenAI, Gemini) when:**
- Hardware is limited (laptop, low VRAM)
- Task requires frontier model capabilities
- Occasional use (cost acceptable)
- Need specific model features (vision, long context)
- Quality > cost tradeoff acceptable

**Use CUSTOM ENDPOINTS when:**
- Running fine-tuned models
- Enterprise/compliance requirements
- Hybrid cloud-local architecture
- Specific infrastructure constraints

### Provider Selection Logic

```python
# Pseudocode for provider selection
def select_provider(slot: str, task: Task, hardware: Hardware) -> Provider:
    slot_config = config.slots[slot]

    # 1. Check user's explicit preference
    if user_prefs.force_provider:
        return get_provider(user_prefs.force_provider)

    # 2. Try primary (usually local)
    primary = slot_config.primary
    if can_load(primary, hardware):
        return get_provider(primary)

    # 3. Walk fallback chain
    for fallback in slot_config.fallback:
        if fallback.is_available() and within_budget(fallback, user_prefs):
            return get_provider(fallback)

    # 4. No options available
    raise NoProviderAvailable(slot)
```

### The LEGO Principle Extended

```
Original LEGO Principle:
  "Any GGUF model that fits the slot connector works"

Extended LEGO Principle (January 2026):
  "Any MODEL from any PROVIDER that meets slot requirements works"

  Slot Requirements:
  - Capability: Can do the task (coding, reasoning, etc.)
  - Interface: Speaks the provider protocol
  - Budget: Within cost constraints (if API)
  - Hardware: Fits available resources (if local)

  If it meets requirements → IT WORKS
```

---

## Slot-Based Architecture

### How It Actually Works

```python
class HIVEOrchestrator:
    def __init__(self):
        # Slots, not specific models
        self.slots = {
            'consciousness': None,  # Any reasoning model
            'coder': None,          # Any code generation model
            'terminal': None,       # Any safe execution model
            'webcrawl': None,       # Any summarization model
            'toolcall': None        # Any function-calling model
        }
        
    def load_into_slot(self, slot_name: str, model_path: str):
        """Load ANY compatible model into a slot"""
        self.slots[slot_name] = Llama(model_path=model_path, ...)
        
    def swap_model(self, slot_name: str, new_model_path: str):
        """Hot-swap a model - unload old, load new"""
        # Unload old
        if self.slots[slot_name]:
            del self.slots[slot_name]
            gc.collect()
        
        # Load new
        self.load_into_slot(slot_name, new_model_path)
```

**The orchestrator doesn't know or care what model is loaded - it just calls the slot.**

---

## Model Requirements Per Slot

### Consciousness Slot

**Purpose:** Always-active reasoning and orchestration

**Requirements:**
- ✅ Good reasoning ability
- ✅ Fast inference (will be called constantly)
- ✅ Small VRAM footprint (2-4GB max)
- ✅ Handles task decomposition

**Current Model:** LFM2-2.6B-Transcript

**Alternative Models:**
- Qwen2.5-3B-Instruct
- Qwen3-4B (when GGUF available)
- Phi-3-Mini-4K
- Any 2-4B reasoning-focused model

**Swap Criteria:** New model with better reasoning at similar size

**How to Swap:**
```python
# Download new model
huggingface-cli download ModelOrg/NewReasoner-3B-GGUF \
  newreasoner-3b-q4_k_m.gguf --local-dir ~/models

# Test it
./llama-cli -m ~/models/newreasoner-3b-q4_k_m.gguf \
  --prompt "Decompose task: Build a web API" -n 256

# If better, update orchestrator config
orchestrator.swap_model('consciousness', '~/models/newreasoner-3b-q4_k_m.gguf')
```

---

### Coder Slot

**Purpose:** Code generation, debugging, architecture

**Requirements:**
- ✅ Strong code generation (HumanEval 70%+)
- ✅ Multi-language support
- ✅ Good at explaining code
- ✅ 7-14B parameters (VRAM constraint)

**Current Models (Pick One):**
- NousCoder-14B (competitive programming focus)
- Qwen2.5-Coder-14B (software engineering focus)
- Qwen2.5-Coder-7B (lightweight)

**Future Alternatives:**
- Qwen3-Coder-14B (when released)
- DeepSeek-Coder-V3-14B
- CodeLlama-34B (if quant fits)
- StarCoder2-15B
- ANY coding model with GGUF

**Swap Criteria:** 
- Better HumanEval/LiveCodeBench scores
- Faster inference at same quality
- Better multi-language support
- Improved reasoning in code

**Example Swap:**
```bash
# New model released: "UltraCoder-14B" with 95% HumanEval
huggingface-cli download UltraAI/UltraCoder-14B-GGUF \
  ultracoder-14b-q5_k_m.gguf --local-dir ~/models

# Test it
./llama-cli -m ~/models/ultracoder-14b-q5_k_m.gguf \
  --prompt "def binary_search(arr, target):" -n 512

# Benchmark (optional but recommended)
python benchmark_coder.py --model ultracoder-14b-q5_k_m.gguf

# If passes, swap in orchestrator
config.coder_model = "~/models/ultracoder-14b-q5_k_m.gguf"
```

**NO code changes needed - just config update!**

---

### Terminal Slot

**Purpose:** Safe command execution

**Requirements:**
- ✅ Understands shell commands
- ✅ RL-trained for safety (or explicit safety checks)
- ✅ Context awareness (file system understanding)
- ✅ 7-10B parameters

**Current Model:** SETA-RL-Qwen3-8B

**Future Alternatives:**
- SETA-V2 (if released)
- Any terminal-focused RL model
- Fine-tuned Qwen/Llama on terminal tasks
- Shell-GPT models

**Swap Criteria:**
- Better safety (fewer dangerous commands)
- Improved command understanding
- Faster inference

---

### WebCrawl Slot

**Purpose:** Summarization and extraction

**Requirements:**
- ✅ Good summarization ability
- ✅ Fast inference (will be used frequently)
- ✅ Small footprint (2-4GB)
- ✅ Can extract structured data

**Current Model:** Qwen2.5-3B-Instruct

**Future Alternatives:**
- Phi-3-Mini
- Gemma-2-3B
- Qwen3-4B
- Any small summarization model

**Note:** Heavy lifting (actual scraping) is code-based (BeautifulSoup), model just summarizes results

---

### ToolCall Slot

**Purpose:** API interaction, function calling

**Requirements:**
- ✅ Can follow structured output formats
- ✅ Understands JSON/function schemas
- ✅ Tiny footprint (<1GB)
- ✅ Fast inference

**Current Model:** Fine-tuned OPT-350M

**Future Alternatives:**
- Gorilla (function calling specialist)
- Fine-tuned Phi-2
- Any small instruction-following model
- Custom fine-tune of ANY base model

**Unique:** This is the ONLY slot where you might fine-tune yourself

---

## Swapping Strategy

### When to Consider Swapping

**Automatic Triggers:**
1. **Better Benchmark:** New model scores higher on relevant benchmark
   - Coder: +5% HumanEval/LiveCodeBench
   - Reasoning: +10% on reasoning tasks
   
2. **Faster Inference:** New model 20%+ faster at same quality
   - Same VRAM, higher tokens/sec

3. **Better Quantization:** New quant method (e.g., "Dynamic 3.0")
   - Same model, less VRAM or better quality

4. **New Architecture:** Fundamental improvement
   - Llama 4, Qwen4, etc.

**Manual Triggers:**
1. You find model better for YOUR specific use case
2. Community recommends alternative
3. Fine-tuned version available for your domain

### Swapping Protocol

**1. Download & Test**
```bash
# Get the new model
huggingface-cli download Org/Model-GGUF file.gguf --local-dir ~/models

# Quick test
./llama-cli -m ~/models/file.gguf --prompt "Test prompt" -n 128
```

**2. Benchmark (Recommended)**
```bash
# Compare to current model on your tasks
python compare_models.py \
  --model-a ~/models/current.gguf \
  --model-b ~/models/new.gguf \
  --tasks coding,reasoning,speed
```

**3. Gradual Rollout**
```python
# Don't immediately replace - test in parallel first
orchestrator.add_alternative('coder', 'new_model', model_path)

# Try new model for some tasks
if random.random() < 0.2:  # 20% of tasks
    result = orchestrator.execute_with('coder', 'new_model', task)
else:
    result = orchestrator.execute_with('coder', 'current', task)

# Compare results, user satisfaction
```

**4. Full Swap**
```python
# If new model is better
orchestrator.set_primary('coder', 'new_model')
orchestrator.remove('coder', 'old_model')

# Update config
config.save({
    'coder_model': 'new_model_path.gguf',
    'swap_date': datetime.now(),
    'reason': 'Better HumanEval scores'
})
```

---

## Model Registry (Living Document)

**Keep track of what's in each slot:**

```yaml
# config/model_registry.yaml

consciousness:
  current: "lfm2-2.6b-q4_k_m.gguf"
  alternatives:
    - "qwen2.5-3b-q5_k_m.gguf"  # Tested, slightly slower
  version: 1.0
  last_swap: "2026-01-20"
  
coder:
  current: "nouscoder-14b-q5_k_m.gguf"
  alternatives:
    - "qwen2.5-coder-14b-q5_k_m.gguf"  # Software eng tasks
    - "qwen2.5-coder-7b-q5_k_m.gguf"   # Lightweight option
  version: 1.0
  last_swap: "2026-01-20"
  notes: "NousCoder for algorithms, Qwen2.5 for apps"
  
terminal:
  current: "seta-rl-qwen3-8b-q5_k_m.gguf"
  alternatives: []
  version: 1.0
  last_swap: "2026-01-20"
  
webcrawl:
  current: "qwen2.5-3b-q5_k_m.gguf"
  alternatives: []
  version: 1.0
  last_swap: "2026-01-20"
  
toolcall:
  current: "opt-350m-toolcall-q8_0.gguf"
  alternatives: []
  version: 1.0
  last_swap: "2026-01-20"
  notes: "Custom fine-tuned"
```

**Update this whenever you swap!**

---

## Real-World Swap Examples

### Example 1: Qwen3-Coder-14B Releases

**Scenario:** Alibaba releases Qwen3-Coder-14B with 92% HumanEval (vs. NousCoder's 87%)

**Action:**
```bash
# Download
huggingface-cli download Qwen/Qwen3-Coder-14B-GGUF \
  qwen3-coder-14b-q5_k_m.gguf --local-dir ~/models

# Test on YOUR tasks
python test_coder.py --model qwen3-coder-14b-q5_k_m.gguf

# If better: Swap
orchestrator.swap_model('coder', '~/models/qwen3-coder-14b-q5_k_m.gguf')

# Update registry
registry.update('coder', {
    'current': 'qwen3-coder-14b-q5_k_m.gguf',
    'previous': 'nouscoder-14b-q5_k_m.gguf',
    'swap_reason': '+5% HumanEval improvement'
})
```

**Time to swap:** 15 minutes
**Code changes:** 0 (just config)

---

### Example 2: Better Reasoning Model at 2B

**Scenario:** "MiniThink-2B" releases with superior reasoning at 2B params

**Action:**
```bash
# Download
huggingface-cli download ThinkLab/MiniThink-2B-GGUF \
  minithink-2b-q4_k_m.gguf --local-dir ~/models

# Benchmark reasoning
python benchmark_reasoning.py \
  --current lfm2-2.6b-q4_k_m.gguf \
  --new minithink-2b-q4_k_m.gguf

# If better AND faster (smaller!)
orchestrator.swap_model('consciousness', '~/models/minithink-2b-q4_k_m.gguf')

# Benefit: Free up 0.5GB VRAM for specialists
```

---

### Example 3: Community Fine-Tune

**Scenario:** Someone fine-tunes Qwen2.5-Coder on YOUR codebase style

**Action:**
```bash
# Download community model
huggingface-cli download YourName/Qwen2.5-Coder-YourStyle-GGUF \
  qwen-yourstyle-q5_k_m.gguf --local-dir ~/models

# Test - should match your coding conventions better
python test_style_match.py --model qwen-yourstyle-q5_k_m.gguf

# Swap to personal version
orchestrator.swap_model('coder', '~/models/qwen-yourstyle-q5_k_m.gguf')
```

**Result:** Code generated matches YOUR style perfectly

---

## Multiple Models Per Slot (Advanced)

**You can keep multiple models loaded for different sub-tasks:**

```python
class HIVEOrchestrator:
    def __init__(self):
        self.slots = {
            'coder': {
                'algorithms': None,      # NousCoder for algorithms
                'webapps': None,         # Qwen2.5 for web dev
                'scripts': None,         # Lightweight 7B for quick scripts
            }
        }
        
    def execute_coding_task(self, task):
        # Route to best sub-specialist
        if 'algorithm' in task or 'leetcode' in task:
            return self.slots['coder']['algorithms'].generate(task)
        elif 'api' in task or 'web' in task:
            return self.slots['coder']['webapps'].generate(task)
        else:
            return self.slots['coder']['scripts'].generate(task)
```

**Trade-off:** More VRAM used, but better specialization

---

## Future-Proofing Checklist

**To ensure your HIVE stays modular and provider-agnostic:**

### Model Agnosticism
- [ ] **Never hardcode model names** in orchestrator logic
- [ ] **Use config files** for model paths
- [ ] **Maintain model registry** (YAML/JSON)
- [ ] **Benchmark new models** before swapping
- [ ] **Keep old models** for rollback (until confident)
- [ ] **Version your configs** (git track model_registry.yaml)
- [ ] **Document swap reasons** (why you changed)
- [ ] **Test after swaps** (run test suite)

### Provider Agnosticism (Added January 2026)
- [ ] **Never hardcode provider types** (llama.cpp, Ollama, API)
- [ ] **All provider access through abstraction layer**
- [ ] **Fallback chains defined for each slot**
- [ ] **API keys in environment variables, NOT config files**
- [ ] **Provider health checks before use**
- [ ] **Cost tracking for API providers**
- [ ] **Hardware detection for local provider selection**

### Future Paradigm Readiness
- [ ] **Interface-based design** (code to protocols, not implementations)
- [ ] **Graceful degradation** (fallback if provider fails)
- [ ] **Capability detection** (query what model CAN do)
- [ ] **Version negotiation** (handle backend API changes)
- [ ] **New format support path** (GGUF today, ??? tomorrow)

---

## The LEGO Principle

```
HIVE Architecture = LEGO Baseplate (fixed)
    ↓
Specialist Slots = LEGO Brick Connectors (fixed)
    ↓
Models = LEGO Bricks (swappable)
    ↓
Any brick that fits the connector works!
```

**Example:**

```
Coder Slot Requirements:
- GGUF format ✓
- Code generation capability ✓
- Fits in 14GB VRAM ✓

Models that fit:
✅ NousCoder-14B
✅ Qwen2.5-Coder-14B
✅ Qwen3-Coder-14B (future)
✅ DeepSeek-Coder-14B
✅ CodeLlama-13B
✅ StarCoder2-15B
✅ Your-Custom-Finetune-14B
✅ ANY 7-14B coding model in GGUF
```

**As long as it connects (GGUF + fits VRAM), it works!**

---

## Monitoring Model Performance

**Track metrics per model to decide when to swap:**

```python
# metrics/model_performance.json
{
  "coder": {
    "nouscoder-14b-q5_k_m": {
      "tasks_completed": 1543,
      "avg_quality_score": 8.7,
      "avg_inference_speed": 18.2,  # tokens/sec
      "user_satisfaction": 0.89,
      "last_30_days": {
        "quality_trend": "stable",
        "speed_trend": "stable"
      }
    }
  }
}
```

**When new model outperforms on YOUR metrics → swap**

---

## Key Takeaway

**HIVE is NOT married to ANY specific model OR provider.**

**When Qwen5, Llama4, DeepSeek-V4, Claude 4, GPT-5, or whatever comes out:**

For LOCAL models:
1. Download model (GGUF or whatever format)
2. Test it
3. If better → swap
4. Done in 15 minutes

For API models:
1. Add provider config
2. Test it
3. If better → swap (or add as fallback)
4. Done in 5 minutes

**The Framework Survives:**
- January 2026: NousCoder-14B local + Claude API fallback
- January 2027: ???-20B local + ???-API fallback (models change, HIVE doesn't)
- January 2028: Who knows? But HIVE adapts.

**Zero architecture changes. Pure modularity. Provider agnostic. That's the power of HIVE.**

---

**Models are LEGO. Swap freely. Stay cutting-edge.** 🧱

---

**End of Model Modularity Guide**

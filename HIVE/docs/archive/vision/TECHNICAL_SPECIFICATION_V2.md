# HIVE Technical Specifications v2

**Complete Technical Reference for Hierarchical Intelligence with Virtualized Execution**

---

## Hardware Specifications

### Your System Configuration

**CPU:**
- Model: AMD Ryzen 5 5500
- Cores: 6 physical cores
- Threads: 12 threads (SMT enabled)
- Base Clock: 3.6 GHz
- Boost Clock: Up to 4.2 GHz
- Cache: 16MB L3
- TDP: 65W
- Architecture: Zen 3 (Cezanne)

**GPU:**
- Model: AMD Radeon RX 9060 XT (Gigabyte Gaming OC)
- Architecture: RDNA 4 (gfx1200)
- VRAM: 16GB GDDR6
- Memory Interface: 256-bit
- Compute Units: TBD (RDNA 4)
- ROCm Support: Yes (6.4.2.1+)

**Memory:**
- Total RAM: 32GB DDR4
- Configuration: 2x16GB (Dual Channel)
- Speed: 2133 MHz
- Type: Corsair Vengeance LPX

**Storage:**
- Primary: NVMe SSD (model not specified)
- Additional: Other drives available
- Free Space: ~188GB on primary

**Power:**
- PSU: 850W be quiet! Pure Power 12 Gold
- Efficiency: 80 Plus Gold
- Headroom: Excellent for system

**Motherboard:**
- Model: ASUS Prime B450M-A II
- Chipset: AMD B450
- Form Factor: Micro-ATX

**Cooling:**
- CPU: Stock AMD Wraith Stealth cooler
- Case: Standard case fans
- Note: Monitor temps during heavy inference

**Display:**
- Refresh Rate: 60Hz
- Note: Not critical for HIVE operation

---

## Software Stack

### Operating System

**Host OS:**
- Platform: Windows 11
- Build: Latest stable

**Virtual Environment:**
- Type: WSL2 (Windows Subsystem for Linux 2)
- Distribution: Ubuntu 24.04 LTS
- Kernel: Linux kernel for WSL2
- Integration: Full GPU passthrough to WSL2

### GPU Compute Stack

**ROCm (AMD GPU Compute Platform):**
- Version: 6.4.2.1
- Components Installed:
  - rocm-hip-sdk
  - rocm-dev
  - rocm-smi-lib
  - HIP runtime
  - ROCm libraries

**Environment Variables:**
```bash
export ROCM_PATH=/opt/rocm
export PATH=$ROCM_PATH/bin:$PATH
export LD_LIBRARY_PATH=$ROCM_PATH/lib:$LD_LIBRARY_PATH
export HIP_VISIBLE_DEVICES=0
export HSA_OVERRIDE_GFX_VERSION=11.0.0  # RDNA4 gfx1200
```

**GPU Detection Status:**
- `rocm-smi` output: AMD Radeon RX 9060 XT detected ✓
- gfx version: gfx1200
- ROCm devices: Found ✓
- HIP support: Enabled ✓

### Inference Engine

**llama.cpp:**
- Repository: https://github.com/ggml-org/llama.cpp
- Build Method: CMake (new standard, not Makefile)
- Installation: ~/llama.cpp/
- Binary Location: ~/llama.cpp/build/bin/llama-cli

**Build Configuration:**
```bash
cmake -B build -DGGML_HIP=ON
cmake --build build --config Release -j6
```

**Capabilities:**
- GGUF format support ✓
- HIP/ROCm acceleration ✓
- Multi-threading ✓
- Context window: Up to 128K tokens (model-dependent)

**Binaries Available:**
- `llama-cli`: Command-line interface
- `llama-server`: API server mode
- `llama-gguf-split`: For merging split GGUF files
- Other utilities

### Python Environment

**Virtual Environment:**
- Location: ~/llama-env/
- Python Version: 3.11+
- Activation: `source ~/llama-env/bin/activate`

**Core Dependencies:**
```
llama-cpp-python==0.3.16  # Python bindings for llama.cpp
numpy==2.4.1              # Numerical operations
click==8.3.1              # CLI framework
rich==14.2.0              # Terminal formatting
```

**llama-cpp-python Build:**
```bash
CMAKE_ARGS="-DGGML_HIP=on -DCMAKE_C_COMPILER=/opt/rocm/llvm/bin/clang -DCMAKE_CXX_COMPILER=/opt/rocm/llvm/bin/clang++" \
  pip install llama-cpp-python --no-cache-dir --force-reinstall
```

**Additional Libraries (Planned):**
```
flask                     # Web framework
fastapi                   # Alternative API framework
websockets                # Real-time communication
sqlalchemy                # Database ORM for MAGMA
numpy                     # Embeddings storage
sentence-transformers     # Semantic embeddings
beautifulsoup4            # Web scraping
scrapy                    # Advanced scraping
redis                     # Optional: Token storage
```

---

## Model Specifications

### Model Storage

**Location:** ~/models/
**Format:** GGUF (GPT-Generated Unified Format)
**Quantization Levels Used:** Primarily Q4_K_M, Q5_K_M, Q8_0

### Consciousness Layer

**Model:** LFM2-2.6B-Transcript

**Specifications:**
- Parameters: 2.6 billion
- Architecture: Transformer decoder
- Context Window: 8192 tokens (native)
- Specialization: Reasoning over long transcripts

**Quantization Options:**
| Quant | File Size | VRAM | Quality | Speed |
|-------|-----------|------|---------|-------|
| Q4_K_M | ~1.8GB | ~2.5GB | Good | Fast |
| Q5_K_M | ~2.2GB | ~3.0GB | Better | Medium |
| Q8_0 | ~3.0GB | ~4.0GB | Best | Slower |

**Recommended:** Q4_K_M (balance of speed + quality)

**Download:**
```bash
huggingface-cli download Mungert/LFM2-2.6B-GGUF \
  lfm2-2.6b-q4_k_m.gguf \
  --local-dir ~/models
```

**Loading Configuration:**
```python
from llama_cpp import Llama

reasoning = Llama(
    model_path="~/models/lfm2-2.6b-q4_k_m.gguf",
    n_gpu_layers=-1,      # All layers on GPU
    n_ctx=8192,           # Full context
    n_threads=6,          # Match CPU cores
    n_batch=512,
    verbose=False
)
```

---

### Specialist: Coder (Option 1)

**Model:** NousCoder-14B

**Specifications:**
- Parameters: 14 billion (Qwen3-14B base)
- Architecture: Qwen3 transformer
- Context Window: 40,960 tokens (extended during training)
- Training: RL on 24K competitive programming problems
- Specialization: Algorithmic coding, competitive programming

**Benchmarks:**
- LiveCodeBench v6: 67.87% Pass@1
- Improvement over base: +7.08%
- Codeforces equivalent: ~2100-2200 rating

**Quantization Options:**
| Quant | File Size | VRAM | Quality | Speed |
|-------|-----------|------|---------|-------|
| Q4_K_M | ~8.5GB | ~9GB | Good | 20-25 t/s |
| Q5_K_M | ~10GB | ~10.5GB | Better | 15-20 t/s |
| Q6_K | ~12GB | ~12.5GB | Best | 12-18 t/s |

**Recommended:** Q5_K_M (best balance)

**Download:**
```bash
huggingface-cli download bartowski/NousResearch_NousCoder-14B-GGUF \
  --include "NousResearch_NousCoder-14B-Q5_K_M.gguf" \
  --local-dir ~/models
```

**Loading Configuration:**
```python
coder = Llama(
    model_path="~/models/NousResearch_NousCoder-14B-Q5_K_M.gguf",
    n_gpu_layers=-1,
    n_ctx=40960,          # Extended context
    n_threads=6,
    n_batch=512,
    verbose=False
)
```

**Best For:**
- Algorithm implementation
- Competitive programming style tasks
- Complex logical code
- Performance-critical code

---

### Specialist: Coder (Option 2)

**Model:** Qwen2.5-Coder-14B-Instruct

**Specifications:**
- Parameters: 14 billion
- Architecture: Qwen2.5 transformer
- Context Window: 128K tokens (with YARN extension)
- Training: 5.5 trillion tokens (code + text-code grounding)
- Specialization: Software engineering, real-world applications

**Benchmarks:**
- HumanEval: ~87%
- MBPP: High performance
- BigCodeBench: Strong
- Multi-language support: 40+ languages

**Quantization Options:**
| Quant | File Size | VRAM | Quality | Speed |
|-------|-----------|------|---------|-------|
| Q4_K_M | ~8.5GB | ~9GB | Good | 20-25 t/s |
| Q5_K_M | ~10GB | ~10.5GB | Better | 15-20 t/s |
| Q6_K | ~12GB | ~12.5GB | Best | 12-18 t/s |

**Recommended:** Q5_K_M

**Download:**
```bash
huggingface-cli download Qwen/Qwen2.5-Coder-14B-Instruct-GGUF \
  --include "qwen2.5-coder-14b-instruct-q5_k_m*.gguf" \
  --local-dir ~/models

# Note: May be split into multiple files
# Merge if needed:
llama-gguf-split --merge \
  qwen2.5-coder-14b-instruct-q5_k_m-00001-of-00002.gguf \
  qwen2.5-coder-14b-instruct-q5_k_m.gguf
```

**Loading Configuration:**
```python
coder = Llama(
    model_path="~/models/qwen2.5-coder-14b-instruct-q5_k_m.gguf",
    n_gpu_layers=-1,
    n_ctx=8192,           # Standard (can extend to 128K if needed)
    n_threads=6,
    n_batch=512,
    verbose=False
)
```

**Best For:**
- Multi-file projects
- API design
- Framework development
- Production code
- Documentation

---

### Specialist: Coder (Lightweight Option)

**Model:** Qwen2.5-Coder-7B-Instruct

**Specifications:**
- Parameters: 7 billion
- Architecture: Qwen2.5 transformer
- Context Window: 128K tokens
- Training: Same corpus as 14B version

**Performance:**
- 80-85% of 14B capability
- 50% less VRAM usage
- Faster inference

**Quantization:**
| Quant | File Size | VRAM | Speed |
|-------|-----------|------|-------|
| Q5_K_M | ~5.5GB | ~6GB | 30-40 t/s |

**Download:**
```bash
huggingface-cli download Qwen/Qwen2.5-Coder-7B-Instruct-GGUF \
  --include "qwen2.5-coder-7b-instruct-q5_k_m*.gguf" \
  --local-dir ~/models
```

**Use Case:** When VRAM is tight or task is simpler

---

### Specialist: Terminal

**Model:** SETA-RL-Qwen3-8B

**Specifications:**
- Parameters: 8 billion
- Architecture: Qwen3 base
- Training: Reinforcement learning for safe terminal use
- Specialization: System commands, file operations

**Features:**
- Understands file system structure
- Prevents dangerous operations
- Context-aware command suggestions
- Safe execution patterns

**Quantization:**
| Quant | File Size | VRAM | Speed |
|-------|-----------|------|-------|
| Q5_K_M | ~6GB | ~6.5GB | 20-30 t/s |

**Download:**
```bash
huggingface-cli download camel-ai/seta-rl-qwen3-8b-gguf \
  seta-rl-qwen3-8b-q5_k_m.gguf \
  --local-dir ~/models
```

**Note:** Check for GGUF availability; may need conversion

---

### Specialist: WebCrawl

**Model:** Qwen2.5-3B-Instruct

**Specifications:**
- Parameters: 3 billion
- Architecture: Qwen2.5 transformer
- Context Window: 32K tokens
- Role: Summarization and extraction (scraping is code-based)

**Quantization:**
| Quant | File Size | VRAM | Speed |
|-------|-----------|------|-------|
| Q5_K_M | ~2.5GB | ~3GB | 50-80 t/s |

**Download:**
```bash
huggingface-cli download Qwen/Qwen2.5-3B-Instruct-GGUF \
  qwen2.5-3b-instruct-q5_k_m.gguf \
  --local-dir ~/models
```

---

### Specialist: ToolCall

**Model:** OPT-350M (Base for fine-tuning)

**Specifications:**
- Parameters: 350 million
- Architecture: OPT (Meta)
- Context Window: 2048 tokens
- Plan: Fine-tune for tool calling/API interaction

**Quantization:**
| Quant | File Size | VRAM | Speed |
|-------|-----------|------|-------|
| Q8_0 | ~400MB | <1GB | Very fast |

**Download:**
```bash
huggingface-cli download facebook/opt-350m \
  --local-dir ~/models/opt-350m
```

**Note:** Will be fine-tuned locally for HIVE-specific tool orchestration

---

## VRAM Budget Analysis

### Idle State
```
Consciousness (LFM2-2.6B Q4_K_M): 2.5GB
Free: 13.5GB
```

### Active States

**Coding (NousCoder-14B):**
```
Consciousness: 2.5GB
NousCoder Q5_K_M: 10.5GB
Total: 13GB
Free: 3GB (safety buffer)
```

**Coding (Qwen2.5-Coder-7B - Lightweight):**
```
Consciousness: 2.5GB
Qwen2.5-Coder-7B Q5_K_M: 6GB
Total: 8.5GB
Free: 7.5GB (room for multi-loading)
```

**Terminal:**
```
Consciousness: 2.5GB
SETA-8B Q5_K_M: 6.5GB
Total: 9GB
Free: 7GB
```

**Web Research:**
```
Consciousness: 2.5GB
Qwen2.5-3B Q5_K_M: 3GB
Total: 5.5GB
Free: 10.5GB
```

**Multi-Agent (WebCrawl + ToolCall):**
```
Consciousness: 2.5GB
WebCrawl: 3GB
ToolCall: 0.5GB
Total: 6GB
Free: 10GB
```

---

## Performance Benchmarks

### Expected Inference Speeds (Your Hardware)

**Model Performance on AMD RX 9060 XT:**

| Model | Quantization | Tokens/Second | Use Case |
|-------|--------------|---------------|----------|
| LFM2-2.6B | Q4_K_M | 40-60 t/s | Reasoning |
| NousCoder-14B | Q5_K_M | 15-25 t/s | Heavy coding |
| Qwen2.5-Coder-14B | Q5_K_M | 15-25 t/s | Software eng |
| Qwen2.5-Coder-7B | Q5_K_M | 30-40 t/s | Fast coding |
| SETA-8B | Q5_K_M | 20-30 t/s | Terminal |
| Qwen2.5-3B | Q5_K_M | 50-80 t/s | Summarization |
| OPT-350M | Q8_0 | 100+ t/s | Tool calls |

**Note:** Actual speeds may vary based on:
- Context window size used
- Batch size
- Temperature settings
- System load

### Load Times

**Model Loading (Cold Start):**
- Small (<3B): 2-4 seconds
- Medium (7-8B): 5-8 seconds
- Large (14B): 8-12 seconds

**Model Unloading:**
- All sizes: 1-3 seconds (with garbage collection)

**Total Hot-Swap Time:**
- Complete cycle (unload + load): 10-15 seconds

---

## Storage Requirements

### Model Storage

**Total Model Storage Needed:**
```
LFM2-2.6B (Q4_K_M):           ~1.8GB
NousCoder-14B (Q5_K_M):       ~10GB
  OR Qwen2.5-Coder-14B:       ~10GB
  OR Qwen2.5-Coder-7B:        ~5.5GB
SETA-8B (Q5_K_M):             ~6GB
Qwen2.5-3B (Q5_K_M):          ~2.5GB
OPT-350M (Q8_0):              ~0.4GB

Total (14B coder): ~26.7GB
Total (7B coder):  ~22.2GB
```

**Available:** 188GB (plenty of headroom)

### MAGMA Memory Storage

**RAM Usage:**
```
Graph databases (SQLite): ~100-500MB
Embeddings (NumPy arrays): ~500MB-2GB
Agent states (JSON): ~10-50MB
Conversation history: ~50-200MB

Total: ~1-3GB of 32GB available
```

**Disk Storage:**
```
Persistent MAGMA data: ~500MB-2GB
Conversation archives: ~100MB-1GB
Agent checkpoints: ~500MB

Total: ~1-4GB
```

---

## Network Configuration

**Current Status:** Network disabled for bash_tool in Claude interface

**WSL2 Network:**
- Internet access: Yes (for model downloads)
- Localhost: Available for Flask/FastAPI server
- Port forwarding: Windows ↔ WSL2 automatic

---

## Monitoring Tools

### GPU Monitoring

**rocm-smi:**
```bash
# Check GPU status
rocm-smi

# Real-time monitoring
watch -n 1 rocm-smi

# Detailed info
rocm-smi --showtemp --showuse --showmeminfo
```

### System Monitoring

**CPU/RAM:**
```bash
htop              # Interactive process monitor
free -h           # Memory usage
df -h             # Disk usage
```

**Temperature:**
```bash
sensors           # CPU temp (if lm-sensors installed)
```

---

## Configuration Files

### Environment Setup

**~/.bashrc additions:**
```bash
# ROCm
export ROCM_PATH=/opt/rocm
export PATH=$ROCM_PATH/bin:$PATH
export LD_LIBRARY_PATH=$ROCM_PATH/lib:$LD_LIBRARY_PATH
export HIP_VISIBLE_DEVICES=0
export HSA_OVERRIDE_GFX_VERSION=11.0.0

# llama.cpp
export LLAMA_CPP_PATH=~/llama.cpp
alias llama-cli="$LLAMA_CPP_PATH/build/bin/llama-cli"

# Python environment
alias activate-llama="source ~/llama-env/bin/activate"
```

### Python Dependencies (requirements.txt)

```
# Core inference
llama-cpp-python==0.3.16
numpy>=2.0.0
torch>=2.0.0

# CLI and formatting
click>=8.0.0
rich>=13.0.0

# Web framework (choose one)
flask>=3.0.0
# OR
fastapi>=0.100.0
uvicorn>=0.20.0

# Database and memory
sqlalchemy>=2.0.0
sqlite3  # Built-in

# Embeddings
sentence-transformers>=2.0.0

# Web scraping
beautifulsoup4>=4.12.0
requests>=2.31.0
scrapy>=2.11.0  # Optional

# Utilities
python-dotenv>=1.0.0
pydantic>=2.0.0
```

---

## Troubleshooting Reference

### ROCm Issues

**GPU not detected:**
```bash
# Verify ROCm installation
rocm-smi
lspci | grep VGA

# Check environment variables
echo $HSA_OVERRIDE_GFX_VERSION

# Restart WSL if needed
wsl --shutdown  # From PowerShell
```

### llama.cpp Issues

**HIP not enabled:**
```bash
# Check build
./build/bin/llama-cli --help | grep hip

# Rebuild if needed
cd ~/llama.cpp
rm -rf build
cmake -B build -DGGML_HIP=ON
cmake --build build --config Release -j6
```

### VRAM Issues

**Out of memory:**
- Use smaller quantization (Q4 instead of Q5)
- Reduce context window (n_ctx)
- Offload fewer layers to GPU (n_gpu_layers)
- Unload other specialists first

---

**End of Technical Specifications v2**

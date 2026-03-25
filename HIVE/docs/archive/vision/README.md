# HIVE Vision & Reference Documents

These documents describe HIVE's **long-term architectural vision** and **design philosophy** --
the multi-agent cognitive architecture with hot-swappable specialists, MAGMA memory, and
consciousness orchestration.

**Important:** These are design/reference documents, not descriptions of the current codebase.
The working app is a Tauri v2 desktop app with model management and chat.
See the root [README.md](../../../README.md) for what's actually built.

## Document Index

| Document | What It Covers |
|----------|---------------|
| **[THE VISION](THE_VISION.md)** | **North star: HIVE as persistent AI entity. Identity model, integration architecture, multi-agent vision, OpenClaw patterns to steal, "doors and keys" design. START HERE.** |
| [Architecture Principles](ARCHITECTURE_PRINCIPLES.md) | Provider-agnostic design philosophy, backend abstraction layer, slot system requirements (Jan 2026, Python pseudocode) |
| [Model Modularity Guide](MODEL_MODULARITY_GUIDE.md) | How to swap models across providers (local/cloud/custom), slot requirements, LEGO principle |
| [Architecture Overview](ARCHITECTURE_OVERVIEW_V2.md) | Full system design: consciousness layer, specialist agents, MAGMA memory, operational workflows, VRAM management |
| [Hot-Swap Mechanics](HOT_SWAP_MECHANICS.md) | Sleep/wake protocol implementation, context injection, state extraction (Python pseudocode) |
| [Implementation Theory](IMPLEMENTATION_THEORY_V2.md) | Research foundations: academic papers (MAGMA, MAP, ToolOrchestra, InfiAgent, TTT-E2E), novel contributions |
| [Technical Specification](TECHNICAL_SPECIFICATION_V2.md) | Hardware specs, software stack, model registry, VRAM budgets, performance benchmarks |
| [Research Findings](RESEARCH_FINDINGS_HIVE_V1.md) | Competitive landscape: what exists (Ollama, Jan, LM Studio), what HIVE adds, what to fork |
| [HIVE README v2](README_HIVE_V2.md) | Original comprehensive project overview with agent roster and glossary |

## Reading Order

1. **THE VISION** -- the north star, what HIVE is becoming (start here)
2. **Architecture Principles** -- the provider-agnostic design philosophy
3. **Hot-Swap Mechanics** -- how specialists sleep/wake
4. **Research Findings** -- what already exists vs. what's novel
5. **Implementation Theory** -- the academic backing
6. **Technical Specification** -- hardware/software reference

## Key Concepts

- **Consciousness Layer**: Always-on small reasoning model that orchestrates specialists
- **Specialist Agents**: Domain-specific models loaded on-demand (Coder, Terminal, WebCrawl, ToolCall)
- **MAGMA Memory**: Multi-graph persistent memory (Semantic, Temporal, Causal, Entity)
- **Hot-Swapping**: Load/unload specialists from VRAM as needed, preserving context via MAGMA
- **Provider Agnosticism**: Any model from any provider can fill any slot

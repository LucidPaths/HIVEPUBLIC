# HIVE Principle Lattice

**A citadel for those with eyes to see, yet a simple roof for anyone too scared to look up.**

---

## What This Is

These are HIVE's axiomatic principles — the non-negotiable values that guide every design decision, every line of code, every architectural choice. They aren't features. They aren't goals. They're the DNA.

When you're stuck on a decision, check it against the lattice. If a choice violates a principle, it's wrong — even if it "works." If it honors multiple principles simultaneously, it's probably right.

Each principle has **instantiations** — concrete proof that the principle lives in the codebase, not just on paper. A principle without instantiations is a wish. We don't do wishes.

### The Design Philosophy

HIVE serves two masters simultaneously:

- **The complete beginner** who has never touched a terminal, doesn't know what VRAM is, and just wants to talk to an AI on their own machine without sending their data to the cloud.
- **The power user** who wants to hot-swap specialist models, orchestrate multi-agent pipelines, and push their 3090 to the limit.

Low floor. High ceiling. Every feature must satisfy both: simple enough that a noob never feels lost, powerful enough that NVIDIA would want to ship it.

---

## The Eight Principles

### 1. Bridges and Modularity

> *One path, two systems. Lego blocks, not monoliths.*

Every component should be a self-contained block. Pull one out — that specific thing stops working. The rest stands. No module should be load-bearing for something unrelated to its purpose.

When two systems need to talk, build a bridge — don't duplicate. The `/mnt/c/` bridge already exists between Windows and WSL. Use it. Don't maintain two download paths, two model directories, two copies of anything.

**Instantiations:**
- Windows↔WSL `/mnt/c/` bridge: download once to Windows, WSL reads the same files
- Provider abstraction layer: swap Local/OpenAI/Anthropic/Ollama without touching UI code
- `wsl_cmd()` helper: single function encapsulates all WSL command spawning
- Tauri invoke pattern: React calls Rust through one interface, never directly
- 18 independent components: all props-only or self-contained, no shared Context
- Shared types in `types.ts` — Tab, Backend, Message, LogEntry, SlotRole used across all components
- Skills system: drop `.md` file in `~/.hive/skills/` → agent learns. No code changes needed
- Tool registry: every tool implements `HiveTool` trait, self-registers, self-describes via JSON Schema
- MCP bidirectional: external tools plug in at runtime without touching HIVE code

**Demands:**
- Each component fails independently — chat breaking doesn't kill model management
- Plugin architecture for future tool/skill modules

---

### 2. Provider Agnosticism

> *The interface is permanent. The backend is replaceable.*

HIVE doesn't care where intelligence comes from. Local llama.cpp, OpenAI's API, Anthropic's Claude, Ollama, or something that doesn't exist yet. The user picks a provider. The chat works. The UI is identical. If a provider disappears tomorrow, HIVE loses nothing but that provider.

**Instantiations:**
- Six providers supported: Local (llama.cpp), OpenAI, Anthropic, Ollama, OpenRouter, DashScope
- Same ChatMessage type across all providers
- Per-provider settings with graceful degradation (no API key = feature hides, doesn't crash)
- Provider-specific quirks handled in api.ts + providers.rs, never in UI code
- Cloud specialist routing: cloud providers are coequal specialist backends, not fallbacks
- Skills injection as separate system message: preserves llama.cpp KV cache across providers
- Memory session-injected: never mutates the system prompt (works identically with any provider)
- OpenAI-compatible providers share unified `chat_openai_compatible()` dispatch — new provider is a 1-line entry

**Demands:**
- New provider = new adapter, zero UI changes
- MCP protocol bridge extends this to tool providers too
- Provider health/status shown uniformly regardless of backend

---

### 3. Simplicity Wins

> *Don't reinvent the wheel. Code exists to be used.*

The best code is code someone else already debugged. Cherry-pick from open repos. Adapt known solutions. Use battle-tested libraries. If something already works — in our own git history, in someone else's MIT repo, in a standard library — use it. Only write novel code for novel problems.

Complexity is a cost, not a feature. Three clear lines beat one clever abstraction. A working simple solution beats an elegant broken one. Always.

**Instantiations:**
- localStorage for settings persistence (not a custom database)
- reqwest for HTTP (not custom networking)
- llama.cpp for inference (not a custom runtime)
- Hooks cherry-picked from vincitamore/claude-org-template (not built from scratch)
- GGUF spec followed exactly for metadata parsing (not a custom format)

**Demands:**
- Before writing a new system, search for existing solutions first
- Before rewriting a function, check git history — maybe the old version worked
- If a dependency does 80% of the job, use it and handle the 20%

---

### 4. Errors Are Answers

> *Every failure teaches. Given a model, the program debugs itself.*

An error message that says "something went wrong" is itself a bug. Every error must say what happened, why, and what the user can do about it. Logs aren't optional — they're the program's memory of its own behavior.

The endgame: if a model is loaded and something breaks, HIVE should be able to read its own logs and diagnose the problem. The program becomes its own debugger.

**Instantiations:**
- Actionable error messages: `"Chat failed (HTTP 500): model not loaded"` not `"Error: {}"`
- Pre-send health checks: verify server is alive before sending a message
- Abort recovery: stop button works, next message works too, no broken state
- `startModel` logging includes all parameters (model, backend, gpuLayers, contextLength, kvOffload)
- Dual-log system: 11 backend modules log lifecycle events to `hive-app.log` via `append_to_app_log()`, frontend bridge auto-persists `[HIVE]` logs
- `check_logs` tool: model can read its own operational state (server crashes, provider errors, VRAM events)
- VRAM eviction and auto-sleep log to persistent app log (not just console.log) — AI can see what happened
- Procedure learning: failed tool chains logged as MAGMA events for future avoidance

**Demands:**
- Capture llama-server stdout/stderr (currently /dev/null — this is a violation)
- Surface server logs in the Logs tab for user visibility
- Token/speed display so users can SEE if something is wrong (slow = KV offload to RAM)
- Aspirational: model reads its own error logs and suggests fixes

---

### 5. Fix The Pattern, Not The Instance

> *Cure the root cause. Don't treat symptoms.*

When you find a bug, the bug is never alone. The same mistake that caused it exists in 3-5 other places — you just haven't hit them yet. Search for the pattern. Fix every instance. If you only fix the one you found, you're treating symptoms while the disease spreads.

This applies to architecture too. If a design keeps producing the same class of bug, the design is wrong — not the individual bugs.

**Instantiations:**
- `.trim()` TypeError: found 3 vulnerable call sites, fixed all 3 (not just the one that crashed)
- localStorage hydration: `{ ...defaults, ...stored }` pattern applied everywhere (not just the one that broke)
- CLAUDE.md documents grep commands for each bug class: "found missing User-Agent? Check ALL HTTP clients"
- Coding standards encode patterns, not individual fixes

**Demands:**
- Every bug fix includes a grep for the same pattern across the codebase
- If a pattern produces bugs twice, add it to CLAUDE.md so it never happens again
- Root cause analysis before fix — the error might be downstream of the real bug

---

### 6. Secrets Stay Secret

> *Military-grade OPSEC. Nothing left open to exploitation.*

If encryption exists, use the strongest available. API keys are not config — they're secrets. They get AES-256-GCM encryption, stored in dedicated encrypted files, never in localStorage, never in plaintext, never logged, never exposed in error messages. Background processes hide their windows. Network requests don't leak headers they shouldn't.

Security is not a feature you add later. It's a property of every line of code.

**Instantiations:**
- AES-256-GCM encrypted API key storage (`~/.hive/secrets.enc`)
- Keys never touch localStorage (settings and secrets are separate systems)
- `CREATE_NO_WINDOW` flag on all background processes (no visible CMD windows)
- User-Agent headers set explicitly (don't leak default client signatures)

**Demands:**
- Audit any new storage mechanism for secret leakage
- Never log API keys, tokens, or credentials (even in debug mode)
- Environment variables for secrets in CI/CD, never committed
- Treat model conversation content as user-private data

---

### 7. The Framework Survives

> *Models evolve. Providers come and go. HIVE endures.*

HIVE is a house. Models are tenants. A tenant moves out — the house is still a house. A new tenant moves in — the house accommodates them. The house was here before any particular tenant, and it will be here after.

This means: never build load-bearing walls around a specific model, a specific API, or a specific provider. The architecture is the product. Everything else is pluggable.

**Instantiations:**
- Provider abstraction survives any single API's deprecation
- GGUF parsing works for any quantization, any model family
- Chat interface is model-agnostic (works with 1B parameter models and 70B alike)
- Vision docs describe architecture independent of current model landscape
- Cognitive harness: identity (`HIVE.md`) survives model swaps. Capabilities auto-update
- MAGMA memory graph is provider-independent — works with any model, any embedding backend
- Skills system is model-agnostic — same `.md` files work with local 3B or cloud Claude
- Specialist slots accept any provider: swap a local coder for a cloud Claude coder in Settings

**Demands:**
- No hardcoded model names, sizes, or capabilities in core logic
- Architecture decisions documented separately from implementation
- When a new model format emerges, only the parser changes — not the framework

---

### 8. Low Floor, High Ceiling

> *A noob can use it. A power user would want to.*

Every feature has two faces: the simple default and the full control surface. A beginner double-clicks `START_HIVE.bat` and is chatting with a local model in minutes — no terminal, no config files, no knowledge of VRAM. A power user adjusts GPU layers, KV cache offload, context length, system prompts, and monitors token throughput.

Neither user should feel the product wasn't built for them.

**Instantiations:**
- `START_HIVE.bat` one-click launcher (handles dependency checks, builds, launches)
- Hardware auto-detection (user never needs to know their GPU model or VRAM)
- Per-model settings with sensible defaults (works out of the box, tunable if you know what you're doing)
- Context length slider capped to model's actual max (can't set an invalid value)
- VRAM pre-launch check: warns before starting, auto-evicts idle specialists if VRAM is tight
- Auto-sleep: specialists that go idle for 5 minutes are automatically stopped — user never has to manage VRAM manually
- Skills: users just drop `.md` files in a folder. No code, no config, no restart
- Routing indicator: when HIVE delegates to a specialist, the user sees what's happening (not just a blank screen)
- Model recommendation engine — three-tier, hardware-adaptive:
  - Fetches real model data from HuggingFace (file lists, sizes, base_model tags)
  - Looks up benchmark scores from Open LLM Leaderboard v2 (quality ranking)
  - Groups by GPU utilization: Fast (≤75% GPU), Quality (75-100%), Big Brain (RAM offload)
  - Thresholds are percentage-based — adapts to any GPU automatically
  - Conservative RAM budget (50% of system RAM) for realistic recommendations
  - All computation local, zero hardware data sent externally (Principle #6)
- Two-layer display: noobs see speed icons + labels, power users see GB breakdown on hover
- RAM-aware compatibility: models that overflow VRAM but fit in VRAM+RAM show as "Slower (uses RAM)" instead of "won't fit"
- Files auto-sorted by compatibility: fastest options shown first
- "Too large" models have download button disabled — prevents wasted bandwidth
- Settings tab "Go to Models" button — no dead ends for lost users

**Demands:**
- VRAM pre-launch estimator: warn before starting, not after crashing
- Progressive disclosure in UI: simple view by default, advanced settings available
- Every advanced feature has a tooltip or help text explaining what it does
- Zero-config path must always work — settings are optional enhancements, never requirements

---

## Using The Lattice

### For Design Decisions

When stuck between two approaches, score them against the principles:

| Approach A | Approach B |
|-----------|-----------|
| Violates #1 (couples two modules) | Honors #1 (clean separation) |
| Honors #3 (simpler) | Violates #3 (complex) |
| **Mixed — needs thought** | **Mixed — needs thought** |

If one approach cleanly honors more principles without violating any, it wins. If both violate something, find a third approach.

### For Code Review

Every PR can be checked: *does this change violate any principle?* Not "is this code clean" — that's subjective. "Does this violate the lattice" — that's answerable.

### For New Contributors

Read this document first. If you understand these 8 principles, you understand how HIVE thinks. The codebase is the implementation; the lattice is the intent.

---

## Lattice Lineage

The principle lattice concept was adapted from [vincitamore/claude-org-template](https://github.com/vincitamore/claude-org-template), where it serves as the structural backbone of a self-maintaining knowledge system. HIVE's principles are our own, but the idea of naming them, tracking their instantiations, and using them as a decision framework comes from Vin's work.

---

*Last updated: February 2026*

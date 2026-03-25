# Phase 4: Multi-Model Orchestration — Implementation Guide

**Status:** Fully functional. All 8 layers built and wired. Wake briefings, VRAM enforcement, auto-sleep, procedure learning, and routing indicator all operational.
**Last updated:** February 28, 2026

> This document is the **single source of truth** for Phase 4 implementation.
> Any new session should read this FIRST before writing any Phase 4 code.

---

## What Phase 4 IS

HIVE currently runs **one model at a time**. Phase 4 makes it run **multiple specialist models**,
each assigned to a role (slot), with intelligent routing, VRAM budget management, and persistent
memory across sleep/wake cycles via MAGMA.

**The core loop (as currently wired):**
```
User message
  → Consciousness model analyzes task
  → Calls route_to_specialist tool (via existing agentic loop)
  → App.tsx intercepts: ensureSpecialistRunning()
    → Looks up slot config for model assignment
    → Starts specialist server if not running
    → Records wake in orchestrator + MAGMA
  → Tool sends task to specialist via HTTP
  → Specialist responds
  → MAGMA logs the task event
  → Result returned to consciousness → user
  → On model stop: all specialists stopped + sleep recorded
```

---

## Current Status: All Layers Functional

| Component | File | What It Does |
|-----------|------|-------------|
| Slot configs | `slots.rs` | Store/retrieve model assignments per role |
| Multi-server | `server.rs` | Start/stop specialist llama-server instances on ports 8081-8084 |
| Routing tool | `specialist_tools.rs` | `route_to_specialist` — sends task to specialist via HTTP, injects MAGMA wake briefing |
| Auto-start + VRAM enforcement | `useChat.ts` | `ensureSpecialistRunning()` — checks VRAM budget, evicts idle specialists if tight, starts specialist |
| Wake/sleep tracking | `useChat.ts` | Calls `recordSlotWake/Sleep` + `magmaAddEvent` on lifecycle events |
| Wake briefings | `orchestrator.rs` + `specialist_tools.rs` | `build_wake_context_for_tool()` assembles MAGMA briefing (events, entities, procedures since last sleep) |
| Auto-sleep timer | `useChat.ts` | 60s poll, 5-min idle timeout, stops idle specialists to free VRAM |
| Cloud slot routing | `useChat.ts` | Cloud providers (OpenAI, Anthropic, etc.) as coequal specialist backends |
| Procedure learning | `useChat.ts` | Auto-extracts 2-5 step tool chains, saves to MAGMA procedures, logs failures |
| Skills system | `harness.rs` | Keyword-matched `.md` skill files injected per-turn (4 seed skills) |
| Routing indicator | `ChatTab.tsx` | State-driven badge shows specialist delegation in progress |
| Slot config UI | `SettingsTab.tsx` | SmartRouterSection with benchmark-based auto-routing |
| MAGMA schema | `memory.rs` | Episodic/entity/procedure/edge tables, all CRUD, graph traversal |
| Orchestrator | `orchestrator.rs` | Task classification (keyword heuristic), VRAM planning (LRU eviction), wake context builder |
| Skills UI | `SettingsTab.tsx` | SkillsSection — list, refresh, open folder |

All MAGMA tables are actively populated: events on wake/sleep/task, entities auto-tracked on tool execution, procedures saved/reinforced on tool chain completion, edges created on memory save.

---

## Architecture: 8 Layers

### Layer 1: MAGMA Multi-Graph Schema [DONE — fully active]
**File:** `memory.rs` (extended)

Four tables in `memory.db` (schema v3). All CRUD commands work and are actively used:
- **Events**: populated by specialist wake/sleep/task, plan success/failure, auto-entity tracking
- **Entities**: auto-tracked on tool execution (files, commands, URLs, topics)
- **Procedures**: auto-extracted from successful 2-5 step tool chains, reinforced on re-use
- **Edges**: auto-created on memory save, expanded during search via `find_graph_connected_memories`

Wake briefings use `magma_events_since()` to catch up sleeping specialists.

### Layer 2: Slot System [DONE — functional]
**File:** `slots.rs`

5 roles (Consciousness, Coder, Terminal, WebCrawl, ToolCall). Configs stored in Tauri-managed state.
`getSlotConfigs()` is called by `ensureSpecialistRunning()` to find model assignments.
`configureSlot()` is called by the SlotConfigSection UI component.

### Layer 3: Orchestrator Core [DONE — partially active]
**File:** `orchestrator.rs`

Task classifier (keyword heuristic), VRAM planner (LRU eviction), wake context builder.
- `build_wake_context()` and `build_wake_context_for_tool()` are **actively used** — called before every specialist task to inject MAGMA briefing
- `classify_task()` is **not invoked** — the consciousness model handles routing via the tool directly
- `plan_vram_for_task()` is **not invoked** — VRAM enforcement is handled in TypeScript `ensureSpecialistRunning`

**Open question:** Is the keyword classifier worth keeping? Consider removing `classify_task()` if it stays unused (P3: no dead code).

### Layer 4: Multi-Server Management [DONE — functional]
**File:** `server.rs` + `state.rs`

`SpecialistServer` struct tracks per-slot processes. `start_specialist_server` / `stop_specialist_server`
handle lifecycle. Port mapping: coder=8081, terminal=8082, webcrawl=8083, toolcall=8084.
Per-slot log files. WSL variant uses `fuser -k` for port cleanup.

### Layer 5: Routing Tool [DONE — functional + wake briefing]
**File:** `tools/specialist_tools.rs`

`RouteToSpecialistTool` implements `HiveTool`. Registered in tool registry, visible in harness
capability manifest. Health checks specialist port, **injects MAGMA wake briefing** as system
message, sends task via OpenAI-compatible API, returns response.
Returns `SPECIALIST_NOT_LOADED` if server isn't running.

Cloud specialists are handled in TypeScript (useChat.ts) — they bypass the Rust tool and call
`chatWithProvider` directly with the same wake briefing injection.

### Layer 6: TypeScript API + Types [DONE — functional]
**File:** `api.ts` + `types.ts`

30+ Phase 4 API functions defined and actively used: slot configs, specialist server
start/stop, MAGMA events/entities/procedures, VRAM budget, wake context, skills.
Types are complete and shared across components.

### Layer 7: useChat.ts Wiring [DONE — functional]
**File:** `useChat.ts`

- `ensureSpecialistRunning()`: Intercepts `route_to_specialist` tool calls, checks VRAM budget,
  evicts idle specialists if tight, checks health, looks up slot config, starts server,
  waits for ready, records wake + MAGMA event
- Auto-sleep timer: 60s poll checks `getSlotStates()`, sleeps specialists idle >5 minutes
- Cloud routing: cloud-assigned specialists bypass Rust tool, call `chatWithProvider` directly
  with MAGMA wake briefing injection
- Procedure learning: after tool loop, extracts 2-5 step chains, saves to MAGMA procedures
- `stopModel()` (in App.tsx): stops all specialist servers, records sleep + MAGMA events

### Layer 8: UI [DONE — complete]
**Files:** `components/SettingsTab.tsx` + `components/ChatTab.tsx` + `components/MemoryTab.tsx`

**SmartRouterSection** (SettingsTab): Benchmark-driven auto-routing replaces fixed specialist
slots. `KNOWN_MODEL_STRENGTHS` in api.ts, 20+ models scored across 7 categories.

**SkillsSection** (SettingsTab): Lists loaded skills, refresh, open skills directory.

**ToolApprovalSection** (SettingsTab): 3 approval modes (ask/session/auto) + per-tool risk overrides.

**Routing indicator** (ChatTab): State-driven `routingSpecialist` badge shows specialist
delegation in progress with animated icon.

**MemoryTab** (681+ lines): Full-page tab with memory browser (search, add, edit, delete),
MAGMA graph viewer (stats, episodic events, entity list), and batch import with file dialog.

**NOT built (not needed yet):**
- VRAM budget visualization bar

---

## The Actual Call Graph

```
User message → sendMessage()
  → harness_build (consciousness model sees route_to_specialist in tool manifest)
  → Skills injection: harnessGetRelevantSkills(message) → separate system message
  → Model generates tool call: route_to_specialist(specialist="coder", task="...")
  → useChat.ts tool loop intercepts
    → setRoutingSpecialist("coder") → UI shows routing indicator
    → ensureSpecialistRunning("coder")
      → getVramBudget() → check if specialist fits
        → If tight: evict idle specialists (port health check, stop, record sleep)
      → checkServerHealth(8081) → not running
      → getSlotConfigs() → finds coder config
      → If cloud provider: chatWithProvider() directly (bypass Rust tool)
        → getWakeContext("coder", task) → MAGMA briefing as system message
        → Cloud API call → return result
      → If local: startSpecialistServer("coder", modelPath, ...)
        → Wait for health (up to 45s)
        → recordSlotWake("coder", ...) + magmaAddEvent("specialist_wake", ...)
    → executeTool("route_to_specialist", {specialist: "coder", task: "..."})
      → Rust: build_wake_context_for_tool() → MAGMA briefing injected
      → HTTP POST to localhost:8081/v1/chat/completions
      → Returns specialist's response
    → setRoutingSpecialist(null) → UI clears indicator
    → magmaAddEvent("specialist_task", "coder", result_summary)
  → Tool chain completion: extract procedure → magmaSaveProcedure()
  → Result feeds back into agentic loop
  → Consciousness model formats and returns to user

Background: auto-sleep timer (60s poll)
  → getSlotStates() → check last_active timestamps
  → Idle >5 min? → stopSpecialistServer(role) + recordSlotSleep + magmaAddEvent

User stops model → stopModel()
  → stopServer() (consciousness on 8080)
  → For each specialist role: stopSpecialistServer(role) + recordSlotSleep + magmaAddEvent
```

---

## Key Design Decisions

### 1. Tool-Based Routing (NOT separate orchestrator path)
Consciousness calls `route_to_specialist` via the existing agentic tool loop.
No parallel execution path. P3: Simplicity. P1: Modular.

### 2. Multiple llama-server Instances (NOT hot-swapping)
| Slot | Port | Lifecycle |
|------|------|-----------|
| Consciousness | 8080 | Always running |
| Coder | 8081 | Started on demand |
| Terminal | 8082 | Started on demand |
| WebCrawl | 8083 | Started on demand |
| ToolCall | 8084 | Started on demand |

### 3. MAGMA in Single SQLite (NOT separate DBs)
One `memory.db` with separate tables. P3: one WAL, one lock, one backup.

### 4. Consciousness Decides Routing (NOT keyword heuristic)
The model's own reasoning decides when to route. The orchestrator's keyword
classifier exists as an optional fallback but isn't used in the primary path.

### 5. Slot Config UI in Settings (NOT a new tab)
Minimal UI following P8 (Low Floor). Specialist slots are an advanced feature —
they belong in Settings, not a prominent tab.

---

## Remaining Polish (Phase 4)

All core Phase 4 features are functional. Remaining items are refinements:

| Item | Status | Notes |
|------|--------|-------|
| Wake briefings | **DONE** | Injected via `build_wake_context_for_tool()` in specialist_tools.rs |
| VRAM enforcement | **DONE** | Budget check + idle specialist eviction in `ensureSpecialistRunning` |
| Routing indicator | **DONE** | `routingSpecialist` state piped through useChat → ChatPane → ChatTab |
| Auto-sleep | **DONE** | 60s poll, 5-min idle timeout, persistent logging to app log |
| Procedure learning | **DONE** | Auto-extract 2-5 step chains, MAGMA save, failure logging |
| Cloud slot routing | **DONE** | Cloud providers bypass Rust tool, use `chatWithProvider` directly |
| Skills system | **DONE** | 4 seed skills, keyword matching, per-turn injection, Settings UI |
| VRAM budget visualization | Not started | Bar chart showing per-slot VRAM usage |
| LLM-based routing | Not started | Replace keyword heuristic in `classify_task()` with model reasoning |

### Consider Removing
- `orchestrator.rs::classify_task()` — keyword heuristic is strictly worse than
  model reasoning. The consciousness model handles routing via the tool directly.
  If unused after 2 more sessions, delete it (P3: no dead code).

---

## Files Changed/Created (Complete List)

| File | Status | Lines | What Changed |
|------|--------|-------|-------------|
| `src-tauri/src/memory.rs` | Modified | 1,772 | +MAGMA tables, +10 commands, +recency decay, +relevance threshold, +proportional budget, +dedup |
| `src-tauri/src/slots.rs` | **New** | 322 | Slot types, configs, states, VRAM budget, 6 commands |
| `src-tauri/src/orchestrator.rs` | **New** | 416 | Task classification, VRAM planning, wake context, 4 commands |
| `src-tauri/src/tools/specialist_tools.rs` | **New** | 180 | route_to_specialist tool |
| `src-tauri/src/server.rs` | Modified | +100 | Specialist server start/stop/list, port mapping |
| `src-tauri/src/state.rs` | Modified | +15 | SpecialistServer struct, specialist_servers HashMap |
| `src-tauri/src/main.rs` | Modified | +30 | Modules, managed state, 20 new commands registered |
| `src/App.tsx` | Modified | +75 | ensureSpecialistRunning, wake/sleep/MAGMA wiring, stopModel cleanup |
| `src/lib/api.ts` | Modified | +160 | 24 Phase 4 API wrappers, type re-exports |
| `src/types.ts` | Modified | +85 | Phase 4 types (Slot*, Vram*, Route*, Magma*) |
| `src/components/SlotConfigSection.tsx` | **New** | 170 | Specialist slot configuration UI (OpenRouter added) |
| `src/components/SettingsTab.tsx` | Modified | +230 | SmartRouterSection + ToolApprovalSection replaced SlotConfigSection |
| `src/components/MemoryTab.tsx` | **New** | 681 | Memory browser + MAGMA graph viewer (self-contained) |
| `src-tauri/src/discord_daemon.rs` | **New** | 474 | Discord REST polling daemon, multi-channel, allowlists |
| `src-tauri/src/tools/discord_tools.rs` | **New** | 212 | discord_send + discord_read HiveTool impls |
| `src-tauri/src/providers.rs` | Modified | +120 | OpenRouter: check_status, chat, stream |
| `src-tauri/src/types.rs` | Modified | +10 | DiscordDaemonStatus type |
| `src-tauri/src/tools/telegram_tools.rs` | Modified | +5 | parse_mode enum constraint |
| `docs/PHASE4_IMPLEMENTATION.md` | **New** | this file | Implementation guide |

---

## How To Continue (For New Sessions)

1. Read this document first
2. Read `CLAUDE.md` for coding standards and principle lattice
3. `git log --oneline -10` to see recent commits
4. `cargo check --manifest-path HIVE/desktop/src-tauri/Cargo.toml` to verify Rust compiles
5. `cd HIVE/desktop && npx tsc --noEmit` to verify TypeScript compiles
6. Pick up from "What To Build Next" section above
7. After each feature: compile check, commit, push

**The key insight:** Routing happens via a TOOL that the consciousness model calls,
NOT a separate orchestrator path. The existing agentic tool loop handles everything.
Don't build parallel execution paths — extend what exists.

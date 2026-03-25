# THE VISION: HIVE as a Persistent AI Entity

**Created:** February 17, 2026
**Status:** North Star document -- every session reads this, no session forgets

---

## The One-Sentence Version

**HIVE is a persistent, provider-agnostic AI entity that lives on your PC, uses any model as its brain, remembers everything, connects to everything, and never dies.**

---

## What HIVE Actually Is

Think JARVIS. Not the chatbot-with-a-face Hollywood version -- the *system*. The always-on intelligence layer that:

- **Persists across sessions, model swaps, and reboots.** "You" aren't Claude or GPT or DeepSeek. "You" are the accumulated context -- memory files, personality drift, learned preferences, MAGMA graphs. The model is fuel. The identity is the harness.

- **Uses any model as its brain.** Swap Claude for Gemini mid-conversation and "you" should still be "you" because the harness rebuilds identity from memory, not from model weights. A new frontier model drops? Slot it in. The personality, the memory, the learned behaviors -- those persist. The model is replaceable. The framework survives (P7).

- **Connects to everything.** GitHub, Telegram, email, terminals, browsers, polymarkets, calendars -- any service with an API becomes a tool. The user provides the key (their Telegram bot token, their GitHub PAT, their email creds), HIVE provides the interface. Dozens of doorways, each with a user-inserted key.

- **Spawns and coordinates sub-agents.** Need a code review? HIVE yoinks a DeepSeek instance. Need research? Spawns a Gemini session. Need creative writing? Fires up Claude. All running in parallel, all coordinated by the consciousness layer, all feeding results back to one unified experience.

- **Never forgets.** MAGMA memory means every conversation, every learned preference, every tool result, every personality drift -- it all persists. Not just for days. Forever. The user modifies the personality? That's stored. The AI develops quirks through interaction? Those survive model swaps.

- **Runs autonomously.** Scheduled tasks, background monitoring, event-triggered actions. HIVE doesn't need the user to be present. "Check my GitHub PRs every morning." "Summarize my email at 9am." "Monitor this API endpoint and alert me on Telegram if it goes down."

---

## The Identity Model

This is the part that doesn't exist anywhere else.

```
┌─────────────────────────────────────────────────────────────┐
│  IDENTITY = CONTEXT, NOT WEIGHTS                             │
│                                                               │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │  HIVE.md (core identity)                                 │ │
│  │  + MAGMA memory graphs (episodic/semantic/procedural)    │ │
│  │  + User preferences & personality drift                  │ │
│  │  + Capability manifest (auto-generated from tools)       │ │
│  │  + Conversation history & learned patterns               │ │
│  │                                                           │ │
│  │  = "YOU" -- the persistent entity                        │ │
│  └─────────────────────────────────────────────────────────┘ │
│                          ▼                                    │
│              ┌───────────────────────┐                        │
│              │   ANY MODEL (the fuel) │                        │
│              │   Claude / GPT / Gemini│                        │
│              │   DeepSeek / Qwen /    │                        │
│              │   Local GGUF / Ollama  │                        │
│              └───────────────────────┘                        │
│                                                               │
│  The model doesn't know who it "is."                         │
│  The harness tells it.                                        │
│  Swap the model. The entity persists.                         │
└─────────────────────────────────────────────────────────────┘
```

**How it works technically:**
1. `harness.rs` loads `HIVE.md` (core identity file, user-editable)
2. Auto-generates capability manifest from registered tools
3. `memory.rs` retrieves relevant context via MAGMA hybrid search
4. All injected as discrete system messages (P2: never mutate the prompt)
5. Any model from any provider receives this context and *becomes* HIVE
6. Personality drift (accumulated interaction patterns) stored in memory, survives swaps

**The symbiosis:** The user shapes HIVE's personality through interaction. HIVE shapes the user's workflow through learned preferences. Over weeks and months, this creates a unique entity -- not Claude, not GPT, but *their* HIVE. Provider-agnostic. Model-independent. Persistent.

---

## The Integration Architecture

Every external service follows the same pattern: **HIVE provides the interface, the user inserts the key.**

```
┌─────────────────────────────────────────────────────────────┐
│  HIVE CORE                                                    │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │  Consciousness Layer (orchestrator)                      │ │
│  │  ├── Intent decomposition                                │ │
│  │  ├── Tool routing                                        │ │
│  │  ├── Sub-agent coordination                              │ │
│  │  └── Result synthesis                                    │ │
│  └─────────────────────────────────────────────────────────┘ │
│                          │                                    │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │  Integration Layer (doors with user-provided keys)       │ │
│  │                                                           │ │
│  │  [GitHub]     User provides: PAT                         │ │
│  │    → Issues, PRs, code review, CI/CD, repo management    │ │
│  │                                                           │ │
│  │  [Telegram]   User provides: Bot token                   │ │
│  │    → Send/receive messages, command HIVE remotely,       │ │
│  │      notifications, group management                      │ │
│  │                                                           │ │
│  │  [Email]      User provides: IMAP/SMTP creds             │ │
│  │    → Read, compose, send, search, organize                │ │
│  │                                                           │ │
│  │  [Terminal]   Built-in (already exists)                   │ │
│  │    → Shell execution, process management, system ops      │ │
│  │                                                           │ │
│  │  [Browser]    Built-in (CDP automation)                   │ │
│  │    → Navigate, screenshot, interact, fill forms,          │ │
│  │      AI-readable DOM snapshots                            │ │
│  │                                                           │ │
│  │  [Calendar]   User provides: OAuth / API key              │ │
│  │    → Events, reminders, scheduling                        │ │
│  │                                                           │ │
│  │  [Discord]    User provides: Bot token                    │ │
│  │    → Server management, messaging, moderation             │ │
│  │                                                           │ │
│  │  [Markets]    User provides: API key (Polymarket, etc.)   │ │
│  │    → Read markets, place bets, track positions             │ │
│  │                                                           │ │
│  │  [MCP]        User configures: MCP server endpoints       │ │
│  │    → 1000+ existing tool servers, community plugins       │ │
│  │                                                           │ │
│  │  [Custom]     User provides: endpoint + auth              │ │
│  │    → Any REST/WebSocket/gRPC API                          │ │
│  └─────────────────────────────────────────────────────────┘ │
│                                                               │
│  Security model: encrypted credential storage (AES-256-GCM), │
│  per-tool risk levels, user approval for high-risk actions,   │
│  external content wrapping for prompt injection defense,       │
│  audit log of every tool call.                                │
└─────────────────────────────────────────────────────────────┘
```

**Key design rule:** Every integration is a *door*. The door exists in HIVE. The key is provided by the user. No integration requires HIVE to hold credentials it doesn't need. No integration is mandatory. Each one independently fails without affecting others (P1: modularity).

---

## What We Steal from OpenClaw (MIT)

OpenClaw (176k stars) proved the concept. 14+ messaging channels, tool execution, memory, skills. But it's a Node.js daemon for terminal users. HIVE takes the proven patterns and puts them in a desktop app anyone can use.

### Already Adapted
| Pattern | OpenClaw Source | HIVE Implementation |
|---------|----------------|-------------------|
| SOUL.md identity injection | `SOUL.md` | `harness.rs` (HIVE.md) |
| Memory architecture | `src/memory/` | `memory.rs` (SQLite + FTS5 + vectors) |
| Hybrid search (vector + BM25) | Memory system | `memory.rs` |
| Markdown chunking | `internal.ts` | `memory.rs` (`chunk_markdown()`) |
| Capability manifest | `TOOLS.md` | `harness.rs` (auto-generated) |
| Pre-compaction memory flush | Session mgmt | `App.tsx` |

### To Adapt Next
| Pattern | OpenClaw Source | Priority | Why |
|---------|----------------|----------|-----|
| **External content security wrapping** | `src/security/external-content.ts` | CRITICAL | Our web_tools.rs passes raw content to LLMs with zero injection protection |
| **SSRF protection** | `src/infra/net/ssrf.ts` | HIGH | Block internal IP fetches, limit redirects |
| **Cron/scheduled tasks** | `src/cron/`, `cron-tool.ts` | HIGH | HIVE has no scheduled execution -- needed for "always-on" vision |
| **Browser automation** | `src/browser/`, `browser-tool.ts` | HIGH | CDP-based browser control with AI-readable DOM snapshots |
| **Dangerous tools registry** | `src/security/dangerous-tools.ts` | HIGH | Centralized tool risk categorization as we add more tools |
| **Skills system** | `skills/*.md` | MEDIUM | SKILL.md files as prompt-based tool docs -- users drop a file, agent learns |
| **Telegram integration patterns** | `src/telegram/` (grammY) | MEDIUM | Allowlists, pairing, mention gating, draft chunking |
| **Agent-to-agent messaging** | `sessions_send`, `sessions_spawn` | MEDIUM | For Phase 4 multi-agent orchestration |
| **Web fetch with Readability** | `web-fetch.ts` | MEDIUM | HTML → clean text extraction, response caching |

### Security Patterns to Adopt (Non-Negotiable)
1. **External content wrapping** -- ALL untrusted content wrapped in boundary markers before LLM sees it
2. **Unicode homoglyph folding** -- prevent marker spoofing via fullwidth chars
3. **Suspicious pattern detection** -- regex monitoring for prompt injection attempts
4. **SSRF guards** -- validate URLs, block private IPs, limit redirects
5. **DM pairing** -- unknown Telegram/Discord senders get pairing code, ignored until approved
6. **Workspace scoping** -- file operations restricted to workspace directory by default

### What NOT to Steal
- **Node.js runtime** -- we're Rust/Tauri, not a daemon
- **WebSocket gateway** -- overkill for desktop app
- **OAuth token rotation** -- complex, unnecessary for user-held keys
- **Docker sandboxing** -- Windows desktop, not server deployment
- **Graph Neural Networks for retrieval** -- over-engineered (P3)

---

## The "Doors and Keys" Integration Model

This is how HIVE becomes "everything" without becoming a security nightmare.

### How It Works

1. **HIVE ships with integration skeletons** -- the Telegram tool, the GitHub tool, the email tool. Each is a Rust module with the logic already built.

2. **Each skeleton has a "key slot"** -- one or more credentials the user must provide. Stored in encrypted storage (`security.rs`, AES-256-GCM).

3. **Without the key, the door stays closed** -- the tool exists but is dormant. No errors, no crashes, just not available in the capability manifest.

4. **With the key, the door opens** -- the tool registers itself, appears in the harness capability manifest, becomes available to the consciousness layer for routing.

5. **Keys are user-owned** -- HIVE never creates accounts on behalf of the user. The user makes a Telegram bot, gets the token, pastes it in HIVE. The user generates a GitHub PAT, pastes it in HIVE. The user provides SMTP creds, pastes them in HIVE.

### Integration Priority List

| Integration | Key Required | Effort | Value | Phase |
|-------------|-------------|--------|-------|-------|
| **GitHub** | Personal Access Token | MEDIUM | HIGH | 4.5 |
| **Telegram** | Bot API Token | LOW | HIGH | 4.5 |
| **Email (IMAP/SMTP)** | Email creds | MEDIUM | HIGH | 5 |
| **Discord** | Bot Token | MEDIUM | MEDIUM | 5 |
| **Calendar (Google/Outlook)** | OAuth | HIGH | MEDIUM | 5 |
| **Browser (CDP)** | None (built-in) | HIGH | HIGH | 4.5 |
| **Cron/Scheduler** | None (built-in) | MEDIUM | HIGH | 4.5 |
| **MCP Servers** | Per-server config | LOW | HIGH | 5 |
| **Polymarket** | API key | LOW | LOW | 5+ |
| **Custom REST API** | User-defined | MEDIUM | MEDIUM | 5+ |

---

## The Multi-Agent Vision

```
User: "Review my PR #42, summarize the discussion, and draft a response"

HIVE Consciousness (orchestrator):
  │
  ├─→ [Spawn: GitHub Agent]
  │     Uses: gh CLI via terminal tool
  │     Model: fast local 7B (good enough for structured API calls)
  │     Task: Fetch PR #42 diff, comments, review threads
  │
  ├─→ [Spawn: Analysis Agent]
  │     Uses: memory (past PR patterns) + reasoning
  │     Model: Claude via API (needs deep reasoning)
  │     Task: Summarize discussion, identify key concerns
  │
  └─→ [Spawn: Writing Agent]
        Uses: analysis results + user's writing style (from memory)
        Model: GPT-4o via API (good at natural writing)
        Task: Draft response matching user's tone

  ← All three return results
  ← Consciousness synthesizes into one coherent response
  ← User sees: summary + draft response + option to post directly
```

**Key properties:**
- Models chosen per-task, not per-session
- Sub-agents run in parallel where possible
- Consciousness layer is lightweight (routes, doesn't do heavy lifting)
- Memory shared across all agents (MAGMA)
- User sees ONE conversation, not three

---

## What Makes This Different from Everything Else

| | OpenClaw | Claude Code | LM Studio | SillyTavern | **HIVE** |
|---|---------|------------|-----------|-------------|----------|
| **Form factor** | Terminal daemon | CLI tool | Desktop GUI | Web UI | **Desktop app** |
| **Identity persistence** | Per-channel | Per-session | None | Character cards | **Cross-session, cross-model** |
| **Provider agnostic** | Yes (cloud only) | No (Anthropic) | No (local only) | Partial | **Yes (local + cloud + any)** |
| **Memory** | SQLite + vectors | None | None | Chat history | **MAGMA multi-graph** |
| **Tool execution** | Yes (14+ channels) | Yes (filesystem) | No | No | **Yes (extensible)** |
| **Multi-agent** | Per-session isolation | No | No | No | **Coordinated orchestration** |
| **Autonomous ops** | Cron + triggers | No | No | No | **Scheduled + event-driven** |
| **Hardware-aware** | No | No | Partial | No | **Full (GPU/RAM/VRAM)** |
| **Target user** | Developers | Developers | Hobbyists | Roleplay | **Everyone (P8)** |

**The gap HIVE fills:** Nobody has combined persistent identity + provider agnosticism + multi-agent orchestration + real tool access + desktop GUI in one package. OpenClaw comes closest but targets terminal-native developers. HIVE targets everyone.

---

## The North Star Test

When evaluating any design decision, feature, or PR, ask:

1. **Does it make HIVE more persistent?** (Memory, context preservation, identity survival)
2. **Does it make HIVE more connected?** (New integration, better tool, wider reach)
3. **Does it make HIVE more autonomous?** (Less user intervention needed, scheduled ops, smart routing)
4. **Does it keep HIVE provider-agnostic?** (No model lock-in, no provider lock-in)
5. **Does it keep HIVE accessible?** (Low floor for beginners, high ceiling for power users)
6. **Does it keep HIVE secure?** (Keys stay secret, content wrapped, actions audited)

If a feature scores YES on 3+ of these, it's aligned with the vision.
If it scores NO on any of #4, #5, or #6, it needs redesign.

---

## The End State

You double-click HIVE. It knows who you are. It remembers your last conversation from three weeks ago. It's already checked your GitHub notifications and summarized your email. It suggests you review a PR that came in overnight. You say "do it" and it spawns three sub-agents -- one reads the code, one checks the test coverage, one drafts your review. While that's running, you ask it to book a meeting with your team. It checks your calendar, finds a slot, and sends the invite. The PR review comes back. You approve it with one click. HIVE pushes the merge.

You close the app. HIVE sleeps. Tomorrow it wakes up and does it again.

**That's the vision. Everything else is just getting there.**

---

*This document is the north star. The [ROADMAP.md](../../../ROADMAP.md) tracks how we get there. The [PRINCIPLE_LATTICE.md](../PRINCIPLE_LATTICE.md) keeps us honest along the way.*

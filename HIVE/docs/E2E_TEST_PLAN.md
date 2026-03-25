# HIVE End-to-End Test Plan

**Purpose:** Step-by-step manual test plan for verifying HIVE works with real models and integrations. Run before merging major changes.

**Prerequisites:**
- HIVE built and running (`npm run tauri dev` or `START_HIVE.bat`)
- At least one cloud API key configured (OpenAI or Anthropic)
- Optionally: a local GGUF model downloaded (TinyLlama 1.1B Q4_K_M recommended for fast testing)
- Optionally: Telegram bot token and Discord bot token configured

---

## 1. Cloud Chat (OpenAI or Anthropic)

**Steps:**
1. Open HIVE, go to Chat tab
2. Select a cloud provider (e.g., OpenAI) and model (e.g., gpt-4o-mini)
3. Send: "What is 2+2? Answer in one word."
4. Verify: streaming response appears, answer is correct

**Expected log entries:**
```
PROVIDER | chat | provider=openai model=gpt-4o-mini
```

**Pass criteria:**
- [ ] Response streams in (not all at once)
- [ ] No error in chat or logs
- [ ] Response is coherent

---

## 2. Local Model Launch + Chat

**Steps:**
1. Go to Models tab, ensure a GGUF model is downloaded
2. Click Start on the model
3. Wait for server health check to pass (green indicator)
4. Switch to Chat tab, send: "Hello, who are you?"
5. Verify: streaming response from local model

**Expected log entries:**
```
SERVER | start | backend=native port=8080 model=<filename>
```

**Pass criteria:**
- [ ] Server starts without visible CMD window
- [ ] Health check passes (green status)
- [ ] Chat response streams correctly
- [ ] Thinking tokens separated if model supports them (collapsible block)

---

## 3. Tool Execution

**Steps:**
1. In chat with any provider, send: "Read the file C:/Users/lc77/HiveMind/README.md"
2. Verify: tool approval dialog appears for `read_file`
3. Approve the tool call
4. Verify: model receives file content and summarizes it

**Expected log entries:**
```
TOOL_CHAIN | tool=read_file status=approved
```

**Pass criteria:**
- [ ] Tool approval dialog shows correct tool name and arguments
- [ ] Tool result appears in collapsible block in chat
- [ ] Model processes the result and responds intelligently
- [ ] Tool result prefixed with `TOOL_OK` in model context

---

## 4. Memory Round-Trip (Save, Search, Recall)

**Steps:**
1. Send: "Remember that my favorite programming language is Rust"
2. Verify: model calls `memory_save`
3. Start a new conversation (click New Chat)
4. Send: "What is my favorite programming language?"
5. Verify: memory recall injects the saved fact, model answers "Rust"

**Expected log entries:**
```
MEMORY | save | tags=... | chars=...
MEMORY | recall | query=... | results=...
```

**Pass criteria:**
- [ ] `memory_save` tool called and approved
- [ ] Memory persists across conversations
- [ ] Auto-recall injects relevant memory as system message
- [ ] Model correctly retrieves the saved information

---

## 5. Specialist Routing (Cloud Specialist with Tools)

**Steps:**
1. Configure a specialist slot (Settings > Slots): assign a cloud model to Coder slot
2. In Consciousness chat, send: "Route this to the coder: write a Python hello world"
3. Verify: routing indicator appears, model calls `route_to_specialist`
4. Verify: specialist receives task and responds

**Expected log entries:**
```
SLOTS | route | target=coder task=...
SLOTS | wake_context_built | specialist=coder chars=...
```

**Pass criteria:**
- [ ] Routing indicator badge appears during delegation
- [ ] Specialist receives MAGMA wake briefing
- [ ] Specialist responds with code
- [ ] Response attributed to specialist in UI

**Known gap to verify:** Cloud specialists currently call `chatWithProvider` (no tools). Check if specialist can use tools — if not, this is the Phase 2D bug to fix.

---

## 6. Worker Spawning

**Steps:**
1. Send: "Spawn a worker to search the web for 'latest Rust release' and save the result to a scratchpad"
2. Verify: model calls `worker_spawn` with appropriate parameters
3. Check WorkerPanel (slide-out from chat status bar) for worker status
4. Wait for worker to complete
5. Send: "Read the scratchpad results"

**Expected log entries:**
```
WORKER_SPAWN | provider=... model=... task=...
WORKER_COMPLETE | worker_id=... turns=... elapsed=...
```

**Pass criteria:**
- [ ] Worker spawns and appears in WorkerPanel
- [ ] Worker progress updates visible
- [ ] Worker completes (not stuck in loop)
- [ ] Scratchpad contains worker results
- [ ] Worker reports back to parent via `report_to_parent`

---

## 7. Telegram Remote Channel

**Prerequisites:** Telegram bot token configured, your chat_id in host_ids

**Steps:**
1. Enable Telegram daemon in Settings
2. Send a message to your bot from Telegram: "What time is it?"
3. Verify: message appears in HIVE logs
4. Verify: HIVE responds via the bot

**Expected log entries:**
```
TELEGRAM | daemon_started
TELEGRAM | incoming | chat_id=... sender=... role=host
```

**Pass criteria:**
- [ ] Daemon starts without errors
- [ ] Incoming messages logged with correct sender role
- [ ] HIVE processes message through agentic loop
- [ ] Response sent back to Telegram
- [ ] Desktop-only tools (run_command, write_file) blocked for remote

---

## 8. Discord Remote Channel

**Prerequisites:** Discord bot token configured, channel selected, your user_id in host_ids

**Steps:**
1. Enable Discord daemon in Settings
2. Send a message in the monitored channel: "Hello HIVE"
3. Verify: message appears in HIVE logs
4. Verify: HIVE responds in the channel

**Expected log entries:**
```
DISCORD | daemon_started
DISCORD | incoming | channel=... sender=... role=host
```

**Pass criteria:**
- [ ] Daemon starts and auto-discovers channels
- [ ] Incoming messages logged
- [ ] Response sent to correct channel
- [ ] Tool restrictions enforced (no run_command from remote)

---

## 9. MCP Server Mode

**Steps:**
1. Run: `hive-desktop --mcp` (or test via Claude Code with HIVE configured as MCP server)
2. Verify: MCP handshake completes
3. Call a tool: `memory_search` with query "test"
4. Verify: tool executes and returns result

**Pass criteria:**
- [ ] MCP server starts on stdio
- [ ] `list_tools` returns all registered HiveTools
- [ ] Tool execution works through MCP protocol

---

## 10. Context Pressure & Memory Flush

**Steps:**
1. Start a long conversation (20+ exchanges) with any model
2. Watch for context truncation (harness volatile context shows message count)
3. Verify: messages are extracted to memory before truncation
4. Start new conversation, verify extracted facts are recallable

**Expected log entries:**
```
FE | context_pressure | messages_truncated=...
MEMORY | save | source=conversation_flush
```

**Pass criteria:**
- [ ] Context pressure detected and logged
- [ ] Oldest messages truncated (not system prompt)
- [ ] Facts extracted to memory before truncation
- [ ] Extracted facts available in subsequent conversations

---

## Quick Smoke Test (5 minutes)

For fast verification after minor changes:

1. [ ] `cargo test` — all pass, count >= 176
2. [ ] `npx tsc --noEmit` — 0 errors
3. [ ] `npx vitest run` — all pass, count >= 52
4. [ ] Start HIVE, send one message to cloud provider — response streams
5. [ ] Send "read file README.md" — tool approval works, result displays

---

*Last updated: March 9, 2026*

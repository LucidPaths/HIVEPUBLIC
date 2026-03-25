# Implementation Plan: Three "Do Now" Features

## Feature 1: Worker Thinking Depth Inheritance

### Problem
Workers pass `None` for `thinking_depth` on every `chat_with_tools()` call (line 553 of `worker_tools.rs`):
```rust
None, // thinking_depth: workers use default (no thinking budget override)
```

When a user has configured thinking depth (e.g., `anthropic: "high"`) in Settings → Thinking Depth, their workers silently ignore it. Workers are cognitive sub-agents — if the user wants deep reasoning from Anthropic, they want it from workers too. This violates P2 (Provider Agnosticism: the setting should apply uniformly) and P8 (High Ceiling: power users configuring thinking depth expect it everywhere).

### Approach
**Extend `SessionModelContext` to include thinking depth.** The session context already flows provider + model_id from frontend → Rust global. Adding `thinking_depth` to this struct is the minimal-surface change — workers already read `SessionModelContext`, so they inherit it automatically.

### Changes

**File 1: `providers.rs` — Extend `SessionModelContext`**
- Add `thinking_depth: Option<String>` field to the `SessionModelContext` struct (line 271)
- Update `set_session_model_context` Tauri command (line 286) to accept `thinking_depth: Option<String>` parameter
- Update the write to include the new field

**File 2: `api.ts` — Pass thinking depth when setting session context**
- Update `setSessionModelContext()` (line 1123) to accept and forward `thinkingDepth` parameter
- Wire it through to the invoke call

**File 3: `useChat.ts` — Pass thinking depth at chat start**
- Where `setSessionModelContext(provider, modelId)` is called, also pass the current `appSettings.thinkingDepth[provider]` value

**File 4: `worker_tools.rs` — Workers inherit thinking depth from session context**
- In `WorkerSpawnTool::execute()`, read `thinking_depth` from the session context
- Pass it through to `run_worker_loop()` as a new parameter
- In `run_worker_loop()`, use the inherited thinking depth in the `chat_with_tools()` call (line 553) instead of `None`

### Verification
- Worker log output should show inherited thinking depth
- No changes to worker_spawn tool schema (workers don't need to specify it — framework provides it, P7)

---

## Feature 2: Tunnel Security Warning (P6)

### Problem
The Cloudflare Tunnel feature in `McpTab.tsx` exposes a local port to the **public internet** via a `trycloudflare.com` URL with zero authentication. There is no security warning — the user can click "Start Tunnel" and expose their inference server or MCP endpoint to anyone who discovers the URL. This violates P6 (Secrets Stay Secret): "Nothing left open to exploitation."

The tunnel backend (`tunnel.rs`) has no auth layer — it's a raw TCP forward. The URL is random but public and discoverable.

### Approach
Add a **visible security warning** in the McpTab tunnel UI. Two parts:
1. A persistent amber/yellow warning banner before the "Start Tunnel" button explaining the risk
2. A confirmation step — when user clicks "Start Tunnel", show a confirmation dialog (or inline confirmation) that makes them acknowledge the risk

### Changes

**File 1: `McpTab.tsx` — Add security warning to tunnel section**
- Before the port input / "Start Tunnel" button (around line 177), add an amber warning banner:
  - Text: "This exposes your local port to the public internet with no authentication. Anyone with the URL can access it. Only use on trusted networks or behind additional authentication."
- Add a confirmation state: user must check a checkbox ("I understand the security implications") before the Start button becomes clickable
- When tunnel IS active, show a persistent red/amber indicator reminding them their port is publicly exposed

**No backend changes needed.** The warning is purely a UI safety gate — the tunnel functionality itself is correct. P6 is about awareness, not blocking legitimate use.

### Verification
- Start Tunnel button is disabled until checkbox is checked
- Warning banner is visible in both inactive and active states
- No functional change to tunnel start/stop

---

## Feature 3: Updater Honesty

### Problem
The auto-updater is scaffolded in config but non-functional:
- `tauri.conf.json` has `plugins.updater` with an empty `pubkey` and an endpoint URL that doesn't exist
- The Rust plugin is registered (`tauri_plugin_updater`)
- The frontend has **zero** UI for checking/installing updates
- The GitHub Actions workflow doesn't generate `latest.json`
- No signing keys are configured

This is dead scaffolding — it creates a false sense of capability. P4 (Errors Are Answers) says the system should be honest about its state.

### Approach
**Remove the non-functional updater config** rather than building a full updater (that's a "Do Later" task). Dead config that does nothing violates P3 (Simplicity) and P7 (Framework Survives — dead code is maintenance debt). Add a clear "Updates" status indicator in Settings that tells the user the current version and that auto-updates aren't configured yet.

### Changes

**File 1: `tauri.conf.json` — Remove dead updater config**
- Remove the `"updater"` block from `plugins` (lines 62-67). An empty pubkey means the updater would accept unsigned updates anyway (P6 violation)
- Keep the plugin registration in `main.rs` and `Cargo.toml` — it's harmless and will be needed when the updater is properly implemented

**File 2: `SettingsTab.tsx` — Add honest version display**
- Add a small "About" section at the bottom of Settings showing:
  - Current version (read from Tauri's `getVersion()` API)
  - Status text: "Auto-updates not yet configured. Check GitHub releases for updates."
  - Link to the GitHub releases page
- This is honest, actionable (P4), and low-floor (P8) — users know where to get updates

**File 3: `api.ts` — Add `getAppVersion()` helper**
- Add a simple wrapper around Tauri's `getVersion()` from `@tauri-apps/api/app`

### Verification
- `tauri.conf.json` no longer has empty pubkey security hole
- Settings shows version and honest update status
- No false promise of auto-update capability

---

## Do Later (Todo List Items — Not Implemented Now)

1. **Full Auto-Updater**: Generate signing keys, add `latest.json` to GitHub Actions, implement frontend check/install UI, configure pubkey
2. **Memory Lifecycle**: Working → short-term → long-term promotion with reinforcement thresholds
3. **Token-Aware Summarization**: Replace context truncation with model-driven summarization at pressure points
4. **Semantic Topic Categorization**: Replace crude pattern-matched tags with embedding-based topic clustering
5. **Tunnel Authentication**: Add optional auth token or IP allowlist to tunnel access (beyond just the warning)

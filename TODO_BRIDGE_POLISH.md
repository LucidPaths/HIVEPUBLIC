# Bridge Polish — TODO List
Created: 2026-03-12 | Branch: fix/audit-findings

## P0: Fix "Thinking" Spam (Code Bug) — DONE

**File:** `HIVE/desktop/src-tauri/src/pty_manager.rs` line 711
**Fix:** Rewrote `strip_ansi_escapes()` to simulate terminal `\r` overwrite behavior:
- Bare `\r` → clear current line buffer (simulates cursor-to-column-0 overwrite)
- `\r\n` → normal newline, `\n` → normal newline
- Added OSC sequence handling (`\x1b]...\x07` and `\x1b]...\x1b\\`)
- 13 tests pass (9 updated + 4 new: spinner overwrite, OSC BEL, OSC ST, carriage return semantics)

---

## P1: Chat Notification for Remote Messages — DONE

**Files:** `useRemoteChannels.ts`, `App.tsx`
**Fix:** Added `onChatInjection` callback to `useRemoteChannels` hook:
- Fires on ALL 6 injection points (Telegram, Discord, Worker completion, Worker message, Routine, Agent bridge) — P5: same pattern fixed everywhere
- `App.tsx` tracks `chatHasUnread` state with `tabRef` to avoid stale closures
- Amber pulsing dot appears on Chat tab when messages arrive while user is on another tab
- Clears on tab switch (both direct click and `onSetTab` from child components via useEffect)
- Consistent with existing HIVE design language (matches green server-running dot in header)

---

## P2: read_agent_output Tool Polish — DONE (auto-fixed by P0)

The circular output buffer stores ANSI-stripped text via `strip_ansi_escapes()`. P0's fix means thinking spinner text is now correctly collapsed to final output. No additional changes needed.

---

## P3: Bridge Filter Enhancement (Optional)

**Current gates:** silence(12s) → rate(8s) → min-chars(30) → dedup(0.70 Jaccard)
**Missing gate:** Content quality — even after fixing `\r`, some terminal noise may get through:
- Progress bars (`[####    ] 45%`)
- Repeated status lines
- Lines that are ONLY spinner chars (⠋⠙⠹⠸ etc.)

**Priority:** Low — P0 fix eliminates the main problem. Only implement if noise persists during real use.

---

## Summary
- P0: DONE (Rust code fix, highest impact)
- P1: DONE (UX notification, user-facing)
- P2: DONE (auto-fixed by P0)
- P3: Deferred (defense-in-depth, only if noise persists)

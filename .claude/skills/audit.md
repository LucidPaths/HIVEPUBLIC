# /audit — Full Codebase Audit Command

You are performing a full adversarial audit of this codebase using parallel domain-specific agents.

## Step 1 — Spawn Parallel Audit Agents

Launch multiple subagents simultaneously, each owning a domain. Every agent reads every file in its domain. Domains:

1. **Security agent** — injection vectors, access control gaps, secret handling, SSRF, path traversal, input validation
2. **Rust core agent** — logic errors, let _ = on critical paths, integer overflow, unsafe blocks, panic paths, mutex ordering
3. **Rust services agent** — daemon lifecycle, process spawning, shell command construction, network code, async correctness
4. **Rust tools agent** — all tool files, risk level consistency, DANGEROUS_TOOLS/WORKER_BLOCKED_TOOLS completeness, cross-file contracts
5. **TypeScript/React agent** — stale closures, missing useEffect deps, unmounted state updates, index keys, optimistic update drift
6. **Architecture agent** — dead code, dead Tauri commands, unused state, P8 violations (features with no frontend wiring), dependency misplacement
7. **Test coverage agent** — security-critical paths with no tests, assertion staleness, magic numbers, theater testing patterns

Each agent produces findings in this format:
```
ID: [DOMAIN][N] e.g. S3, B7, R2
File: path:line
Impact: what breaks or what attack this enables
Principle: which lattice principle is violated (P1-P8)
```

## Step 2 — Deduplicate and Apply Design Review

After all agents complete:
- Deduplicate findings that describe the same root cause
- For each finding, check against HIVE threat model — mark as "Accepted By Design" if it is a deliberate architectural trade-off (document the reasoning)
- Verify all cross-file contracts are in sync: DANGEROUS_TOOLS, DESKTOP_ONLY_TOOLS, SPECIALIST_PORTS, SenderRole, Tauri command registrations, TypeScript↔Rust type contracts

## Step 3 — Categorize by Severity

| Severity | Criteria |
|----------|----------|
| Critical | Active exploit path, data loss, dead integration |
| High | Security bypass, crash, broken feature |
| Medium | Silent failure, logic error, P4 violation |
| Low | Code quality, dead code, minor inconsistency |
| Info | Documentation drift, good patterns worth noting |

## Step 4 — Write the Audit File

Save to `audits/AUDIT_YYYY-MM-DD.md` with this structure:

```markdown
# HIVE Full Codebase Audit — YYYY-MM-DD
**Scope:** [file count, line count breakdown]
**Method:** 7 parallel domain agents
**Branch:** [current branch]

## Executive Summary
[severity table with raw / deduped / after design review counts]

## Accepted By Design
[table: finding | reasoning | principle]

## PRIORITY 1: Security (fix now)
### S1. [title]
File: path:line
Impact: ...
Principle: ...

## PRIORITY 2: Crashes / Data Corruption
### B1. ...

## PRIORITY 3: Silent Failures
### ...

## PRIORITY 4: UX / Logic Bugs
### ...

## PRIORITY 5: Dead Code
### ...

## PRIORITY 6: React Patterns
### ...

## PRIORITY 7: Code Quality
### ...

## PRIORITY 8: Test Coverage Gaps
### ...

## PRIORITY 9: Documentation Drift
### ...

## Verified Positive Findings
[what is working correctly — do not skip this section]

## Fix Execution Order
Each item is fixed one-by-one. Delete the line upon completion.
Priority logic: Security → Crashes → Silent failures → UX → Dead code → React → Quality → Tests → Docs

- [ ] S1 — [title]
- [ ] S2 — [title]
[... all findings in priority order ...]
```

## Step 5 — Report to User

After the file is saved, print:
- Total finding count (raw → deduped → after design review)
- Top 3 most critical findings with one-line summaries
- Path to the audit file
- A single suggested first command to start fixing: `ultrathink [ID] → fix [title], then delete that line from audits/AUDIT_YYYY-MM-DD.md`

---

## Audit Checklist (run every time, no exceptions)

- [ ] Every `let _ =` on a critical path flagged (P4)
- [ ] Every `format!()` into a shell command checked for injection (P6)
- [ ] Every new tool present in DANGEROUS_TOOLS and WORKER_BLOCKED_TOOLS if applicable (P6)
- [ ] Every Tauri command has a TypeScript caller or is marked dead (P8)
- [ ] Every cross-file contract verified in sync
- [ ] All string slicing uses `.chars()` not byte indexing
- [ ] All async code free of blocking calls (`std::thread::sleep` in async, etc.)
- [ ] All React useEffect dependency arrays complete
- [ ] All dynamic list renders use stable keys
- [ ] Test suite count has not decreased since last audit

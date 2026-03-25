# Principle Lattice — Quick Decision Reference

Five universal principles that apply to every project. When stuck between approaches, score against these. If a choice violates one, reconsider.

---

## 1. Modularity

> *Lego blocks, not monoliths.*

Every component fails independently. Pull one out — that specific thing stops. The rest stands. When two systems need to talk, build a bridge — don't duplicate.

**Demands:**
- No hidden coupling between unrelated modules
- Shared types and interfaces live in a single location
- Modules own their own state (call APIs directly, don't thread through orchestrator)

<!-- [ADAPT] Add concrete instantiations from your project:
     - "Components use props, not global state — any component is replaceable"
     - "API routes are independent — auth failing doesn't break health checks"
-->

---

## 2. Simplicity Wins

> *Don't reinvent the wheel. Code exists to be used.*

Three clear lines beat one clever abstraction. A working simple solution beats an elegant broken one. Before writing new code, check git history and existing libraries.

**Demands:**
- Before rewriting, check git history — maybe the old version worked
- If a dependency does 80% of the job, use it
- Don't create abstractions for one-time operations

<!-- [ADAPT] Add concrete instantiations from your project:
     - "Using Zod for validation instead of hand-rolled checks"
     - "localStorage for settings (not a custom database)"
-->

---

## 3. Errors Are Answers

> *Every failure teaches. Errors must be actionable.*

An error that says "something went wrong" is itself a bug. Every error says what happened, why, and what to do about it. Logs are the program's memory of its own behavior.

**Demands:**
- Every error message is actionable (says what to do, not just what happened)
- Logs at key lifecycle events (startup, shutdown, errors, state changes)
- No silent failures — if something goes wrong, someone knows
- Honest status tables (Working / PARTIAL / MISSING / BROKEN)

<!-- [ADAPT] Add concrete instantiations from your project:
     - "API errors include HTTP status + response body + suggested fix"
     - "Startup checks verify all required env vars before proceeding"
-->

---

## 4. Fix The Pattern, Not The Instance

> *Cure the root cause. Don't treat symptoms.*

One bug = the same mistake in 3-5 other places. Search for the pattern. Fix every instance. If you only fix the one you found, you're treating symptoms while the disease spreads.

**Demands:**
- Every bug fix includes grep for the same pattern across the codebase
- If a pattern produces bugs twice, add it to CLAUDE.md as a Trap
- Root cause analysis before fix — the error might be downstream
- Cross-file contracts go in the contracts table

<!-- [ADAPT] Add concrete instantiations from your project:
     - "Found missing null check — grepped for all .property accesses, fixed 4 more"
     - "Same validation bug in 3 endpoints — extracted shared validator"
-->

---

## 5. Secrets Stay Secret

> *Nothing left open to exploitation.*

Security is not a feature you add later. It's a property of every line. Closed by default — empty allowlists mean "deny all", never "allow all."

**Demands:**
- Closed by default — empty lists = deny all
- Never log API keys, tokens, or credentials
- When security logic exists in two layers, both MUST be updated together
- Audit any new storage mechanism for secret leakage

<!-- [ADAPT] Add concrete instantiations from your project:
     - "API keys in .env, never committed (in .gitignore)"
     - "HTTPS enforced for all external API calls"
-->

---

## Using The Lattice

**Quick score:** When choosing between approaches, which honors more principles without violating any? That one wins. If both violate something, find a third approach.

**For code review:** Does this change violate any principle? Not "is this clean" (subjective) — "does this violate the lattice" (answerable).

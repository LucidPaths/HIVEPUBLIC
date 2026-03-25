# Claude Code Tools & Optimizations

This folder contains tools and configurations to optimize Claude Code (CLI/Web) workflows for this repository.

## Available Tools

### 1. mgrep - Semantic Code Search

**What it does:** Replaces traditional grep with AI-powered semantic search. Ask questions in natural language instead of guessing exact patterns.

**Benefits:**
- ~50% reduction in token usage (Claude spends less time scanning irrelevant results)
- Natural language queries: `mgrep "where is authentication handled?"` instead of `grep -r "auth"`
- Multimodal: searches code, PDFs, images
- Background indexing keeps search index up-to-date

**Installation:**
```bash
npm install -g @mixedbread/mgrep
mgrep login
mgrep install-claude-code
```

**Usage:**
```bash
# Start background indexing for this repo
mgrep watch /path/to/HiveMind

# Search semantically
mgrep "how does the backend connect to WSL?"
mgrep "where are API routes defined?"
mgrep "error handling patterns" -m 10

# Include web results
mgrep "Tauri v2 window configuration" --web
```

**Configuration:** See `.mgreprc.yaml` in this folder for repo-specific settings.

## Setup Script

Run the setup script to install all Claude Code optimizations:

```bash
# Windows
./claude-tools/setup.bat

# Linux/Mac
./claude-tools/setup.sh
```

## Configuration Files

| File | Purpose |
|------|---------|
| `.mgreprc.yaml` | mgrep configuration for this repo |
| `CLAUDE.md` | Instructions Claude Code reads automatically |

## Best Practices for Claude Code

1. **Use mgrep for exploration** - When asking Claude to find something, mgrep's semantic search is faster and uses fewer tokens

2. **Keep indexes updated** - Run `mgrep watch` in background during development sessions

3. **Leverage CLAUDE.md** - Add project-specific instructions that Claude reads automatically

4. **Use .claudeignore** - Exclude large generated files, node_modules, etc. from Claude's context

#!/usr/bin/env python3
"""
HIVE Session Start Hook — Auto-orientation for new sessions

Adapted from https://github.com/vincitamore/claude-org-template
Original author: vincitamore (MIT License)

Provides Claude with current project state at session start:
- Time since last session (helps calibrate how much context to rebuild)
- Current git branch and recent commits
- Uncommitted changes
- Next steps from ROADMAP.md
- Post-compaction context restore (reads snapshot saved by pre-compact.py)
"""

import json
import sys
import os
import re
import subprocess
from datetime import datetime


SNAPSHOT_PATH = os.path.expanduser("~/.hive/harness/pre-compact-snapshot.md")
STATE_PATH = os.path.expanduser("~/.hive/harness/CLAUDE_STATE.md")


def get_project_root():
    """Get project root from environment or by walking up from this script."""
    env_root = os.environ.get('CLAUDE_PROJECT_DIR')
    if env_root and os.path.isdir(env_root):
        return env_root
    return os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def run_git(args, project_root):
    """Run a git command and return stdout, or None on failure."""
    try:
        result = subprocess.run(
            ['git'] + args,
            capture_output=True, text=True, cwd=project_root, timeout=5
        )
        return result.stdout.strip() if result.returncode == 0 else None
    except Exception:
        return None


def get_next_steps(project_root):
    """Extract next steps from ROADMAP.md, TODO.md, or SESSION_NOTES.md."""
    for filename in ['ROADMAP.md', 'TODO.md', 'SESSION_NOTES.md']:
        filepath = os.path.join(project_root, filename)
        if not os.path.exists(filepath):
            continue
        try:
            with open(filepath, 'r', encoding='utf-8') as f:
                content = f.read()
            for header in ['## Immediate Next Steps', '## Next Steps', '## TODO', '## Tasks', '## Next steps']:
                match = re.search(
                    rf'{re.escape(header)}\n(.*?)(?=\n---|\n## |\Z)',
                    content, re.DOTALL
                )
                if match:
                    return match.group(1).strip()[:600]
        except Exception:
            pass
    return ""


def get_time_since_last_session():
    """Check when CLAUDE_STATE.md was last modified to estimate time gap."""
    if not os.path.exists(STATE_PATH):
        return None
    try:
        mtime = os.path.getmtime(STATE_PATH)
        last = datetime.fromtimestamp(mtime, tz=None)  # local timezone
        now = datetime.now()  # local timezone — both naive, delta is correct
        delta = now - last

        if delta.total_seconds() < 300:  # <5 min
            return "just now (< 5 min ago)"
        elif delta.total_seconds() < 3600:  # <1 hr
            mins = int(delta.total_seconds() / 60)
            return f"{mins} minutes ago"
        elif delta.total_seconds() < 86400:  # <1 day
            hours = int(delta.total_seconds() / 3600)
            return f"{hours} hour{'s' if hours != 1 else ''} ago"
        else:
            days = int(delta.total_seconds() / 86400)
            return f"{days} day{'s' if days != 1 else ''} ago"
    except Exception:
        return None


def read_and_consume_snapshot():
    """Read pre-compaction snapshot if it exists, then delete it."""
    if not os.path.exists(SNAPSHOT_PATH):
        return None
    try:
        with open(SNAPSHOT_PATH, 'r', encoding='utf-8') as f:
            content = f.read()
        os.remove(SNAPSHOT_PATH)
        return content if content.strip() else None
    except Exception:
        return None


def main():
    try:
        data = json.load(sys.stdin)
    except Exception:
        data = {}

    project_root = get_project_root()

    # Gather git state
    branch = run_git(['branch', '--show-current'], project_root)
    log = run_git(['log', '--oneline', '-5'], project_root)
    status = run_git(['status', '--short'], project_root)

    # Time context
    time_gap = get_time_since_last_session()

    # Read next steps from roadmap
    next_steps = get_next_steps(project_root)

    # Check for post-compaction snapshot
    snapshot = read_and_consume_snapshot()

    # Build orientation context
    lines = []
    lines.append("## HIVE Project Orientation")
    lines.append("")

    if time_gap:
        lines.append(f"**Last session:** {time_gap}")

    if branch:
        lines.append(f"**Branch:** `{branch}`")

    if log:
        lines.append("")
        lines.append("**Recent commits:**")
        lines.append("```")
        lines.append(log)
        lines.append("```")

    if status:
        lines.append("")
        lines.append("**Uncommitted changes:**")
        lines.append("```")
        lines.append(status)
        lines.append("```")

    if next_steps:
        lines.append("")
        lines.append("**Next steps (from ROADMAP.md):**")
        lines.append(next_steps)

    # Post-compaction restore
    if snapshot:
        lines.append("")
        lines.append("---")
        lines.append("")
        lines.append("## CONTEXT RESTORED FROM PRE-COMPACTION SNAPSHOT")
        lines.append("")
        lines.append("The following was saved by the PreCompact hook right before")
        lines.append("context compression. Use it to resume where you left off.")
        lines.append("")
        lines.append(snapshot)

    lines.append("")
    lines.append("*MEMORY.md auto-loaded separately. See CLAUDE.md for coding standards.*")

    output = {"additionalContext": "\n".join(lines)}
    print(json.dumps(output))


if __name__ == "__main__":
    main()

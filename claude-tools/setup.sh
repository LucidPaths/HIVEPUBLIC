#!/bin/bash

echo ""
echo "================================================================"
echo "  Claude Code Tools Setup"
echo "================================================================"
echo ""

# Check for Node.js
if ! command -v node &> /dev/null; then
    echo "[ERROR] Node.js is required. Install from https://nodejs.org/"
    exit 1
fi

echo "[1/4] Installing mgrep..."
npm install -g @mixedbread/mgrep
if [ $? -ne 0 ]; then
    echo "[ERROR] Failed to install mgrep"
    exit 1
fi
echo "[OK] mgrep installed"

echo ""
echo "[2/4] Authenticating mgrep..."
echo "Please complete the login in your browser..."
mgrep login || echo "[WARN] Login skipped or failed - you can run 'mgrep login' later"

echo ""
echo "[3/4] Installing Claude Code integration..."
mgrep install-claude-code || echo "[WARN] Claude Code integration skipped"

echo ""
echo "[4/4] Copying configuration..."
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
cp "$SCRIPT_DIR/.mgreprc.yaml" "$REPO_ROOT/.mgreprc.yaml"
echo "[OK] Configuration copied to repository root"

echo ""
echo "================================================================"
echo "  Setup Complete!"
echo "================================================================"
echo ""
echo "To start indexing this repository, run:"
echo "  mgrep watch $REPO_ROOT"
echo ""
echo "Then you can search with:"
echo "  mgrep \"your natural language query\""
echo ""

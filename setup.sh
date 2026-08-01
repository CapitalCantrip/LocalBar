#!/bin/bash
# LocalBar — one-time project setup
# Run this once to install XcodeGen and generate the Xcode project.
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "→ Checking for XcodeGen..."
if ! command -v xcodegen &>/dev/null; then
    echo "→ Installing XcodeGen via Homebrew..."
    brew install xcodegen
fi

echo "→ Generating LocalBar.xcodeproj from project.yml..."
xcodegen generate

echo ""
echo "✓ Done. Open LocalBar.xcodeproj in Xcode to build."
open LocalBar.xcodeproj

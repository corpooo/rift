#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

if ! command -v xcodegen >/dev/null 2>&1; then
  echo "xcodegen is not installed. Install with: brew install xcodegen" >&2
  exit 1
fi

xcodegen generate

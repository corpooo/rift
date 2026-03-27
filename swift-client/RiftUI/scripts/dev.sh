#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

watch_paths=(
  "App"
  "Modules"
  "Package.swift"
  "project.yml"
)

app_binary=".build/debug/RiftUI"
app_pid=""

log() {
  printf '[%(%H:%M:%S)T] %s\n' -1 "$*"
}

snapshot() {
  find "${watch_paths[@]}" \
    -type f \
    \( -name '*.swift' -o -name '*.plist' -o -name '*.yml' -o -name '*.yaml' -o -name 'Package.swift' \) \
    -exec stat -f '%m %N' {} + 2>/dev/null | sort
}

generate_project_if_possible() {
  if command -v xcodegen >/dev/null 2>&1; then
    xcodegen generate >/dev/null
  fi
}

build() {
  log "Generating Xcode project (if xcodegen is installed)..."
  generate_project_if_possible
  log "Building RiftUI..."
  if xcrun swift build --product RiftUI; then
    log "Build succeeded"
    return 0
  fi

  log "Build failed"
  return 1
}

stop_app() {
  if [[ -n "$app_pid" ]] && kill -0 "$app_pid" 2>/dev/null; then
    log "Stopping RiftUI (pid $app_pid)"
    kill "$app_pid" 2>/dev/null || true
    wait "$app_pid" 2>/dev/null || true
  fi
  app_pid=""
}

start_app() {
  if [[ ! -x "$app_binary" ]]; then
    log "Cannot launch app; expected binary at $app_binary"
    return 1
  fi

  log "Launching RiftUI"
  "$app_binary" &
  app_pid=$!
  log "RiftUI running (pid $app_pid)"
}

restart_app() {
  stop_app
  start_app
}

cleanup() {
  stop_app
}

trap cleanup EXIT INT TERM

if build; then
  start_app || true
else
  log "Initial build failed. Waiting for changes to retry."
fi

log "Watching for file changes..."

if command -v fswatch >/dev/null 2>&1; then
  while fswatch -1 "${watch_paths[@]}" >/dev/null; do
    if build; then
      restart_app || true
    fi
  done
else
  log "fswatch not found; using 1s polling fallback"
  previous_snapshot="$(snapshot)"
  while true; do
    sleep 1
    current_snapshot="$(snapshot)"

    if [[ "$current_snapshot" != "$previous_snapshot" ]]; then
      previous_snapshot="$current_snapshot"
      if build; then
        restart_app || true
      fi
    fi
  done
fi

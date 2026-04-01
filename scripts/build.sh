#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

profile="release"

usage() {
  cat <<'EOF'
Usage: scripts/build.sh [profile] [cargo build args...]
       scripts/build.sh --profile <profile> [cargo build args...]

Build Rift with the selected Cargo profile.

Profiles:
  release       Default. Uses `cargo build --release --bins`
  dev|debug     Uses `cargo build --bins`
  <custom>      Uses `cargo build --profile <custom> --bins`

Examples:
  scripts/build.sh
  scripts/build.sh release-fast
  scripts/build.sh --profile dev --bin rift
  scripts/build.sh release --target aarch64-apple-darwin
EOF
}

pass_through=()

while (($#)); do
  case "$1" in
    -h|--help)
      usage
      exit 0
      ;;
    -p|--profile)
      if (($# < 2)); then
        echo "error: missing value for $1" >&2
        exit 1
      fi
      profile="$2"
      shift 2
      ;;
    --)
      shift
      pass_through+=("$@")
      break
      ;;
    -*)
      pass_through+=("$1")
      shift
      ;;
    *)
      if [[ "$profile" == "release" && ${#pass_through[@]} -eq 0 ]]; then
        profile="$1"
      else
        pass_through+=("$1")
      fi
      shift
      ;;
  esac
done

cargo_args=(build --bins)
case "$profile" in
  release)
    cargo_args+=(--release)
    ;;
  dev|debug)
    ;;
  *)
    cargo_args+=(--profile "$profile")
    ;;
esac

cargo_args+=("${pass_through[@]}")

exec cargo "${cargo_args[@]}"

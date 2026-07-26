#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
VISIBLE=0
RENDER_BACKEND=""
IPC_DIR="${TALOS_IPC_DIR:-}"

usage() {
  cat <<'EOF'
Usage: ./start-simulator.sh [--visible] [--render-backend vulkan] [--ipc-dir PATH]
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --visible) VISIBLE=1; shift ;;
    --render-backend)
      [[ $# -ge 2 ]] || { echo "--render-backend requires a value" >&2; exit 2; }
      RENDER_BACKEND="$2"; shift 2 ;;
    --ipc-dir)
      [[ $# -ge 2 ]] || { echo "--ipc-dir requires a value" >&2; exit 2; }
      IPC_DIR="$2"; shift 2 ;;
    --help|-h) usage; exit 0 ;;
    *) echo "unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
done

if [[ -z "$IPC_DIR" ]]; then
  IPC_DIR="$ROOT/runtime/talos-ipc"
fi

BINARY="$ROOT/bin/daedalus"
[[ -x "$BINARY" ]] || { echo "Simulator executable is missing or not executable: $BINARY" >&2; exit 1; }
mkdir -p "$IPC_DIR"

export TALOS_IPC_DIR="$IPC_DIR"
export WGPU_BACKEND="${RENDER_BACKEND:-vulkan}"
export WGPU_POWER_PREF="high"
export PATH="$ROOT/bin:$PATH"

if [[ "$VISIBLE" == 1 ]]; then MODE=visible; else MODE=performance; fi
export DAEDALUS_RELEASE_MODE="$MODE"

echo "Daedalus launch mode=$MODE backend=$WGPU_BACKEND ipc=$TALOS_IPC_DIR"
exec "$BINARY"

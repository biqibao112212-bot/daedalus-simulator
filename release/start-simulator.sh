#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
VISIBLE=0
RENDER_BACKEND=""
IPC_DIR="${TALOS_IPC_DIR:-}"
CORNER_LABELS_JSONL=""

usage() {
  cat <<'EOF'
Usage: ./start-simulator.sh [--performance|--visible] [--render-backend vulkan] [--ipc-dir PATH] [--corner-labels-jsonl ABSOLUTE_PATH]

Defaults to headless performance mode. Use --visible to open a preview window.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --visible) VISIBLE=1; shift ;;
    --performance) VISIBLE=0; shift ;;
    --render-backend)
      [[ $# -ge 2 ]] || { echo "--render-backend requires a value" >&2; exit 2; }
      RENDER_BACKEND="$2"; shift 2 ;;
    --ipc-dir)
      [[ $# -ge 2 ]] || { echo "--ipc-dir requires a value" >&2; exit 2; }
      IPC_DIR="$2"; shift 2 ;;
    --corner-labels-jsonl)
      [[ $# -ge 2 ]] || { echo "--corner-labels-jsonl requires a value" >&2; exit 2; }
      CORNER_LABELS_JSONL="$2"; shift 2 ;;
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
if [[ -n "$CORNER_LABELS_JSONL" ]]; then
  [[ "$CORNER_LABELS_JSONL" == /* ]] || { echo "corner label output must be absolute" >&2; exit 2; }
  [[ "$CORNER_LABELS_JSONL" == *.jsonl ]] || { echo "corner label output must end in .jsonl" >&2; exit 2; }
  [[ ! -e "$CORNER_LABELS_JSONL" ]] || { echo "corner label output already exists: $CORNER_LABELS_JSONL" >&2; exit 2; }
  [[ -d "$(dirname -- "$CORNER_LABELS_JSONL")" ]] || { echo "corner label parent directory does not exist" >&2; exit 2; }
  case "$CORNER_LABELS_JSONL" in "$ROOT"/*) echo "corner label output must be outside the installed Release tree" >&2; exit 2 ;; esac
  export DAEDALUS_CORNER_LABELS_JSONL="$CORNER_LABELS_JSONL"
else
  unset DAEDALUS_CORNER_LABELS_JSONL || true
fi

echo "Daedalus launch mode=$MODE backend=$WGPU_BACKEND ipc=$TALOS_IPC_DIR"
exec "$BINARY"

#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET="x86_64-unknown-linux-gnu"
MODE="performance"
CONFIG_PATH="$ROOT/config.performance.toml"
OUTPUT_ROOT="$(cd -- "$ROOT/../.." && pwd)/runtime/simulator-performance"
DURATION_SECONDS=20
MINIMUM_MAIN_UPDATE_HZ=100
MINIMUM_CAPTURE_SUBMIT_HZ=100
BUILD=0

die() { echo "measure-performance.sh: $*" >&2; exit 1; }
usage() {
  cat <<'EOF'
Usage: ./scripts/measure-performance.sh [options]

Options:
  --mode performance|visible       Distribution launcher mode (default: performance)
  --duration-seconds N             Measurement duration, 10..600 (default: 20)
  --minimum-main-update-hz N       Required main update throughput (default: 100)
  --minimum-capture-submit-hz N    Required capture submit throughput (default: 100)
  --config PATH                    Performance configuration path
  --output-root PATH               Parent directory for retained raw evidence
  --build                          Build the Linux Release binary first
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --mode) [[ $# -ge 2 ]] || die "--mode requires a value"; MODE="$2"; shift 2 ;;
    --duration-seconds) [[ $# -ge 2 ]] || die "--duration-seconds requires a value"; DURATION_SECONDS="$2"; shift 2 ;;
    --minimum-main-update-hz) [[ $# -ge 2 ]] || die "--minimum-main-update-hz requires a value"; MINIMUM_MAIN_UPDATE_HZ="$2"; shift 2 ;;
    --minimum-capture-submit-hz) [[ $# -ge 2 ]] || die "--minimum-capture-submit-hz requires a value"; MINIMUM_CAPTURE_SUBMIT_HZ="$2"; shift 2 ;;
    --config) [[ $# -ge 2 ]] || die "--config requires a value"; CONFIG_PATH="$2"; shift 2 ;;
    --output-root) [[ $# -ge 2 ]] || die "--output-root requires a value"; OUTPUT_ROOT="$2"; shift 2 ;;
    --build) BUILD=1; shift ;;
    --help|-h) usage; exit 0 ;;
    *) die "unknown argument: $1" ;;
  esac
done

[[ "$(uname -m)" == "x86_64" ]] || die "Linux performance measurements require x86_64; got $(uname -m)"
[[ "$MODE" == "performance" || "$MODE" == "visible" ]] || die "--mode must be performance or visible"
[[ "$DURATION_SECONDS" =~ ^[0-9]+$ && "$DURATION_SECONDS" -ge 10 && "$DURATION_SECONDS" -le 600 ]] || die "--duration-seconds must be an integer in 10..600"
command -v cargo >/dev/null || die "cargo is required"
command -v git >/dev/null || die "git is required"
command -v python3 >/dev/null || die "python3 is required"
[[ -f "$CONFIG_PATH" ]] || die "performance configuration is missing: $CONFIG_PATH"

BINARY="$ROOT/target/$TARGET/release/daedalus"
if [[ "$BUILD" == 1 ]]; then
  cargo build --locked --release --features talos,distribution-release,contest-release --target "$TARGET"
fi
[[ -x "$BINARY" ]] || die "Release simulator binary is missing or not executable: $BINARY"
if pgrep -x daedalus >/dev/null; then
  die "a Daedalus process is already running; stop it before collecting isolated evidence"
fi

STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
RUN_DIRECTORY="$OUTPUT_ROOT/$STAMP-$MODE"
mkdir -p "$RUN_DIRECTORY/talos-ipc"
STATS_PATH="$RUN_DIRECTORY/simulator.stats.json"
STDOUT_PATH="$RUN_DIRECTORY/simulator.stdout.log"
STDERR_PATH="$RUN_DIRECTORY/simulator.stderr.log"
EVIDENCE_PATH="$RUN_DIRECTORY/performance-evidence.json"

cleanup() {
  if [[ -n "${SIMULATOR_PID:-}" ]] && kill -0 "$SIMULATOR_PID" 2>/dev/null; then
    kill "$SIMULATOR_PID" 2>/dev/null || true
    wait "$SIMULATOR_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT INT TERM

env \
  DAEDALUS_CONFIG="$CONFIG_PATH" \
  DAEDALUS_RELEASE_MODE="$MODE" \
  TALOS_PERFORMANCE_EVIDENCE_JSON="$STATS_PATH" \
  TALOS_IPC_DIR="$RUN_DIRECTORY/talos-ipc" \
  WGPU_BACKEND=vulkan \
  WGPU_POWER_PREF=high \
  "$BINARY" >"$STDOUT_PATH" 2>"$STDERR_PATH" &
SIMULATOR_PID=$!
sleep "$DURATION_SECONDS"
if ! kill -0 "$SIMULATOR_PID" 2>/dev/null; then
  wait "$SIMULATOR_PID" || exit_code=$?
  die "simulator exited early with code ${exit_code:-0}; see $STDERR_PATH"
fi
[[ -f "$STATS_PATH" ]] || die "simulator did not write frequency statistics: $STATS_PATH"

python3 - "$ROOT" "$BINARY" "$CONFIG_PATH" "$STATS_PATH" "$EVIDENCE_PATH" "$DURATION_SECONDS" "$TARGET" "$MODE" "$MINIMUM_MAIN_UPDATE_HZ" "$MINIMUM_CAPTURE_SUBMIT_HZ" <<'PY'
import hashlib
import json
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

root, binary, config, stats_path, evidence_path = map(Path, sys.argv[1:6])
duration, target, mode, minimum_main, minimum_capture = sys.argv[6:]
metrics = json.loads(stats_path.read_text(encoding="utf-8"))
for key, minimum in (("main_update_hz", float(minimum_main)), ("capture_copy_submit_hz", float(minimum_capture))):
    try:
        value = float(metrics[key])
    except (KeyError, TypeError, ValueError) as exc:
        raise SystemExit(f"measure-performance.sh: missing numeric {key}: {exc}")
    if value < minimum:
        raise SystemExit(f"measure-performance.sh: {key} {value:.3f} Hz is below required {minimum:.3f} Hz")
if metrics.get("talos_image_transport") != "tcp":
    raise SystemExit(f"measure-performance.sh: expected TCP image transport, got {metrics.get('talos_image_transport')!r}")
source_dirty = bool(subprocess.run(["git", "-C", str(root), "status", "--porcelain", "--", "."], check=True, capture_output=True, text=True).stdout.strip())
evidence = {
    "schema": "daedalus-performance-v1",
    "version": (root / "VERSION").read_text(encoding="utf-8").strip(),
    "profile": "release",
    "rust_target": target,
    "features": ["talos", "distribution-release", "contest-release"],
    "corner_labels_enabled": False,
    "measurement_mode": mode,
    "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
    "started_utc": datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ"),
    "duration_seconds": int(duration),
    "source_commit": subprocess.run(["git", "-C", str(root), "rev-parse", "HEAD"], check=True, capture_output=True, text=True).stdout.strip(),
    "source_dirty": source_dirty,
    "binary_path": str(binary),
    "config_path": str(config),
    "metrics": metrics,
}
evidence_path.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
print(f"performance_evidence={evidence_path}")
print(f"main_update_hz={metrics['main_update_hz']}")
print(f"capture_copy_submit_hz={metrics['capture_copy_submit_hz']}")
print(f"preview_present_hz={metrics.get('preview_present_hz', 0.0)}")
PY

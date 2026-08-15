#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
RUNTIME_DIR="${XDG_RUNTIME_DIR:-/tmp}/daedalus-contest-${UID}"
PID_FILE=""
LOG_FILE=""

usage() {
  cat <<'EOF'
Usage: daedalus-contest [--runtime-dir PATH] <command> [options]

Commands:
  start [--performance] [--scene shooting-range|large-energy]
  stop | status | doctor
  scene shooting-range|large-energy
  frame
  aim YAW_DEG PITCH_DEG [--fire]

`start` launches a visible local contest simulator by default. Use
`--performance` only for the headless high-performance mode. All other
commands talk to the same instance through its runtime directory. The only
selectable maps are Shooting Range and the large Energy Mechanism.
EOF
}

die() { echo "daedalus-contest: $*" >&2; exit 1; }

while [[ $# -gt 0 && "$1" == "--runtime-dir" ]]; do
  [[ $# -ge 2 ]] || die "--runtime-dir requires a path"
  RUNTIME_DIR="$2"
  shift 2
done
[[ $# -ge 1 ]] || { usage >&2; exit 2; }
COMMAND="$1"
shift
PID_FILE="$RUNTIME_DIR/daedalus-contest.pid"
LOG_FILE="$RUNTIME_DIR/daedalus-contest.log"
CLIENT="$ROOT/sdk/bin/daedalus_contest_client"

[[ -x "$CLIENT" ]] || die "contest C++ client is missing: $CLIENT"

owned_process_running() {
  [[ -f "$PID_FILE" ]] || return 1
  local pid args
  pid="$(<"$PID_FILE")"
  [[ "$pid" =~ ^[0-9]+$ ]] || return 1
  kill -0 "$pid" 2>/dev/null || return 1
  args="$(ps -p "$pid" -o args= 2>/dev/null || true)"
  [[ "$args" == *"$ROOT/bin/daedalus"* ]]
}

client() { "$CLIENT" --ipc-dir "$RUNTIME_DIR" "$@"; }

case "$COMMAND" in
  start)
    VISIBLE=1
    SCENE="shooting-range"
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --visible) VISIBLE=1; shift ;;
        --performance) VISIBLE=0; shift ;;
        --scene)
          [[ $# -ge 2 ]] || die "--scene requires a value"
          SCENE="$2"; shift 2 ;;
        *) die "unknown start option: $1" ;;
      esac
    done
    [[ "$SCENE" == "shooting-range" || "$SCENE" == "large-energy" ]] || \
      die "--scene must be shooting-range or large-energy"
    mkdir -p -- "$RUNTIME_DIR"
    if owned_process_running; then
      die "contest simulator is already running (pid $(<"$PID_FILE")); use status or stop"
    fi
    rm -f -- "$PID_FILE"
    START_ARGS=(--ipc-dir "$RUNTIME_DIR")
    [[ "$VISIBLE" == 1 ]] && START_ARGS=(--visible "${START_ARGS[@]}")
    "$ROOT/start-simulator.sh" "${START_ARGS[@]}" >"$LOG_FILE" 2>&1 &
    printf '%s\n' "$!" >"$PID_FILE"
    for _ in $(seq 1 100); do
      if client health >/dev/null 2>&1; then
        client scene "$SCENE"
        echo "started pid=$(<"$PID_FILE") runtime_dir=$RUNTIME_DIR visible=$VISIBLE"
        exit 0
      fi
      if ! owned_process_running; then
        cat "$LOG_FILE" >&2 || true
        rm -f -- "$PID_FILE"
        die "simulator exited before it became ready"
      fi
      sleep 0.1
    done
    kill "$(<"$PID_FILE")" 2>/dev/null || true
    rm -f -- "$PID_FILE"
    die "simulator did not become ready within 10 seconds; see $LOG_FILE"
    ;;
  stop)
    [[ $# -eq 0 ]] || die "stop takes no arguments"
    if owned_process_running; then
      kill "$(<"$PID_FILE")"
      for _ in $(seq 1 50); do
        kill -0 "$(<"$PID_FILE")" 2>/dev/null || break
        sleep 0.1
      done
      echo "stopped pid=$(<"$PID_FILE")"
    else
      echo "not running"
    fi
    rm -f -- "$PID_FILE"
    ;;
  status)
    [[ $# -eq 0 ]] || die "status takes no arguments"
    if owned_process_running; then
      echo "running pid=$(<"$PID_FILE") runtime_dir=$RUNTIME_DIR log=$LOG_FILE"
      client health
    else
      echo "not running runtime_dir=$RUNTIME_DIR"
      exit 1
    fi
    ;;
  doctor)
    [[ $# -eq 0 ]] || die "doctor takes no arguments"
    echo "runtime_dir=$RUNTIME_DIR"
    missing="$(ldd "$ROOT/bin/daedalus" 2>/dev/null | awk '/not found/{print $1}' | paste -sd, -)"
    [[ -z "$missing" ]] || die "missing runtime libraries: $missing"
    ldd "$ROOT/bin/daedalus" >/dev/null
    if owned_process_running; then client health; else echo "simulator not running"; fi
    ;;
  scene|frame|aim)
    client "$COMMAND" "$@"
    ;;
  --help|-h|help)
    usage
    ;;
  *)
    usage >&2
    exit 2
    ;;
esac

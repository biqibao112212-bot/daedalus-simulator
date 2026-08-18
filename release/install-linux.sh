#!/usr/bin/env bash
set -euo pipefail

VERSION=1.3.1-contest-r2
SOURCE_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
PREFIX="${HOME:?HOME is required}/.local/opt/daedalus-simulator/$VERSION"
BIN_DIR="$HOME/.local/bin"
FORCE=0
NO_LINKS=0

usage() {
  cat <<'EOF'
Usage: ./install-linux.sh [--prefix PATH] [--bin-dir PATH] [--force] [--no-links]
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --prefix) [[ $# -ge 2 ]] || { echo '--prefix requires a path' >&2; exit 2; }; PREFIX="$2"; shift 2 ;;
    --bin-dir) [[ $# -ge 2 ]] || { echo '--bin-dir requires a path' >&2; exit 2; }; BIN_DIR="$2"; shift 2 ;;
    --force) FORCE=1; shift ;;
    --no-links) NO_LINKS=1; shift ;;
    --help|-h) usage; exit 0 ;;
    *) echo "Unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
done

[[ "$(uname -m)" == x86_64 ]] || { echo 'Daedalus Simulator requires x86_64 Linux.' >&2; exit 1; }
PREFIX="$(realpath -m -- "$PREFIX")"
BIN_DIR="$(realpath -m -- "$BIN_DIR")"
case "$PREFIX" in
  /|"$HOME"|"$SOURCE_ROOT"|"$SOURCE_ROOT"/*) echo "Unsafe install prefix: $PREFIX" >&2; exit 1 ;;
esac
case "$SOURCE_ROOT" in
  "$PREFIX"/*) echo "Install prefix contains the source package: $PREFIX" >&2; exit 1 ;;
esac

if [[ -e "$PREFIX" ]]; then
  [[ -f "$PREFIX/release.json" ]] || { echo "Existing directory is not a Daedalus installation: $PREFIX" >&2; exit 1; }
  grep -q '"product"[[:space:]]*:[[:space:]]*"daedalus-simulator"' "$PREFIX/release.json" || {
    echo "Existing directory has an unexpected release marker: $PREFIX" >&2
    exit 1
  }
  if [[ "$FORCE" == 0 ]]; then
    if [[ -t 0 ]]; then
      read -r -p "Daedalus Simulator already exists at $PREFIX. Replace it? [y/N] " answer
      [[ "$answer" == y || "$answer" == Y || "$answer" == yes || "$answer" == YES ]] || exit 2
    else
      echo 'Existing installation requires --force in non-interactive mode.' >&2
      exit 2
    fi
  fi
  rm -rf -- "$PREFIX"
fi

mkdir -p -- "$PREFIX"
cp -a -- "$SOURCE_ROOT/." "$PREFIX/"
chmod +x -- "$PREFIX/bin/daedalus" "$PREFIX/start-simulator.sh" \
  "$PREFIX/daedalus-contest.sh" "$PREFIX/install-linux.sh" \
  "$PREFIX/sdk/bin/daedalus_contest_client"

if [[ "$NO_LINKS" == 0 ]]; then
  mkdir -p -- "$BIN_DIR"
  printf '#!/usr/bin/env bash\nexec "%s/daedalus-contest.sh" "$@"\n' "$PREFIX" \
    > "$BIN_DIR/daedalus-contest"
  chmod +x -- "$BIN_DIR/daedalus-contest"
fi

missing="$(ldd "$PREFIX/bin/daedalus" 2>/dev/null | awk '/not found/{print $1}' | paste -sd, -)"
if [[ -n "$missing" ]]; then
  echo "WARNING: missing runtime libraries: $missing" >&2
fi
if ! ldconfig -p 2>/dev/null | awk '/libvulkan\.so\.1/{found=1} END{exit !found}'; then
  echo 'WARNING: Vulkan loader libvulkan.so.1 was not found.' >&2
fi

echo "Installed Daedalus Contest $VERSION (Linux x86_64 only)"
echo "Location: $PREFIX"
[[ "$NO_LINKS" == 1 ]] || echo "Command: $BIN_DIR/daedalus-contest"
echo "Read first: $PREFIX/README_ZH.md"

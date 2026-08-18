#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
OUTPUT_ROOT=""
SKIP_BUILD=0
SKIP_PERFORMANCE_VALIDATION=0
FORCE=0
PERFORMANCE_EVIDENCE=""
PACKAGE_ID="linux-x86_64"
TARGET="x86_64-unknown-linux-gnu"

die() { echo "package-release.sh: $*" >&2; exit 1; }
usage() {
  cat <<'EOF'
Usage: ./scripts/package-release.sh [--output-root PATH] [--performance-evidence PATH] [--skip-build] [--skip-performance-validation] [--force]

--skip-performance-validation is an explicit release-authority override.  It
does not claim that a performance baseline was met and adds a notice to the
package instead of a performance evidence file.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --output-root) [[ $# -ge 2 ]] || die "--output-root requires a value"; OUTPUT_ROOT="$2"; shift 2 ;;
    --performance-evidence) [[ $# -ge 2 ]] || die "--performance-evidence requires a value"; PERFORMANCE_EVIDENCE="$2"; shift 2 ;;
    --skip-build) SKIP_BUILD=1; shift ;;
    --skip-performance-validation) SKIP_PERFORMANCE_VALIDATION=1; shift ;;
    --force) FORCE=1; shift ;;
    --help|-h) usage; exit 0 ;;
    *) die "unknown argument: $1" ;;
  esac
done

[[ "$(uname -m)" == "x86_64" ]] || die "Linux release packages require x86_64; got $(uname -m)"
command -v git >/dev/null || die "git is required"
command -v python3 >/dev/null || die "python3 is required to generate the release manifest"
command -v zip >/dev/null || die "zip is required"
command -v tar >/dev/null || die "tar is required"

# A Windows-created worktree has a .git pointer containing a Windows path.
# Native Linux Git cannot resolve that pointer from WSL, so use git.exe only
# for this local compatibility case. Native Linux checkouts keep using Git.
GIT_BIN=git
GIT_ROOT="$ROOT"
if ! "$GIT_BIN" -C "$GIT_ROOT" rev-parse --show-toplevel >/dev/null 2>&1; then
  command -v git.exe >/dev/null || die "git cannot read this worktree and git.exe is unavailable"
  command -v wslpath >/dev/null || die "wslpath is required for a Windows-created WSL worktree"
  GIT_BIN=git.exe
  GIT_ROOT="$(wslpath -w "$ROOT")"
  "$GIT_BIN" -C "$GIT_ROOT" rev-parse --show-toplevel >/dev/null || die "git cannot read the worktree"
fi

[[ -n "$OUTPUT_ROOT" ]] || OUTPUT_ROOT="$(cd -- "$ROOT/../.." && pwd)/releases/daedalus-simulator"
VERSION="$(tr -d '[:space:]' < "$ROOT/VERSION")"
PACKAGE_ROOT="$OUTPUT_ROOT/$VERSION"
TARGET_DIR="$PACKAGE_ROOT/$PACKAGE_ID"
ZIP_PATH="$PACKAGE_ROOT/$PACKAGE_ID.zip"
TAR_GZ_PATH="$PACKAGE_ROOT/$PACKAGE_ID.tar.gz"
LEARNING_RELEASE=0
if [[ "$VERSION" == *-learning ]]; then
  LEARNING_RELEASE=1
fi

if [[ "$SKIP_BUILD" == 0 ]]; then
  "$ROOT/scripts/build-release.sh"
fi

SOURCE_COMMIT="$("$GIT_BIN" -C "$GIT_ROOT" rev-parse HEAD)"
[[ -z "$("$GIT_BIN" -C "$GIT_ROOT" status --porcelain -- .)" ]] || die "Release packaging requires a clean committed worktree."
if [[ "$SKIP_PERFORMANCE_VALIDATION" == 0 ]]; then
  [[ -n "$PERFORMANCE_EVIDENCE" ]] || PERFORMANCE_EVIDENCE="$ROOT/benchmarks/$VERSION/performance-release-linux.json"
  MINIMUM_MAIN_UPDATE_HZ=100
  MINIMUM_CAPTURE_SUBMIT_HZ=100
  if [[ "$VERSION" == 1.3.* ]]; then
    # The 1.3.x Linux line is accepted only if the same host can meet the
    # published Windows 1.3.0 high-performance baseline.
    MINIMUM_MAIN_UPDATE_HZ=171.190
    MINIMUM_CAPTURE_SUBMIT_HZ=163.228
  fi
  python3 "$ROOT/scripts/check-performance-evidence.py" --root "$ROOT" --evidence "$PERFORMANCE_EVIDENCE" --version "$VERSION" --rust-target "$TARGET" --binary "$ROOT/target/$TARGET/release/daedalus" --minimum-main-update-hz "$MINIMUM_MAIN_UPDATE_HZ" --minimum-capture-submit-hz "$MINIMUM_CAPTURE_SUBMIT_HZ"
else
  [[ -z "$PERFORMANCE_EVIDENCE" ]] || die "--performance-evidence cannot be combined with --skip-performance-validation"
fi
[[ -f "$ROOT/LICENSE" ]] || die "repository LICENSE is missing"
[[ -f "$ROOT/release/INTERNAL_LAB_USE_NOTICE.md" ]] || die "internal-use notice is missing"

if [[ "$FORCE" == 0 && ( -e "$TARGET_DIR" || -e "$ZIP_PATH" || -e "$TAR_GZ_PATH" ) ]]; then
  die "Release already exists; use --force to replace it"
fi
if [[ "$FORCE" == 1 ]]; then
  case "$TARGET_DIR" in "$OUTPUT_ROOT"/*) ;; *) die "unsafe package target: $TARGET_DIR" ;; esac
  case "$ZIP_PATH" in "$OUTPUT_ROOT"/*) ;; *) die "unsafe package zip: $ZIP_PATH" ;; esac
  case "$TAR_GZ_PATH" in "$OUTPUT_ROOT"/*) ;; *) die "unsafe package tar.gz: $TAR_GZ_PATH" ;; esac
  rm -rf -- "$TARGET_DIR"
  rm -f -- "$ZIP_PATH"
  rm -f -- "$TAR_GZ_PATH"
fi

SDK_INSTALL="$ROOT/build/release/$PACKAGE_ID/sdk-install"
BINARY="$ROOT/target/$TARGET/release/daedalus"
[[ -x "$BINARY" ]] || die "missing executable: $BINARY"
[[ -d "$SDK_INSTALL" ]] || die "missing SDK install tree: $SDK_INSTALL"

mkdir -p "$TARGET_DIR/bin" "$TARGET_DIR/docs" "$TARGET_DIR/schemas" "$TARGET_DIR/sdk"
cp -- "$BINARY" "$TARGET_DIR/bin/daedalus"
cp -a -- "$ROOT/assets" "$TARGET_DIR/assets"
cp -- "$ROOT/release/release.json" "$ROOT/release/platform-matrix.json" \
  "$ROOT/release/camera-calibration.json" "$TARGET_DIR/"
cp -- "$ROOT/release/start-simulator.sh" "$TARGET_DIR/"
if [[ "$LEARNING_RELEASE" == 1 ]]; then
  sed \
    -e 's/daedalus-contest/daedalus-learning/g' \
    -e 's/Daedalus Contest/Daedalus Learning/g' \
    -e 's/contest simulator/learning simulator/g' \
    "$ROOT/release/daedalus-contest.sh" > "$TARGET_DIR/daedalus-learning.sh"
  chmod +x "$TARGET_DIR/daedalus-learning.sh"
else
  cp -- "$ROOT/release/daedalus-contest.sh" "$TARGET_DIR/"
fi
if [[ "$SKIP_PERFORMANCE_VALIDATION" == 0 ]]; then
  cp -- "$PERFORMANCE_EVIDENCE" "$TARGET_DIR/docs/performance-release.json"
else
  cat > "$TARGET_DIR/docs/PERFORMANCE_NOT_MEASURED.md" <<EOF
# Performance verification not run

This package was intentionally created with
\`--skip-performance-validation\` under explicit release authority. It does
not contain a formal performance baseline result for version \`$VERSION\`.
EOF
fi
if [[ "$LEARNING_RELEASE" == 1 ]]; then
  cp -- "$ROOT/release/LEARNING_GUIDE_ZH.md" "$TARGET_DIR/README_ZH.md"
else
  cp -- "$ROOT/release/CONTEST_GUIDE_ZH.md" "$TARGET_DIR/README_ZH.md"
fi
cp -- "$ROOT/release/install-linux.sh" "$TARGET_DIR/install-linux.sh"
chmod +x "$TARGET_DIR/start-simulator.sh" "$TARGET_DIR/install-linux.sh"
cp -- "$ROOT/LICENSE" "$TARGET_DIR/LICENSE.txt"
cp -- "$ROOT/release/INTERNAL_LAB_USE_NOTICE.md" "$TARGET_DIR/INTERNAL_LAB_USE_NOTICE.md"
cp -- "$TARGET_DIR/README_ZH.md" "$ROOT/sdk/README.md" \
  "$ROOT/SIMULATOR_TROUBLESHOOTING.md" "$TARGET_DIR/docs/"
cp -- "$ROOT/sdk/contract.json" "$TARGET_DIR/docs/sdk-contract.json"
cp -a -- "$SDK_INSTALL/." "$TARGET_DIR/sdk/"
chmod +x "$TARGET_DIR/start-simulator.sh" "$TARGET_DIR/install-linux.sh" \
  "$TARGET_DIR/bin/daedalus" \
  "$TARGET_DIR/sdk/bin/daedalus_contest_client"
if [[ "$LEARNING_RELEASE" == 1 ]]; then
  chmod +x "$TARGET_DIR/daedalus-learning.sh"
else
  chmod +x "$TARGET_DIR/daedalus-contest.sh"
fi

if find "$TARGET_DIR" -type f -printf '%f\n' | grep -Eiq '(cuda|cudnn|tensorrt|onnx|\.engine$|\.plan$|\.trt$|\.pt$|\.pth$|\.safetensors$|\.ckpt$|\.tflite$|\.pb$|\.mlmodel$|checkpoint)'; then
  die "simulator package contains forbidden inference payloads"
fi
if find "$TARGET_DIR" -type f \( -name '*.rs' -o -name '*.cpp' -o -name '*.cc' -o -name '*.cxx' -o -name '*.pdb' -o -name 'Cargo.toml' -o -name 'Cargo.lock' \) -print -quit | grep -q .; then
  die "simulator package contains forbidden source/debug files"
fi
if find "$TARGET_DIR" -type f \( -name '*.jsonl' -o -name '*.npy' -o -name '*.npz' -o -name '*.raw' -o -name '*.rgba' \) -print -quit | grep -q .; then
  die "simulator package contains protected labels or raw capture data"
fi
if find "$TARGET_DIR" -type d | grep -Eiq '/(capture|captures|label|labels|dataset|datasets)$'; then
  die "simulator package contains a protected capture/label data directory"
fi

python3 - "$TARGET_DIR" "$VERSION" "$SOURCE_COMMIT" "$PACKAGE_ID" "$TARGET" <<'PY'
import hashlib
import json
import pathlib
import sys
from datetime import datetime, timezone

target = pathlib.Path(sys.argv[1])
version, commit, package_id, rust_target = sys.argv[2:]
files = []
for path in sorted(p for p in target.rglob("*") if p.is_file() and p.name != "release-manifest.json"):
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    files.append({"path": path.relative_to(target).as_posix(), "bytes": path.stat().st_size, "sha256": digest})
manifest = {
    "schema_version": 1,
    "product": "daedalus-simulator",
    "version": version,
    "package_id": package_id,
    "rust_target": rust_target,
    "source_commit": commit,
    "generated_at": datetime.now(timezone.utc).isoformat(),
    "files": files,
}
(target / "release-manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
PY

(cd "$TARGET_DIR" && zip -qr "$ZIP_PATH" .)
python3 - "$TARGET_DIR" "$TAR_GZ_PATH" <<'PY'
import pathlib
import sys
import tarfile

target = pathlib.Path(sys.argv[1])
archive_path = pathlib.Path(sys.argv[2])
executables = {
    pathlib.PurePosixPath("bin/daedalus"),
    pathlib.PurePosixPath("install-linux.sh"),
    pathlib.PurePosixPath("start-simulator.sh"),
    pathlib.PurePosixPath("daedalus-contest.sh"),
    pathlib.PurePosixPath("daedalus-learning.sh"),
    pathlib.PurePosixPath("sdk/bin/daedalus_contest_client"),
}

def normalized(member: tarfile.TarInfo) -> tarfile.TarInfo:
    relative = pathlib.PurePosixPath(member.name.removeprefix("./"))
    member.uid = 0
    member.gid = 0
    member.uname = "root"
    member.gname = "root"
    if member.isdir():
        member.mode = 0o755
    elif member.issym():
        member.mode = 0o777
    elif relative in executables:
        member.mode = 0o755
    else:
        member.mode = 0o644
    return member

with tarfile.open(archive_path, "w:gz", format=tarfile.PAX_FORMAT) as archive:
    archive.add(target, arcname=".", recursive=True, filter=normalized)
PY
echo "release_dir=$TARGET_DIR"
echo "release_zip=$ZIP_PATH"
echo "release_tar_gz=$TAR_GZ_PATH"

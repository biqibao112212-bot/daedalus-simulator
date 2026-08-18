#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET="x86_64-unknown-linux-gnu"
PACKAGE_ID="linux-x86_64"
BUILD_ROOT="$ROOT/build/release/$PACKAGE_ID"
SDK_BUILD="$BUILD_ROOT/sdk-build"
SDK_INSTALL="$BUILD_ROOT/sdk-install"
BUILD_STAMP="$BUILD_ROOT/build-stamp.env"
BINARY="$ROOT/target/$TARGET/release/daedalus"
SDK_CONFIG="$SDK_INSTALL/lib/cmake/DaedalusSimSdk/DaedalusSimSdkConfig.cmake"
FEATURES="talos,distribution-release,learning-release"

die() { echo "build-release.sh: $*" >&2; exit 1; }

remove_reproducible_build_dir() {
  local path="$1"
  [[ -e "$path" ]] || return 0
  [[ ! -L "$path" ]] || die "refusing to remove build symlink: $path"
  case "$path" in
    "$ROOT/target/"*|"$ROOT/build/release/"*) ;;
    *) die "refusing build cleanup outside repository allowlist: $path" ;;
  esac
  rm -rf -- "$path"
}

prune_non_linux_builds() {
  local path name
  if [[ -d "$ROOT/target" ]]; then
    for path in "$ROOT/target"/*; do
      [[ -d "$path" ]] || continue
      name="$(basename -- "$path")"
      if [[ "$name" != "$TARGET" ]]; then
        remove_reproducible_build_dir "$path"
      fi
    done
    remove_reproducible_build_dir "$ROOT/target/$TARGET/debug"
  fi
  if [[ -d "$ROOT/build/release" ]]; then
    for path in "$ROOT/build/release"/*; do
      [[ -d "$path" ]] || continue
      name="$(basename -- "$path")"
      if [[ "$name" != "$PACKAGE_ID" ]]; then
        remove_reproducible_build_dir "$path"
      fi
    done
  fi
}

[[ "$(uname -m)" == "x86_64" ]] || die "Linux release builds require an x86_64 host; got $(uname -m)"
command -v cargo >/dev/null || die "cargo is required"
command -v rustc >/dev/null || die "rustc is required"
command -v cmake >/dev/null || die "cmake is required"
command -v ctest >/dev/null || die "ctest is required"
command -v c++ >/dev/null || die "c++ is required"
command -v sha256sum >/dev/null || die "sha256sum is required"

[[ -z "$(git -C "$ROOT" status --porcelain --untracked-files=normal)" ]] || \
  die "a reusable Linux Release build requires a clean Git worktree"

SOURCE_COMMIT="$(git -C "$ROOT" rev-parse HEAD)"
RUSTC_ID="$(rustc -Vv | sha256sum | awk '{print $1}')"
CMAKE_ID="$(cmake --version | sha256sum | awk '{print $1}')"
CXX_ID="$(c++ --version | sha256sum | awk '{print $1}')"
BUILD_KEY="$(printf '%s\n' \
  "source_commit=$SOURCE_COMMIT" \
  "target=$TARGET" \
  "profile=release" \
  "features=$FEATURES" \
  "rustc_id=$RUSTC_ID" \
  "cmake_id=$CMAKE_ID" \
  "cxx_id=$CXX_ID" | sha256sum | awk '{print $1}')"

prune_non_linux_builds

if [[ -f "$BUILD_STAMP" && -x "$BINARY" && -f "$SDK_CONFIG" ]] &&
   grep -Fqx "build_key=$BUILD_KEY" "$BUILD_STAMP"; then
  echo "reused=true"
  echo "source_commit=$SOURCE_COMMIT"
  echo "rust_target=$TARGET"
  echo "binary=$BINARY"
  echo "sdk_install=$SDK_INSTALL"
  exit 0
fi

# A changed build key replaces the previous Linux build. Source history lives
# in Git, so repository-local build products never accumulate by revision.
remove_reproducible_build_dir "$ROOT/target/$TARGET/release"
remove_reproducible_build_dir "$BUILD_ROOT"

mkdir -p "$BUILD_ROOT"
cargo test --locked --release --target "$TARGET" --features "$FEATURES" --bin daedalus
cargo build --locked --release --target "$TARGET" --features "$FEATURES"

cmake -S "$ROOT/sdk/cpp" -B "$SDK_BUILD" \
  -DCMAKE_BUILD_TYPE=Release \
  -DBUILD_TESTING=ON \
  -DCMAKE_INSTALL_PREFIX="$SDK_INSTALL"
cmake --build "$SDK_BUILD" --parallel
ctest --test-dir "$SDK_BUILD" --output-on-failure
cmake --install "$SDK_BUILD"

{
  printf 'build_key=%s\n' "$BUILD_KEY"
  printf 'source_commit=%s\n' "$SOURCE_COMMIT"
  printf 'target=%s\n' "$TARGET"
  printf 'profile=release\n'
  printf 'features=%s\n' "$FEATURES"
  printf 'rustc_id=%s\n' "$RUSTC_ID"
  printf 'cmake_id=%s\n' "$CMAKE_ID"
  printf 'cxx_id=%s\n' "$CXX_ID"
} > "$BUILD_STAMP.tmp"
mv -f -- "$BUILD_STAMP.tmp" "$BUILD_STAMP"

echo "reused=false"
echo "source_commit=$SOURCE_COMMIT"
echo "rust_target=$TARGET"
echo "binary=$BINARY"
echo "sdk_install=$SDK_INSTALL"

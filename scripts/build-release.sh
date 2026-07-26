#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET="x86_64-unknown-linux-gnu"
PACKAGE_ID="linux-x86_64"
BUILD_ROOT="$ROOT/build/release/$PACKAGE_ID"
SDK_BUILD="$BUILD_ROOT/sdk-build"
SDK_INSTALL="$BUILD_ROOT/sdk-install"

die() { echo "build-release.sh: $*" >&2; exit 1; }

[[ "$(uname -m)" == "x86_64" ]] || die "Linux release builds require an x86_64 host; got $(uname -m)"
command -v cargo >/dev/null || die "cargo is required"
command -v cmake >/dev/null || die "cmake is required"
command -v ctest >/dev/null || die "ctest is required"

mkdir -p "$BUILD_ROOT"
cargo build --locked --release --features talos,distribution-release --target "$TARGET"

cmake -S "$ROOT/sdk/cpp" -B "$SDK_BUILD" \
  -DCMAKE_BUILD_TYPE=Release \
  -DBUILD_TESTING=ON \
  -DCMAKE_INSTALL_PREFIX="$SDK_INSTALL"
cmake --build "$SDK_BUILD" --parallel
ctest --test-dir "$SDK_BUILD" --output-on-failure
cmake --install "$SDK_BUILD"

echo "rust_target=$TARGET"
echo "binary=$ROOT/target/$TARGET/release/daedalus"
echo "sdk_install=$SDK_INSTALL"

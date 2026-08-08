# Daedalus Simulator release process

`VERSION` is the single product-version source. `release/release.json` is the
runtime release contract, `sdk/contract.json` is the SDK contract, and
`release/platform-matrix.json` defines the supported OS/architecture targets.
These files must agree before a package is published.

## Supported packages

- Windows x86_64: `x86_64-pc-windows-msvc`, `bin/daedalus.exe`.
- Linux x86_64: `x86_64-unknown-linux-gnu`, `bin/daedalus`.
- 32-bit i686 and ARM are not release targets.

Each platform builds and tests its own Rust binary and C++ SDK. Never copy the
Linux SDK install tree into a Windows package or vice versa.

## Build and validate

Windows requires Rust, MSVC, CMake, and CTest on `PATH`:

```powershell
.\scripts\build-release.ps1 -Platform windows -Arch x86_64
.\scripts\check-compatibility.ps1
```

Linux requires an x86_64 Linux runner with Rust, CMake, CTest, a C++17 compiler,
and the system window/audio/Vulkan development packages:

```bash
bash scripts/build-release.sh
```

Both build paths use
`cargo build --locked --release --features talos,distribution-release`, run
the SDK CTest suite, and install the SDK into a platform-specific build tree.

## Mandatory performance gate

Every simulator source change that can affect runtime throughput, and every
new release version, requires a fresh local GPU measurement before packaging.
Run only the optimised Release-profile binary; Debug binaries are categorically
invalid as performance evidence:

```powershell
.\scripts\measure-performance.ps1 -DurationSeconds 20
```

The command requires `target\release\daedalus.exe`, TCP image transport, and
at least 100 Hz for both `main_update_hz` and `capture_copy_submit_hz`. For a
formal package, run it from a clean committed checkout, promote the resulting
`performance-evidence.json` to `benchmarks/<VERSION>/performance-release.json`,
and retain the raw run directory. Both package scripts validate that evidence
and reject stale, dirty, Debug-profile, wrong-version, or sub-threshold data.
If `Cargo.toml`, `Cargo.lock`, `src`, `assets`, `config.performance.toml`,
`release`, or `sdk` changes after the evidence source commit, the benchmark
must be repeated before packaging.

For local source commits, enable the tracked hook once per worktree:

```powershell
git config core.hooksPath .githooks
```

It rebuilds and measures Release automatically when staged files can affect
throughput. The package gate remains mandatory even if the local hook ran.

## Package

Release packaging requires a clean committed worktree. The output layout is:

```text
<output-root>/<version>/windows-x86_64/
<output-root>/<version>/windows-x86_64.zip
<output-root>/<version>/linux-x86_64/
<output-root>/<version>/linux-x86_64.zip
<output-root>/<version>/linux-x86_64.tar.gz
```

The Linux `tar.gz` archive is the primary end-user artifact because it
preserves executable mode bits. The Linux ZIP is retained as a convenient
fallback for browsing and transfer from Windows hosts.

```powershell
.\scripts\package-release.ps1 -Platform windows -Arch x86_64
```

```bash
bash scripts/package-release.sh
```

Every package contains the simulator binary, assets, fixed calibration,
the matching SDK install tree, public contracts, launch script, and a
`release-manifest.json` with the source commit and SHA256 for every file.
Editable TOML and simulator source are intentionally absent. Packaging is
configured for non-commercial use inside the owning laboratory and carries
the repository license plus `INTERNAL_LAB_USE_NOTICE.md`. External or public
distribution requires a separate licensing and dependency review.

## GPU and inference boundary

The package does not contain GPU drivers, CUDA, cuDNN, TensorRT, ONNX files, or
TensorRT engines. Windows uses DX12 for the measured high-performance mode and
Vulkan for visible validation; Linux uses Vulkan for both. Drivers are supplied
by the host system.

wgpu selects a high-performance adapter from the selected OS backend. After
renderer initialization the exact adapter, backend, vendor/device IDs and
driver strings are published to
`$TALOS_IPC_DIR/daedalus-runtime-capabilities-v1.json` and read through the
SDK `readRuntimeCapabilities()` API.

CUDA/TensorRT inference is owned by the consumer bridge. Different inference
versions are selected by rebuilding that consumer against its own CUDA/TensorRT
profile; the simulator package and SDK contract do not change. The existing
recorded baseline is Windows simulator + WSL consumer bridge with CUDA 12.8 and
TensorRT 10.9.0.34. Other profiles require an independent joint validation.

See `release/PLATFORM_SUPPORT.md` for the full platform matrix and release
acceptance gates.

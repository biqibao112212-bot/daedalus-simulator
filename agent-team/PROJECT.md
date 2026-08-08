# Daedalus Simulator release branch context

- Protocol: `agent-team-fixed/v2`
- Repository: `daedalus-simulator`
- Branch: `release/simulator-multiplatform-x86`
- Isolated worktree: `D:\仿真\isolated\daedalus-simulator-multiplatform-x86`
- Protected main checkout: `D:\仿真\repos\daedalus-simulator` (`main`)
- Last fully accepted implementation baseline: `6883879`
- Product candidate: simulator `1.1.1`, SDK `1.1.0`, SHM v7 / ABI revision 2

## Mission

## Approved current change (2026-08-07)

The latest formal baseline is simulator 1.1.1, source commit
`2a8470204a9d3bf3ffd3ac646f985900d54471be`, on
`release/simulator-multiplatform-x86`. The approved development candidate is
1.2.0 / SDK 1.2.0. It adds Scene Control v2 geometry control that scales only
the four armor-root horizontal positions for shooting-range targets. The
frozen 1.1.1 package remains untouched; consumer locks wait for clean release
and acceptance gates.

Define and implement an internal-laboratory x86_64 release flow for Windows and Linux.
Each target must build its own Rust binary and C++ SDK, run target-platform
tests, and produce a platform-specific package with a SHA256 manifest.

## Owned scope

This branch owns simulator build scripts, release metadata, launchers, SDK
packaging, platform documentation, and release validation. The public runtime
contract remains `release/release.json` plus `sdk/contract.json`.

The supported public surface is scene control, RGB image transport, fixed
read-only calibration, gimbal command/actual-state feedback, and runtime GPU
capabilities. Inference models and vision-result upload are outside the
simulator release.

## Platform contract

- Windows: `x86_64-pc-windows-msvc`, `daedalus.exe`, DX12 performance default,
  Vulkan visible default.
- Linux: `x86_64-unknown-linux-gnu`, `daedalus`, Vulkan performance and visible
  defaults. The internal-lab release gate accepts software Vulkan startup and
  IPC diagnostics without a discrete GPU; real-time RGB performance is not
  promised by that no-GPU acceptance scope.
- `i686`, ARM, and other targets are outside this release.
- GPU drivers are system prerequisites and are never bundled.
- CUDA/TensorRT/model/engine files are not simulator assets. Inference is owned
  by the consumer bridge; the selected consumer build profile determines its
  CUDA/TensorRT version.

## Stable commands

```text
scripts/build-release.ps1 -Platform windows -Arch x86_64
scripts/build-release.sh
scripts/check-compatibility.ps1
scripts/package-release.ps1 -Platform windows -Arch x86_64
scripts/package-release.sh
```

Release claims require a clean committed tree and target-platform Rust/CMake
validation. Do not modify the protected `main` checkout or formal release
assets from this isolated branch without explicit intent.

The current distribution profile is `internal-lab`: non-commercial training
and research inside the owning laboratory. It does not require a commercial
license file. External/public distribution remains a separate review scope.

# Daedalus Simulator release board

## Current status

- Observation-rate diagnosis (2026-08-08): the reported approximately 4 Hz
  source cadence was produced by the unoptimised `target\\debug\\daedalus.exe`,
  not a formal Release. With the same Windows DX12, headless, absolute
  `config.performance.toml` settings, that debug binary remained at 4 Hz even
  after the intended one physics substep applied. The existing
  `target\\release\\daedalus.exe` measured 184.856 Hz main update, 164.872 Hz
  capture submit, and 161.874 Hz TCP send over 20.192 s. Raw development
  evidence is retained under `D:\仿真\runtime\simulator-*-compare-20260808`;
  it is not Release evidence because the checkout is dirty.
- The immediate consumer-side remedy is operational: benchmark/development
  capture must use an optimised Release build compatible with the v1.2.0
  scene-control work. Before a new consumer lock or performance claim, build
  and measure the exact committed v1.2.0 Release from a clean checkout.
- Release-only performance gate is being established: `measure-performance.ps1`
  refuses Debug paths and measures the source/capture stages at a 100 Hz floor;
  both package scripts validate clean, version-matched evidence and reject a
  release if performance-relevant files changed after that evidence.

- Isolation established: the protected simulator checkout remains clean on
  `main`; all branch work is in the separate worktree.
- Platform matrix and package layout are implemented for Windows/Linux
  `x86_64`.
- Windows Rust release + thin LTO build passed for
  `x86_64-pc-windows-msvc`.
- Windows MSVC SDK Release/x64 build, all 7 CTest tests, and install passed.
- Linux Rust release build passed for `x86_64-unknown-linux-gnu`.
- Linux SDK CMake build, all 7 CTest tests, and install passed.
- Script syntax, JSON parsing, and simulator/SDK compatibility checks passed.
- Distribution lock, fixed calibration contract, actual GPU capability query,
  and command-id based gimbal feedback are implemented.
- Image frame identity now resolves to exact exposure-time gimbal state through
  a 16-frame history; the Chinese user guide documents the complete loop.
- Rust `cargo check --locked --features talos,distribution-release` passed.
- Internal-laboratory packaging no longer requires a commercial license file;
  packages carry the repository license and internal-use notice.
- A dedicated Chinese SDK API reference documents signatures, parameters,
  return values, errors, and examples.
- The Windows package runs from its final layout with RTX 4060 Laptop GPU /
  DX12, all three loopback endpoints, 76992-byte metadata, and no missing
  assets. A package-SDK C++ consumer validated image/timestamp, exact-frame
  gimbal state, scene ACK, gimbal command ID, and applied-command feedback.
- Native acceptance found and fixed Visual Studio multi-config, parallel PDB,
  CMake install-prefix, and headless process-lifetime defects.
- Windows visible Vulkan mode was visually accepted on the RTX 4060: the
  responsive window rendered robots, targets, armor plates, scene geometry,
  and the status overlay without black output, missing assets, or corruption.
- Linux native Rust/GNU builds and all SDK tests passed. The package started
  under Mesa llvmpipe with all endpoints, IPC metadata, and runtime capability
  reporting; the agreed Linux gate does not require a discrete GPU or real-time
  RGB delivery.
- Camera calibration revision 2 fixes digital exposure at EV100 9.7 with auto
  exposure and tonemapping disabled; physical shutter/gain do not apply.
- Final binaries use accepted implementation
  `6883879f74549eb960cbbc5283aede5222f57fb7`; installer-ready packages were
  regenerated from clean commit
  `9aa339d1a8226b900986f28ffb61eac8fd7127b2`. The Windows manifest lists 47
  payload files and the Linux manifest lists 46, including platform-specific
  Chinese root READMEs, installers, SDK documentation, and the machine-readable
  short performance result. Every directory/ZIP payload hash was independently
  verified.
- Final Windows package live acceptance returned a 1440x1080 RGBA32 frame,
  exposure timestamp, exact-frame gimbal state, scene status, and matching
  command/applied-command ID on the RTX 4060 / DX12 adapter.
- Final Linux package no-GPU acceptance passed on Vulkan llvmpipe: stable
  startup, TCP 5602, UDP 5601/5603, 76992-byte IPC metadata, and locked runtime
  capabilities.
- Windows non-admin installation passed in an isolated directory using the
  packaged PowerShell installer; the installed executable matches the package
  byte-for-byte. Linux per-user installation passed with an isolated prefix;
  both generated launch commands are executable and syntactically valid, and
  the installed executable matches the package byte-for-byte. The Linux
  installer's unsafe-target guard also correctly refused an accidental `/opt`
  target during command-line harness debugging.
- Archive SHA256: Windows
  `d6dbaf48e6b7ec7e4429123190d01ec7eef8da4c1aa71a4ea0b8c6efbc3dcbe8`;
  Linux `608ad757a03be6b5593f59b2b846dabefc7d2296a9ee527ac330f683a3abbffe`.
- A 5.021-second package-SDK TCP short test under an independent concurrent
  CUDA training load received 424 RGBA32 frames: 84.44 Hz and 500.94 MiB/s.
  Payload throughput is 7.24% below the old RGB24 joint baseline, while FPS is
  30.43% lower because the frame is 33.33% larger and the GPU load is not
  comparable. The user accepted this contended lower bound for internal-lab
  release; an idle-GPU measurement remains optional follow-up.

## In progress

## Approved 1.2.0 geometry-control work (2026-08-07)

- User approval received to modify the simulator using the latest formal 1.1.1
  baseline; old 1.0.x assumptions are not used for the implementation.
- Scene Control v2 and `set_range_target_geometry` are implemented in the
  working tree. Geometry changes are stationary-only, absolute relative to
  stock, bounded to 0.75..1.25, and reset on scene reset/new session.
- Rust workspace checks/tests pass. CMake is unavailable in this environment,
  so the C++ SDK test remains a release-environment gate.
- Frozen 1.1.1 artifacts remain untouched. Do not update consumer locks until
  a clean 1.2.0 commit, package manifest, hashes, and acceptance evidence exist.

Version 1.1.0 was published before AutoDL acceptance and exposed a Linux
headless Winit crash; its release has zero downloads and will be marked
superseded. Version 1.1.1 is the patch candidate: Linux performance mode now
runs without X11/Wayland, while SDK 1.1.0 / ABI revision 2 remains unchanged.
The first RTX 3090 SDK run exposed a 250 Hz fixed-step catch-up spiral (2.85
delivered FPS) and a related low-frame-rate command-expiry defect. Both root
causes are fixed and covered by focused tests. Exact RC2 remeasurement passed:
723 frames in 20.007 seconds (36.138 FPS, 214.393 MiB/s), 722/722 successful
exact-frame gimbal timestamp matches, tracked command feedback, all scene ACKs,
and zero TCP timeout on the RTX 3090 Vulkan adapter.

## Remaining gates

- Any external or public distribution requires a separate licensing and
  third-party dependency review; the current profile is internal-lab only.
- Consumer CUDA/TensorRT/model validation remains outside the simulator
  release because inference payloads are not distributed.

## Next steps

1. Build and verify the exact 1.1.1 Windows ZIP, Linux TAR.GZ, and Linux ZIP.
2. Publish `simulator-v1.1.1` on GitHub and validate the Linux artifact through
   AutoDL's GitHub acceleration channel on an RTX 3090 host.
3. Record the exact performance evidence and final release asset hashes.
4. Add a branded GUI installer later only if the laboratory wants to maintain
   an Inno Setup, NSIS, or MSI toolchain.

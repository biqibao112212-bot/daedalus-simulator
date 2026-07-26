# Daedalus Simulator release board

## Current status

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
- Final packages were regenerated from clean committed implementation
  `6883879f74549eb960cbbc5283aede5222f57fb7`. Each manifest lists 43 payload
  files and every directory/ZIP payload hash was independently verified.
- Final Windows package live acceptance returned a 1440x1080 RGBA32 frame,
  exposure timestamp, exact-frame gimbal state, scene status, and matching
  command/applied-command ID on the RTX 4060 / DX12 adapter.
- Final Linux package no-GPU acceptance passed on Vulkan llvmpipe: stable
  startup, TCP 5602, UDP 5601/5603, 76992-byte IPC metadata, and locked runtime
  capabilities.
- Archive SHA256: Windows
  `ec47bba7fa36e259fdc92492b5dde0f9c4cd3eb2b4e20730f722c1bc254550d6`;
  Linux `addbba42d53ded68546606fdf4bb5018d5139bfcba42a9eabda408a699a79a57`.
- A 5.021-second package-SDK TCP short test under an independent concurrent
  CUDA training load received 424 RGBA32 frames: 84.44 Hz and 500.94 MiB/s.
  Payload throughput is 7.24% below the old RGB24 joint baseline, while FPS is
  30.43% lower because the frame is 33.33% larger and the GPU load is not
  comparable. This is retained as a contended lower bound, not a regression or
  final performance pass.

## In progress

The user accepted the contended performance lower bound for internal-lab
release. Regenerating both archives with the updated manuals and machine-
readable short-test result is active; an idle-GPU test is optional follow-up.

## Remaining gates

- Any external or public distribution requires a separate licensing and
  third-party dependency review; the current profile is internal-lab only.
- Consumer CUDA/TensorRT/model validation remains outside the simulator
  release because inference payloads are not distributed.

## Next steps

1. Commit the accepted report and package-script documentation addition.
2. Regenerate both archives with `--skip-build` / `-SkipBuild`, then verify
   manifests, ZIP payload hashes, and package boundaries.

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

## In progress

Fixed-exposure validation and final clean-commit package refresh are active.

## Remaining gates

- Rebuild and regenerate both platform packages from the final clean committed
  revision, then verify manifests, archives, package boundaries, and hashes.
- Any external or public distribution requires a separate licensing and
  third-party dependency review; the current profile is internal-lab only.
- Consumer CUDA/TensorRT/model validation remains outside the simulator
  release because inference payloads are not distributed.

## Next steps

1. Commit the fixed-exposure contract and updated acceptance record.
2. Rebuild Windows and Linux from that clean revision.
3. Regenerate both formal packages and run final archive/manifest checks.

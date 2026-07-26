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

## In progress

Windows native acceptance is complete. Final documentation commit and package
manifest refresh are active; Linux native packaging remains next.

## Remaining gates

- Linux package generation from the final clean committed revision.
- Linux runtime smoke test with an actual GPU driver.
- Windows visible Vulkan mode visual acceptance.
- Final physical camera exposure/calibration values and revision approval.
- Any external or public distribution requires a separate licensing and
  third-party dependency review; the current profile is internal-lab only.
- Consumer CUDA/TensorRT/model validation remains outside the simulator
  release because inference payloads are not distributed.

## Next steps

1. Refresh the Windows package manifest from the final documentation commit.
2. Generate and validate the Linux package from the same revision.
3. Complete final camera calibration and Windows visible-mode acceptance.

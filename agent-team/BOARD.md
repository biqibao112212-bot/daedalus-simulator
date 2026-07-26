# Daedalus Simulator release board

## Current status

- Isolation established: the protected simulator checkout remains clean on
  `main`; all branch work is in the separate worktree.
- Platform matrix and package layout are implemented for Windows/Linux
  `x86_64`.
- Windows Rust release check passed for
  `x86_64-pc-windows-msvc`.
- Linux Rust release build passed for `x86_64-unknown-linux-gnu`.
- Linux SDK CMake build, all 7 CTest tests, and install passed.
- Script syntax, JSON parsing, and simulator/SDK compatibility checks passed.
- Distribution lock, fixed calibration contract, actual GPU capability query,
  and command-id based gimbal feedback are implemented.
- Image frame identity now resolves to exact exposure-time gimbal state through
  a 16-frame history; the Chinese user guide documents the complete loop.
- Rust `cargo check --locked --features talos,distribution-release` passed.

## In progress

Implementation and local validation are complete; no formal package is claimed
from the uncommitted worktree.

## Remaining gates

- Windows CMake/CTest/SDK install on a Windows host with CMake and MSVC.
- Windows package generation from a clean committed revision.
- Linux package generation from a clean committed revision; current source
  changes intentionally keep the worktree dirty so no formal release artifact
  is claimed from uncommitted source.
- Target-platform runtime smoke tests with actual GPU drivers.
- Final physical camera exposure/calibration values and revision approval.
- Approved `release/COMMERCIAL_LICENSE.txt`; current AGPL repository license is
  an explicit closed-source publication blocker.
- Consumer-side joint validation for each CUDA/TensorRT profile. The existing
  CUDA 12.8 + TensorRT 10.9.0.34 Windows/WSL baseline is recorded metadata,
  not a new validation performed by this branch.

## Next steps

1. Run clean-checkout/package validation after an authorized commit.
2. Complete Windows CMake/CTest/SDK validation on a native Windows toolchain.
3. Publish platform-specific ZIPs only after both native SDK gates pass.

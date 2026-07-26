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
- Internal-laboratory packaging no longer requires a commercial license file;
  packages carry the repository license and internal-use notice.
- A dedicated Chinese SDK API reference documents signatures, parameters,
  return values, errors, and examples.

## In progress

Native Windows build, SDK CTest, runtime smoke test, and package generation are
the active acceptance stage.

## Remaining gates

- Linux package generation from a clean committed revision; current source
  changes must be committed before a formal release artifact is generated.
- Linux runtime smoke test with an actual GPU driver.
- Final physical camera exposure/calibration values and revision approval.
- Any external or public distribution requires a separate licensing and
  third-party dependency review; the current profile is internal-lab only.
- Consumer-side joint validation for each CUDA/TensorRT profile. The existing
  CUDA 12.8 + TensorRT 10.9.0.34 Windows/WSL baseline is recorded metadata,
  not a new validation performed by this branch.

## Next steps

1. Complete native Windows build, runtime smoke test, and package inspection.
2. Generate the Linux package from the same committed revision.
3. Complete final camera calibration and the consumer auto-aim joint test.

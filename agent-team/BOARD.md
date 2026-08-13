# Daedalus Simulator board

## Current status

- Windows x86_64 Release `1.3.0` remains accepted and immutable at
  `D:\仿真\releases\daedalus-simulator\1.3.0\windows-x86_64`, manifest
  source `2bce032ebcc55bfc4cfa0e6e793802a55ea22c70`.
- Linux `1.3.0` native build and runtime validation passed on Ubuntu x86_64 /
  RTX 4060. Vulkan high-performance mode reached `316.921 Hz` main update and
  `199.321 Hz` capture submit; visible Vulkan reached `282.194 Hz` main,
  `197.436 Hz` capture and `59.829 Hz` preview.
- The Linux high-performance acceptance line for `1.3.0` is the accepted
  Windows evidence: at least `171.190 Hz` main update and `163.228 Hz`
  capture submit. Visible mode is measured and retained separately.

## Freeze and blockers

- Never overwrite or mutate any existing formal Release, especially `1.2.1`.
- The accepted Windows package is immutable. Labels, TCP identity ledgers, raw
  RGBA, successful/failed experiment sessions, and prior failed packages are
  protected assets and remain retained.
- The former Linux packager incorrectly validated a Windows `.exe` evidence
  path. A platform-specific Linux measurement/evidence path is now being
  validated before any package is created.

## Ordered next steps

1. Linux `1.3.0` has been packaged at the version/platform-scoped location,
   rehashed 54/54, and package-smoke-tested at `198.849 Hz` capture / `249.811
   Hz` physics on the selected NVIDIA Vulkan device.
2. The immutable Linux `1.1.1` package rehashed 47/47 and its package SDK TCP
   consumer received `197.288 FPS` for 15 seconds on this host. This is a
   same-host startup/data-plane observation, not a revision of its AutoDL
   baseline or a cross-machine regression claim.
3. The active release package source is `ee39776`; do not overwrite it. Later
   documentation-only context commits do not alter the packaged binary or
   evidence hash.

## Validation still required

- Windows and Linux package integrity/runtime checks are complete. `pwsh` and
  three X11 development headers are unavailable on this host without sudo
  credentials, so the PowerShell compatibility script was not rerun locally;
  native Rust/SDK/format/clippy validation did pass.
- Strict workspace clippy remains pre-existing frozen-baseline debt and is not
  a Release gate: `crates/exact` range-loop and `talos-ipc` derivable-Default.

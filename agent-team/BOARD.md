# Daedalus Simulator board

## Current status

- Windows x86_64 Release `1.3.0` remains accepted and immutable at
  `D:\仿真\releases\daedalus-simulator\1.3.0\windows-x86_64`, manifest
  source `2bce032ebcc55bfc4cfa0e6e793802a55ea22c70`.
- Linux `1.3.0` is in progress on Ubuntu x86_64 / RTX 4060. The release gate
  now requires target-native Vulkan evidence and binds its binary SHA256 to
  the package; no Linux release claim has yet been made.
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

1. Validate the Linux build, public compatibility, headless performance and
   visible Vulkan modes from the exact clean commit.
2. Package the new Linux `1.3.0` archive only if high-performance evidence
   meets the Windows-equivalent gate; then rehash and smoke-test it.
3. Run the protected Linux `1.1.1` package under the same Ubuntu host and
   report it as a historical-version result, not as a replacement artifact.

## Validation still required

- Linux runtime, package integrity, and 1.1.1 comparative checks remain
  required. Windows acceptance remains complete.
- Strict workspace clippy remains pre-existing frozen-baseline debt and is not
  a Release gate: `crates/exact` range-loop and `talos-ipc` derivable-Default.

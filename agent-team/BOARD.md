# Daedalus Simulator board

## Current status

- Contest branch `release/contest-linux-1.3.1` is implementing the Linux-only
  `1.3.1-contest` package. Its C++ client, constrained scene control and
  participant launcher passed clean Release Rust `199/199` and SDK CTest
  `8/8`. Its high-performance Vulkan gate measured `306.822 Hz` main update
  and `197.885 Hz` capture submit against the 171.190/163.228 Hz acceptance
  minima. The final Linux package is at
  `/home/potato/Projects/仿真/releases/daedalus-simulator/1.3.1-contest/linux-x86_64`;
  its manifest rehash passed `43/43`, and its package source is `97f9f5f`.
- Contest runtime now defaults to direct participant vehicle control and no
  longer displays unsupported F-key controls: WASD/Shift movement, Q/E chassis
  yaw, arrows/right mouse gimbal, and Space firing are live; automatic aim is
  off at startup so manual gimbal input owns the controlled vehicle.
- Package-runtime acceptance passed on Ubuntu/RTX 4060: the default visible
  launcher opened an actual 2560x1440 X11 Vulkan window; the installed C++ SDK
  read a 1440x1080 TCP frame, sent aim/fire, and switched both allowed scenes.
  A focused X11 Right-arrow injection changed the observed gimbal yaw from
  `0.0000°` to `-85.6756°`, proving the released keyboard input path is live.
- Package-runtime acceptance on Ubuntu/RTX 4060 passed: `daedalus-contest`
  started visible Shooting Range, created an actual 2560x1440 `daedalus` X11
  window, reported `1.3.1-contest` / Vulkan / RTX 4060, returned RGBA32
  1440x1080 TCP frames, applied a C++ SDK gimbal command, then switched to
  large Energy Mechanism and applied its large-rune state. The desktop lacks
  the screenshot tools required for a PNG capture, so the saved proof is the
  command/runtime evidence rather than a screenshot asset.
- `daedalus-contest start` defaults to visible Vulkan rendering for the
  competition. `--performance` is the explicit opt-in headless mode.

- Linux x86_64 Release `1.3.1` is packaged at
  `/home/potato/Projects/仿真/releases/daedalus-simulator/1.3.1/linux-x86_64`.
  Its default-off Release collector now owns raw full-frame export; consumers
  must use `--save-rgba-frames --until-eof` and validator
  `--require-raw-frames`, not a copied TCP implementation.

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
- Retain only one repository-local build: the currently usable Linux x86_64
  Release binary plus its matching Linux SDK build/install. Reuse an exact
  stamped build; never accumulate Windows, debug or prior-revision products.
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
3. The immutable 1.3.0 Linux package source is `ee39776`; do not overwrite it.
   The new immutable 1.3.1 Linux package source is `d7637d0`; later
   documentation-only context commits do not alter either package binary or
   evidence hash.
4. 1.3.1 full-frame export passed source Rust `196/196`, SDK `7/7`, a clean
   20-second Linux performance gate (`357.044/198.468 Hz` main/capture),
   package integrity, and a package-runtime raw-frame/label smoke. Consumers
   may now update only through its release manifest and SDK contract.

## Validation still required

- Windows and Linux package integrity/runtime checks are complete. `pwsh` and
  three X11 development headers are unavailable on this host without sudo
  credentials, so the PowerShell compatibility script was not rerun locally;
  native Rust/SDK/format/clippy validation did pass.
- Strict workspace clippy remains pre-existing frozen-baseline debt and is not
  a Release gate: `crates/exact` range-loop and `talos-ipc` derivable-Default.
- Build-retention validation must show no repository-local build directory
  outside `target/x86_64-unknown-linux-gnu/release` and
  `build/release/linux-x86_64`; the build script must reuse a matching stamp.

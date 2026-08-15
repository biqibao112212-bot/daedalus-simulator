# Daedalus Simulator board

## Current status

- In progress: finalize the `1.3.1-contest` Linux publish package with updated
  SDK/readme guidance for large-rune score fields and the in-window statistics
  HUD. Because public release/SDK inputs are package-gated, the final clean
  release build, performance evidence, manifest and installed smoke will be
  regenerated after the documentation commit.

- The contest big-rune score now appears in the existing bottom-left
  projectile-statistics line, directly after `pct`. It is shown only in Energy
  while the large rune is selected, with red/blue activated-arm count, average
  ring and latest ring. Clean Release validation and SDK CTest passed; the
  refreshed performance gate reached `341.167/192.530 Hz` main/capture and the
  replacement package source is `67620bf` with manifest rehash `43/43`.
  Installed visible Vulkan smoke recorded an actual red valid hit as
  `arms=1 avg=5.0 last=5` while blue remained zero.

- Contest `1.3.1-contest` now provides a read-only big-rune ring score through
  C++ `ContestClient`, Scene Control v2 and `daedalus-contest score red|blue`.
  It records a valid active-leaf collision point in the target-local plane and
  reports per-side current-run data. Clean Release Rust validation and SDK
  CTest `8/8` passed; the fresh high-performance Vulkan evidence reached
  `357.758 Hz` main update and `198.311 Hz` capture submit. The replacement
  package source is `2f759ec`, its manifest rehash passed `43/43`, and its
  installed visible Vulkan runtime returned red/blue active-run score records
  plus a `1440x1080` TCP frame on the RTX 4060.

- Contest branch `release/contest-linux-1.3.1` is implementing the Linux-only
  `1.3.1-contest` package. Its C++ client, constrained scene control and
  participant launcher passed clean Release Rust validation and SDK CTest
  `8/8`. Its high-performance Vulkan gate measured `349.598 Hz` main update
  and `197.772 Hz` capture submit against the 171.190/163.228 Hz acceptance
  minima. The final Linux package is at
  `/home/potato/Projects/仿真/releases/daedalus-simulator/1.3.1-contest/linux-x86_64`;
  its manifest rehash passed `43/43`, and its package source is `74fd730`.
- Contest runtime now defaults to direct participant vehicle control and no
  longer displays unsupported F-key controls: WASD/left Shift movement,
  arrows/right mouse gimbal, and Space firing are live; automatic aim is off
  at startup so manual gimbal input owns the controlled vehicle. Q/E turns the
  chassis in Shooting Range and switches small/large rune mode in Energy.
- Package-runtime acceptance passed on Ubuntu/RTX 4060: the default visible
  launcher opened an actual 2560x1440 X11 Vulkan window; the installed C++ SDK
  read a 1440x1080 TCP frame, sent aim/fire, and switched both allowed scenes.
  A focused X11 Right-arrow injection changed the observed gimbal yaw from
  `0.0000°` to `-85.6756°`, proving the released keyboard input path is live.
- Energy-scene visual acceptance now starts the vehicle on the flat west apron
  rather than the central ramp, locks the root body against overturning, and
  uses the range-proven initial camera pitch. The SDK reports `63°` initial
  pitch and an X11 capture shows the energy mechanism in view. The HUD uses
  ASCII English controls because the packaged default font has no CJK glyphs.
- Package-runtime acceptance on Ubuntu/RTX 4060 passed: `daedalus-contest`
  started visible Shooting Range, created an actual 2560x1440 `daedalus` X11
  window, reported `1.3.1-contest` / Vulkan / RTX 4060, returned RGBA32
  1440x1080 TCP frames, applied a C++ SDK gimbal command, then switched to
  large Energy Mechanism. The desktop lacks
  the screenshot tools required for a PNG capture, so the saved proof is the
  command/runtime evidence rather than a screenshot asset.
- `daedalus-contest start` defaults to visible Vulkan rendering for the
  competition. `--performance` is the explicit opt-in headless mode.
- Selecting large Energy through the C++ contest SDK now leaves the simulator
  in its live large-rune state machine rather than applying a frozen empty
  target snapshot: it starts with two active leaves, advances after hits and
  recovers through its timeouts.
- Package-runtime acceptance after the lifecycle fix passed on Ubuntu/RTX 4060:
  the visible 2560x1440 X11 Vulkan window stayed open through Energy →
  Shooting Range → Energy, while the installed C++ SDK received 1440x1080
  RGBA frames before and after the switches. The host has no desktop key
  injector, so Q/E is covered by the contest input unit tests and the visible
  scene-specific HUD rather than an artificial X11 key event.

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

- Package scenario acceptance covers both Energy modes: Q/E select small/large
  for normal local use, while C++ RuneScenario can apply either rules-driven
  motion or a stopped five-leaf state made only from the four supported visual
  appearances. The installed package accepted a static small scenario and a
  rule-driven large scenario, then completed Energy to Shooting Range to Energy
  switching while returning a 1440x1080 frame from the RTX 4060 Vulkan runtime.

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

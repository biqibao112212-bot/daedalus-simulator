# Daedalus Simulator board

## Current status

- `1.4.0-learning-r2` fixes the learning mode parser that treated an explicit
  `performance` value as Visible. Source commit `e3a3d5c` is on
  `release/learning-linux-1.4.0`. Default and explicit performance launches now
  run persistently in the background without a window; only `--visible` opens
  a window. The runtime smoke removed DISPLAY/WAYLAND_DISPLAY/WAYLAND_SOCKET,
  observed zero X11 windows and zero preview presentations, received consecutive
  1440x1080 RGBA frames, applied SDK aim, switched Shooting Range to large Energy,
  verified learning truth and its opt-out, and survived SIGHUP. Visible smoke
  opened a real 2560x1440 window and continued the same SDK flows.
- Rust Release tests passed `216/216`; C++ SDK CTest passed `8/8`. Native clean
  source evidence at `benchmarks/1.4.0-learning-r2/performance-release-linux.json`
  records `448.761 Hz` main update, `199.893 Hz` capture submit and `0 Hz` preview
  over 20 seconds on Ubuntu/RTX 4060. This is a machine-specific observation.
  Validation used an exact clean Git archive of `e3a3d5c` and its matching native
  artifacts, preserving the user's untracked TeX/output/tmp files in place.
- The new immutable local package is
  `/home/potato/Projects/仿真/releases/daedalus-simulator/1.4.0-learning-r2/linux-x86_64`.
  Its source is `e3a3d5c`; all `43/43` manifest hashes and both archive integrity
  checks passed. The packaged launcher also passed no-display startup, SDK
  image and truth checks. SDK remains `1.4.0-learning-r1`, TCP v1 / SHM v7 / ABI r2.
  ZIP SHA-256: `49b27877416c50ba2b5f33563ba1419d1b6b455ef9acd8f8d59806b3741c1f89`.
  TAR.GZ SHA-256: `bee7c582e226380797e0fa48a6bfd66d25f50ba3f769ff99b376cfdd68107eba`.
  Prior packages remain unchanged. This r2 package is local, not a GitHub binary Release.
- The existing source repository
  `https://github.com/biqibao112212-bot/daedalus-simulator` is PUBLIC under the
  user's explicit 2026-09-22 instruction. All four origin branch histories were
  scanned (1,563 text blobs) with no common credential-pattern hits or protected
  ML/dataset paths found. Source attribution and AGPL-3.0 remain intact; the
  user's unrelated untracked documents and runtime artifacts are not pushed.
- Retention: conclusions are in the owning context, code/docs and benchmark
  summary are public resources, formal r1/r2 releases are protected, and the
  task's source archive/staging links are reproducible temporary artifacts.
  Runtime reproduction logs and smoke results remain under
  `/home/potato/Projects/仿真/runtime/learning-headless-before-xvo98vhb`.

- `1.4.0-learning-r1` is a Linux x86_64-only learning distribution on
  `release/learning-linux-1.4.0`, derived from the final contest-r2 source
  lineage (`8bdb184`) and packaged from `2983e553`.  Its protected local
  package is at
  `/home/potato/Projects/仿真/releases/daedalus-simulator/1.4.0-learning-r1/linux-x86_64`,
  and its public download page is
  `https://github.com/biqibao112212-bot/daedalus-simulator-contest-releases/releases/tag/1.4.0-learning-r1-linux`.
  It shares the release repository under dedicated branch
  `learning/1.4.0-learning-r1`; competition assets/tags remain unchanged and
  `1.3.1-contest-r2-linux` remains that repository's Latest release.  The package
  declares `distribution_profile=learning`, `competition_eligible=false`, and
  `future_truth_included=false`, exposes truth by default only through the
  existing frame-synchronised SDK reader, and keeps the contest input/scenes.
  Its Rust learning and contest-lock regression suites, Release build, and
  installed C++ SDK CTest `8/8` passed; the package ZIP/TAR integrity checks
  passed.  Runtime smoke on Ubuntu/RTX 4060 received Shooting Range truth
  (`3` targets), switched to Energy (`1` target, `2` runes), and verified the
  opt-out (`DAEDALUS_LEARNING_TRUTH=0`) returns zero target/rune truth.  The
  old local `1.4.0-learning` draft is retained unmodified and superseded.

- Formal performance evidence was explicitly skipped for `1.4.0-learning-r1`;
  the package contains `docs/PERFORMANCE_NOT_MEASURED.md` and must not be used
  for a performance-baseline claim.

- `1.3.1-contest-r2` is published as a separate Linux x86_64-only revision at
  `https://github.com/biqibao112212-bot/daedalus-simulator-contest-releases/releases/tag/1.3.1-contest-r2-linux`.
  It is built from source commit `8fd0558`; its archive manifest contains 43
  files, both archive formats passed local integrity reads, and a tarball
  downloaded back from the public Release has SHA-256
  `b3574cfb537142aaea8b2c5594bc46f767f299f1ce5322cf48742007f2823377`.
  Simulator contest tests and all 8 installed C++ SDK tests passed. It fixes
  SDK commands in manual mode, removes the legacy auto-aim/bridge HUD fields,
  and classifies target hits against full-size armor plates only. The prior
  `1.3.1-contest-linux` Release is preserved unchanged but superseded.

- The publisher explicitly waived **formal r2 performance verification** for
  this release. The package has no performance-evidence JSON and instead
  contains `docs/PERFORMANCE_NOT_MEASURED.md`; do not make a performance
  comparison or baseline-acceptance claim from r2. `package-release.sh`
  requires the named `--skip-performance-validation` override for this state,
  so a normal package still requires clean benchmark evidence.

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

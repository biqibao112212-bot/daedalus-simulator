# Daedalus Simulator release decisions

0. Performance acceptance and consumer observation-rate diagnosis must never
   use `target\\debug\\daedalus.exe` as a substitute for a Release binary. The
   2026-08-08 controlled comparison measured 4 Hz from the debug binary versus
   184.856 Hz main update / 164.872 Hz capture from the existing release-profile
   target binary under the same DX12 headless performance configuration. The
   outstanding task is a clean, committed v1.2.0 Release rebuild and rerun;
   until then no regression in the v1.2.0 source is proven.
0a. Release packaging is gated by version-matched, clean-checkout performance
    evidence from `scripts/measure-performance.ps1`. The evidence must show
    Release profile, TCP transport, and at least 100 Hz in both main update and
    capture submit. Any later change to performance-relevant simulator,
    configuration, release, SDK, assets, or Cargo files invalidates it.
0b. Version 1.2.1 keeps SDK 1.2.0, ABI revision 2, protocol 2 and all ports
    unchanged. On Windows, UDP `ConnectionReset` and `ConnectionRefused` while
    receiving Scene Control are recoverable because they can be delayed ICMP
    responses to an ACK sent after a retrying client closed its old ephemeral
    socket. The transport must log (rate-limited) and continue; fatal receive
    failures still stop the worker.

1. Work is isolated in `D:\仿真\isolated\daedalus-simulator-multiplatform-x86`
   on branch `release/simulator-multiplatform-x86`; the canonical `main`
   checkout is never switched or written by this task.
2. The release architecture uses two native build paths: PowerShell for
   Windows MSVC and Bash for Linux GNU. A Windows WSL SDK build must not be
   reused as a Windows SDK artifact.
3. The release architecture targets x86_64 only. 32-bit i686 and ARM are not
   promised because the fixed SDK layout and GPU/runtime validation are 64-bit.
4. Release directories are version/platform/architecture scoped so Windows
   and Linux packages can coexist without replacing protected artifacts.
5. Windows defaults to DX12 for high-performance mode and Vulkan for visible
   validation. Linux defaults to Vulkan for both modes. GPU drivers remain
   system dependencies.
6. The simulator does not bundle or link CUDA, cuDNN, TensorRT, ONNX, or engine
   files. Inference versions are consumer-owned build profiles; the simulator
   package is unchanged between profiles. Joint tests must prove the actual
   consumer backend and engine hash.
7. The release candidate boundary is SDK 1.1.0, SHM v7, ABI revision 2,
   default TCP RGBA32 1440x1080 (legacy SHM RGB24), TCP 5602, UDP 5601, and
   scene control 5603. ABI revision 2
   adds frame/command identity and typed actual gimbal feedback while keeping
   the metadata region size unchanged.
8. A formal package requires a clean committed source revision, target-native
   Rust/CMake tests, a platform-specific binary/SDK pair, and a SHA256 file
   manifest. Dirty local builds are evidence for development only.
9. The Linux packager uses native Git on native Linux and falls back to
   `git.exe` only when WSL is reading a Windows-created worktree whose `.git`
   pointer contains a Windows path. This keeps local WSL packaging diagnostic
   without weakening the clean-tree gate.
10. Distribution builds compile with `distribution-release`: configuration is
    embedded, hot reload and local mutation/debug paths are disabled, network
    controls bind to loopback, and ground truth is not published.
11. Fixed camera calibration is read-only. Revision 2 records the digital
    camera geometry and fixes renderer exposure at EV100 9.7 with auto exposure
    and tonemapping disabled. Physical shutter time and analog gain do not
    apply to this digital renderer. No calibration setter or vision-result
    upload API is added.
12. wgpu owns adapter selection; the SDK reads the actual selected adapter and
    driver from the runtime capabilities record. CUDA/TensorRT remains wholly
    consumer-owned and absent from the package.
13. A read-only Agent Team audit identified same-user IPC bypass limitations
    and the repository AGPL-3.0 license. Strong anti-bypass requires OS account
    or container isolation. The active package profile is now non-commercial
    use inside the owning laboratory: it carries the repository license and an
    internal-use notice without requiring a commercial license file. External
    or public distribution remains outside this decision and requires review.
14. A complete auto-aim loop must not pair an image with an arbitrary latest
    gimbal state. Distribution builds publish an empty-target exposure record
    into the existing 16-slot history, and SDK consumers query exact exposure
    state by the TCP image `source_sequence`.
15. The Chinese user guide explains the end-to-end workflow; a separate
    `docs/SDK_API_REFERENCE_ZH.md` is the function-level source of truth for
    signatures, parameters, return values, errors, and usage examples.
16. Windows release acceptance uses Visual Studio multi-config explicitly in
    Release mode and enables `/FS` for parallel MSVC PDB writes. Headless
    performance mode must use `ExitCondition::DontExit`; otherwise the absence
    of a primary window terminates a valid release process.
17. Windows is the full runtime acceptance platform: DX12 validates the image,
    timestamp, synchronized gimbal, scene-control, and command-feedback loop;
    Vulkan validates the visible rendered window on the selected discrete GPU.
18. The internal-lab Linux gate intentionally accepts native x86_64 build/SDK
    tests plus Mesa llvmpipe software-Vulkan startup, ports, IPC, and runtime
    capability reporting. It does not require a discrete GPU and makes no
    real-time 1440x1080 RGB or complete live auto-aim performance claim.
19. Each platform archive has a root Chinese README and a non-admin per-user
    installer. Windows uses `setup.cmd` plus a PowerShell installer and Start
    menu shortcuts; Linux uses `install-linux.sh` plus user-local command
    wrappers. Installers never download GPU drivers, runtimes, inference
    frameworks, or models. A GUI Setup.exe/MSI is not claimed without a
    maintained installer toolchain.
20. Linux releases publish `tar.gz` as the primary end-user archive so Unix
    executable mode bits survive extraction; archive generation normalizes
    directories and launchers to `0755` and ordinary payload files to `0644`
    even when packaging from a Windows-mounted WSL worktree. ZIP remains a
    cross-platform fallback. Remote AutoDL acceptance downloads the exact
    GitHub Release asset through AutoDL's documented GitHub acceleration
    service instead of copying artifacts over the SSH control channel.
21. Linux performance mode is truly display-server independent: it disables
    Bevy's Winit plugin and uses a zero-delay `ScheduleRunnerPlugin` loop while
    retaining the RenderPlugin for off-screen Vulkan capture. Linux visible
    mode and the already accepted Windows runner behavior continue to use
    Winit. Requiring Xvfb for the public headless launcher is not acceptable.
22. Linux headless mode overrides Bevy's 250 ms virtual-time maximum delta with
    16 ms. This prevents one slow frame from scheduling up to 62 catch-up steps
    at the simulator's 250 Hz fixed rate and entering a persistent low-frame-rate
    spiral. Network gimbal commands are drained and applied, in that order, in
    the same `FixedUpdate`; command freshness must not depend on main-frame rate.
23. Command acknowledgement and exposure synchronization are separate SDK
    concepts. `readGimbalStateForFrame(sequence)` supplies the exposure-time
    angle/timestamp, while `readGimbalState()` supplies the latest
    `last_applied_command_id`. Release acceptance must use each interface for
    its declared role and start UDP control only after simulator readiness.
24. The approved radius experiment is implemented on the latest 1.1.1 release
    branch, never on the old main/1.0.x line and never by overwriting the frozen
    1.1.1 package. The development release is 1.2.0 / SDK 1.2.0 with Scene
    Control v2. `set_range_target_geometry` scales only the four armor roots'
    horizontal local positions, uses an absolute stock baseline, requires a
    stationary target, and resets to stock on reset/new session.
25. The consumer lock stays unchanged until a clean 1.2.0 source commit,
    native CMake/CTest SDK validation, formal package manifest/hashes, and
    runtime acceptance are complete. Dirty binaries are development evidence,
    not a consumer Release contract.
26. Decision 1's old isolated-worktree location is superseded. Current release
    work is owned by `D:\仿真\repos\daedalus-simulator` on
    `release/simulator-multiplatform-x86`; the task must not switch to an
    unrelated main branch or create a temporary worktree.
27. Offline exact-corner rows travel as an internal sidecar with the exact GPU
    capture/TCP frame. They are appended only after the existing TCP v1 header
    and full payload complete a write to an active connection. Mailbox replace,
    reject, no-client, partial-write, disconnect, and identity mismatch never
    write a row. The TCP wire, SHM v7, ABI r2, and ports stay unchanged.
28. Schema v1 exports only active Shooting Range #3 small armor. Nominal
    135 x 55 mm is retained as a specification field, while exact projection
    and free-IPPE use `vehicle.glb` marker vertices (asset SHA256
    `1cc0a3cd1ab05bc9822b616271db3afb64d078e56b9bbf452a8acc6d9bad0a6f`),
    measured about 133.77 x 53.89 mm and tilted about 15 degrees. The #1
    HERO large armor is outside v1 rather than falsely relabeled.
29. `motion_uniform` is a conservative training filter with a fixed 100 ms
    guard and 1 mm endpoint epsilon. It freezes same-exposure ROS-odom linear
    and angular velocities, marks initialization/state changes false, and
    excludes deterministic reciprocal endpoints and both reversal
    neighborhoods. It exports no future pose/command and always states
    `future_truth_included=false`.
30. The public offline schema/contract is a backward-compatible minor
    capability, so simulator and SDK advance to 1.3.0 while SHM v7, ABI r2,
    TCP protocol 1, Scene Control 2, and ports 5601/5602/5603 remain frozen.
    Distribution online target/rune truth remains locked to zero even while
    the explicit offline exporter is active.
31. Labels, TCP identity ledgers, raw RGBA frames, experiment manifests, and
    formal Releases are protected assets. Packaging carries only the schema,
    bilingual contracts, collector, and validator and rejects JSONL/raw capture
    payloads. Failed and successful development experiment sessions under
    `D:\仿真\runtime\corner-label-1.3.0-dev-*` are retained rather than cleaned.
32. Formal performance evidence must hash the exact
    `talos,distribution-release` MSVC binary with offline export disabled.
    Frequency-only acceptance telemetry uses
    `TALOS_PERFORMANCE_EVIDENCE_JSON`; it does not relax the distribution
    `DAEDALUS_*` allowlist or expose target truth.
33. Windows 1.3.0 formal performance evidence is bound to clean implementation
    commit `988cc11eff8180f02423dbfcde414d88058d4686`, the exact MSVC
    `talos,distribution-release` binary SHA256
    `df6b109d87f7808e4b8e2d4429bdba755650c5b86b87df83660a3f5c78111efa`,
    and a disabled exporter. The 20-second measurement reached 207.210 Hz
    main update and 190.275 Hz capture submit; it is the package gate, while
    protected opt-in label experiments remain separate acceptance evidence.
34. Release validation exposed that screen-canonical `bl,tl,tr,br` ordering
    cannot identify a physical marker's long axis: a plate may face the camera
    such that the screen-vertical edge is the physical width. Exported
    `measured_width_m` and `measured_height_m` therefore derive from the
    long/short opposing-edge spans of the true quadrilateral, not fixed index
    pairs. The rejected first package and ZIP are retained as protected failed
    evidence rather than overwritten; all subsequent 1.3.0 evidence must be
    regenerated after this fix.
35. Schema v1 treats a labeled target exposure as atomic Z4 supervision. If
    any one of the four #3 small armors fails real projection or screen-order
    validation, the exporter writes no rows for that target exposure. This
    fail-closed policy prevents consumers from receiving partial slots. Generic
    planar IPPE validates the real asset's non-coplanar marker geometry with a
    published `0.025 px` RMS / `0.125 mm` equivalent default bound; it is not
    a substitute for or approximation of the exact rendered corner labels.
36. The accepted Windows x86_64 `1.3.0` package is immutable at
    `D:\仿真\releases\daedalus-simulator\1.3.0\windows-x86_64`, manifest
    source commit `2bce032ebcc55bfc4cfa0e6e793802a55ea22c70`. The failed
    width/height and partial-Z4 packages remain separately retained as
    protected evidence. Consumers receive the accepted package, schema, and
    SDK; they do not receive labels, raw frames, or private Agent Team context.
37. Linux performance evidence is target-native and distinct from Windows
    evidence: each package validates the SHA256 of its own Release binary and
    declares its Rust target. Linux `1.3.0` high-performance acceptance uses
    the accepted Windows `1.3.0` frequency values (171.190 Hz main update and
    163.228 Hz capture submit) as explicit minima on the same approved Ubuntu
    GPU host. Visible Vulkan measurements are retained as runtime evidence but
    are not confused with the package's headless capture gate.
38. A distribution binary launched directly from `target/<triple>/release`
    must be able to resolve the immutable source assets for clean-checkout
    measurement, while an installed package must still prefer its adjacent
    `<package>/assets` directory. The asset resolver therefore tries the
    package layout first for distribution builds and falls back to current
    source/manifest assets only when that package directory is absent. This
    prevented false 0-Hz capture evidence caused by an empty target directory.
39. Release 1.3.1 adds the simulator-owned full-frame companion to the 1.3.0
    exact-corner sidecar. It is collector-only, default-off and requires
    `--save-rgba-frames --until-eof`; every complete TCP RGBA32 payload is
    create-new persisted under `frames/`, hash-bound to the existing identity
    ledger, and summarized by `daedalus.offline-frame-capture/1`. The validator
    must receive `--require-raw-frames` before a session can be called an
    image-to-label training asset. TCP v1/SHM v7/ABI r2/ports and distribution
    online target truth remain unchanged; raw payloads remain protected runtime
    assets and are never packed. This is the sole supported full-frame path for
    consumers; a consumer TCP parser is outside its module boundary.
40. Repository-local build products follow a single-Linux-version rule. The
    only retained build is `x86_64-unknown-linux-gnu` Release plus its matching
    Linux SDK build/install. A build stamp binds HEAD, toolchains, target,
    profile and features; an exact match is reused without compilation, while
    a changed key replaces the old build first. Windows, debug, incremental,
    alternate-target and prior-revision products are reproducible caches and
    must not remain in the canonical checkout. Git provides historical source;
    formal Release packages and protected runtime evidence remain immutable and
    are explicitly outside this cache rule.
41. `1.3.1-contest` is a separate Linux x86_64-only maintenance branch for an
    internal laboratory algorithm competition. Its distribution build enables
    `contest-release` in addition to `distribution-release`, rejects Normal
    Map/Outpost runtime requests, defaults to an allowed map and
    exposes C++17 `ContestClient` plus a participant launcher. The stable
    transport ABI remains TCP v1, SHM v7, ABI r2 and Scene Control v2.
42. The contest client is a participant-facing visible simulator rather than a
    command-only production distribution. It therefore enables local controlled
    vehicle input while preserving SDK transport: WASD/left Shift movement,
    Q/E scene control, arrows/right-mouse gimbal and Space firing. Auto aim is
    disabled at contest startup so it cannot silently take gimbal ownership;
    ordinary distribution releases keep their SDK-command-driven lock.
43. The contest energy scene has a dedicated, deterministic participant spawn:
    the flat west apron at `(-4.0, 0.0, -5.6)`, yaw `PI`, and the proven range
    camera pitch of `-27°`. Its local root rotation is locked so terrain contact
    cannot overturn the vehicle; Q/E continues to rotate the chassis child.
    The visible HUD is ASCII rather than Chinese because the shipped default
    font lacks CJK glyphs, and a missing-glyph prompt is worse than an English
    prompt in this internal competition client.
44. In `1.3.1-contest`, Q/E is scene-specific to keep the participant control
    surface small and meaningful: Shooting Range uses it for chassis yaw;
    Energy uses Q for clockwise and E for counter-clockwise large-rune motion.
    The Red/Blue mechanism faces receive opposite local directions, preserving
    their paired physical rotation. The C++ `ContestClient` must only select
    the Energy scene and never submit an empty explicit `RuneState`: that API
    represents a frozen inspection snapshot and blocks hit-driven rounds. The
    scene's native large-rune state machine instead starts two active leaves,
    advances/reset on hit/timeout, and remains the competition default.
45. Decision 44 is superseded for the Energy scene: Q selects the small rune
    and E selects the large rune. Neither key changes the selected game
    direction, because the rules require opposite red/blue directions fixed
    through a game. The C++ RuneScenario interface supersedes empty RuneState
    snapshots for annotation: it provides rules-driven small/large behaviour,
    or stopped rotation with exactly five pre-defined leaf appearances.
46. `1.3.1-contest` exposes a read-only per-face large-rune score through
    Scene Control v2 and the C++17 `ContestClient`. A valid collision on a lit
    large-rune leaf contributes the physics engine's actual contact point in
    the target local plane. The 300 mm effective detection diameter is mapped
    deterministically to ten equal 15 mm radial bands (ring 10 at centre,
    ring 1 at the edge), because the public rule identifies ten rings and
    1 mm radial contact precision but does not publish a textual radial-band
    table. Each response contains run identity/activity, activated-arm count,
    average ring and the latest impact; it never permits callers to forge
    score, target state or truth.
47. The contest client exposes large-rune score in the existing bottom-left
    projectile-statistics HUD instead of a separate panel: it follows the
    `pct` field and reports Red/Blue valid-arm count, average ring and last
    ring. To preserve the concise client control surface, the HUD segment is
    present only on the Energy map while the large rune is selected; SDK and
    launcher queries remain available for all read-only score inspection.
48. `1.3.1-contest-r3` exposes a read-only latest-valid-armor-hit event through
    Scene Control v2 and the C++17 `ContestClient`. The event is appended only
    after the existing full-size authoritative armor-plane collision predicate
    increments accurate-hit statistics. It carries a monotonic event ID,
    optional projectile trace ID, scored armor identity and cumulative accurate
    count. It deliberately excludes misses, unhit armor, target pose and any
    target enumeration, so it is participant hit feedback rather than an
    online-truth interface.

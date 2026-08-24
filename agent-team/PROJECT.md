# Daedalus Simulator project context

- Protocol: `agent-team-fixed/v2`
- Repository: `D:\仿真\repos\daedalus-simulator`
- Active branch: `release/contest-linux-1.3.1`
- Frozen implementation baseline at task start: `48b9437c389c2911e0a135cf1d727e36a68317ab`
- Frozen formal release: simulator `1.2.1`, SDK `1.2.0`, SHM v7 / ABI revision 2
- Active Linux release target: simulator/SDK `1.3.1`, adding collector-owned
  offline full-frame export without changing the real-time SDK ABI

## Contest release target

- `1.3.1-contest` is a separately maintained Linux x86_64-only internal
  laboratory competition line derived from the accepted Linux `1.3.1` source.
- Its runtime exposes only Shooting Range and Energy Mechanism. Energy supports
  both small and large rune modes; Normal Map and Outpost are rejected by the
  release binary rather than merely hidden by its launcher.
- The supported participant API is C++17 `ContestClient`, an SDK facade over
  the existing image, exposure-synchronised gimbal, UDP command and Scene
  Control contracts. It does not add target truth or algorithm interfaces.
- `1.3.1-contest-r4` adds the read-only `get_latest_armor_hit` Scene Control
  operation, mapped to `ContestClient::getLatestArmorHit()`. It reports only
  the latest valid vehicle-armor score with a monotonic event ID and target
  identity after the existing authoritative collision decision; it never
  reports misses, unhit armor, target pose, or arbitrary target truth.
- The visible contest client starts with automatic aim disabled and supports
  direct local vehicle control: WASD/left Shift movement, arrows or
  right-mouse gimbal motion, and Space firing. Q/E controls chassis yaw in
  Shooting Range and selects small/large rune mode in the Energy Mechanism.
  The C++ RuneScenario surface supports only rules-driven rotation or a
  stopped, five-leaf selection from existing visual states for annotation.

## Ownership and boundaries

This repository exclusively owns the Rust simulator, rendered and rigid-body
geometry, scene controls, image capture, simulator truth, public SDK/contracts,
packaging, and formal Release artifacts.

The consumer repository is read-only and out of scope. Do not copy simulator
implementation into it or read its private Agent Team context. Cross-repository
information is published only through versioned schemas, contracts, SDKs,
Release manifests, and consumer locks.

The formal `1.2.1` Release under
`D:\仿真\releases\daedalus-simulator\1.2.1` is immutable. Models, labels,
raw captures, datasets, and every formal Release are protected assets.

## Repository-local build retention

- The canonical checkout may retain exactly one usable build: Linux x86_64,
  target `x86_64-unknown-linux-gnu`, profile `release`, with the matching Linux
  SDK build/install under `build/release/linux-x86_64`.
- Never retain repository-local Windows, debug, incremental, alternate-target,
  or multiple-revision build products. Git owns source history; an old binary
  is rebuilt from its commit when genuinely needed, not kept beside the active
  build.
- `scripts/build-release.sh` is the only persistent-build entry point. It
  reuses a complete build when HEAD, toolchains, target, profile and features
  match its stamp. A changed key replaces the old Linux build before compiling.
  Do not bypass the reuse check or repeat a successful matching build.
- Formal packages under `/home/potato/Projects/仿真/releases` and protected
  runtime evidence are not repository-local build caches and remain immutable.

## Approved offline-label capability

The offline exact-corner JSONL exporter is explicit opt-in and disabled by
default. Every written row must be joined fail-closed to the same exposure as
the real TCP image by `(producer_epoch, frame_seq, timestamp_ns)`. Corners come
from the simulator's actual rendered/rigid-body geometry and exposure camera,
never from detections, hand labels, motion commands, future truth, or a consumer
approximation. Distribution builds keep online ground truth locked with
`target_count=0`; labels are a sidecar file and never an online detector/PnP/
predictor input.

## Public dependency references

- Approved consumer proposal: independent Git reference
  `e721b26:modules/autoaim/docs/corner_repair_image_training_data_proposal.md`
- Runtime Release contract: `release/release.json`
- SDK contract: `sdk/contract.json`
- Camera calibration: `release/camera-calibration.json`
- Scene Control: `docs/SCENARIO_CONTROL.md`
- Offline exact-corner contracts:
  `docs/OFFLINE_EXACT_CORNER_EXPORT.md`,
  `docs/OFFLINE_EXACT_CORNER_EXPORT_ZH.md`, and
  `sdk/schemas/offline-exact-corners-v1.schema.json`; full-frame capture adds
  `sdk/schemas/offline-frame-capture-v1.schema.json`
- Release/performance gates: `RELEASE.md`, `SIMULATOR_PERFORMANCE.md`

## Stable validation commands

```text
cargo fmt --all -- --check
cargo test --locked --release --target x86_64-unknown-linux-gnu --features talos,distribution-release,contest-release
cargo clippy --locked --release --target x86_64-unknown-linux-gnu --features talos,distribution-release --all-targets -- -D warnings
scripts/check-compatibility.ps1
scripts/build-release.ps1 -Platform windows -Arch x86_64
scripts/measure-performance.ps1 -DurationSeconds 20
scripts/package-release.ps1 -Platform windows -Arch x86_64
bash scripts/build-release.sh
bash scripts/measure-performance.sh --mode performance --duration-seconds 20
bash scripts/measure-performance.sh --mode visible --duration-seconds 20
bash scripts/package-release.sh
```

Persistent repository builds use `bash scripts/build-release.sh`; direct Cargo
commands above are validation primitives and must not be rerun when the stamped
Linux Release build already proves the same command/input boundary.
PowerShell/Windows build commands remain source-controlled for on-demand native
release reconstruction, but their repository-local outputs are disposable and
must be removed before task completion; they are never a retained second build.

Compatibility, performance, and formal Release claims require the exact clean
committed revision. Development builds and dirty-tree runs are implementation
evidence only.

## Current evidence boundary

- Accepted Windows package: `D:\仿真\releases\daedalus-simulator\1.3.0\windows-x86_64`.
  Its manifest source commit is `2bce032ebcc55bfc4cfa0e6e793802a55ea22c70`.
- Clean performance evidence is `benchmarks/1.3.0/performance-release.json`,
  bound to implementation commit `1ba59ad`; it records `171.190 Hz` main and
  `163.228 Hz` capture submit with distribution mode and exporter disabled.
- Protected accepted package-runtime evidence is retained under
  `D:\仿真\runtime\corner-label-1.3.0-release-*-final*`; the opt-in retry
  received 525 TCP frames, exported 1,756 labels over 439 complete Z4
  exposures, retained target/rune counts at zero, and validated free-IPPE at
  `0.000866109666 px` / `4.44063144e-6 m` maximum.
- Failed package attempts are separately preserved under
  `D:\仿真\releases\daedalus-simulator\failed-evidence`; Linux remains an
  unrun target-native Release gate.

- Worktree implementation tests: Rust distribution-release `195/195` and
  `talos-ipc 7/7` passed.
- Dirty-tree native development build: MSVC Release binary built; SDK CTest
  `7/7` passed. This is not formal Release evidence.
- Protected dirty-tree runtime experiment:
  `D:\仿真\runtime\corner-label-1.3.0-dev-20260811T162716Z`.
  It received 268 real TCP frames, exported 1064 rows for 266 exposures,
  proved online `target_count=0/rune_count=0`, contained both certified and
  excluded motion rows, and closed free-IPPE at maximum
  `0.000682232101 px` / `2.74679494e-6 m` equivalent error.
- Formal performance, package, manifest, and package-runtime evidence must be
  regenerated after the implementation commit from a clean tree.

## Linux 1.3.1 full-frame export (2026-08-14)

- Commit `b1fe340` adds a backward-compatible, explicit Release collector
  option `--save-rgba-frames` (only with `--until-eof`). For every complete
  RGBA32 TCP identity it writes a create-new raw file, ledger relative path and
  matching SHA-256, then writes a no-truth capture manifest. The new validator
  gate `--require-raw-frames` verifies every raw file before label join.
- The public TCP v1, SHM v7, ABI r2, Scene Control v2 and online truth lock are
  unchanged. Raw frames remain collector-owned protected assets, never package
  payloads; consumers consume the Release collector rather than implementing
  TCP capture.
- Clean Linux performance evidence is `benchmarks/1.3.1/performance-release-linux.json`
  from `b1fe340`: `357.044 Hz` main and `198.468 Hz` capture submit. The
  package source is `d7637d0`, adding only that evidence. Package smoke
  `corner-repair-linux-1.3.1-full-frame-smoke-20260814-02` retained 2,482 raw
  identities, 9,108 labels and passed full-frame/Z4/free-IPPE validation.

## Contest big-rune scoring (2026-08-15)

- `1.3.1-contest` adds `get_big_rune_score` to Scene Control v2 and maps it to
  `SceneControlClient::getBigRuneScore(RuneTeam)` and
  `ContestClient::getBigRuneScore(RuneTeam)`. It is read-only and is available
  through the participant launcher as `daedalus-contest score red|blue`.
- The mechanism records only a valid collision with a currently lit large-rune
  leaf during a rules-driven activation. The result carries the per-face run
  identity, whether it is active, valid activated-arm count, average ring and
  most recent contact. No online ground truth, target mutation or arbitrary
  score injection is introduced.
- The public RM 2026 rule defines a 300 mm effective detecting diameter,
  ten rings, and 1 mm radial contact accuracy. It does not publish a textual
  numerical table for the ring radii. The contest simulator therefore makes
  its reproducible interpretation explicit: ten equal 15 mm radial bands,
  centre=10 and outer edge=1. This is implementation-defined mapping, not an
  assertion that the rule text specifies those exact boundaries.

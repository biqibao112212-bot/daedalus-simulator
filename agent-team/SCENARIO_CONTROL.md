# Simulator scenario-control contract

- Owner: `integration/perf-main`
- Contract status: `legacy-0`
- Reference revision: `2fa0de2feeb1`

This is the public description of how consumers select scenes and control test
targets. Read the canonical copy with:

```powershell
git show integration/perf-main:agent-team/SCENARIO_CONTROL.md
```

## Existing legacy controls

The committed baseline exposes environment-variable configuration, including:

- scene selection through `DAEDALUS_SCENE_MODE` or
  `DAEDALUS_AUTO_AIM_MODE`;
- capture profile through `DAEDALUS_CAPTURE_SCENE_PROFILE`;
- Talos dimensions and transport through `DAEDALUS_TALOS_WIDTH`,
  `DAEDALUS_TALOS_HEIGHT`, `DAEDALUS_TALOS_IMAGE_TRANSPORT`, and TCP settings;
- deterministic auto-generation seed through `DAEDALUS_AUTO_GEN_SEED`;
- armor/energy generation mode, distance/yaw/pitch sweeps, jitter, settle
  frames, camera height, lighting, rune mode/team/state/targets;
- shooting-range target distance, active target number, target pose, and
  initial yaw controls;
- energy scene player and gimbal initial pose controls.

Exact accepted values and defaults remain implementation-defined in
`src/auto_gen.rs`, `src/setup.rs`, `src/capture.rs`, and
`src/talos/plugin.rs`. These environment variables are legacy configuration,
not yet a stable remote-control API.

## Required v1 controls

The stable control API must support deterministic session creation, scene
selection, reset/step/run, random seed, player/camera/gimbal initial state,
target spawn/despawn, target identity/team/type, pose and motion profile,
armor/rune state, active target selection, lighting/capture profile, and
ground-truth/telemetry subscription. Each request needs an acknowledgement and
defined error response.

Consumers must use this public API once v1 is released. They must not mutate
simulator internals, depend on entity names, or require consumer-specific scene
patches.

# Scene Control v2

The 1.2.0 research release uses `daedalus.scene-control/2` on UDP
`127.0.0.1:5603`. Every request keeps the existing envelope fields:
`protocol`, `command_id`, `session_id`, `op`, and `args`.

## Range target geometry

`set_range_target_geometry` changes only the horizontal orbit radius of the
four armor centers on a shooting-range target. It does not scale the vehicle,
armor dimensions, textures, collision shape dimensions, or rigid-body motion.

```json
{
  "protocol": "daedalus.scene-control/2",
  "command_id": 42,
  "session_id": "stage3-radius-08",
  "op": "set_range_target_geometry",
  "args": {"target": 3, "radial_scale": 0.8}
}
```

`target` is `1` or `3`. `radial_scale` is finite and must be in `[0.75, 1.25]`;
`1.0` is stock geometry. The target must be stationary before changing its
geometry. The scale is absolute relative to stock geometry, so repeating the
same command does not compound the previous scale. A scene reset constructs
stock geometry again.

The simulator applies the scale to the four armor-root transforms after the
scene hierarchy is ready. The next propagated frame is the first frame whose
rendering, collision hierarchy, and internal ground-truth geometry share the
new positions. The command response's `applied_frame_seq` identifies the
command-processing frame; collectors should wait for the next complete frame
and verify `armor_count == 4` plus the four relative positions and radii.

## Energy-mechanism annotation scenarios

The set_rune_scenario operation provides a deliberately bounded configuration
surface for annotation and algorithm evaluation. It supports the real small and
large energy mechanisms, but does not permit arbitrary material, colour, mesh,
or truth-data changes.

Rule-driven mode retains the simulator's official mechanism model. Small
mechanism and inactive large mechanism use pi/3 rad/s; an activated large
mechanism uses its per-round a*sin(omega*t)+b model. Red and blue faces are
oppositely directed. The caller can select one of the two official initial
directions, while normal contest keyboard use preserves the game-selected
direction for the whole round.

~~~json
{
  "protocol": "daedalus.scene-control/2",
  "command_id": 43,
  "session_id": "rune-labels-01",
  "op": "set_rune_scenario",
  "args": {
    "mode": "large",
    "motion": "rule",
    "direction": "clockwise",
    "leaf_states": []
  }
}
~~~

Static mode stops rotation and accepts exactly five pre-defined visual states,
one per leaf: deactivated, activating, activated, or completed. It is intended
for a reproducible labelled frame, not for overriding the rules-driven
hit/lifecycle state machine.

~~~json
{
  "protocol": "daedalus.scene-control/2",
  "command_id": 44,
  "session_id": "rune-labels-01",
  "op": "set_rune_scenario",
  "args": {
    "mode": "small",
    "motion": "static",
    "direction": "counter_clockwise",
    "leaf_states": [
      "activating", "activated", "completed", "deactivated", "deactivated"
    ]
  }
}
~~~

The C++17 SDK exposes this as RuneScenario and
SceneControlClient::setRuneScenario; ContestClient forwards the same bounded
method. The legacy set_rune_state remains available for compatible non-contest
tooling, but new annotation tooling should use RuneScenario.

## Large-rune ring score

`get_big_rune_score` returns the read-only score for one face's current (or
most recently completed) rules-driven large-rune activation. It is intended
for evaluation and annotation bookkeeping, not for changing the mechanism.
Only valid impacts on currently active large-rune leaves contribute. The
simulator uses the physics contact point in the target plane, maps the 300 mm
effective detection diameter into ten equal 15 mm radial bands, and returns
ring `10` at the centre through ring `1` at the outer edge. The public rule
specifies the ten rings and 1 mm radial contact accuracy; the equal-band
boundary mapping is the contest simulator's explicit, reproducible mapping.

~~~json
{
  "protocol": "daedalus.scene-control/2",
  "command_id": 45,
  "session_id": "rune-labels-01",
  "op": "get_big_rune_score",
  "args": {"team": "red"}
}
~~~

The successful response includes a `data` object with `run_id`, `run_active`,
`activated_arms`, `has_hit`, `average_ring`, `last_ring`, `last_radius_mm`, and
`last_target`. `activated_arms` is the number of valid lit-arm hits in this
activation (5–10 for a completed large-rune cycle under the native rules);
`average_ring` is zero before the first valid hit. C++ users call
`SceneControlClient::getBigRuneScore(RuneTeam::Red)` or the matching
`ContestClient` facade.

## Compatibility

The frozen 1.1.1 package and Scene Control v1 remain unchanged. Consumers must
update to the 1.2.0 SDK and lock before sending v2 requests. Ground-truth
fields remain ABI-compatible; distribution-locked packages may still publish
empty ground-truth batches, so geometry validation belongs to the controlled
research build and its evidence manifest.

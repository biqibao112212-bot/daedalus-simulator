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

## Compatibility

The frozen 1.1.1 package and Scene Control v1 remain unchanged. Consumers must
update to the 1.2.0 SDK and lock before sending v2 requests. Ground-truth
fields remain ABI-compatible; distribution-locked packages may still publish
empty ground-truth batches, so geometry validation belongs to the controlled
research build and its evidence manifest.

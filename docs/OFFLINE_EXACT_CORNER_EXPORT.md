# Offline same-exposure exact-corner export

Status: public research contract for Daedalus Simulator 1.3.0.

## Boundary

The exporter is a write-only offline sidecar. It is disabled unless
`DAEDALUS_CORNER_LABELS_JSONL` names a new absolute `.jsonl` file outside the
simulator executable/Release tree. The parent directory must already exist and
the file must not exist; the simulator never overwrites labels.

This option does not unlock real-time truth. Distribution builds still publish
`target_count=0` and `rune_count=0` through the SDK, and the TCP image protocol,
SHM v7 / ABI revision 2, and ports 5601/5602/5603 are unchanged. Labels never
enter the detector, PnP, predictor, or gimbal-control input path.

## Exact exposure identity

Each JSONL row describes one small armor plate in a TCP frame identified by
`(producer_epoch, frame_seq, timestamp_ns)`. Geometry and camera state are
frozen in the same Bevy `ExtractSchedule` snapshot that creates the GPU image.
The private label sidecar then follows that image through GPU readback and the
latest-only TCP mailbox. Rows are appended only after the complete TCP header
and RGBA payload have been written to an active client.

Consequently, an absent client, rejected frame, mailbox replacement, GPU
capture failure, partial socket write, disconnect, or identity mismatch writes
no label. A label identity is therefore always a subset of actually sent TCP
image identities; use the triple as the only join key.

## Geometry and corner order

Schema v1 exports only the active Shooting Range #3 target's small armors. The
public nominal specification is 135 x 55 mm, but exact pixels use the real
`*_ARMOR_MARKER` vertices embedded in `assets/vehicle.glb`, SHA256
`1cc0a3cd1ab05bc9822b616271db3afb64d078e56b9bbf452a8acc6d9bad0a6f`.
Those asset quads measure approximately 133.77 x 53.89 mm, are non-rectangular/
non-coplanar by about 4-5 micrometres, and are tilted approximately 15 degrees.
The row publishes both nominal dimensions and the actual four
`object_corners_armor_m`; an exact free-IPPE closure must use the latter.
The marker is preserved as authored (about 4--5 um non-coplanar), so generic
planar OpenCV IPPE is an independent closure check rather than an algebraic
inverse at grazing/out-of-frame views. The supplied validator defaults to a
`0.025 px` RMS / `0.125 mm` equivalent bound; use stricter
`--max-reprojection-px` or `--max-equivalent-error-m` values only for a
restricted capture geometry with supporting evidence.

The #1 Shooting Range target uses `HERO.glb` and approximately 228.77 x 53.89
mm large armor. It is intentionally outside schema v1 and is never mislabeled
as 135 x 55 mm.

`exact_corners_px` and `object_corners_armor_m` use the same screen-canonical
order `bl,tl,tr,br`, where image x points right and image y points down. Pixels
retain subpixel precision and may lie outside the image. A row is dropped when
any corner is behind the camera, non-finite, or degenerate.

`visibility` is a conservative geometry/frustum description. It reports the
scene-hidden state and how many corners lie inside the image; the high-rate TCP
path does not perform a depth occlusion test, so `occlusion_tested=false` must
not be interpreted as an unobstructed-visibility claim.

## Motion fields

Velocities are the current prescribed kinematic rigid-body derivatives frozen
at the same exposure and converted to ROS odom axes, in m/s and rad/s.
`distance_m` is from the exposure camera centre to the asset-marker centre.

`motion_uniform` uses a fixed 100 ms guard. First observations and motion-state
changes are false for the guard duration. For reciprocal translation, a sample
is true only when its current distance to the nearest deterministic endpoint is
strictly greater than `speed * 0.100 s + 0.001 m`; thus the endpoint, reversal,
and deterministic neighborhoods on both sides are excluded. Stable stationary
and constant-spin segments can be true. No future pose, future command, motion
instruction approximation, or endpoint-prediction model is exported, and every
row fixes `future_truth_included=false`.

## Usage

Create a dedicated runtime directory and choose a new file:

```powershell
$session = 'D:\仿真\runtime\corner-label-session-001'
New-Item -ItemType Directory -Path $session
Set-Location D:\仿真\releases\daedalus-simulator\1.3.0\windows-x86_64
.\start-simulator.ps1 -CornerLabelsJsonl (Join-Path $session 'exact-corners.jsonl')
```

In a second terminal, run the shipped collector. It creates a Scene Control v2
session, selects Shooting Range #3, commands a short reciprocal motion, reads
complete TCP frames, and records every wire identity plus the RGBA payload hash:

```powershell
D:\Anaconda\envs\yolov8\python.exe .\docs\capture-corner-label-experiment.py `
  --output-dir D:\仿真\runtime\corner-label-session-001 `
  --until-eof --linear-span-m 0.6 --save-first-rgba
```

Let it run for at least several reciprocal periods, then stop the simulator.
The collector drains every complete frame already present on the TCP
connection to EOF before it closes its identity ledger. This coordinated stop
is required for a strict received-frame coverage check; a fixed-frame client
can intentionally leave later socket-buffered frames unrecorded.

Without an active TCP client the label file remains empty. After collection,
validate schema/identity/Z4/motion exclusion and generic free-IPPE closure:

```powershell
D:\Anaconda\envs\yolov8\python.exe .\docs\verify-corner-label-export.py `
  D:\仿真\runtime\corner-label-session-001\exact-corners.jsonl `
  --tcp-identities D:\仿真\runtime\corner-label-session-001\tcp-identities.jsonl `
  --require-complete-z4 --require-uniform-and-excluded
```

The optional identity file contains one received TCP triple per JSON line. The
validator checks the shipped schema contract, fields, asset hash, uniqueness,
absence of future truth, and generic OpenCV `SOLVEPNP_IPPE`
reprojection/equivalent metric closure. The JSONL, identity ledger, raw frame,
and experiment directory are protected collection assets and are never packed
into the simulator Release.

Schema: `schemas/offline-exact-corners-v1.schema.json`.

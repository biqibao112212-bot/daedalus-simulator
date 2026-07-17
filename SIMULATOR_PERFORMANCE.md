# Simulator performance runbook

This file is the reproducible performance baseline for `integration/perf-main`.
Use it instead of editing `config.toml` for a measurement run.

## Latest verified measurement

| Item | Value |
| --- | --- |
| Date | 2026-07-17 |
| GPU/backend | NVIDIA RTX 4060 Laptop GPU / DX12 |
| Build | optimized `cargo build --release --features talos` |
| Scene | normal simulator scene, auto-aim subscription enabled, managed WSL bridge disabled |
| Sample duration | 36.33 s after startup |
| Render FPS | 203.15 Hz |
| Physics step rate | 249.95 Hz |
| Talos RGB/publish rate | 197.17 Hz |
| Capture errors | 0 queue drops, 0 fast-buffer drops, 0 map errors; 1 non-blocking publisher-lock drop |
| Fixed control cadence | 250 Hz |
| Avian inner substeps | 1 |
| Talos RGB capture cadence | 200 Hz |
| Presentation | disabled; shadows, FXAA, diagnostics, and Egui disabled |
| Capture path | RGB-only; no depth/dataset pass |
| Measurement output | `artifacts/performance/latest.json` |

The expected acceptance threshold is at least 150 render FPS while preserving
250 Hz fixed control and approximately 200 Hz Talos RGB capture. Update the
table with the actual final values after every intentional renderer, physics,
capture, or Bevy upgrade.

## Exact Windows/PowerShell command

Run from the repository root. `config.performance.toml` leaves the normal
interactive `config.toml` untouched.

```powershell
cargo build --release --features talos

$env:BEVY_ASSET_ROOT = (Get-Location).Path
$env:WGPU_BACKEND = 'dx12'
$env:WGPU_POWER_PREF = 'high'
$env:DAEDALUS_CONFIG = 'config.performance.toml'
$env:DAEDALUS_PERF_DISABLE_UI = '1'
$env:DAEDALUS_TALOS_RGB_ONLY = '1'
$env:DAEDALUS_TALOS_CAPTURE_MAX_HZ = '200'
$env:DAEDALUS_AUTO_AIM_ON_START = '1'
$env:DAEDALUS_STATS_JSON = 'artifacts/performance/latest.json'
New-Item -ItemType Directory -Force artifacts/performance | Out-Null
.\target\release\daedalus.exe
```

Let it warm up for at least 10 seconds and sample for at least 30 seconds.
Close the process normally, then inspect `artifacts/performance/latest.json`.
The file is rewritten every 100 ms and contains `render_fps`, `physics_step_hz`,
`talos_frame_fps`, `capture_copy_submit_hz`, and all capture/drop counters.

## Required acceptance checks

```powershell
$r = Get-Content artifacts/performance/latest.json -Raw | ConvertFrom-Json
$r.render_fps
$r.physics_step_hz
$r.talos_frame_fps
$r.capture_fast_no_buffer_drop_total
$r.capture_fast_map_error_total
```

Accept only when all conditions hold:

1. `render_fps >= 150`.
2. `physics_step_hz` is near 250 Hz (within the test's normal scheduling jitter).
3. `talos_frame_fps` and/or `capture_copy_submit_hz` are near the requested
   200 Hz capture cadence.
4. `capture_fast_no_buffer_drop_total` and `capture_fast_map_error_total` do
   not grow unexpectedly. Latest-frame replacement may increase under bounded
   backpressure; it must not turn into an unbounded queue.

## What each performance switch does

| Setting | Measured value | Why |
| --- | --- | --- |
| `fixed_hz` | `250` | Keeps the control/physics timebase at 4 ms. |
| `substep_count` | `1` | High-throughput solver profile; increase only after separately validating physics fidelity. |
| `DAEDALUS_TALOS_CAPTURE_MAX_HZ` | `200` | Limits source captures to one frame per 5 ms without catch-up bursts. |
| `DAEDALUS_TALOS_RGB_ONLY` | `1` | Removes depth/dataset capture work for a color-stream benchmark. |
| `preview.enabled` | `false` | Removes the visible preview pass. A black/no preview window is expected; off-screen capture continues. |
| `DAEDALUS_PERF_DISABLE_UI` | `1` | Disables Egui, inspector, and scene panels. |
| `shadows`, `main_camera_fxaa`, `diagnostics` | `false` | Avoids optional presentation/diagnostic GPU and CPU cost. |
| `present_mode` | `immediate` | Does not wait for VSync. |
| `managed_bridge_enabled` | `false` | Prevents WSL/TensorRT startup from contaminating renderer measurements. |
| `WGPU_BACKEND`, `WGPU_POWER_PREF` | `dx12`, `high` | Pins the Windows GPU backend and requests the discrete high-power adapter. |

## Important distinctions

- A black visible window during this profile is **not** evidence that capture
  failed; it is the intended result of disabling preview. Inspect Talos output
  or the stats JSON to verify the off-screen pipeline.
- This is a throughput profile, not a data-quality acceptance test. Before
  collecting auto-aim data, separately verify a visible target, exposure, and
  image content.
- Do not compare Debug builds with these numbers. Measure only the optimized
  Release executable and record hardware, backend, exact config, and duration.

# Simulator public interface

- Owner: `integration/perf-main`
- Contract status: `legacy-0`
- Reference revision: `2fa0de2feeb1`
- Stability: descriptive only; not a compatibility guarantee

This document is the canonical consumer-facing interface record. Consumers
should read it with:

```powershell
git show integration/perf-main:agent-team/SIMULATOR_INTERFACE.md
```

## Committed legacy baseline

At the reference revision, the Rust Talos IPC layout declares:

- shared-memory magic `0x54414C05` and layout version `6`;
- maximum image payload 1280×720 with three channels;
- metadata and image-pool names `talos_ipc_meta` and
  `talos_ipc_image_pool`;
- image metadata, camera intrinsics, poses, gimbal commands, chassis
  observations, target/rune ground truth, and exposure history;
- file-backed image transport by default, with optional TCP image transport;
- runtime image width/height constrained by the compiled maximum.

The Rust layout is currently the implementation source of truth. Existing C++
consumers may contain independently maintained constants or struct layouts.
Therefore `legacy-0` does not assert Rust/C++ ABI compatibility. Consumers must
not infer compatibility from the version number alone.

## Required v1 boundary

The first stable contract must provide:

- an explicit major/minor version handshake and capability flags;
- generated or single-source Rust/C++ bindings;
- image frames with dimensions, format, sequence, and monotonic timestamp;
- camera intrinsics and named coordinate-frame/pose semantics;
- simulator time, heartbeat, reset/session identity, and error state;
- target/rune observations and optional ground truth clearly separated;
- command channels for gimbal and public scenario control;
- conformance tests that run both producer and consumer from clean builds;
- endpoint discovery that does not depend on a worktree absolute path.

Breaking layout or semantic changes require a new major contract version.
Additive capability changes require a minor version and feature negotiation.

## Consumer rule

Auto-aim and energy-buff modules may depend on a tagged simulator contract, but
must not copy simulator source, private context, or handwritten layout mirrors.
Until v1 exists, each consumer must treat `legacy-0` as explicitly pinned and
must prove compatibility in its own clean-checkout validation.

# Simulator board

## Current state

- Management architecture: active.
- Business/source modifications: frozen by user instruction.
- Canonical committed baseline: `integration/perf-main@2fa0de2feeb1`.
- Published interface status: `legacy-0` (descriptive, not compatibility-stable).
- Old context/evidence: retained in place; archival deferred.

## Next work after the freeze is lifted

1. Design and implement simulator contract v1 as a standalone public API.
2. Add generated Rust/C++ bindings and contract conformance tests.
3. Move all consumer access behind v1; remove duplicated simulator layouts.
4. Validate Release builds and runtime from a clean checkout.
5. Tag a simulator release and pin consumers to that release.

No item above is authorized as a source-code change during the current freeze.

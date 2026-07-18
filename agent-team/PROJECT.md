# Simulator module

- Branch: `integration/perf-main`
- Context schema: `agent-team-fixed-v1`
- Module owner: simulator
- Status: management architecture active; business-code changes frozen
- Baseline at migration: `2fa0de2feeb1`

## Scope

This branch exclusively owns simulator rendering, physics, capture, IPC,
scenario construction, target behavior, benchmarking, and release packaging.
It exposes consumer behavior through the two tracked public documents below:

- `agent-team/SIMULATOR_INTERFACE.md`
- `agent-team/SCENARIO_CONTROL.md`

Auto-aim armor, learned estimator, downstream fire-control, and energy-buff
research are consumers. Their private context and algorithms are out of scope.

## Context boundary

`PROJECT.md`, `BOARD.md`, and `DECISIONS.md` are private to this branch and must
be committed with relevant management changes. Other branches may read only the
two public contract documents. Legacy files under `agent-team/context/`,
`agent-team/handoffs/`, and `agent-team/scratch/` are preserved but inactive
until a dedicated archival pass.

## Release gate

A simulator revision is consumable only after its public contract has a named
version and a clean-checkout build/runtime validation record. Dirty binaries,
uncommitted layouts, and local absolute paths are never release evidence.

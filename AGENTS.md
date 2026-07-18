# Simulator Module Instructions

This worktree is the canonical simulator module on `integration/perf-main`.

Before any work, verify the branch and read only this branch's tracked
`agent-team/PROJECT.md`, `agent-team/BOARD.md`, and `agent-team/DECISIONS.md`.
The old lower-case `agent-team/context/` tree is legacy evidence: preserve it,
but do not load or update it unless the task is explicitly archival or forensic.

The simulator owns rendering, physics, capture, IPC, scene construction, target
control, and the public simulator contract. Consumer-specific auto-aim and
energy-buff logic does not belong here. Publish consumer-visible behavior only
through `agent-team/SIMULATOR_INTERFACE.md` and
`agent-team/SCENARIO_CONTROL.md`; never synchronize simulator source by copying
it into feature branches.

Do not claim compatibility from a dirty worktree or an old binary. Before a
simulator release, validate the committed revision from a clean checkout and
record the exact contract version, build command, runtime command, and evidence.

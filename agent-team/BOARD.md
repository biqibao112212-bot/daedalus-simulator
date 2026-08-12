# Daedalus Simulator board

## Current status

- User approval for `SIMULATOR_CHANGE_APPROVAL_REQUIRED` was received after
  review of consumer proposal `e721b26`.
- Work started from clean branch `release/simulator-multiplatform-x86` at
  `48b9437c389c2911e0a135cf1d727e36a68317ab`.
- Simulator `1.2.1`, SDK `1.2.0`, SHM v7 / ABI revision 2, TCP/UDP ports, and
  the distribution ground-truth lock are frozen compatibility baselines.
- Implementation, schema/contracts, bilingual documentation, packaging gates,
  and reproducible TCP/free-IPPE experiment tooling are present in the working
  tree as simulator/SDK `1.3.0`.
- Development validation passed: Rust `195/195`, talos-ipc `7/7`, native MSVC
  SDK CTest `7/7`, compatibility/JSON/script checks, default-off runtime, and
  a real TCP label/IPPE experiment.
- Strict workspace clippy with `-D warnings` is not green at the frozen
  baseline: reproduction is
  `cargo clippy --locked --features talos,distribution-release --all-targets -- -D warnings`;
  it stops first on pre-existing `crates/exact` range-loop and `talos-ipc`
  derivable-Default lint debt. The new exporter-specific type/argument
  complexity lint sites are explicitly scoped and do not redefine the Release
  build/test gates.

## In progress

The scoped implementation is staged but not committed. The mandatory
pre-commit Release-performance gate built the exact Windows distribution
binary successfully, then rejected the commit because `main_update_hz` and
`capture_copy_submit_hz` were `36.932`, below the `100 Hz` floor. Evidence is
retained at
`D:\仿真\runtime\simulator-performance\20260811T165041Z\performance-evidence.json`.
At the same time, unrelated PID 20416 (`nightreign`) occupied about 92--93% of
the GPU 3D engine; historical same-machine baselines are 168--186 Hz. Do not
terminate that user process, bypass the hook, commit, or package until the
isolated gate can be rerun without the external GPU contender.

## Freeze and blockers

- Never overwrite or mutate any existing formal Release, especially `1.2.1`.
- Do not edit the consumer repository or its version lock.
- Do not write a label unless TCP image identity and exposure truth match
  strictly; ambiguity is a dropped label, not a best-effort row.
- Formal `1.3.0` packaging waits for an implementation commit, a clean tree,
  native SDK tests, fresh version-matched performance evidence, and manifest
  verification.

## Ordered next steps

1. Wait for the unrelated GPU workload to end, then rerun the mandatory
   pre-commit gate unchanged.
2. Commit the implementation only if the unchanged `100 Hz` thresholds pass.
3. From that exact clean commit, run Release build/performance/package gates
   into a new `1.3.0` directory only; verify hashes and runtime behavior.
4. Run the retention pass and finish with a clean worktree or a concrete gate
   failure report.

## Validation still required

- Implementation/runtime gates above are development evidence only until the
  implementation is committed.
- Clean-commit Windows x86_64 Release build, performance gate, new-directory
  packaging, manifest hashes, and protected-asset retention.
- Linux native build/runtime/package acceptance remains a separate
  target-native gate if no Linux x86_64 runner is used in this task.

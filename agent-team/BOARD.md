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

Implementation is committed at `988cc11` and the clean Windows x86_64
distribution build has passed Rust `195/195`, native SDK CTest `7/7`, and the
public compatibility gate. Clean-commit performance evidence is committed with
the Release metadata: distribution mode, exporter disabled, `207.210 Hz` main
update and `190.275 Hz` capture submit. The next operation is a new-directory
Windows `1.3.0` package followed by default-off and opt-in package-runtime
acceptance; do not claim Linux acceptance from this Windows gate.

## Freeze and blockers

- Never overwrite or mutate any existing formal Release, especially `1.2.1`.
- Do not edit the consumer repository or its version lock.
- Do not write a label unless TCP image identity and exposure truth match
  strictly; ambiguity is a dropped label, not a best-effort row.
- Formal `1.3.0` packaging waits for an implementation commit, a clean tree,
  native SDK tests, fresh version-matched performance evidence, and manifest
  verification.

## Ordered next steps

1. Commit clean performance evidence and current release context.
2. Package only to the absent `1.3.0/windows-x86_64` target, then verify every
   manifest hash and protected-capture exclusion.
3. Run package default-off distribution-lock and opt-in TCP/label acceptance.
4. Record release result, run retention, and finish clean without pushing.

## Validation still required

- The package and its runtime acceptance remain unproven until the new,
  manifest-verified Release is built from this committed evidence.
- Linux native build/runtime/package acceptance remains a separate
  target-native gate if no Linux x86_64 runner is used in this task.

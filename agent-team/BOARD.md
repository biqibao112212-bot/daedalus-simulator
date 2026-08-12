# Daedalus Simulator board

## Current status

- Windows x86_64 Release `1.3.0` is accepted from manifest source commit
  `2bce032ebcc55bfc4cfa0e6e793802a55ea22c70` at
  `D:\仿真\releases\daedalus-simulator\1.3.0\windows-x86_64`.
- The 55-file manifest was independently rehashed, contains no protected
  capture payload, and identifies simulator/SDK `1.3.0`, SHM v7, ABI r2.
- Clean evidence for implementation commit `1ba59ad` measured `171.190 Hz`
  main update and `163.228 Hz` capture submit with the exporter disabled.
- Native Rust distribution tests passed `196/196`; Windows SDK CTest passed
  `7/7`; compatibility checks preserve ports 5601/5602/5603 and online truth
  lock semantics.
- Package runtime acceptance passed. Default-off emitted no JSONL and the
  public SDK observed `target_count=0/rune_count=0`. Opt-in received 525 real
  TCP frames and exported 1,756 rows for 439 complete Z4 exposures; strict
  validation found 1,664 uniform and 92 excluded rows, with max free-IPPE
  closure `0.000866109666 px` / `4.44063144e-06 m`.

## Freeze and blockers

- Never overwrite or mutate any existing formal Release, especially `1.2.1`.
- The accepted Windows package is immutable. Labels, TCP identity ledgers, raw
  RGBA, successful/failed experiment sessions, and prior failed packages are
  protected assets and remain retained.
- Linux native build/runtime/package acceptance remains a separate target-native
  gate; this Windows delivery makes no Linux Release claim.

## Ordered next steps

1. Consumers may pin the accepted public `1.3.0` Release/SDK and independently
   validate its schema and distribution lock.
2. Run Linux target-native acceptance before publishing a Linux package claim.

## Validation still required

- No Windows release validation remains outstanding.
- Strict workspace clippy remains pre-existing frozen-baseline debt and is not
  a Release gate: `crates/exact` range-loop and `talos-ipc` derivable-Default.

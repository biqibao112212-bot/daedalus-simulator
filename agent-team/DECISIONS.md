# Simulator decisions

## D-001 — Single simulator owner

The simulator is owned only by `integration/perf-main`. Feature branches do
not carry or synchronize private simulator modifications.

## D-002 — Public contract replaces shared context

Consumer branches learn simulator behavior from versioned public interface and
scenario-control documents, not by reading simulator private context or by
cross-branch Agent Team handoffs.

## D-003 — Committed evidence only

Performance and compatibility claims must identify a committed revision and be
reproduced from a clean checkout. Results from dirty source or stale binaries
are exploratory only.

## D-004 — Legacy context is retained

Existing lower-case context, handoffs, evidence, and scratch records remain in
place and are not yet archived. They are not part of the active context set.

# Codebase and gameplay audit, 2026-11-13

**Role:** Dated review evidence, not implementation authority. The current
contracts remain `AGENTS.md`, `ARCHITECTURE.md`, `TECHNICAL_DESIGN.md`,
`TESTING.md`, `STATUS.md`, and `GAMEPLAY_EVALUATION.md`.

## Scope and evidence standard

The `main` worktree was clean at entry. The project and workspace agent
contracts, technical design, test authority, review checklist, prior audits
and current gameplay reports guided a cross-owner source review. Inspected
control-rate and slow-tick orchestration; grounded player movement, delayed
work and support; world generation, tree growth, targeting, felling, skylight
and perennial regrowth; fluid stores, natural springs, thermal exposure and
hydration; surface pickup, storage, placement and material custody; scout
foraging, patrol, hydration, combat and den care; mechanical transmission,
production, mining and associated replay or accounting boundaries.

This was a broad owner-guided review, not a line-by-line formal proof of all
source files or all generated seeds. The baseline wilderness and all-scope
gameplay reports passed before any modification. Source searches for legacy,
deprecated, compatibility, TODO and placeholder paths did not establish an
active obsolete implementation that could safely be removed. Explicit
rejection of old save shapes is strict current-schema enforcement, not a
compatibility adapter. No speculative deletion or cosmetic module splitting
was performed.

## Confirmed scaling defects and corrections

**Root moisture survey.** The transient natural-spring survey sorted its
source records by X and narrowed by X before filtering Z and
elevation. In a long explored north-south territory, each perennial plant
could still inspect every spring with a similar X coordinate, including
thousands well beyond its finite horizontal root reach. On a vegetation
front with many plants this could produce repeated long scans. The survey
now uses one disposable two-axis B-tree lookup. Exact local horizontal
distance, vertical reach and the wettest qualifying source retain the
same finite fluid-owner observations and original crop rules. No water,
matter, persistent cache, new ownership surface or synthetic resource was
added.

**Living-tree spatial index.** The world owner's standing-tree map was
ordered by X/Y/Z. Its sky and canopy query could bound X but needed to
filter all Z values afterward, causing frequent light observations to
inspect potentially distant woodland. The sole authoritative tree map
now keys each living root by X/Z/Y. Each nearby X column performs its
own bounded Z range. Exact trunk lookup, materialization obstructions,
transplant/felling operations and accumulated atmospheric wood accounting
use this same index, not a derived parallel authority. The existing
canonical tree-array serializer and strict duplicate/order rejection
operate on the new spatial order. This intentionally adopts the present
save shape without an old-order compatibility path; trusted loading
retains all seed identity, provenance and physical occupancy checks.
The redundant standing-tree iteration-order assertion was removed from
trusted-load validation, because strict index construction already
establishes canonical order.

The current technical design now states the authoritative tree index's
horizontal ordering and local-query behavior. The changes do not add
world rules, altered authored balance or alternate commit routes.

## Contract and regression evidence

- A moisture regression retains exact boundary behavior across circle
  radius, vertical separation and extreme signed coordinates while placing
  thousands of nonlocal springs along the same X coordinate.
- A standing-tree regression uses thousands of distant standing roots
  along one X column, checks local canopy-candidate results, both extreme
  world-coordinate corners, direct trunk lookup and upper-trunk structural
  obstruction, plus canonical serialization/roundtrip and reordered-save
  rejection.
- Existing terrain, tree, skylight, transplant, felling, scout and natural
  forage tests continue to exercise the canonical owner operations. In
  particular, the saved woodland identity and provenance validators remain
  part of the full core lane.

Verification on the final source changes:

- `python ci.py quick`: passed all four build-free checks.
- `python ci.py gate --lint`: passed the repository-owned Clippy lane.
- Focused moisture spatial-boundary regression: passed.
- Focused standing-tree index and canonical-roundtrip regression: passed.
- `python ci.py audit --all`: passed 2,663 core tests and 198 gameplay
  tests, including the widened standing-tree regression.
- `python ci.py report --scope all`: passed after the owner changes,
  with separately labeled executed player progression, exact-local worlds,
  spatial-proxy counterfactuals and controlled capability evaluations.
- The baseline `python ci.py report --scope wilderness` and all-scope
  report passed before the change. No gameplay report failure was hidden
  by modifying the evaluator.

A separate unconfigured all-targets Clippy invocation reported widespread
existing `expect`/`unwrap` lint diagnostics. That command does not replace the
project's authoritative lint gate; no warnings were suppressed or unrelated
invariant assertions rewritten to satisfy it.

The report observes deterministic simulated gameplay and finite resource
consequences, not a graphical client or universal behavior across every
possible seed. It does not convert controlled capability setups or
spatial-proxy episodes into claims of ordinary-player reachability.

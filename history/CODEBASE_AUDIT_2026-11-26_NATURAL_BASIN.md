# Codebase audit, 2026-11-26: natural basin occupancy

## Findings and resolution

The audit traced structural failure, mounted equipment, supported stockpiles,
fluid support and egress, dismantling reservations, and production settlement.
A candidate mounting/dismantling race did not reproduce: ordinary mounting
requires co-location and the dismantling target must be reachable without
intersecting the player's body. The experimental tests were discarded; no
mounting behavior was altered.

The ordinary grounded construction owner authorized full-cell timber
materialization in a generated natural spring basin. The direct pointing
path excludes stationary occupants, and natural generation excludes solid
members, but neither alone closes the construction and load boundaries.
A basin retains its identity after complete drainage.

Full-cell materialization now returns a typed NaturalSpringOccupied error
with the member, store and voxel. The single-cell case queries the existing
fluid location index; larger bounds query finite natural basin records.
Natural spring creation is bound to terrain generation, so the existing
world revision guard covers basin creation. Trusted-load spring validation
rejects an embodied full-cell member at the basin. Narrow framing and inert
plans remain permitted by the owner. No persisted representation changes.

## Verification

- Baseline job 7169 ended at the stall watchdog. Retry 7172 failed the
  new regression because ordinary construction was authorized at a wet basin.
- Two intermediate test compilation errors (7184, 7187) were corrected.
- Final full audit 7193: 2851 core and 187 gameplay tests passed.
  Foundry sample, controlled-capability, variation 0x35A3DA0D0D241843.
- New tests exercise wet and dry basin rejection, unchanged state on rejection,
  conserved transfer into a receiving vessel, successful narrow framing,
  exact matter and fluid accounting, round-trip persistence, and rejection
  of a forged member location overlapping a natural spring.
- Quick validation 7231 passed all four checks.
- Rebuilt text-play client 7228, maintained seed 24318: trekked east and south
  to (9,2,9), drank 250 mL, completed at control 432 / simulation 6.
  Available water fell from 4499.488 mL to 4249.488 mL. Session exited normally.
  This is a maintained-opening drinking check, not a manual natural-basin
  construction reproduction. The latter is covered by the owner regression.

## Scope and working-tree preservation

This pass is a targeted cross-owner audit, not exhaustive validation of
all collapse dynamics, every procedural seed, graphics, or release performance.
The twelve independently modified files remain unstaged and excluded from the
audit commit. Validation exercised the combined local working tree, not an
isolated checkout.

# Codebase audit, 2026-11-26: grounded structural foundations

## Findings and resolution

Grounded structural members terminate load propagation at their base. The
ordinary construction owner, prospective terrain admission, and trusted
state validation accepted embodied grounded members without supporting terrain.
The direct player pointing path alone did not close these boundaries.

One shared world predicate checks each base cell of a grounded member in
generated columns against solid, unreserved terrain. The scan clips to
actual generated chunks. Other members and living trunks do not provide
an implicit ground anchor; member-to-member support uses the explicit
structural graph. Abstract columns retain their existing admission model.
Inert plans do not claim a foundation. Materialization, terrain admission,
trusted load, and footing removal share the foundation semantics.
Construction tokens bind player-work revision so pending excavation
cannot claim the floor between validation and commit. Typed errors identify
the member and offending base cell. No persisted representation changes.

Five regressions cover ordinary unsupported materialization, stale terrain
authorization, unsupported prospective terrain admission, invalid trusted
state, successful generated anchoring, and reserved excavation footing
(the ordinary materialization test also checks stale authorization).
Existing occupancy diagnostics retain precedence. Active roof and equipment
fixtures use explicit support connections where required; remaining
fixture geometry supplies real terrain footing. Unfinished obstruction
fixtures continue to test embodied, inactive material rather than
pretending it has an active ground anchor.

## Verification

- Baseline jobs 7113 and 7115 reproduced the three missing validation
  boundaries with failing tests.
- Intermediate jobs 7122 and 7138 found compilation errors, corrected
  before acceptance. Full audit 7127 exposed 28 fixtures with invalid
  assumptions or diagnostic ordering; fixture repairs retained the
  tested behavior and explicit load ownership.
- Final full audit 7140: 2,849 core tests and 187 gameplay tests passed.
  Settlement sample, ordinary-exact-local-after-disclosed-bootstrap,
  variation 0x15D807461F5B3259.
- Rebuilt text-play client 7141, maintained seed 24318: trekked to
  (10,2,9), gathered two 5 kg timber batches, selected the log stack,
  targeted stone at (11,1,9), and completed a load-bearing post at
  (11,2,9) through ordinary secondary use and work scheduling. At control
  8208 / simulation 114, the member held 3250 g and the pack held 6750 g.
  The timber source fell from 36279.054 to 26279.054 g. Session ended normally.

## Scope and working-tree preservation

This is a targeted structural and cross-owner audit, not exhaustive
certification of collapse dynamics, all seeds, graphics, or release
performance. The twelve independently modified files remain unstaged;
task-owned hunks in shared files are isolated in the audit commit.
Validation exercised the combined local working tree, not an isolated
checkout. The technical design records the current foundation contract.

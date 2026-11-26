# Codebase audit: exact gathering intent across alignment

## Corrected defect

`begin gather` used to advance the real clock and then parse the same numbered gathering row from the new catalog. The earlier location guard did not protect the identity of a source at an unchanged voxel. Nighttime senescence could remove a crop without changing aggregate scent, weather or visible resource cues. Selecting its old row then started a different physical job; selecting a surviving source whose row moved could instead be rejected.

Single and repeated gathering now share one parsed `GatheringOrder` containing the surface owner's opaque physical source and technique, requested quantity and repeat count. Alignment captures that order before time advances, then reobserves the exact option through the existing surface refresh operation. A vanished source or method stops the request, with only the actual elapsed alignment frames committed. A surviving choice keeps its identity despite row movement. Default portions remain current assessments; explicit quantities still pass current canonical admission. No core state, legality rule, sensory model or save format changed. Independent alignment interruptions and the starting/ending voxel guard retain priority, and invalid requests may still report a rejection after spending real alignment time.

## Organization

Removed separate single/repeat gathering parsers and the duplicated repeat-count parsing in gathering, mining and ore dressing. Mining repeat orchestration now lives with the text-play geological order it consumes; `MiningOrder` and its helpers no longer need visibility outside that module. Boundary alignment and queued-admission regressions now have a dedicated `alignment_tests.rs` module rather than extending the general adapter test file. Existing assertions were retained; no test-only production API was introduced.

## Reproduction and evidence

- Controlled seed 24318 save, actor at (3,1,-5), one frame before slow tick 66600. The seeded berry crop has its real senescence due on that front. A second fresh berry source and a stone source are colocated to preserve aggregate scent and provide the competing row; the independent opening colony is removed for isolation. The fixture passes the production trusted-load path. The ordinary neutral boundary step changes exact-local gathering source IDs from [3,13,15] to [3,15] without an independent interruption or change in actor voxel.
- The prior executable actually started 100 g of stone for `begin gather 2 100`, although row 2 was berries before alignment. The two new regressions both failed on the unfixed adapter: that wrong job was admitted, and the surviving stone requested as row 3 was rejected as out of range. Post-fix coverage exercises default and explicit quantities, single and repeated orders, with complete serialized-state comparison against an ordinary boundary wait or an explicit gathering command for the surviving physical source.
- `cargo test --locked -j 4 --bin text-play`: 128 passed after all edits. `python ci.py quick`: four checks passed. The task-scoped diff was reviewed, including the relocated tests, and `git diff --check` passed.
- Rebuilt the actual `text-play` executable and loaded the same controlled fixture. A depleted single or repeat request spent one control tick (50 ms), reported the missing source or method, admitted no work and saved byte-identical state to an ordinary boundary wait. The surviving row-3 request began the intended 100 g stone gathering job, producing a byte-identical save to explicit alignment followed by `gather 2 100`. All task-owned temporary saves were removed.

## Scope and limits

Reviewed the numbered gathering, crafting, mining, survey, ore, construction and thermal command paths; carried stack membership and production feedback; and ordinary versus work-wait interruption semantics. This patch binds gathering orders specifically, not every text command. No independently reproduced non-gathering row drift justified a broader rewrite. No measured performance gain is claimed. The text-play lane is the complete focused proof for this adapter-only change; the broad core/gameplay audit was not rerun. All 12 pre-existing terrain/custody modifications were preserved and excluded from the commit. Verification ran against the combined working tree, not a separate clean checkout.

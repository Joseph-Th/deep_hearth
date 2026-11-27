# Codebase audit: excavation target authority

## Finding and correction

Active block excavation persisted target coordinates, state, mass, tool trace and schedule, but did not preserve the pointed ray. A decoded save could replace the target with an equivalent obstructed or distant stone block while satisfying the other admission checks. The new regression failed on the first obstructed-cell case before the correction.

The durable work record now binds the exact normalized fixed-point aim selected by the authoritative world raycaster. Trusted-load validation reconstructs that direction with the existing checked constructor and replays the same first-hit query against the held actor's physical eye origin and stance. No approximate centre ray, secondary targeting owner, or compatibility path is introduced. Invalid direction and mismatched first hit have distinct validation errors. The current save schema is 137.

## Verification

- Baseline regression job 9276: failed because the obstructed equivalent stone cell was accepted.
- Final quick and block-excavation suite job 9288: four quick checks and eight focused tests passed. The regression rejects obstructed and distant targets, malformed or redirected aim and omitted aim, and checks legitimate save/replay and matter conservation.
- Broad audit and lint job 9296: 2,888 core tests and 187 gameplay tests passed; woodworking sample, ordinary-exact-local-after-disclosed-bootstrap evidence, variation `0xE13DCC60940BB7F3`. Production Clippy passed.
- Text-play job 9322: 137 tests passed.

## Scope

Reviewed excavation selection, physical ray traversal, tree felling and relocation, fixed-footing dependencies, loaded woodland history, and exclusive player-work validation. The correction changes trusted admission, not excavation rates, mass, tool wear, control timing, or ordinary targeting rules. The regression uses the production admission, save/load, and scheduled completion paths with disclosed deterministic terrain fixture setup. No separate manual CLI playthrough or exhaustive world-state proof is claimed. Existing unrelated working-tree edits remain outside this change.

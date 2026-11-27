# Codebase audit: player labor lifecycle and chopping custody

## Findings and corrections

- `PlayerWorkState` advances its revision once for each durable attention admission and once for each release. Trusted loading previously accepted idle odd revisions and active even revisions. The owner-level validation now rejects inconsistent lifecycle parity with a typed diagnostic. Existing checked release headroom remains enforced.
- Tree-felling start requires the selected carried adze, and equipment cannot be dropped during work. Trusted loading previously used generic local-equipment access, which also admits stationary equipment. A combination of separately valid chopping and dropped-tool custody snapshots therefore loaded an impossible active job. Admission, scheduled completion and trusted-load checks now use the logistics owner's carried-equipment predicate. Existing tool condition, occupancy, proximity, duration and footing rules remain in effect.

## Verification

- Lifecycle regression job 9334: failed before correction because idle revision 1 was accepted. Job 9340: passed; legitimate idle, active and completed manual-craft snapshots round-trip, while four inconsistent revisions receive the typed rejection.
- Stationary-adze regression job 9345: initial test compile failed on unused imports; corrected the assertion to require the exact typed error. Job 9346: reproduced the accepted invalid save. Job 9349: all 12 tree-chopping tests passed, including the new regression; all four quick checks passed.
- Final broad audit and lint job 9352: 2,890 core and 187 gameplay tests passed; liberation sample, ordinary-exact-local-after-disclosed-bootstrap evidence, variation `0x8ABD122B9078ACC8`. Production Clippy passed.

## Scope

Reviewed exclusive attention admission and release, production suspension/resume and fatal interruption, output custody, and timed work replay. Changes preserve the save schema, authored rates, material quantities, schedules and wear. Verification uses canonical gameplay operations and trusted persistence; no independent manual CLI playthrough or exhaustive world-state proof is claimed. Existing unrelated working-tree edits remain outside the correction.

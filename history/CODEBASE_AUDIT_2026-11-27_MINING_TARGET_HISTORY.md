# Codebase audit: acquired mining target history

## Findings and corrections

Live mining admission resolved a single acquired voxel and requested constituent, but the durable record retained only the larger hidden deposit identity. Load validation accepted an active job with every acquired observation removed and checked actor locality against the whole deposit. It also replayed tool capability using exact hidden hardness instead of the conservative acquired upper bound used at admission.

The job identity now retains its acquired region and constituent. The canonical target resolver supports historical replay through admission time, using the same evidence aggregation and matching rules as live resolution. Geological availability for extraction excludes bodies depleted at the settled admission tick; observational availability retains its existing pre-tick semantics. Working actors must remain at the acquired voxel. Load validation binds the replayed target to the persisted deposit and applies the same shared hardness-selection function as live admission. Retained output after depletion or later contradictory observations remains valid against its original admission evidence. Save schema is 138, without old-schema handling.

## Verification

- Baseline job 9384 failed because a mining job without geological knowledge loaded successfully.
- Job 9393 passed 102 mining tests. The new target-history module extends the regression to typed rejections of within-deposit relocation, forged target locality, and later-acquired evidence. It also verifies genuine field inspection, complete depletion, obstructed settlement, a later field inspection, save/load, and exact recovered output. Job 9398 initially failed compilation on an ignored must-use test receipt; corrected the test. Job 9402 passed both targeted tests and quick checks.
- Job 9404 passed 2,892 core and 187 gameplay tests; Clippy identified a redundant must-use attribute. Removed it and centralized admission hardness selection. Final job 9435 passed four quick checks, 2,892 core tests, 187 gameplay tests (ore sample, controlled-capability evidence, variation `0x74BE0E75B71DF0EA`), production Clippy, 137 text-play tests and the executable build.
- Actual text-play replay job 9460, maintained seed 24318: gathered and crafted the geological hammer and pick, completed a detailed local copper survey, started 200 g extraction, saved while the job was working, confirmed schema 138 and durable target fields, loaded the save through the executable, and completed the job. The resulting pack contained exactly 200 g copper ore with zero reserved inbound mass. The uniquely named audit save was removed by the task's cleanup trap.

## Scope and limitations

Reviewed geological knowledge aggregation, observation history, target authorization, mining admission, physical tool constraints, finite extraction and deferred settlement. Authored extraction rates, material yields, wear and ordinary player-facing commands are unchanged. The real replay used the maintained starter; the blocked settlement proof uses the disclosed controlled terrain/custody fixture. No exhaustive geological history proof or all-seed gameplay coverage is claimed. The 15 pre-existing unrelated working-tree edits are excluded.

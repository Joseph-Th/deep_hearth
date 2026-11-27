# Production schedule and assembly admission audit — 2026-11-27

## Findings and corrections

- Unattended production starts exposed a deadline, but subsequent status reports omitted it and scheduled waits required personal work. Added a production-owned observation projection using retained-output indexes for carried and exact-local ground stockpiles. It deduplicates jobs by ID, includes suspended jobs without inventing a release time, and exposes no remote endpoints or input traces.
- The terminal readout now displays observable local production schedules and does not repeat the active manual job. Scheduled waits prioritize personal work and queued interactions, then select the earliest observable running production release. Unattended waits retain ordinary attention and existing wait caps.
- Equipment assembly discovery classified usable materials as readiness even when exclusive manual work prohibited assembly. The owner now separates material availability from admission, shares context and transaction validation with execution, and reuses the selected materials for the assessment. The terminal adapter consumes this projection without validating each ready option again. Direct assembly selects the authored definition by ID instead of rebuilding the catalog.

## Verification

- Before correction, the stationary-output terminal regression failed for missing local production status; the owner-level equipment regression failed because a busy player was still classified as ready.
- Initial quick validation identified excessive control-dispatch complexity. Extracted scheduled wait dispatch into a dedicated function; final quick validation passed all four checks.
- Terminal suite: 137 passed. Extended coverage includes local and remote output custody, save/load consistency, both scheduled wait variants, suspended work and recovery, exact completion boundaries, read-only observation, and no duplicate active-job report.
- Full local audit: 2,886 core and 187 gameplay tests passed (foundry-bootstrap sample, variation 0xE66FC06E49F62F9E).
- Rebuilt terminal executable, seed 24318: fired 1,600 g of greenware with a 1m 59s schedule using wait finish; cooled the fired body with a 26s schedule using wait work. Final body: 1,600 g at 20 C, zero reserved inbound mass.
- Rebuilt terminal executable, same seed: knife assembly was correctly blocked during gathering, succeeded after completion, and pickup and recovery displayed matching blockers during the next active work interval before becoming ready again.

## Scope

No save schema, physical assembly rule, production duration, or matter/energy rule changed. Only task-owned paths are intended for commit; pre-existing working-tree changes are excluded. This was a targeted subsystem audit, not exhaustive manual verification of every system.

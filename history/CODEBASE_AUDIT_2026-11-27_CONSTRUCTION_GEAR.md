# Codebase audit: structural construction and equipment action feedback

Date: 2026-11-27

## Review scope

Traced structural construction admission and material custody, inventory allocation and selection, equipment assembly and maintenance, energy-store accounting, combustion, mechanical transmission, fluid and inventory supports, production reservation guards, and trusted-load validation. Reviewed the terminal equipment and ceramic-workshop interfaces against their authoritative action validators. The shared work-commit guard already rechecks future equipment and energy revision demand for manual-power admission; no duplicate guard was added.

## Corrections

- Existing-member construction and atomic planning/materialization now share target, world clearance, material and exact required-mass validation, plus commit checks for dependency revisions, inventory revision and target identity/lifecycle. Each transaction still independently checks its structural revision ordering and stockpile-load assumptions.
- The text-play equipment catalog formerly labeled material-backed assembly as ready even during exclusive work. It now checks the real assembly validator before claiming readiness, and displays pickup and recovery admission results for each local stationary item. Assembly feedback no longer implies pickup is always available.
- Added a production-backed command regression that checks busy assembly, pickup and recovery, subsequent readiness, and state preservation during readouts and rejected actions. Updated the current text-play documentation.

## Verification

- Quick lane: four checks passed.
- Text-play: 137 tests passed, including the new equipment catalog regression.
- Full audit: 2,886 core and 187 gameplay tests passed. Sample: ore, controlled-capability, variation 0x1676CB5CEF7947DA.
- Actual rebuilt terminal executable, maintained seed 24318: gathered timber and flint, prepared a knife head and handle, checked gear during gathering, assembled a 420 g knife, and compared pickup and recovery blockers during a second gathering operation. Each reported blocker matched the real command rejection; both actions became ready after work completed.
- Actual ceramic workshop replay, same seed: gathered and processed materials, assembled a kiln, built a hearth and cooling bed, burned 1,000 g of timber, fired 1,600 g of greenware, and cooled the resulting 1,600 g fired crock body to 20 C. No inbound reservation remained.

## Boundaries

The initial working tree contained fifteen unrelated modified or untracked paths. Verification ran on the combined tree; only task-owned paths are committed here. No save schema, compatibility layer, material recipe, or physical action rule changed. This is a targeted cross-system audit, not proof of exhaustive manual coverage. No obsolete compatibility path was established for safe removal in this pass.

# Codebase audit: scout hydration and buffered direct-action custody

Date: 2026-11-27

## Confirmed defects and changes

- A scout drinking an incomplete ration at the junction left for the dry patrol tip even though a second accessible vessel contained the remaining water. Strengthened the existing partial-ration replay test before changing production code: it failed with actual position (1, 0, 0), expected junction (0, 0, 0). Hydration now checks the canonical admissible local withdrawal after partial intake and reserves the next five-tick scout slot when another serving remains. A threat overrides the hold; empty or inaccessible vessels do not reserve a slot. No new persistence field, fabricated movement, virtual consumption, or alternate fluid operation is introduced.
- Primary and secondary buffered direct-action commit guards duplicated clock, survival, and logistics revision checks. Replaced the two internal modules with one owner module and shared transactional precheck/error type, retaining action-specific admission and clear operations. Added atomic rejection checks for both action kinds after a logistics revision change and expanded survival-staleness coverage.
- Production Clippy identified duplicate-prefix commit-error variants in both old modules and a wildcard matching future buffered secondary target variants. The shared error names and exhaustive target handling resolve these failures without suppressions.
- Updated technical design descriptions of partial water intake, route selection, terminal accounting, and the already implemented pre-sortie watchman visibility. Existing save schema remains unchanged.

## Verification

- Red regression: `two_partial_pools_complete_one_real_scout_drink_across_saved_patrols` failed before the production fix at the expected position assertion.
- Hydration owner suite: 26 passed, including exact five-tick continuation after save/load, retreat with unconsumed second serving, empty-source departure, and conserved fluid accounting.
- `python ci.py quick`: all four checks passed after code and technical-design edits.
- `python ci.py gate --lint`: passed after the three initial lint failures were corrected.
- `python ci.py audit --all`: 2,885 core and 187 gameplay tests passed. Survival sample, ordinary-exact-local-after-disclosed-bootstrap, variation `0x8CF52E8CD38615B1`.
- Full validation ran against the working tree, which included fifteen existing unrelated modifications/untracked files. Those files are not part of this audit's changes.

## Review boundary

Reviewed the scout's hydration, patrol/care, salvage and cache eviction, direct input buffering, target dispatch, and relevant owner/continuation contracts. Kept existing finite-custody and deterministic scheduling policy where no demonstrated defect justified changing it. This pass does not claim exhaustive verification of every reachable world state or full real-player progression.

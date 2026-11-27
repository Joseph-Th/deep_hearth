# Codebase audit: local drop observation and subsystem consistency

Date: 2026-11-27

## Review scope

Traced authoritative control and simulation scheduling, player survival and direct consumption, surface gathering and drop custody, inventory ingress and revision reservations, terrain generation and visibility, kobold patrols and resource acquisition, production work, and energy/fluid transfers. Reviewed the text-play adapter against the owner APIs and actual command outcomes. Used the repository's module diagnostic to investigate possible orphaned code, not as a completion metric.

## Confirmed corrections

- Exact-local loose drops previously appeared in the coarse sighting list without their mass, temperature or pickup condition. Text play now uses the surface owner's projection for those details, while remote sightings remain coarse. Local drops are listed once.
- The surface owner now shares pickup-availability tick arithmetic between admission and observation. The projection uses the owner's deliberate-loading classification rather than restating the timber rule in the adapter.
- Extended owner and real-command regressions for deliberate timber loading, the self-pickup delay, local drop quantities and conservation.
- Corrected current documentation for local pickup and the permanent watchman. The watchman is visible at the den before its first sortie; the den is shared with the scout. The owner regression `permanent_watchman_is_visible_at_the_den_before_any_funded_sortie` checks this.
- Eight apparent module orphans were traced to active feature-gated soak tests. Retained them. No confirmed obsolete compatibility implementation was identified for deletion.

## Verification

- Local quick lane: four checks passed.
- Local full audit: 2,883 core tests and 187 gameplay tests passed. Sample: ore, controlled-capability, variation 0xC4EFCD0959932DA6.
- Real text-play replay, seed 24318: `e 20; wait 10; gather 1 0.125; wait work; select 1; drop; look; wait 60; look; inventory; quit`.
- The 0.125 g berry drop was reported at (9,2,8), 20 C, with self-pickup delay active. Forty control ticks later, ordinary pickup returned exactly 0.125 g to the pack. The local loose-drop entry disappeared; the rooted berry source remained distinct.

## Boundaries

The initial main working tree contained fifteen unrelated modified or untracked paths. Validation ran on that combined working tree; only task-owned paths belong in the audit commit. No save schema or physical pickup policy was changed. General passive cooling and broader missing simulation capabilities are documented scope gaps, not claimed fixed. The sampled controlled ore scenario does not establish ordinary reachability. This is a cross-system review with targeted corrections and a broad executable checkpoint, not proof that every module or possible game state is defect-free.

# Codebase audit, 2026-11-24 (scout cargo execution)

## Reproduced behavior

A hungry scout could reach a cell with a full wild-food serving and a 1 g
loose food fragment, then recover the fragment rather than the larger
serving. The pre-correction regression failed at the junction: the
fragment moved to the den while the wild source remained untouched.
The same inconsistent policy affected adjacent and two-edge food routes.

## Correction

Share the local loose-food transfer decision and forage comparison between
outbound planning, intermediate raid continuation, and return-edge cargo
execution. A sufficient loose serving still wins; a larger harvest can
replace insufficient loose food. Existing carried food retains custody
priority. If a preferred harvest cannot commit, preserve the ordinary
whole/split/fallback sequence. A return edge commits at most one successful
cargo exchange. Cargo planning now stays inside its owner rather than
round-tripping intermediate fields through the movement coordinator.

No authored quantities, save schema, or inventory ownership changed.

## Evidence

- Regression across junction, adjacent, and two-edge forage sites verifies
  physical source custody, hunger resolution, exact matter conservation,
  trusted-load validation, and deterministic replay.
- A matched intact-ration and revision-exhaustion case verifies unchanged
  harvest source and cooldown when loose food is collected instead.
- Kobold suite before the additional fallback test: 401 passed.
- Full audit after all changes: 2,773 core and 194 gameplay tests passed.
- Quick checks: all four passed.

## Limits

The change addresses food selection and cargo admission, not every
possible route, resource distribution, or weather state. No graphical
play inspection was performed in this pass.

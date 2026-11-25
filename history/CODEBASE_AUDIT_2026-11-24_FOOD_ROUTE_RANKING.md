# Codebase audit, 2026-11-24 (food route ranking)

## Reproduced defects

- Incomplete adjacent and distant loose-food priorities were compared without
  their partial-serving sizes. A 1 g adjacent fragment hid a 15 g distant
  fragment from the decision about whether to take a 5 g junction parcel.
  The pre-fix regression returned to the den at tick 585 instead of going
  toward the larger visible serving.
- A distant intact fragment prevented an intermediate waypoint's newly
  available full bulk serving from entering the comparison. The pre-fix
  regression continued to the 1 g distant parcel at tick 590 instead of
  returning with a physical split of the 700 g local pile.

## Corrections and organization

Represent the hunger and mixed-salvage ranking policies with named ordered
classes. Compare complete classes between distances, retaining stable resource
identity within each distance and shorter travel on equal class values.
Allow intermediate bulk food to compete with an incomplete distant parcel;
preserve the established preference for a distant complete intact ration.

Keep the selected recoverable forage yields instead of rescanning selected
cells. A sufficient adjacent harvest makes a farther search unnecessary.
The adjacent and far search helpers are private; existing tests exercise the
canonical forage route selector without test-only re-exports.

No save schema, physical quantities, custody rules, or dependencies changed.

## Verification

- Both new failure scenarios reproduced against the unfixed implementation.
- The intermediate regression also preserves the complete distant intact
  meal and full distant split controls.
- Final focused suite: 403 kobold tests passed, including saved replay,
  exact source positions and remaining mass, and matter conservation.
- Quick validation: all four checks passed.
- Two attempts at the full core/gameplay checkpoint ended during the core
  build when the local connector stall watchdog terminated silent processes.
  Other workspace builds were active. No failed test was reported by either
  stalled attempt; neither is a passing full audit.

## Natural-world player-facing observations

The existing text-play executable from the preceding verified revision
(884c518) was used to play a natural damp-grove opening, seed 1837596006,
with no authored starter trail. Starting at (-217,-8,-259), the player
followed visible terrain and resources through 12 north, 5 east and 5 north
cells to the berry patch at (-212,-11,-276), gathered 500 g, then traveled
8 south and 1 west to a visible natural spring. Drinking reduced the spring
from 6562.012 mL to 6312.012 mL. Eating consumed 13.78 g, leaving 486.22 g
in the pack. Vitality, food reserve and hydration were 100% at tick 39.
These observations cover the existing natural-world travel and survival
behavior, not direct interactive evidence of the changed scout ranking.

## Limits

The final focused and quick lanes completed; the broad checkpoint did not.
The reduction in source lookups is structural, not a measured runtime speedup.
No complete wilderness campaign, visual inspection, or exhaustive route
coverage was attempted.

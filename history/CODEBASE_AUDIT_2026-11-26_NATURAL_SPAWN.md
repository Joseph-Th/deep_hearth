# Codebase audit: natural spawn surface admission (2026-11-26)

## Scope and finding

Reviewed representative terrain generation, natural resources, spring and
vegetation cycles, direct consumption and survival projections, control-rate
fast-forward, travel, respawn admission, and text-play persistence paths.
Searched for obsolete runtime compatibility paths and traced the relevant
references. Save-schema rejection tests and active physical compatibility
rules are not backward-compatibility implementations. The maintained opening
and reference clearing remain active regression fixtures. No additional
obsolete production implementation was verified for removal in this pass.

Natural spawn selection previously examined every elevation of each candidate
column. If the surface was unsuitable, a physically walkable underground
temple chamber in that column could be selected as the wilderness start. The
walking-route preference alone did not distinguish underground rooms from
the exterior landscape.

The selector now checks only the canonical seed-derived surface elevation per
column. Existing physical admission, habitat selection, deterministic column
ordering, and level-route preference remain authoritative. This also removes
the vertical scan; no measured wall-clock speedup is claimed. The regression
uses a real generated temple and a controlled obstruction at its overlying
surface. TEXT_PLAY.md documents the surface-only rule.

## Verification

- Pre-fix regression build (7482) terminated by the local stall watchdog
  before the test executed. The old failure path was traced in source, not
  observed as a failing test execution.
- Quick lane (7491): all four checks passed.
- Focused spawn suite (7493): all seven tests passed, including the new
  underground-chamber regression and existing deterministic and movement
  contracts.
- Rebuilt text-play (7496): natural damp-grove seed 1837596006 opened at
  (-217,-8,-259), standing at full reserves with north and south level exits.
  The ordinary look and quit commands completed successfully. This sample
  observes surface startup, not the blocked-temple regression scenario.
- Full audit (7510): 2,856 core tests and 187 gameplay tests passed.
  Wilderness sample, ordinary-generated-opening+exact-local-after-admission,
  variation 0xD8C8D58EBD68B6B2.

## Boundaries

The starting tree contained twelve independently modified files. All remain
outside the audit changes; the validation results cover the combined actual
working tree, not an independently tested clean checkout. This was a
risk-directed source review with the broad executable audit, not an exhaustive
line-by-line inspection or proof of every generated seed, industrial path,
graphical output, or release performance.

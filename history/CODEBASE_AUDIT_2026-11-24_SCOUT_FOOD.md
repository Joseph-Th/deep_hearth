# Codebase audit, 2026-11-24 (scout food recovery)

## Confirmed defects and corrections

- A hungry scout could disregard a 25 g recoverable food parcel at its
  normal patrol tip in favor of a 1 g parcel on a side route. Include the
  patrol tip in food-specific adjacent drop selection.
- A hungry scout could turn home with a 1 g junction fragment despite a
  visible 25 g meal at the patrol tip. Compute the preferred food route
  before the turn-home decision and reuse it for outbound routing. Loose
  food remains eligible during forage cooldown.

Preserve local complete-meal priority, whole-versus-split ranking, and
distance-first mixed salvage. Retain candidate rankings across adjacent
and far searches instead of scanning the same destinations again. No
changes to saved state, authored quantities, or registry definitions.

## Evidence

Both new regressions reproduced the incorrect scout position before the
corresponding correction. Both now verify source custody, exact remaining
mass, hunger resolution, matter conservation, trusted-load validation, and
deterministic replay. Existing route tests exercise the canonical route
selector rather than obsolete separate side/far selection wrappers.

- Kobold subsystem: 400 tests passed.
- Full audit: 2,771 core and 194 gameplay tests passed.

## Limits

This pass focused on food recovery, patrol decisions, and the associated
custody and persistence boundaries. It is not an exhaustive proof of every
possible resource combination or world seed.

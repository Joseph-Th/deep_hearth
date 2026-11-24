# Codebase audit, 2026-11-24 (continued)

## Confirmed change

Remove the zero-valued player-work logistics revision reservation methods
and the unreachable logistics future-demand overflow error. Buffered direct
actions are the actual source of future logistics revision demand. Both
admission headroom and trusted-load validation use that same bounded demand;
revision capacity checks remain checked. No save representation changes.

## Reviewed boundaries

Reviewed representative owner paths for tick settlement and neutral span
advancement, player work, logistics and material custody, survival and
consumption, scout patrols and den care, water withdrawal, world generation,
structural work, production availability, and persistence. Compared findings
with the existing audit history to avoid repeating completed corrections.
No further reproducible gameplay defect was established in this pass.

The orphan-module diagnostic reported eight soak modules, all of which are
explicitly included by the test-soak feature. Retain them and strict
save-format rejection coverage; neither constitutes compatibility scaffolding.

## Verification and limits

- Baseline full audit: 2,769 core and 194 gameplay tests passed.
- Modified code: quick checks and full audit passed (2,769 core and 194 gameplay tests).
- Natural dry-wilderness text-play startup, seed 24318: look, status, help, quit.
- This was a representative cross-system review, not exhaustive proof across
  every source line, world seed, or interactive scenario.

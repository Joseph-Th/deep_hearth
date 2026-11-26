# Codebase audit, 2026-11-25: material-only buffered use

## Scope

Reviewed representative production ownership and cross-system boundaries in
the world, ecology and moisture cycle, scout patrol, sight and hydration
routing, spatial indexes, survival intake and physiology, control scheduling,
and persistence. Checked authored design and current authority documents,
searched for retired/legacy paths and traced callers where behavior was
uncertain. The maintained opening is a gameplay regression fixture, not a
backward-compatibility implementation. The natural opening remains separate.
No additional obsolete production implementation was identified with
sufficient evidence to remove. No arbitrary resource-balance retuning,
warning suppression, or unmeasured performance claim was made.

This was a broad, risk-directed pass, not a line-by-line certification of
all systems, every generated seed, graphics, or release performance.

## Defect and correction

The contextual secondary-use buffer admitted a living player with an empty
hand or an equipment selection even though every buffered target was a
material-driven construction or combustion operation. The persistent
record carried an optional generic hand selection and relied on due-tick
rejection of impossible selection kinds. Existing low-level tests also
constructed these impossible records.

The logistics admission owner now requires a selected material and returns
a typed rejection before reserving the buffer. The pending record stores
the required commodity directly; secondary settlement no longer has an
impossible-selection fallback. Context matching still compares the original
body origin, selected material and aim, and still rejects stale context.
Primary break legitimately permits an empty hand, so its representation
is unchanged. The target-specific production validators retain legality and
custody ownership; the buffered record is not an authorization to consume
matter.

The current save schema advances to 136. No old-layout decoder or migration
is retained. Updated persistence coverage requires the commodity and target,
rejects null commodity and obsolete gathering fields, and round-trips the
valid current record. Other timing, cancellation and replay fixtures now
select real carried timber instead of admitting an empty-hand buffer.
The technical design documents the material-only invariant.

## Verification

- Starting full audit (6743): 2,831 core tests, 187 gameplay tests; fieldwork,
  ordinary-system spatial proxy, variation `0xAF939C907D06CC44`.
- New empty-hand regression before correction (6753): failed as intended
  because the low-level validator returned a successful authorization.
- Quick lane (6766): all four checks passed after formatting correction.
- Initial post-change full audit (6769): 2,831 of 2,832 core tests passed;
  the remaining sub-voxel replay fixture also used an empty-hand buffer.
  Corrected that setup while retaining its original assertions.
- Final full audit (6778): 2,832 core and 187 gameplay tests passed;
  progression sample, ordinary-system spatial proxy, variation
  `0xE6C7D7937046A5FB`.

## Actual player observations

Baseline natural seed 1837596006 (damp grove) showed no exact-local
gathering/drinking source despite distant observations, without granting
remote access. This is one observed spawn, not a guarantee about all worlds.

Rebuilt text-play client (6779), maintained seed 24318: trekked east one
cell, gathered 500 g of berries, ate 10.146 g through the ordinary eating
operation, trekked south one cell, drank 250 mL, and completed the work at
control 2016 / simulation 28. The visible spring volume decreased from
4499.488 to 4249.488 mL; the observed quantities matched the baseline.

Rebuilt client (6782), same maintained seed: trekked east, south, east,
gathered two 5 kg batches of timber, selected the carried stack and aimed
east-down at the top face of stone terrain (11,1,9). Contextual use queued
for the next simulation boundary; wait finish admitted and completed the
construction. At control 8208 / simulation 114 the active load-bearing
member at (11,2,9) held 3250 g, carried timber was 6750 g, and the timber
source had decreased from 36279.054 to 26279.054 g. No synthetic materials
or bypassed validators were used.

## Working-tree preservation

The twelve files modified before this audit were excluded from its commit.
The starting and final full audit runs tested the actual local working
tree including those pre-existing changes; the validation receipt does
not certify an isolated checkout of the audit commit.

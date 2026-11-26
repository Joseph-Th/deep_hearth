# Terrain admission and surface support audit

## Finding

A player-admitted abstract position at (0, 100, 0) can hold a 25 g stone-chip drop. Terrain generation at that column must reject the drop if the prospective terrain cannot support it. The regression test demonstrates the unsupported state through the ordinary material-drop transaction; the baseline generation preflight accepts it (job 6886).

## Contract

Direct and neighborhood generation use the same prospective fixed-footing assessment. Live surface parcels and depleted perennial parcels with remaining rooted biomass require a valid floor. Fully exhausted historical parcels do not. Terrain, prospective trunks and existing full-cell structural members can provide that floor; scheduled floor removal excludes it.

Surface collision and support admission share indexed parcel enumeration. Support rejection reports the lowest offending resource ID deterministically and leaves state unchanged. Existing inbound stockpile reservations use the same prospective geometry assessment.

Three kobold fixtures establish supported generated terrain before resource admission. Their defense, den disposal, visibility and mass-conservation assertions remain intact.

## Verification

- Baseline targeted regression: failure reproduced (job 6886).
- Full local audit: 2,834 core tests and 187 gameplay tests pass (job 6910).
- Quick validation: all four checks pass (job 6912).

Verification covers the combined local working tree, including independent ground-custody edits. Those edits remain separate from the surface-support patch. The shared prospective geometry helper covers coordinate bounds and floor-removal reservations. The independent expansion from inbound reservations to stored stockpile mass is not included.

## Scope

The scope is terrain admission. General creation and trusted-load rejection of every unsupported surface record, graphical rendering, and release performance are not verified here.

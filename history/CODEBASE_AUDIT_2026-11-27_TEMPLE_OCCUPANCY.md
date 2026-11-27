# Codebase audit: temple occupancy

## Findings and correction

- Confirmed trusted-load omission: the warden and archivist body terrain checks were confined to saved excursion cursors, leaving implicit home occupants unchecked. A generated sanctuary with a solid block at the warden's home body cell was accepted by the old load path.
- Added a single shared occupancy pass following cursor validation. Each generated sanctuary resolves the current anchors of both residents and checks the two-cell warden and twelve-cell archivist against world collision, indexed stationary custody, and materialized structures. Structural placement reuses the same body iterators and current-anchor resolver.
- Removed the superseded separate infrastructure and displaced-body checks. Kept deterministic structural-overlap cell ordering and the intentional distinction between missing supporting floor and intersecting body matter. No save format change.

## Verification

- Regression `implicit_temple_resident_bodies_cannot_be_buried_in_trusted_saves`: failed before correction because a stone-filled warden body was accepted; now rejects malformed warden foot/head and archivist lower/upper-corner cells, while an excavated supporting floor still loads.
- Focused kobold suite: 426 tests passed.
- Final `python ci.py quick`: 4 checks passed.
- Final `python ci.py audit --all`: 2,887 core and 187 gameplay tests passed; workshop sample, controlled-capability evidence, variation `0xE626A06A05283048`.
- Final production Clippy gate passed.

## Scope

Reviewed resident movement, visibility, hydration, food/cache custody, defense and generated sanctuary geometry. This pass changes trusted admission and shared geometry queries, not authored patrol behavior. The audit is not an exhaustive proof of all possible simulation states. No unrelated working-tree changes are included.

# Stationary asset footing audit (2026-11-26)

## Findings

Material-backed campfire and equipment assembly required footing, but terrain admission and trusted loading did not consistently validate the same physical claim. A material-built pick or campfire placed in abstract space could survive generation of its location without a supporting floor. A saved world with a removed floor could also retain unsupported stationary assets.

## Resolution

- Share fixed-footing claim predicates between world removal, terrain generation, and logistics trusted-load validation.
- Validate prospective terrain support for indexed stationary material-built energy stores and unmounted material-built equipment, with typed errors and deterministic asset selection.
- Retain explicit structural support for mounted equipment and the existing exemption for unembodied capability fixtures.
- Cover direct and neighborhood terrain admission, rejected-state preservation, strict loading, and supported generation with asset preservation and save replay.
- Construct valid supported fixtures for the equipment burial, campfire overlap, and cross-chunk warmth tests without weakening their assertions.
- Document the physical contract in TECHNICAL_DESIGN.md. No persisted schema changes.

## Verification

- Baseline job 7014: both new unsupported-footing regressions failed.
- Initial full audit 7028: 2,840 of 2,841 core tests passed; the remaining warmth fixture placed a campfire without physical support.
- Corrected warmth fixture: focused job 7054 passed.
- Jobs 7048 and 7052 were interrupted by the runner watchdog.
- Final quick validation 7053: all four checks passed.
- Final full audit 7056: 2,841 core tests and 187 gameplay tests passed (3,028 total). Gameplay sample: ore / controlled-capability / variation 0x17F5566BE600568E.
- Rebuilt text-play client, job 7061, seed 24318: walked to (11,2,8), collected 1,000 g stone from 8,862.673 g, leaving 7,862.673 g at the source. Built a stone-ring campfire (store 1) with 24 MJ capacity, zero initial stored energy, and zero remaining carried matter. Session exited successfully.

## Scope and limitations

Validation covers the combined local working tree, including independent edits; the isolated staged commit was not separately built. Existing unrelated edits were excluded from staging. Campfire ignition, graphical presentation, and release performance were not exercised. This audit addresses stationary tools and material-built energy stores; it does not establish every infrastructure support invariant.

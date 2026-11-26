# Codebase audit, 2026-11-25: natural surface footing

## Scope

Continued the risk-directed review of fixed footing, material custody,
surface gathering and pickup, carried equipment capacity, work reservations,
scout patrol, respawn, and persistence. The confirmed defect was in the shared
fixed-footing owner; no parallel mining or structural legality path was added.

## Defect and correction

Finite natural surface opportunities were excluded from the floor claim used
by excavation and structural removal, although player-created loose drops
were included. Removing the last represented mass of the supporting block
could therefore leave a natural parcel suspended. These parcels have no
falling simulation.

The fixed-footing owner now covers all available non-perennial parcels
regardless of source kind. Rooted biomass retains its separate occupancy
rule, including when the visible crop is depleted. Depleted non-perennial
sites remain nonblocking. Excavation admission, stale commit checks and
held-tool batch fallback use the same typed SurfaceParcelRequiresFloor
error. No persistent schema change was required.

The existing loose-drop regression now shares its proof with a natural
opportunity regression. A two-milligram floor can accept a one-milligram
partial cut but cannot be entirely removed under a 250 g live parcel.
The test checks rejection without mutation, conservation, valid loaded
state and round-trip equality.

The scout dismantling fixture previously scheduled removal beneath a
junction with existing natural parcels. The stronger rule correctly
rejected that setup. The fixture now collects that matter into explicit
test inventory custody without advancing the patrol clock, checks mass
conservation, and retains its original scout and replay assertions.

## Verification

- Baseline regression (6795): failed as expected, accepting the full
  two-milligram cut despite the natural parcel above it.
- First full audit (6811): 2,832 of 2,833 core tests passed; the scout
  fixture described above needed correction.
- Rerun (6826): runner stall watchdog terminated the job before a result.
- Final full audit (6827): 2,833 core tests and 187 gameplay tests passed.
  Workshop controlled-capability sample, variation
  `0xDB8BE84386331BDC`.
- Quick validation (6840): all four checks passed.
- Rebuilt text-play (6841), maintained seed 24318: trekked east to
  (9,2,8), admitted 500 g berry gathering with begin gather, completed at
  simulation 21 / control 1512. Source mass decreased from 5746.64 g to
  5246.64 g and carried inventory contained exactly 500 g of berries.
  This was an ordinary gathering smoke test, not a manual full-floor
  excavation; the controlled excavation regression covers the latter.

## Working-tree preservation and limits

Code commit `dbcfbb3` includes only this pass's changes. Two files
contained pre-existing user edits; only the audit hunks were staged.
All twelve pre-existing modified files remain in the working tree.
Validation ran on the combined local tree, not an isolated commit checkout.
This pass does not certify all generated seeds, forged unsupported surface
records, rendering, or release performance.

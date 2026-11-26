# Codebase audit, 2026-11-26: retained production equipment support

## Findings and resolution

The audit traced production admission, exact output routing and reservations,
equipment occupancy, suspension, labor resumption, completion settlement,
and trusted-load reconstruction. An ordinary material-built woodworking adze
could acquire a structural mount while its freestanding production job was
suspended for failed output support. The job retained its original
support requirement, leaving resumption incompatible with the trusted-load
support-state contract.

Equipment support mode remains fixed at job admission. The shared equipment
support occupancy decision permits recovery only for suspended jobs that
require active structural support. Freestanding jobs retain unmounted
equipment until completion. Validation and commit use the same decision.
Both equipment support errors use the existing ProductionOccupancyRelease
descriptor rather than a completion tick that does not exist for paused
work. Trusted load permits temporarily unmounted support-dependent jobs
during suspension, but rejects mounted providers of freestanding jobs.
No persisted representation changes.

## Verification

- Baseline job 7247 failed the new freestanding suspension regression at
  the mounting admission assertion.
- The corrected test covers freestanding and mounted adzes, output
  suspension, denied mount, legal unmount/remount recovery, a forged
  support-mode save, preserved inbound mass, deterministic replay,
  exact completion mass, and committed equipment wear. The thermal
  occupancy test exercises retained mounting authorization against both
  scheduled and suspended freestanding production.
- Full audit 7264 passed 2852 core tests and 187 gameplay tests (3039 total).
  Fieldwork sample, ordinary-system-spatial-proxy, variation
  0x6ECAEC2577C66594.
- Quick validation 7266 passed all four checks.
- Rebuilt text-play client 7272, maintained seed 24318: collected stone
  and timber, knapped a stone tool and shaped a handle, assembled and
  picked up a 1000 g stone woodworking adze, then shaped a 1000 g log
  into 800 g boards and 200 g chips using that tool. The operation ran
  for 28 simulation ticks (2016 control ticks); displayed condition
  went from 100% to 97%. Total carried load remained 3000 g, including
  the adze. Session completed normally at control 9504 / simulation 132.

## Scope and working-tree preservation

The text-play session verifies ordinary tool acquisition and assisted
fabrication, not a manually induced structural failure. Suspension and
recovery use owner-level cross-system regression fixtures. Validation
covers the combined working tree rather than an isolated commit checkout.
The twelve independent modified files are excluded from staging. No
exhaustive procedural-seed, graphical, or release-performance claim is made.

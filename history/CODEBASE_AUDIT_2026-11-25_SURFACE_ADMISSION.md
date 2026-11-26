# Surface resource admission audit (2026-11-25)

## Confirmed defects

- Surface creation checked obstruction but did not require fixed support in generated terrain. Available drops and natural opportunities could be admitted above empty air.
- Whole-parcel scavenger relocation had a separate obstruction-only check. Split and derived parcels used the creation path and inherited its incomplete admission rule.
- A validated creation token guarded surface, world and structure revisions, but not the player-work revision. A floor-removal reservation could invalidate the physical admission without changing those guarded owners.
- Trusted loading accepted physically unsupported live surface records.

## Resolution

Surface creation, relocation and trusted loading share the physical site validator in `src/surface/state/validation.rs`. Generated positions require permanent footing, including rejection of reserved floor removal. Obstruction diagnostics retain their existing precedence. Root-bearing biomass remains physical; exhausted records without living roots do not claim support. Ungenerated positions retain abstract-world semantics until terrain admission establishes authority. Tree felling retains its explicit atomic trunk-removal exception.

The creation token records the player-work revision and rejects a stale token before mutation. Player material drops, tree felling and initial generation map that typed rejection at their boundaries. No save-schema change is required.

Regression fixtures that force invalid terrain now distinguish unsupported physical behavior from valid save continuation. Incidental finite opportunities are transferred to explicit test custody when they are not the subject of the test. Tests concerning actual stranded cargo verify rejection of invalid saves and equality of repaired-world replay, preserving material and fluid accounting and the underlying movement, defense, nutrition, work and visibility assertions.

## Evidence

- Baseline job 6937: three regressions failed on unsupported creation, relocation and trusted load.
- First full audit 6943: 2,822 of 2,838 core tests passed; 16 fixtures required valid footing or explicit invalid-world expectations.
- Job 6958 caught two unused imports; removed without warning suppression. Job 6962 was terminated by the stall watchdog and is not a pass.
- Job 6980: 2,837 of 2,838 passed. The remaining water-consumption fixture retained a live drop on forced unsupported terrain; the fixture now verifies rejection and repairs footing before replay.
- Full audit 6988: all 2,838 core and 187 gameplay tests passed. Gameplay sample settlement, variation 0x4842181C528CBFAD, ordinary-exact-local-after-disclosed-bootstrap.
- Quick validation 7000: all four checks passed on the final source and audit report.
- Rebuilt text-play job 6998, seed 24318: walked to (9,2,8), gathered 500 g berries from a 5746.64 g source, selected and dropped the full 500 g at the supported position. The source retained 5246.64 g, the pack held 0 g, and the separate loose drop was visible at control 1513 / simulation 21. The client exited normally.

The new regression tests verify both drop and opportunity rejection, whole and split parcel relocation with conserved quantities, strict loading with exhausted-history exemption, and the floor-removal reservation race. The latter confirms that complete excavation still succeeds with no dependent parcel and conserves matter.

## Scope

Evidence covers the actual combined working tree, including independent local edits not included in this audit patch. No isolated checkout verification, graphical runtime audit or release performance benchmark is claimed. Unrelated changes in the patrol and structural placement test files remain outside the staged audit patch.

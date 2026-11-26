# Storage enclosure footing audit (2026-11-26)

## Findings

Ordinary storage construction rejects an unsupported generated-world site, and full excavation protects enclosure footing. Terrain generation and trusted loading lacked the same requirement. An empty field box has an embodied construction mass even with zero stored material. Both terrain admission of an abstract unsupported box and trusted promotion of a box with a removed floor were accepted by baseline production code.

Structural mounting accounts for enclosure body mass and contents through the stockpile load channel. Fixed-footing removal nevertheless treated a mounted enclosure as an independent ground claim. An explicit support assignment must carry its physical load without demanding an additional underlying terrain cell.

## Resolution

- Use one enclosure footing claim predicate for terrain removal, prospective generation and trusted load. Unmounted enclosures claim fixed footing even when empty; mounted enclosures use explicit support.
- Reject unsupported stationary enclosures with typed generation and logistics validation errors. Preserve existing solid-terrain collision diagnostics and indexed deterministic identity ordering.
- Retain the existing unmount admission that rejects physical custody without fixed footing.
- Regressions cover direct and neighborhood admission, exact state preservation on rejection, strict save promotion, and supported generation with preserved inventory. The mounted enclosure test verifies the claim transition, represented matter conservation, rejected unmounting and valid saved-state reconstruction.
- Adjust the supported-generation fixture to select actual seeded terrain rather than suspend the box above it. Update TECHNICAL_DESIGN.md; no persisted schema change.

## Verification

- Job 7065: both new baseline unsupported-enclosure tests failed at their rejection assertions.
- Job 7076: test compilation exposed an assertion requiring equality and debug traits on the internal footing owner. Corrected with pattern matching without adding production trait implementations.
- Job 7092: full audit passed 2,844 core tests and 187 gameplay tests (3,031 total). Settlement sample, ordinary-exact-local-after-disclosed-bootstrap, variation 0x436C3BCEFC680BAD.
- Job 7094: all four quick checks passed.
- Rebuilt text-play client, job 7102, seed 24318: gathered 2,000 g of timber at (10,2,9), leaving 34,279.054 g at the source. Crafted boards and a 1,600 g rough timber field box body. Placed box stockpile 2 at (9,2,10), capacity 10,000 g, leaving 400 g of chips carried. Job 7106 replayed the same ordinary commands and stashed and withdrew exactly 400 g of chips; carried and stored amounts reconciled at every transfer. Both sessions exited successfully. A first script invocation failed to parse before running the client; the corrected commands produced these observations.

## Scope

The audit validates the combined local working tree, including independent edits. The isolated staged commit was not separately built. Existing unrelated changes were excluded from staging. Respawn temple-resident movement is covered by the existing simulation-tick guard; no additional revision guard was added. Mounted enclosure floor removal was assessed through the fixed-footing owner and a controlled floor mutation, not a separate manual excavation session. Graphical presentation, release performance and other infrastructure support invariants were not certified.

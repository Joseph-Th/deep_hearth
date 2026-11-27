# Equipment service and upgrade audit: 2026-11-27

## Findings and changes

- Text play exposed construction, pickup, use, wear and disassembly, but no ordinary command to service or upgrade an existing tool. The first-copper progression could create copper reinforcement without exposing the additive upgrade route.
- Added service and upgrade command routing through the existing equipment owners. Gear lists idle worn carried and local equipment, replacement and spent materials, service duration, target condition and admission. Upgrade rows identify the current stationary equipment number, authored target definition, additions and admission. Service uses the ordinary timed maintenance path; upgrade retains the existing equipment identity.
- Equipment maintenance and upgrade projections now expose full typed admission alongside material-only readiness. The maintenance catalog calls the canonical service validator. Upgrade shares context and transaction validation and reuses the exact selected material. The adapter renders the owner result without repeating either resolver. Direct actions always revalidate.
- Updated the gameplay inventory gesture consumer for the non-Copy maintenance projection, and current technical and text-play documentation.

## Evidence

- On the previous implementation, the extended first-copper terminal regression failed because the worn pick had no service row.
- Extended the ordinary progression through real gathered stone and copper, including missing replacement rejection, invalid selector rejection, carried and stationary service, unchanged condition until scheduled completion, save/load continuation, 800 g spent stone recovery, busy upgrade rejection, stable equipment identity, 1,020 g upgraded mass, and exact matter conservation.
- Core regressions distinguish material readiness from complete admission during unrelated exclusive work. The same canonical validator returns the reported blocker.
- Quick validation passed all four checks. Terminal suite: 137 passed. Full audit: 2,886 core and 187 gameplay tests passed (sample foundry; controlled-capability evidence; variation 0xD00199477F3B2EF3). Production Clippy gate passed.
- Rebuilt actual terminal executable, seed 24318: collected stone and timber, crafted tools, surveyed copper, mined 400 g ore, broke and sorted four 100 g batches, recovered 26 g native copper, fabricated 20 g reinforcement, serviced the 99.85% pick to 100.00% using 800 g replacement stock and recovered 800 g scrap. The displayed upgrade 1 8 consumed reinforcement and produced the 1,020 g copper-reinforced pick at the same location. No command rejections occurred in this replay.

## Boundaries

No physical recipe, service cost, timing, wear, conservation law or save schema changed. This pass exposes existing equipment service and upgrade rules; it is not a complete industrial-control interface or exhaustive manual audit of every owner. Existing unrelated changes remain outside the commit. The audit ran on the actual working tree including those pre-existing changes.

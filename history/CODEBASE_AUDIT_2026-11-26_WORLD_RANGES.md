# Codebase audit: generated world and standing trunk ranges

## Confirmed scaling defect

The generated-chunk rectangle query used two X-only partition searches, then linearly discarded excluded Z columns. The construction trunk lookup likewise ranged only over X and checked every tree in that slab. Both allowed remote north-south exploration to inflate a local query even when the local result was unchanged. Callers include prospective structural clearance, generated foundation checks, trusted structural integrations, and field prospecting terrain-authority checks. The existing small-radius canopy and landmark query already bounds both horizontal axes; it is unchanged.

## Correction

The sorted chunk vector now seeks into the first and last full X/Z keys, and uses partition searches on the remaining slice when an excluded Z run is encountered in an intermediate X row. Matching entries retain their canonical order. No missing coordinate space is enumerated. `ChunkCoord::covering_span` centralizes the half-open voxel to inclusive chunk conversion for both selection and complete-authority assessment. The latter still requires all intersected columns and supported vertical bounds.

The existing bounded B-tree traversal now accepts both real ordered voxel key types through `OrderedVoxelKey`. The standing tree map retains its X/Z/Y key and its persistent canonical order. Trunk clearance includes the root layer immediately below the requested volume, with saturating subtraction at the minimum signed coordinate; exact root and checked upper-cell intersection still choose the same first obstruction. Logistics and surface callers retain their existing X/Y/Z keys. No extra index, saved field, mutation, or serialization change was needed.

## Measured result

Temporary ignored test `audit_world_range_timing` populated an internal `WorldState` with empty `WorldChunk` records and standing tree records, one at each z in -remote..=0 at chunk X=0, tree root `(0,0,16*z)`. The local box was `[ (0,0,0), (1,2,1) )`. The fixture is deliberately query-only, not a claim of complete seed-valid world state. The same actual three query methods were measured before and after, with `black_box` on inputs and results, three runs of 1,000 calls per query, in the unoptimized local test build. The returned chunk count, authority result and first trunk were asserted identical. The initial timing test omitted two imports; after adding them, both baseline and revised runs completed. The timing fixture was removed from the final tree.

| Fixture | Query | Baseline median | Revised median |
| --- | --- | ---: | ---: |
| Local only | Intersecting chunks | 0.0942 ms | 0.0968 ms |
| Local only | Generated authority | 0.1230 ms | 0.1401 ms |
| Local only | Trunk clearance | 0.1062 ms | 0.1307 ms |
| 20,000 distant plus local | Intersecting chunks | 166.6182 ms | 0.3208 ms |
| 20,000 distant plus local | Generated authority | 166.1511 ms | 0.3703 ms |
| 20,000 distant plus local | Trunk clearance | 692.0106 ms | 0.6673 ms |

The remote fixture produced roughly 519x, 449x and 1037x reductions respectively. Small-fixture timings increased slightly, most visibly for trunk clearance, with absolute differences below 25 microseconds per 1,000 calls. Timings are empirical debug-build microbenchmarks, not whole-game frame time, release throughput, or memory measurements. Cost can still depend on occupied rows, selected results and index search depth; this is not a constant-time guarantee.

## Verification

- Added direct reference comparisons for 4,356 ordinary chunk rectangles plus four extreme/vertical cases, checking exact ordered intersection and complete generated authority. Added 9,261 ordinary trunk boxes plus five extreme cases, checking the first obstruction against an independent ordered full scan. Total 13,626 bounds. The fixtures isolate query semantics, including coordinate extremes not used as actual generated worlds. Existing live generation, terrain occupancy, woodland and trusted-load tests remain active.
- `python ci.py quick`: four checks passed; `git diff --check` passed. `python ci.py audit --all`: 2,864 core tests and 187 gameplay tests passed, total 211.0 seconds, sampled `ore`, evidence `controlled-capability`, variation `0xEE0E4414C8047EC6`.
- Rebuilt the actual `text-play` executable and ran the same ordinary seed 24318 script before and after: `e 20; wait 10; e 20; wait 10; e 20; wait 10; gather 1 2000; wait finish; craft 7 2; wait finish; w 20; wait 10; s 20; wait 10; gather 1 2000; wait finish; craft 8 2; wait finish; assemble 21; take 1; assemble 6; take 1; w 20; wait 10; s 20; wait 10; wait boundary; prospect copper 1; wait finish; prospect copper 2 with 1; wait finish; knowledge; quit`. Both produced byte-identical 933-line stdout (SHA-256 `a02d5ffac77a209701c9ee1d2efa79d439658d5aba932fd47753d7226ddcc5ab`). The acquired copper sample at (9,2,10) retained 75,000–125,000 ppm abundance, 400–450 MPa hardness, 25–30 kg sampled mass, and ticks 199/223 for the two observations. Temporary transcript removed. This ordinary run proves survey/observation behavior, not every possible construction or world-layout outcome; construction is covered by the existing owner and integration tests.

## Scope and limits

Reviewed chunk selection, generated authority, living tree keys, construction clearance and foundation callers, and adjacent structural broadphase. `ElementSpatialIndex::candidates_in_columns` still uses an X-only range over its independent column map. That distinct path was identified but not benchmarked or altered. No other performance improvement is claimed. All twelve unrelated pre-existing terrain/custody changes were preserved; the proof ran on the combined working tree, not an isolated clean checkout.

# Codebase audit: bounded spatial ranges

## Confirmed scaling problem

Logistics chunk-column and nearby position queries, and surface available/perennial bounds and chunk queries, took an X-only B-tree range and filtered its entries for the remaining axes. An explored north-south strip therefore made small local queries visit distant positions sharing an X coordinate. Exact-point queries did not have this problem. The affected callers include local finite light, warmth, water landmarks, equipment observations, forage landmarks and scent preflight, construction clearance, and terrain generation custody checks.

## Correction

`src/spatial/indexed_positions.rs` owns an inclusive box iterator over the existing X/Y/Z-ordered voxel map. It consumes the current B-tree range normally while positions match. On an excluded Y or Z entry it seeks to the next possibly matching coordinate, skipping the intervening run. A seek always advances strictly; guarded increments preserve the signed coordinate extrema. Empty space is not enumerated. The existing X/Y/Z order and per-position ID order remain intact, including first-conflict diagnostics. `ChunkCoord::inclusive_column_bounds` centralizes the inclusive horizontal column endpoints using the existing bounded origin; it preserves full vertical custody and returns no bounds for unrepresentable chunks. Surface half-open bounds convert their validated exclusive endpoints to inclusive endpoints. All four stationary indexes and both surface indexes reuse the same traversal. Exact-point lookups, authority records, mutations, index reconstruction and serialization are unchanged.

The approach does not introduce a parallel index or allocation for query results, and does not enumerate remote entries within an excluded run. The work can still depend on the number of distinct occupied X/Y rows, especially in full-height chunk queries, and on the cost of B-tree seeks. It is not a claim of constant time or a whole-world performance guarantee.

## Measurement

Temporary ignored test `audit_spatial_strip_measurement` in the logistics owner used actual `LogisticsState::apply_energy_store_placement`, `energy_store_locations_near`, `energy_stores_in_chunk` and `energy_stores_at`. The fixture was location-only: three stores at (0,0,z), z=0,1,2, and optionally 20,000 further stores at z=100+16*i for i=3 through 20002. Each of three runs measured 1,000 calls to each query with `std::hint::black_box`; radius 14, chunk (0,0), exact point (0,0,0). Both range queries returned three identities throughout. `cargo test --locked -j 4 --lib audit_spatial_strip_measurement -- --ignored --nocapture` ran before and after the change in the same unoptimized local test configuration. The temporary timing test was removed before final verification; wall-clock timings are not a CI gate.

| Fixture | Query | Baseline median | Revised median |
| --- | --- | ---: | ---: |
| Three local | Nearby | 0.7003 ms | 0.7186 ms |
| Three local | Chunk | 0.6047 ms | 0.6096 ms |
| Three local | Exact point | 0.1068 ms | 0.1031 ms |
| 20,000 distant plus three local | Nearby | 625.5979 ms | 1.6141 ms |
| 20,000 distant plus three local | Chunk | 809.8194 ms | 1.5567 ms |
| 20,000 distant plus three local | Exact point | 0.1644 ms | 0.1630 ms |

The baseline remote third sample was slower (near 1039.5793 ms; chunk 1263.7579 ms). Medians are reported rather than the slowest sample. The remote fixture showed roughly 388x and 520x improvements in the two range queries, respectively. The small fixture had comparable times, with a small measured increase in the two ranges. No release-profile, end-to-end frame-time, or memory benchmark was performed; no speedup is claimed for all world layouts or for surface calls separately measured. The source trace establishes that surface calls use the same replacement range traversal.

## Verification

- New tests compare exact ordered results to independent direct filtering for 21,952 dense/sparse boxes apiece, signed-coordinate endpoints, empty maps and gaps across the full signed coordinate range. Both passed. Existing integration cases now include remote same-X surface/stockpile positions, out-of-range Y/Z light sources, unrepresentable chunk coordinates, and inclusive extreme columns, retaining save reconstruction, depletion and tie-order assertions.
- `python ci.py quick`: four checks passed. `python ci.py audit --all`: 2,862 core tests and 187 gameplay tests passed, total 243.4 seconds; sample `power-provider`, evidence `ordinary-exact-local-after-disclosed-bootstrap`, variation `0x93FAC5D532779155`. `git diff --check` passed, with only the normal line-ending warning on the edited logistics file.
- Rebuilt `text-play` in the actual project. Seed 24318 ordinary commands `look; trek e 1 steady; gather 1 500; wait finish; trek e 1 steady; gather 1 500; wait finish; look; inv; recipes; quit` produced a byte-identical 301-line stdout transcript before and after (SHA-256 `5b07c75b3786a8c50883692fc16aed6d1176d60a093bab956d4170450af9e2ae`). Visible water, travel, exact-local harvest and carried matter remained unchanged: 500 g berries, 500 g nuts, 1,000 g total. Temporary baseline transcript removed.

## Scope and limits

Reviewed the logistics and surface indexes, coordinate arithmetic, caller ordering and observation/terrain/construction interfaces. The geological and spring-root moisture indexes already bound both horizontal axes. Tree trunk construction and generated-chunk construction queries still have separate X-slab filters; they use different storage and were not part of the measured hot paths or altered in this pass. No speculative rewrite or performance claim for them. All twelve pre-existing terrain/custody modifications remain untouched; the checks ran against the combined working tree rather than a clean isolated checkout.

# Codebase audit: geological observation replay

## Corrected defects

Trusted-load validation previously required authored abundance and hardness evidence but accepted a null resource-scale measurement even when the same authored survey necessarily measures a unique exact-footprint body with definite material presence. Live survey and historical replay now share the same body-localization predicate. Historical method matching checks both presence and absence of resource-scale metadata and uses the production quantizer to validate the persisted band.

The old width/alignment predicate could also accept a widened ceiling band for a body whose entire possible historical mass interval lies in the previous ordinary quantization bucket. The new check requires an actual positive mass within the remaining-to-initial interval that produces the exact persisted band. A historical interval may span more than one bucket; this does not reconstruct unrecorded partial extraction times. No save-schema shape or runtime acquisition policy changed, and no missing measurement is synthesized on load.

The obsolete independent bucket-shape predicate was removed. Mining execution fixtures now derive required resource-scale metadata through the existing canonical projection, exposed only to crate tests. The depleted-body hardness fixture retains its corresponding mandatory mass evidence. Invalid-evidence fixtures remain invalid.

## Reproduction and validation

- Extended the real detailed-field-survey regression to round-trip a completed survey and then replace its persisted resource mass with null. Before the production fix, the test failed because trusted loading returned success. After the fix, the exact authored-method mismatch is returned.
- The ceiling tests check that a valid widened band is accepted, a narrow fabricated band is rejected, and a widened band with only an earlier-bucket mass is rejected even though the band contains that mass. The last case is a source-traced defect with post-fix regression coverage; it was not independently run against the old code.
- `cargo test --locked --lib --quiet`: 2,858 passed.
- `python ci.py quick`: all four checks passed.
- `python ci.py audit --all`: 2,858 core and 187 gameplay tests passed; workshop sample, controlled-capability evidence, variation `0x6EA2569D6E42D874`.
- Rebuilt and ran the actual text-play executable on controlled opening seed 24318. Gathered stone and timber, crafted parts, assembled and took both stone tools, then completed the surface and instrumented copper surveys at (9,2,10). The detailed observation at tick 223 reported 75,000–125,000 ppm abundance, 400–450 MPa hardness and 25–30 kg surveyed mass. Saved and reloaded successfully with both observations intact. A separate copy with only the detailed measurement replaced by null was rejected by the executable with the authored-method mismatch. Both temporary save slots were removed.

## Scope and limitations

Reviewed persistence decoding, derived-index reconstruction, slow-tick settlement, transaction commit boundaries, and cross-owner logistics, structural, surface, resident, world, and geological validation. The confirmed defect concerned replay rather than ordinary acquisition. Historical partial extraction chronology remains bounded by the existing persisted facts, not reconstructed exactly. No speculative module split or compatibility layer was introduced. All 12 pre-existing terrain/custody edits were preserved and excluded from the audit commit; validation ran against the combined working tree. This targeted review is not an exhaustive correctness proof.

# Codebase and gameplay audit, 2026-11-09

**Role:** Dated review evidence, not current design or testing authority. The project
contracts remain in `AGENTS.md`, `ARCHITECTURE.md`, `STATUS.md`, `TESTING.md`, and
`GAMEPLAY_EVALUATION.md`.

## Review scope

The review followed the owner/operation map rather than counting source lines or
rewriting invariant assertions to satisfy static-analysis scores. Source paths
inspected included control-clock stepping and fast-forward admission, continuous
body collision and exertion, survival resource settlement, temperature-safe
fluid transfer, finite energy storage and passive losses, forage regrowth and
root moisture, woodland growth and felling, ore separation recovery projections,
inventory lot identity/coalescing/stack projection, and trusted persistence.

The project was already dirty at the start of the review. Existing changes in
combat, structural, kobold, energy, survival, logistics, world, and other owners
were treated as someone else's uncommitted work, not rewritten or staged.

## Confirmed defect and repair

`src/inventory/stack_projection.rs` grouped otherwise compatible non-food lots
into one read-side stack but used the first lot's storage exposure as the
representative. When another lot in the group was older, the visible age and
retained storage-history projection understated the oldest exposure. The writer
in `src/inventory/state/lot_mutation.rs` already chose the oldest exposure when
authoritatively merging those same lots, so the read and write paths disagreed.

The repair places the shared selection in
`src/inventory/coalescing.rs::merge_compatible_storage_histories`. Both the
writer and the read-only stack now use it. This preserves the oldest existing
history for non-food material and requires equivalent future projections when
merging age-sensitive food. No new persistent state, cache, compatibility layer,
or public API was introduced. Tests cover both lot orders, incompatible food
histories, and a current-schema trusted inventory containing fragmented but
merge-compatible lots.

The trusted-load regression was verified against the original projection:
it failed with observed exposure `0` versus the correctly projected `1,000,000`
parts. It passed with the corrected owner decision.

## Observed gameplay outcomes

- The generated wilderness episode exercised exact-local food acquisition,
  hydration decisions, thermal comfort and fire timing, scavenger loss and
  recovery, carrying, shelter, and ore progression. It completed with matter
  conserved. No additional confirmed physics or player-action defect emerged.
- The fieldwork report exercised bounded barren-site search, reserve
  uncertainty, retooling, shortfall recovery, and a separate generated-world
  mining expedition. The expedition returned 30 kg through two finite-capacity
  haul trips with conservation intact.
- The settlement report exercised machine investment, bounded throughput,
  finite flywheel charging, wear-based servicing decisions, and delegated work.
  Its 160-batch shared-bank worksite had 10 charge cycles and demonstrated
  actual concurrent player construction during delegated production.

These are particular reported worlds and actor policies, not guarantees about
all seeds, all player strategies, human usability, or unimplemented content.

## Verification and limits

- `python ci.py quick`: pass after the repair and shared-helper refactor.
- `python ci.py audit --all`: pass after the repair, 2,573 core tests and 198
  gameplay tests (sample: progression).
- `python ci.py gate --lint`: pass after the shared-helper refactor.
- `python ci.py report --scope wilderness`, `fieldwork`, `settlement`: pass.
- Focused trusted-load regression: observed failing before the repair and
  passing after it.

A generic all-targets Clippy invocation with `-D warnings` reported existing
repository-wide `expect`/`unwrap` violations, while the repository-owned lint
gate passed. These invariant assertions were not mechanically rewritten. No
active backward-compatibility implementation was identified in the inspected
paths; explicit rejection of obsolete persisted schemas is not compatibility
scaffolding. This was a broad contract and risk-based source review, not a
claim that every source line or every possible gameplay state was executed.

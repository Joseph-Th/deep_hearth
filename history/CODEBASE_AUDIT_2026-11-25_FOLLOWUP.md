# Codebase and gameplay audit continuation, 2026-11-25

**Role:** Dated evidence following `CODEBASE_AUDIT_2026-11-25.md`.
Current design and verification authorities remain `AGENTS.md`,
`TECHNICAL_DESIGN.md`, `TEXT_PLAY.md`, and `TESTING.md`.

## Queued contextual actions must retain physical intent

The earlier queue persisted the actor's body origin, active selection, and
view direction but resolved the action again at admission. A regression
queued a floor-directed timber construction with a local empty campfire,
placed a solid obstruction at the destination before the due boundary,
then advanced normally. The original implementation returned
`CombustionFuelAdded` with 12 MJ instead of rejecting changed context.
The player had not moved or changed material selection.

Buffered contextual use now records the resolved physical action and
target (grounded destination, supported member and destination, combustion
store, or combustion assembly definition). Admission revalidates that
target with the existing geometry, material, and energy owners. It does
not retry general contextual dispatch on a target change. Ambiguous
combustion choices return the typed ambiguity instead of reserving an
unidentified target. The unit deferral sentinel is removed; the successful
buffer operation directly returns the queued outcome.

Two regressions exercise target changes after a real direct-control
request, including a new fire becoming admissible after the original fire
fills. Both assert rejection, unchanged relevant custody and output,
matter conservation, trusted-state validity, and deterministic replay.
Current-schema decoding requires the target and hand selection fields.
The schema advances from 134 to 135 without compatibility scaffolding.

## Local illumination

Empty stores and stores without a light emission profile no longer perform
emitter occlusion checks. The bounded breadth-first propagation search
records blocked cells as visited, preventing repeat spatial lookups for
the same cell within one search. The Manhattan bound uses exact widened
unsigned coordinate differences; three u64 values fit in u128. Light
falloff, opaque-cell behavior, finite emission, and stable strongest-source
selection are unchanged. No frame-time improvement is claimed without
measurement.

## Verification and observation

- Before correction, the new construction-retargeting regression failed
  with a 12 MJ fuel addition; afterward it passed with
  `BufferedContextChanged`.
- `python ci.py report --scope wilderness` passed before the correction
  (variation `0x3DC1C602AD7934D5`).
- `python ci.py audit --all` passed after the changes: 2,829 core and
  187 gameplay tests (liberation sample, ordinary exact-local evidence
  after disclosed bootstrap, variation `0x0A51711B14026FCC`).
- Production text play on maintained seed 80705 gathered timber and stone,
  constructed a fire and post, crafted boards, and used `look e-shallow`
  and `use e-shallow`. The preview targeted (9,0,11). The action queued
  at control tick 43345, admitted at 43416, and completed at 48096 with
  an active 6,500 g panel at that same destination. The campfire decreased
  from 6.7728 MJ to 6.060 MJ through elapsed dissipation rather than
  receiving an unintended fuel charge. The panel was correctly not
  described as shelter over the actor's current position.

The original twelve unrelated modified files remain outside this audit's
commit. The audit is representative cross-system review and regression
evidence, not a proof of all generated worlds or a release/graphics
benchmark.

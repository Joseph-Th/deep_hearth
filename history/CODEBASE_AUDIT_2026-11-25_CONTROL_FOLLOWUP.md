# Control-boundary audit follow-up (2026-11-25)

## Defects reproduced
- Baseline job 6702: an identical secondary use sample at the due simulation boundary discarded the pinned action and dispatched against the changed context. An obstructed queued timber construction could add 12 MJ to a campfire instead.
- A held structural-break sample at its due boundary could retarget obstructing terrain rather than reject the changed original target (baseline job 6709).
- A validated pending-action token could commit after a control-clock advance with unchanged logistics and survival revisions (baseline job 6708).

## Corrections
- Centralized due-boundary arbitration and shared held-context matching in the direct-action buffered module. Matching origin, selection and direction continues the existing action; genuinely changed input can supersede it. A settled secondary result takes precedence over new dispatch for that sample.
- Primary targeting respects a matching pending action before tree selection and rechecks tree priority on settlement.
- Pending-action commit tokens bind the control tick as well as owner revisions.
- Split buffered contextual regression cases into their own test module; cover held and released samples, deterministic save/load replay, changed targets and exactly-once valid fuel charges.
- Updated technical design; save schema remains 135.

## Verification
- Quick lane: four checks passed (6716).
- Full audit: 2,831 core and 187 gameplay tests passed (6717); ordinary exact-local after disclosed bootstrap, power-provider sample, variation 0x50023636AF13FA65.
- Terminal gameplay (6734, seed 80705): gathered timber and stone, built and charged campfire, built post, crafted boards, previewed panel at (9,0,11). First use queued at control 43345, waited 70 control ticks, repeated identical use at boundary 43416. Construction completed at 48096; active panel 6500 g, remaining boards 700 g; campfire declined normally from 6.7728 MJ to 6.060 MJ. No additional charge.
- Twelve unrelated pre-existing working-tree changes excluded.

## Remaining scope
The optional selection on the saved pending-secondary record permits structurally impossible empty-hand records to be admitted by low-level validation; due settlement safely rejects them. Narrowing that representation and updating fixtures remains separate hardening work. This pass is not an exhaustive graphical, every-seed, or performance benchmark.

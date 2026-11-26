# Codebase and gameplay audit, 2026-11-25

**Role:** Dated audit evidence. Current project authorities remain the project
and workspace `AGENTS.md`, `ARCHITECTURE.md`, `TECHNICAL_DESIGN.md`,
`STATUS.md`, `TESTING.md`, and `TEXT_PLAY.md`.

## Scope and preservation

Reviewed representative production paths across control dispatch, physical
targeting and work admission, slow-tick settlement and fast-forward,
equipment custody, inventory and material input selection, terrain support,
surface acquisition, weather and moisture, survival physiology, respawn,
and save/load invariants. Traced cross-owner rules against the current
architecture and existing regression contracts. Scanned Rust source for
obsolete and compatibility paths and examined relevant callers instead of
using source size or diagnostic counts as a measure of quality. This is a
cross-system audit with broad executable coverage, not a line-by-line proof
of every module or an exhaustive exploration of every generated world.

The starting `main` working tree contained twelve modified files, mainly
terrain-footing and custody changes, plus the board-fuel regression in
`src/simulation/mod_tests/structural_placement.rs`. The audit retains
those changes. Only formatting was corrected in the pre-existing regression
test, to satisfy the repository's formatting gate; that file remains
separate from the audit-owned commit.

## 1. Selected boards could not fuel a local campfire

The initial broad checkpoint failed
`boards_without_a_construction_target_can_fuel_the_local_fire` with
`NoStructuralPlacementTarget`. The selected-board dispatch branch
returned the cover-placement miss even when no terrain storage target
existed, so it never reached the existing combustion owner.

The dispatcher now preserves physically pointed cover and storage
precedence, then tries canonical selected-fuel combustion when neither
placement target exists. With no eligible local combustion target, the
original placement miss is retained. The board-specific cover routine now
returns a concrete outcome, eliminating its redundant optional return,
duplicate commodity guard, and the caller's invariant `expect`. No
additional fuel, material, or placement owner was introduced.

The pre-existing regression exercises real generated-world targeting,
exact board and log custody, released energy, unchanged structure/work
state, matter conservation, trusted loading, and deterministic replay.
The corrected core checkpoint passes it alongside the existing
cover-construction and fuelwood-rack placement contracts.
`TECHNICAL_DESIGN.md` and `TEXT_PLAY.md` describe the current precedence
and authored fuel charge.

## 2. Equipment carrying exposed inaccessible equipment restrictions

Both pickup and drop consulted equipment definition, mounting, and
occupancy before establishing actor locality. A new owner-adjacent
regression mounted a portable tool, verified the local mounted blocker,
then relocated the player. Before correction, remote pickup returned
`EquipmentMounted` instead of `EquipmentRemote`.

Equipment carrying now delegates locality to the same
`validate_player_equipment_access` operation used by other equipment
owners, ahead of target-specific portability and occupancy checks. Pickup
no longer implements a separate coordinate comparison. Both remote carry
directions reject without changing state; local mounted pickup remains
blocked. The regression also verifies trusted-state validity after
rejection. Existing revision-bound capacity and footing commitments
remain intact.

## 3. Legacy and structural review

The examined legacy/obsolete references concern rejected prior save
schemas, stale selections, or active material compatibility rules. They
do not provide runtime backward compatibility. No verified dormant
compatibility implementation was identified for deletion. The primary
combustion profile and default-fuel operation still have active contracts
and callers. No new compatibility scaffolding or alternate authority was
introduced. Existing module splits were retained where owner boundaries
were already clear; no file was split solely to improve a metric.

## Executed evidence

- Initial `python ci.py audit --all`: 2,825 core tests passed, one failed;
  gameplay did not run.
- New equipment regression before correction: failed with the wrong
  mounted-vs-remote error.
- `python ci.py quick`: all four checks passed after correcting the
  formatting of the existing board test.
- Corrected `python ci.py audit --all`: 2,827 core tests and 187 gameplay
  tests passed. Gameplay variation `0x14D4456C734768C9`,
  power-provider sample; ordinary exact-local evidence after disclosed
  bootstrap.

### Actual text play

Ran the production terminal client on maintained seed 80705, gathering
14 kg of timber and 1 kg of stone, constructing and charging the local
campfire, setting a timber post, and crafting nine board batches. The
initial command sequence stopped short of completing the craft; replay
with `wait finish` completed it. The final observed inventory contained
750 g logs, 1,800 g chips, and 7,200 g boards. With the board stack selected,
`look up` found no targetable block or load-bearing member and `use up`
reported “Added 6.000 MJ of fuel energy.” The fire increased from
6.7728 MJ to 12.7728 MJ, boards decreased to 6,700 g, and logs and chips
remained unchanged. No special state injection or debug material was used.

Also exercised a natural damp-grove world with seed 1837596006.
From (-217,-8,-259), ordinary steady travel moved 12 cells north and four
east, finishing grounded at (-213,-13,-271) after 423 control ticks
(21.15 simulated seconds). Local gathering and drinking correctly rejected
where no exact-local source existed despite visible distant resources.
The exploration was a bounded observation, not a claim of universal
natural-world solvability.

No graphics or audio output was modified. The audit does not claim a
complete graphics, release, soak, or every-seed verification.

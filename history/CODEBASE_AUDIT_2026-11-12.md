# Codebase and gameplay audit, 2026-11-12

**Role:** Dated review evidence, not current implementation authority. The live contracts
are `AGENTS.md`, `ARCHITECTURE.md`, `TECHNICAL_DESIGN.md`, `TESTING.md`, `STATUS.md`, and
`GAMEPLAY_EVALUATION.md`.

## Scope and approach

The `main` worktree was clean at entry. The workspace and project agent guides,
review checklist, owner map, technical design, testing contract, gameplay-evaluation
rules, current reachability, and prior dated audits were consulted. The source
review followed control-rate and slow-tick orchestration, delayed labor and
equipment reservations; continuous body geometry, fixed versus movable footing,
world targeting and excavation; generated terrain, trees, forage, weather and
fluid cycles; surface resources and inventory custody; combat contact and
resident feeding, drinking and hearing; and exact accounting and persistence.
This was an owner-guided cross-system review, not a line-by-line proof of the
entire repository.

Searching for compatibility, legacy, deprecated, placeholder and obsolete
paths did not establish an active compatibility subsystem suitable for deletion.
The existing tests that reject old save payloads enforce the current strict
loader contract, rather than providing old-format compatibility. No speculative
deletion or structural churn was introduced.

## Confirmed outcome defect and repair

A slow block-excavation job may place fragments in the player's carried
inventory or, when capacity is short, materialize a permanent surface parcel at
the player's discrete position. Unlike ordinary material drops, ground caches,
portable equipment and energy-store assembly, the overflow route checked only
the player's work footing, which can be supplied by the edge of the continuous
body on a neighboring ledge. The parcel could therefore be created over an
empty shaft without a fixed support beneath its actual voxel. Its physical
location was persistent, yet it had no falling simulation.

The canonical excavation admission now checks the logistics owner's existing
`player_placement_has_fixed_floor` assessment for generated-world overflow
destinations. Carried output still needs only the actor's work footing. An
explicit typed error explains an unsupported overflow destination. A surface
plan also binds the structural revision before committing. Trusted-load
validation enforces the same fixed-site rule for durable surface-bound work,
instead of accepting a fabricated output promise over empty space.

Held-tool primary excavation chooses a batch based on current feasibility.
The initial correction exposed a second interaction: the selection routine
would treat the new overflow error as final even when reducing the batch to
available carried capacity would resolve it. That error is now amount-reducible
in the existing canonical selection, so the familiar mining gesture proceeds
with the largest authorized carried batch and completes on its normal schedule.

The output-custody portion of the trusted-load validator was extracted into a
focused private function to keep scheduling, energy and hydration validation
independent from physical custody. It now uses the block definition's canonical
fragmented lot, preserving graded-ore composition instead of validating a
separately reconstructed pure-material lot. No compatibility aliases, new
persisted fields, duplicate gravity rules or alternative mutation path were
added. `ARCHITECTURE.md` records the resulting shared placement contract.

## Evidence

The new generated-world regression constructs a real ledge under only the edge
of the actor's body and an empty discrete placement voxel. It proves that an
oversized explicit excavation request is rejected without mutating state or
matter; a carried-output request remains admissible; and held primary control
reduces the requested 200,000 mg to the 199,999 mg that fits, then finishes
using the normal control tick, leaving correct world and inventory ownership,
exact conserved matter and a valid trusted state. A separate saved-work
regression proves that removing that center footing makes both in-memory
validation and save loading reject a surface-output promise.

Executed verification:

- Targeted excavation-overflow owner tests: **2 passed**.
- `python ci.py quick`: **passed** all four build-free checks.
- `python ci.py audit --all`: **passed** 2,644 core tests and 198 gameplay tests.
- `python ci.py report --scope wilderness`: **passed**.
- `python ci.py report --scope all`: **passed**; examined generated-world
  provisioning, climate, scouting, conservation, mining and hauling alongside
  explicitly labeled exact-local, spatial-proxy and capability evidence.
- `python ci.py gate --lint`: **passed** after correcting redundant borrows in
  the regression helper.

The gameplay reports provide deterministic modeled consequences, not a visual
inspection of a rendered game or proof of every possible generated seed. The
maintained progression evidence still explicitly separates spatial proxies,
controlled infrastructure and modeled handoffs from a single ordinary-player
journey. Those are existing integration limits, not grounds for inventing
unowned game systems during a code audit.

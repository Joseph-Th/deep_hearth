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

## Same-day follow-up: finite scout meal and detour decisions

A separate owner-guided pass inspected the game clock, active body and work
authority, generated terrain and forage, weather and natural-water custody,
surface and inventory transfers, resident patrol/feeding/hydration, combat,
mechanical energy selection, and save/load continuation. Existing explicit
current-schema rejection was retained; searches for legacy, deprecated, and
compatibility branches did not reveal a justified obsolete runtime to remove.
The review was broad, not a formal line-by-line proof of every system.

A concrete gameplay inconsistency was found at the hungry scout's loose-drop
route ranking. It judged a nearby intact parcel or recoverable bulk split against
the entire 20 g meal, even when the den feeding owner had already credited a
partial meal. The wild-forage planner and return-edge cargo decision correctly
used the remaining shortage. This disagreement could cause an unnecessary
two-edge food raid and needless bulk handling for a scout missing only 2 g.

The scout's outward food decision now supplies its actual outstanding ration
to the shared local and distant loose-drop selectors. Their one-cell filter
and rank calculation are unified, and the route planner distinguishes food
shortage from non-food salvage with an explicit optional quantity instead of
a boolean. Non-food scouting now returns before food-only source rescans.
No new owner, state field, migration, or duplicate custody path was introduced.
The current technical design records the resulting decision contract.

Two new owner tests establish the change. A read-only route regression checks
the 2 g versus 20 g decision, unchanged state, exact matter and valid persistence.
A physical scheduled continuation lets the resident first eat 18 g at the den,
then choose and carry a nearby 5 g parcel rather than a distant 100 g parcel or
a neighboring 2 kg bulk pile. The resident consumes the remaining 2 g and
retains 3 g in the den; both other sources remain untouched. Save/load replay,
strict loaded-state validation, and total represented matter remain identical.

Verified after the final source edit:

- `python ci.py quick`: **passed** all four checks.
- `python ci.py audit --all`: **passed** 2,658 core tests and 198 gameplay tests.
- `python ci.py gate --lint`: **passed** the repository's authoritative Clippy lane.
- `python ci.py report --scope all`: **passed**, including real opening scout
  and finite-cache situations and separately labeled proxy/capability evidence.

A direct all-targets Clippy invocation that bypassed the repository lane produced
existing repository-wide `expect_used` and `unwrap_used` diagnostics. The
authoritative local lint gate passes without suppressions or unrelated changes.
Gameplay reports demonstrate deterministic modeled execution, not rendered
appearance or universal coverage of generated worlds.

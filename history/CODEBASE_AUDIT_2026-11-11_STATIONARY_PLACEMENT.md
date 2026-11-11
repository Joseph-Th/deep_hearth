# Codebase and gameplay audit, 2026-11-11: stable placement footing

**Role:** Dated review evidence, not current implementation authority. The live
contracts are `AGENTS.md`, `ARCHITECTURE.md`, `TECHNICAL_DESIGN.md`,
`TESTING.md`, `STATUS.md`, and `GAMEPLAY_EVALUATION.md`.

## Scope and method

The working tree was clean on `main` at the start of this review. Workspace and
project agent rules, architecture, technical design, verification rules, and dated
prior audits were read. Source navigation and runtime tests followed control/slow
tick authority, scheduled work and death cleanup, gravity and materialized
collision, generated terrain/weather, surface resources and material transfers,
inventory custody, portable equipment, production energy and shaft arbitration,
and player-facing permanent placement. The repository contains approximately
339,000 Rust source lines in more than 1,100 files; this was a cross-owner
investigation and execution audit, not a line-by-line proof of every function.
The wilderness and combined gameplay reports were executed to assess practical
consequences, not to optimize their coverage metrics.

Broad searches for obsolete, legacy, deprecated, and TODO-marked runtime paths
did not establish an active compatibility subsystem or independently justified
dead-code deletion. Strict tests rejecting obsolete save fields and payloads
are current persistence safeguards and remain intact. Existing large owner
modules are generally partitioned around distinct physical operations; no
further split or generic indirection was justified by a demonstrated defect.

## Confirmed cross-owner defect

Five player actions created persistent voxel custody while using the dynamic
grounding query. That query includes the physical occupancy of temple residents,
who can depart on a later slow tick. A player could therefore create loose
material, a stationary cache, dropped equipment, assembled equipment, or an
energy store while standing only on a warden. When the resident left, the
persisted object had no supporting floor and no independent gravity trajectory.
The first correction exposed a second independently reproducible gap: the
existing fixed-floor query evaluates any cell under the continuous player's
footprint. Standing on the lip of a shaft can therefore be fixed-floor grounded
while the discrete voxel that receives the object is unsupported. The additional
generated-world regression built an actual adjacent stone ledge, relocated the
continuous body toward it, proved the player was fixed-floor grounded with an
air block beneath the current custody voxel, and witnessed the unfixed validator
authorize a loose parcel over the shaft.

A generated-world regression placed the player over a real temple resident
without fixed terrain support, deposited conserved carried stone, and called
the canonical material-drop validator. The test established dynamic grounding
and the absence of fixed flooring, then failed against the unfixed code:
`validate_drop_player_material` returned a valid plan instead of the expected
rejection. This was a witnessed wrong admission, not a guessed visual defect.

The repair adds a narrow logistics-owned `player_placement_has_fixed_floor`
assessment that reuses `player_body_has_fixed_floor` plus the canonical
fixed-cell collision query immediately beneath the actual discrete custody
voxel. It is used by the five owner admission points:

- `surface/material_drop.rs`: loose material parcels.
- `logistics/ground/allocation.rs`: local ground cache allocations.
- `logistics/equipment_carry.rs`: equipment drops.
- `equipment/construction_execution.rs`: equipment assembly.
- `energy/construction_execution.rs`: stationary power-store assembly.

The existing generated-world versus ungenerated-opening distinction remains
unchanged. Fixed terrain and embodied structures retain their normal authority,
while transient actor collision and neighboring ledges can no longer justify
leaving stationary matter in a shaft. No duplicate physics state, collision
routine, persistence field, compatibility alias, or alternate mutation path
was introduced.
Error descriptions were corrected to state the actual fixed-footing requirement.
`ARCHITECTURE.md` now records the cross-owner contract.

Five owner-adjacent deterministic regressions exercise resident-only support
through each canonical validator. A sixth checks partial ledge support over
an empty placement voxel. They require typed rejection with unchanged state;
where matter changes custody, they also verify exact accounting. The generated
fixtures pass trusted-state validation. Existing accepted-ground, airborne,
removed-support stale-commit, save/reload, and control-rate tests remain part
of the audited suite.

## Verification and gameplay evidence

- Before repair, the targeted loose-drop regression **failed** because the
  validator accepted unsupported permanent placement.
- The ledge-specific regression also **failed** with the first correction and
  **passed** after the exact custody-voxel check was added.
- After repair, `python ci.py quick` **passed** its four checks.
- `python ci.py audit --all` **passed**, including **2,635 core tests** and
  **198 gameplay tests**.
- `python ci.py report --scope wilderness` **passed** and exercised finite
  provisioning, generated-world survival, exposed food, shelter and scout
  consequences.
- `python ci.py report --scope all` **passed**, with distinct generated-world
  survival, gathering, movement, mining and finite haul continuity, alongside
  controlled-capability and spatial-proxy industry/progression probes.

The reports establish executable modeled consequences, not a finished
continuous ordinary-player progression from the opening to the industrial
foundry. The combined report explicitly labels spatial-proxy and
controlled-capability evidence separately from generated-world witnesses.
No live rendered frame was inspected because the repository does not provide
a running graphics adapter here. The review makes no claim of exhaustive
world seeds, every save shape, or every possible actor policy.

# Codebase and gameplay audit, 2026-11-10

**Role:** Dated review evidence, not current implementation or verification authority. Consult
`AGENTS.md`, `ARCHITECTURE.md`, `STATUS.md`, `TESTING.md` and
`GAMEPLAY_EVALUATION.md` for active contracts.

## Starting state and audit method

The audited `main` worktree contains 62 already-modified tracked files, plus untracked
working files. These edits span structural construction, mechanical transmission,
world visibility and weather, inventory, direct control, survival, and kobold behavior.
They were not authored by this review and were left intact. The review considered
their diffs where relevant rather than staging or overwriting the pending work.

The project and workspace agent contracts, ownership routing, review guidance,
testing authority, technical design and prior dated audit were read. The inspection
followed production owners, invariant dependencies, physical constraints, and
player-visible decisions. Optional gameplay reports were used for actual consequences,
not as a target score. This was a bounded source and behavioral audit, not a
line-by-line proof of every reachable state.

## Areas inspected and concrete findings

- **Control time and work:** Control/slow tick ordering, fast-forward admission,
  movement, gravity, carried-load exertion, due production and mining support loads,
  tree felling, and labor release were traced. Control stepping is retained when
  pending actions, nonneutral motion, unsupported footing, fall injury, movable
  temple support, or scheduled structure-load changes require a physical sample.
  There was no demonstrated fast-forward bypass or partial-commit defect.
- **Material and thermal custody:** Manual-craft input availability and temperature
  cohorts, production output reservation and completion, homogeneous ore-processing
  choices, and melting/casting mass envelopes were inspected. The pure phase-change
  one-milligram unit-energy projections are exact at the defined nanojoule scale:
  integer authored heat capacities and latent heat make those pure-material
  quantities linear in milligrams. Thus the suspected fractional-unit
  underestimation did not apply to these planners.
- **Fluid and natural-water continuity:** Local/gravity transfer preconditions,
  represented-temperature mixing, finite spring standing-water/soil exchange,
  seed-derived natural spring validation, structural-load prechecks and withdrawal
  were compared. Spring soil and surface volume are separate custody domains;
  capping the *ecological moisture effect* at basin capacity does not imply
  both stores share one physical storage limit. No water creation or capacity
  bypass was demonstrated.
- **Environment and survival:** Regional wind and local airflow, sky visibility,
  standing woodland geometry, harvested-root regeneration, canopy shade,
  available food and water, and the local root-moisture index were followed.
  The root index uses bounded spatial searches, and an exhausted basin has
  the same crop effect as no basin. The pending eye-height observation
  and sheltered-wind changes were read without modifying their uncommitted owners.
- **Mechanical energy:** Finite supply/sink timing and passive recovery,
  transmission conflicts and the pending incremental conflict-component
  selector were examined. Shaft runs contain real rollers, so candidates
  for the same run share embodied physical occupancy. The new selector retains
  prior feasible allocations as components merge. No concrete conflicting
  simultaneous shaft selection was established.
- **Rendering data and determinism:** Texture layout, cutout coverage-preserving
  mips, palette resolution, and ordered registry baking were inspected. The
  texture bake's hash maps provide only lookup/deduplication: GPU layer and
  palette identities are assigned in stable registry-ID traversal order.
  Cutout opacity is keyed only on represented texels; the registry validates
  palette references and alpha mode. No new presentation discrepancy was proven.
- **Legacy code and persistent state:** Searches for obsolete/deprecated
  runtime markers and compatibility paths found no active obsolete implementation
  requiring removal. Tests rejecting previous save shapes and registry versions
  are current strict-load safeguards, not compatibility shims.

Candidates were evaluated against canonical validators before considering a
change. In particular, numerical saturation used for bounded gameplay ratios,
independent fluid stores, and indexed production suspension are not by themselves
proofs of bugs. No production change was made solely to satisfy a metric or
to reshape a module without an identified ownership or maintenance cost.

## Executed evidence

- `python ci.py report --scope wilderness`: **PASS**, finite forage,
  provisioning, cold exposure, and conservation in a generated world.
- `python ci.py report --scope fieldwork`: **PASS**, reserve uncertainty,
  depletion and rerouting, plus an exact-local generated-world expedition
  carrying 30 kg in two trips within 25 kg per-trip capacity.
- `python ci.py report --scope settlement`: **PASS**, finite machine work,
  power and maintenance decisions.
- `python ci.py report --scope liberation`: **PASS**, observed primitive
  assembly versus manual-work decisions.
- `python ci.py report --scope foundry`: **PASS**, bounded melting/casting
  and retained-feed recovery.
- `python ci.py report --scope all`: **PASS**, the combined generated-world,
  progression, survival, energy, workshop, foundry and decision probes.

Git-Wizard also found successful **content-addressed existing receipts** for the
repository's `quick` and `full` validation lanes at the inspected source
revision. Those lanes returned success using matching existing evidence; they
were not freshly rerun by this audit. The comprehensive gameplay reports above
were executed on the actual working copy. All audit-owned processes terminated.

## Disposition and limits

**No independently confirmed new source defect or executable legacy subsystem
was found in the examined paths.** No production code or prior pending changes
were modified. This disposition avoids speculative churn in a busy shared working
tree. It is not a claim that every source file was inspected, that all seeds
and actor policies were exhausted, or that the project already implements
every design ambition. Game-loop reports include controlled-capability and
spatial-proxy episodes alongside real generated-world witnesses; these
evidence types must not be conflated. No rendered GPU frame was inspected
because this repository does not provide a live rendering adapter.

# Codebase and gameplay audit, 2026-11-11

**Role:** Dated review evidence, not current architecture, gameplay, or validation authority. The
active contracts are `AGENTS.md`, `ARCHITECTURE.md`, `TESTING.md`, `STATUS.md`, `DIRECTION.md`, and
`GAMEPLAY_EVALUATION.md`.

## Starting state and approach

The `main` working tree was clean before this review. The project agent rules, owner map,
architectural boundaries, testing authority, current reachability, design priorities, and previous
dated code/gameplay audits were read. Source inspection traced actual owner dependencies and
player-visible transitions instead of trying to reduce static metrics. The review covered the
control/slow-tick orchestration, scheduled labor and revision headroom; body, collision, and
excavation footing; generated terrain, trees, forage, seasons, wind, springs, and moisture;
surface pickup; resident patrol, feeding, and drinking; inventory relocation; structural damage;
material/process progression; and deterministic texture data. Persistence and strict-load rules
were compared with the changed transition.

The current code already splits large production domains around physical owners. Broad searches
for legacy/deprecated branches, persistence aliases/defaults, abandoned implementations, and
loose deterministic collections found no active compatibility subsystem or independently justified
legacy removal. Tests that *reject* obsolete saves are current-schema safeguards and were retained.
Several apparent numerical or physical concerns were already guarded by canonical admission,
checked mass/volume transfer, or documented absent scope; no speculative substitute owner was
introduced.

## Confirmed defect: due work versus incidental vegetation

A scheduled surface gathering reserves a future surface revision. When a weather front coincided
with the gathering completion and that reservation was the final representable revision, the
old slow-tick sequence completed the work on a staged copy, released the reservation, then tried
to spend a second revision for natural fruit senescence. This rejected the entire slow tick as
`SurfaceRevisionExhausted`. The player could not finish already admitted work because an
unrelated, optional ecological change had become due. The affected paths included the ordinary
slow-tick interface and the 20 Hz control boundary.

An existing targeted regression was changed to require actual completion, and was run *before*
the repair. It failed with `SurfaceRevisionExhausted` at the control boundary. The production
repair now obtains the vegetation owner's existing reservation/headroom decision before settling
the due player work, retains it across the tick, and defers incidental aging when the promised
revision leaves no spare headroom. Ordinary no-reservation revision exhaustion still rejects
atomically. The already-deferred path avoids an unnecessary full-state snapshot. The shared
predicate lives in `world/forage_cycle.rs`; `simulation/slow_tick.rs` owns its timing.

Two end-to-end owner regressions verify completion with the last revision: gathering the very
plant whose fruit would age and gathering separate stone while an unrelated plant ages. The
tests exercise actual production authorization and commits, retained actor completion, exact
matter accounting, loaded-state validation, save/reload continuity, and 20 Hz versus slow-tick
parity where both interfaces apply. A two-revision control case still applies both the harvest
and the vegetation transition, proving that the fix does not suppress ordinary senescence.

## Executed evidence

- Baseline `python ci.py quick`: PASS; `python ci.py gate --lint`: PASS.
- Baseline `python ci.py audit --all`: PASS, 2,628 core and 198 gameplay contracts.
- Baseline focused wilderness and progression reports: PASS. Their authored, exact-local, and
  spatial-proxy boundaries remain explicitly distinct.
- Pre-fix harvest regression: FAIL with the predicted `SurfaceRevisionExhausted`.
- Post-fix harvest and independent stone/forage regression: PASS individually.
- Final `python ci.py quick`: PASS.
- Final `python ci.py audit --all`: PASS, 2,629 core and 198 gameplay contracts.
- Final `python ci.py report --scope all`: PASS. The reported witnesses included generated-world
  survival/provisioning and finite-water use, a 30 kg generated-world ore expedition using two
  finite-capacity haul trips, preservation and workshop investment choices, resident food theft
  and recovery, world construction, and controlled foundry/ore capability probes.
- Final `python ci.py gate --lint`: PASS.

## Gameplay assessment and limits

The all-scope report distinguishes executed actions from passive exposure. Its maintained
opening still contains a small set of authored decisions and a long inactive interval; a complete
free-roaming first-day loop, general inventory/crafting UI, and a continuous ordinary journey
to later foundry operations remain *documented integration gaps*, not evidence that existing
owner transactions should be bypassed. Capability-only and spatial-proxy campaigns were not
represented as ordinary generated-world reachability. No GPU frame could be inspected because
the project has no graphics runtime adapter.

This is a broad, owner-guided source and behavior audit, not formal exhaustive proof of all
possible worlds, saves, actor policies, or code paths. The confirmed fault was repaired at the
canonical scheduling/vegetation boundary without adding persistence compatibility, independent
mutations, alternative tick execution, or surface-level module reshuffling.

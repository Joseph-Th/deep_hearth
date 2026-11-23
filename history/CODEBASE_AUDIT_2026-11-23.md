# Codebase and gameplay audit, 2026-11-23

**Role:** Dated audit evidence, not current implementation authority. The
project and workspace `AGENTS.md` guides, `STATUS.md`, `TESTING.md`,
`ARCHITECTURE.md`, `TECHNICAL_DESIGN.md`, and `GAMEPLAY_EVALUATION.md` retain
ownership of the current rules.

## Scope and evidence

Reviewed project routing, existing audit history and the workspace review
checklist before tracing representative live owner paths. Inspected control
and slow-tick scheduling, deferred completion and revision reservations,
neutral fast-forward, direct consumption, survival forecasts and death,
generated-world weather, canopy, terrain and ecology, fluid exchange and
natural catchments, finite material/inventory custody, structural work,
scout patrol and hydration, energy accounting, save admission, and text-play
control and observation. Searched source for obsolete/compatibility paths,
unfinished implementations, silent fallbacks and unchecked arithmetic.

Ran the production text client in a seeded damp-grove wilderness with
`look`, `status`, `inventory`, `recipes`, `storage`, `drink` and `help`, then
quit. This inspected player-visible environmental pressure, actual terrain,
food and material landmarks, inventory and actionable local sources rather
than only fixture assertions. The all-scope gameplay report exercised the
world/survival, manual-to-mechanized progression, wilderness, fieldwork,
energy, settlement, woodworking, ore and foundry domains. Separate targeted
wilderness, progression and survival reports also passed.

This is a broad cross-system source and executed-behavior review, **not** a
line-by-line formal proof of all modules, world seeds or long-term runs.
The tree already had uncommitted graphics, textures, meshing, world-generation,
documentation and test tooling work. Those unrelated changes were left
untouched. Prior audits had already addressed several ownership, travel,
resource-query and storage-fabrication defects; they were not recreated or
reformatted just to increase the number of edited files.

## Confirmed finding: preservation ignored the actor's material budget

**Location:** `tests/gameplay_harness/survival_probe/preservation_decision.rs`
and `preservation_evaluation.rs`. This is the production-backed automated
gameplay **actor decision policy**, not the storage or spoilage owner itself.

The policy advertised an explicit share of available raw material for
preservation, but raised the resulting budget to the cost of the cheapest
buildable enclosure. In a resource-scarce scenario this could spend more
material on storage than the actor had committed, leaving less for tools or
other work. The investment selector also assumed that at least one enclosure
must be affordable. Supplying a worthwhile 2,000,000 mg enclosure with a
1,999,999 mg budget caused a panic instead of a valid decision not to build.
The focused regression failed against the original implementation on that
specific panic.

**Correction:** Keep the material budget at the actor's actual stated
percentage of disclosed raw supply. When no option fits, the canonical
investment policy returns `None` without mutating any world owner. The
evaluation may still execute an independently labeled counterfactual
enclosure against the physically available raw material for comparison, but
cannot turn that comparison into the actor's selected investment. The report
now distinguishes `raw-budget-insufficient` from a candidate rejected on
expected return. No save format, stockpile authority, pricing constant,
world conservation rule, or production-operation shortcut changed.

Two regressions protect the boundary: the precise one-milligram-under-budget
rejection and a real generated preservation opportunity that independently
calculates the declared material allowance and verifies selected investments
against the candidate costs and actual raw supply. The first test failed before the fix and passed afterward.

The preservation report's apparent spoiled-food count required no separate
fix: the spoiled endings belonged to **declined**, not constructed,
investments. The project intentionally allows an actor to decline storage
when the relative resource or attention cost does not justify it. After the
budget correction, the bounded survival report recorded five builds and
seven declines; selected builds still retained fresh food at the stated
comparison horizon.

## Source hygiene and unmodified boundaries

No confirmed active backward-compatibility adapter, duplicate gameplay
authority or unused legacy mechanism was found in the inspected paths.
Explicit rejection tests for obsolete save representations remain essential
strict-load checks, not runtime compatibility support. Existing large
modules already have purpose-specific submodules; splitting them further
without a concrete ownership, readability or performance gain would add
indirection rather than remove it. No test-only public production API,
conservation bypass, arbitrary optimization, or additional persisted state
was introduced. Current registry and gameplay semantics outside the
preservation actor policy remain unchanged.

## Local verification

- `python ci.py quick`: **PASS**, four checks after the decision correction.
- `python ci.py gate --lint`: **not globally clean**. Its six findings are
  confined to already user-dirty `src/world/generation/understory.rs` and
  untracked `src/meshing/trees.rs`/`mod_tests.rs`. No audit-owned file
  appeared in the diagnostics. Those concurrent workstreams were not edited.
- The unconfigured all-targets Clippy lint likewise reports repository-wide
  invariant `expect` assertions; this is not the configured lint gate and
  does not justify replacing strict invariant assertions with silent recovery.
- `python ci.py audit --all`: **PASS**, 2,748 core tests and 194 gameplay
  tests against the changed working tree.
- `cargo test --quiet --locked --test gameplay_survival_contracts --features
  test-gameplay`: **PASS**, all 19 contracts.
- Focused budget-rejection test: **FAIL before fix**, **PASS after fix**.
- Focused real-opportunity budget regression: **PASS**.
- `python ci.py report --scope survival`: **PASS** after the correction;
  executed preservation choices and canonical effects still agree.
- `python ci.py report --scope all`: **PASS** during the baseline audit;
  all relevant domains, including preservation, were sampled.
- `cargo run --locked --quiet --bin text-play -- --natural --seed 1837596006
  --habitat damp`: **PASS** with scripted read-only commands and clean exit.

No changes were made to the pre-existing user-dirty worktree paths.

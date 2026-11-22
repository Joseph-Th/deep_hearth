# Codebase and gameplay audit, 2026-11-21

**Role:** Dated evidence, not implementation authority. Current contracts
remain the project and workspace `AGENTS.md` guides, `ARCHITECTURE.md`,
`TECHNICAL_DESIGN.md`, `TEXT_PLAY.md`, and `TESTING.md`.

## Scope and preservation

This pass reviewed the authoritative owner map and representative source paths
for world generation and standing-site admission, terrain excavation and
fragment custody, surface gathering, finite food and scent, water and local
drinking, survival exposure and exertion, combat contact, scouting and
den-resource decisions, production reservations, structural support,
multi-rate stepping, player-control commands and observations, persistent
state, save publication, and deterministic replay. Gameplay evaluation used
the actual seeded terminal client as well as repository-owned contract probes.

The `main` working tree already contained substantial **uncommitted** renderer,
meshing, texture, generator, test and documentation work. None of that work was
staged, overwritten, removed or reset. The scoped changes below are limited to
the terminal travel client, its tests, this historical report, and its current
behavioral documentation.

Source searches for TODO, FIXME, legacy, obsolete, deprecated and compatibility
paths did not identify an active alternative owner or compatibility adapter
that could be safely removed. Tests that **reject** obsolete save payloads
enforce strict current-schema loading; they are not compatibility runtime.
Existing large modules already have owner-specific submodules. No speculative
renaming, splitting, assertion removal, or format migration was done merely to
change code-shape statistics.

## Reproduced incorrect player outcome

In an unmodified natural mixed-woodland world, seed `1837596006`, the starting
player at `(70,-1,-363)` could already see stone at `(53,-1,-363)`, 17 cells
west. The command `trek w 12` nevertheless stopped at `(65,-1,-363)`,
five cells later, with **“A new loose resource is visible.”** The stone was
the same already-observed site, now 12 cells away. This was a false discovery
and an unnecessary interruption of an otherwise physically valid journey.

The cause was a mismatch between the sight owner's 24-cell landmark range and
the text travel adapter's 12-cell attention window: the adapter discarded
more distant **already visible** sites from its snapshot. As the player walked
closer, an old site first entered the snapshot and appeared newly discovered.
The same range-crossing mistake was present for visible water.

## Correction and boundaries

The travel adapter now retains all visible landmark identities in each
read-only snapshot, but interrupts only when an **as-yet-unseen** resource,
water source, or copper exposure is inside the current player's 12-cell
horizontal attention window. A completed leg refreshes the comparison
baseline. The adapter uses only existing public, physical observations, not
hidden global world queries or a new state owner.

The existing windborne forage-scent read model now contributes categorical
discovery attention during deliberate travel. A newly noticed fruit or nut
scent can interrupt; a faint-to-distinct intensity change of an already
noticed category cannot. The explicit `steady` travel option continues
to ignore discovery interruptions. Resident safety cues continue to be
assessed separately. No world, physical resource, inventory, clock, or save
representation was changed.

The real client now reports **“Trekked 5 of 5 cells in 112 control ticks.”**
for the same seeded route, with the original resource still visible. Regression
tests exercise that production-backed route and the distinct policies for
known distant sites, newly visible nearby sites, irrelevant distant sites,
and new scent categories.

## Verification on the working copy

- `python ci.py quick`: passed all four project checks.
- `cargo test --locked -j 4 --bin text-play`: **101 tests passed**, including
  two new travel regressions.
- `python ci.py audit --all`: core stage **2,741 tests passed**. The later
  gameplay stage could not replace the default `target/local-ci/debug/text-play.exe`
  because a separate console process was already running that file.
- The **same project-owned gameplay stage** with a project-local isolated Cargo
  target directory: **192 gameplay tests passed**, including its exact-local
  survival sample. The running text client was not terminated or replaced.
- `python ci.py report --scope wilderness` under the isolated target:
  **passed**. The report distinguishes ordinary-generated progression from
  exact-local observations and does not infer capabilities from proxies.
- Actual rebuilt isolated `text-play` binary, seed `1837596006`, mixed:
  the five-cell route completed with correct output.

The configured `python ci.py gate --lint` **did not pass** in the pre-existing
user-dirty tree. Its six diagnostics concern a wildcard match in the already
modified `src/world/generation/understory.rs` and five style diagnostics in
untracked `src/meshing/` tests and tree meshing. No lint error implicated the
audit-owned travel changes. That independent, ongoing code was not rewritten
merely to clear warnings.

An additional unconfigured `cargo clippy --all-targets -- -D warnings`
reported a large existing baseline of invariant `expect`/`unwrap` assertions,
as well as the same meshing warnings. Such a sweep is not the repository's
configured lint lane; no behavioral contract was weakened and no blanket
suppression was applied in order to satisfy that proxy.

This is an owner-guided, broad code and executed-play review, not a formal
line-by-line proof of every source file, every generated seed, or an assertion
that unfinished gameplay mechanics are already implemented.

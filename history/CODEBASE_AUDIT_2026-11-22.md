# Codebase and gameplay audit, 2026-11-22

**Role:** Dated audit evidence, not a substitute for current project authority.
The project and workspace `AGENTS.md` contracts, `STATUS.md`, `TESTING.md`,
`ARCHITECTURE.md`, and gameplay documentation remain authoritative.

## Scope and preservation

Reviewed owner routing and representative production paths through simulation
control and slow-tick settlement, direct contextual actions, generated-world
targeting, world and surface acquisition, matter and inventory custody, storage
and spoilage, water and survival, structural construction, scouts, production,
and persistence. Scanned source for obsolete/compatibility paths, discarded
errors, risky arithmetic, non-deterministic collections, and panic boundaries.
Used executed production-backed wilderness and survival gameplay reports to
investigate practical outcomes rather than treating code-shape statistics as
results. This was a broad cross-system review, not a formal proof of every
source line or generated seed.

The starting `main` working tree already had uncommitted texture, meshing,
graphics, world-generation, documentation, build, and test-tooling work. Those
changes were not discarded, overwritten, staged, or included in this audit's
commit. Source inspection did not identify an active backward-compatibility
adapter or unowned parallel subsystem safe to delete. Tests rejecting obsolete
save representations enforce strict current-schema loading, not legacy runtime.
Large source modules with clear existing owner splits were left intact.

## 1. Board use silently selected the wrong physical action

**Reproduction:** `python ci.py report --scope wilderness` originally
failed the production long-stay shelter roof sequence with
`NoStoragePlacementTarget`. Selected timber boards were registered both as
roof-panel material and the one-piece fuelwood-rack assembly input. The
secondary-action dispatcher tried storage first, so a roof-directed action
never reached the structural cover owner.

**Correction:** A selected board first resolves an actually pointed structural
cover target. A terrain-directed use still goes through canonical storage
placement; an empty miss does not invent a storage destination. Structural
cover rejects unsupported footprints before resolving carried lots. Two
owner-adjacent tests execute the actual control path for roof construction and
fuel-rack placement, checking receipts, durable structure, matter conservation,
and trusted state.

**Evidence:** The same wilderness gameplay report now passes, including roof
construction. Focused selected-board tests pass (3), including the existing
temperature-exposure cover test.

## 2. Preservation rewards did not reflect constructed physical enclosures

**Reproduction:** The comprehensive core checkpoint identified a winter
provisions cache with 4x food preservation but only loose stone and raw boards
as assembly inputs. The 150 kg covered fuelwood rack was also receiving a
1.25x food-aging bonus despite being an open bulk fuel structure. In the
survival market this rewarded an implausibly cheap, very large provisions rack.

**Correction:** The 75 kg winter cache now requires a manually crafted 4.8 kg
insulated timber enclosure body plus its 6 kg stone facing. Its fourfold
preservation therefore has a real fabrication and material commitment.
The open 150 kg fuelwood rack retains its board-only construction and capacity
but ages food at the ambient rate, so it is not a food-preservation investment.
The inventory storage owner now admits ambient-rate ventilated enclosures
without admitting faster-than-ambient spoilage. The anti-arbitrage content
contract requires a fabricated and salvageable body for a genuine preservation
gain, rather than mistaking generic boards for an enclosure.

**Evidence:** Core storage preservation, recipe recoverability, and salvage
contracts pass. Candidate ranking reflects real capacity, preservation,
construction attention, and disclosed raw resources.

## 3. Raw-material assembly routes were incorrectly treated as fabricated

**Reproduction:** Broader gameplay tests exposed a preservation harness that
required every input to have manual production stages and treated directly
gatherable stone as if it needed a recipe. This prevented ordinary exact-local
execution of the newly corrected mixed stone/timber cache.

**Correction:** Recursive construction plans distinguish disclosed raw roots
from fabricated components. The evaluation moves raw stone through canonical
player pickup and drop transactions between real local stockpiles; crafted
bodies still execute each manual production stage and consume work time.
Candidate tradeoff tests inspect available raw-material opportunities instead
of assuming all capacity-feasible storage can be built, or that the actor must
buy the strongest option despite the costs. The large winter cache now ranks
above smaller, weaker chests when actually feasible.

**Evidence:** Executed survival provisioning probe and survival market
contracts pass; the survival report evaluates 12 matched episodes with
physical construction and optional investment or decline, preserving real
food freshness, resource cost, and continuation checks.

## Verification on the user's working copy

- `python ci.py quick`: **PASS**, all four repository checks.
- `python ci.py audit --all`: **PASS**, **2,748 core** and **192 gameplay**
  tests in the final broad checkpoint.
- `python ci.py report --scope wilderness`: **PASS**, real long-stay
  shelter roof construction through the existing world.
- `python ci.py report --scope survival`: **PASS**, 12 executable
  preservation cases, including four organic cases.
- `cargo test --locked -j 4 --lib selected_boards_`: **PASS**, 3 tests
  after the final board-dispatch edit.
- `python ci.py gate --lint`: **not globally clean**. The task-introduced
  collapsible conditional was corrected. Remaining diagnostics are confined
  to the preexisting dirty `src/world/generation/understory.rs` and untracked
  `src/meshing/` edits; those other workstreams were not changed here.

No new backward-compatibility machinery, alternative state owner, public
test-only production API, synthetic conservation bypass, or GitHub Actions
workflow was introduced.

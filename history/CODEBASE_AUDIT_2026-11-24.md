# Codebase and gameplay audit, 2026-11-24

**Role:** Dated audit evidence, not current implementation authority. The
project and workspace agent guides, README, STATUS, ARCHITECTURE,
TECHNICAL_DESIGN, TESTING and GAMEPLAY_EVALUATION retain their roles.

## Scope

Reviewed the owner map and representative live paths for control and slow
ticks, deferred completion and revision headroom, neutral fast-forward,
player admission, survival and consumption, movement and custody, surface
gathering and resource freshness, inventory and equipment, production
and mining indexes, generated terrain and ecology, fluid exchange,
energy, resident feeding and patrols, persistence, text-play control,
and the newly integrated meshing boundary. Searched for active legacy or
compatibility adapters, unfinished implementations, silent fallbacks, and
unsafe physical arithmetic. Consulted recent audit history to avoid
duplicating already completed changes.

The tree started clean on main. This is a broad owner-guided audit with
targeted regressions, not exhaustive line-by-line verification of every
module, renderer configuration, or world seed. No new dependency,
compatibility adapter, public test API, save field, or parallel state owner
was introduced.

## Confirmed findings

### 1. Ordinary patrol tip omitted from forage ranking

The normal patrol endpoint was excluded from adjacent wild-food selection.
A hungry scout offered a 1 g junction patch and 300 g fresh food on the
ordinary patrol tip went directly home rather than collecting the
recoverable full serving. The production-tick regression failed at the
observed route position before correction.

The indexed adjacent-food search now includes the normal endpoint along
with side branches. The route selector retains the ordinary patrol state
when that endpoint wins, rather than serializing an invalid redundant
detour. Existing safety, freshness, capacity and movement admission
remain authoritative. Renamed the search and its callers to describe
the expanded domain; no old alias remains.

### 2. Prolonged thirst bypassed normal food ranking

With no reachable water, a water-deprived hungry scout chose a 1 g loose
food parcel rather than a nearby fresh 300 g wild-food patch. The regression
failed on the actual next patrol position before correction.

Both hydration states now use the same food-serving comparison. Water
priority and the restriction on nonessential salvage remain intact.
The comparison of a forage route with loose food uses the destination's
world-dependent forage batch rather than the unconditional base batch.
Both new regressions check actual depletion, hunger recovery, matter
conservation, strict load validation and deterministic save/replay.

### 3. Den-feeding search repeated unnecessarily

The scheduled scout meal scanned the same indexed local surface parcels
twice if no single parcel could complete its remaining ration. A single
ranking pass now selects the smallest sufficient parcel first, or the
largest usable partial portion when none suffices. Stable identity ties,
revision headroom and exact terminal matter accounting are unchanged.
Existing watchman and partial-meal tests cover these priorities.

### 4. Existing lint failures in committed world and meshing code

The configured lint gate identified six existing diagnostics in the clean
baseline. Understory habitat matching now enumerates all current variants,
so a new habitat requires an explicit decision. Tree meshing uses the
standard divisibility predicate for the same deterministic crown hashes.
Mesh tests use fixed-size quad slices and remove an unnecessary conversion.
These changes preserve the existing generation and geometry algorithms;
no visual redesign or visual-quality claim is made.

## Evidence and readiness

The production terminal client was run on seeded opening 24318. Commands
inspected local observations, moved east, settled movement, gathered
500 g of berries, inspected carried custody, ate 10.139 g and waited for
completion. The client reported the expected physical gathering and
consumption transitions and exited normally. This is opening smoke
evidence, not a natural-seed or full progression playthrough.

The repository's current routing, rejection, persistence, determinism and
verification authorities identify the production owners and narrow test
commands. Representative owner paths were read and both new regressions
exercised normal simulation ticks rather than a parallel policy.
The current technical design documents the corrected route behavior.

No active backward-compatibility code was confirmed in the inspected
source. Tests rejecting obsolete representations enforce strict load
admission and remain in place. Existing purpose-specific module boundaries
were retained where additional splitting would add indirection without
a concrete ownership or behavioral benefit.

Verification on the working copy:

- Both new regressions: reproduced the intended failure before the fix.
- `python ci.py quick`: PASS, all four checks.
- `python tools/run_test.py --suite kobolds`: PASS, 397 tests.
- `python ci.py audit --core`: PASS, 2,766 tests.
- `python ci.py audit --gameplay`: PASS, 194 tests, with a fieldwork
  ordinary-system spatial-proxy sample, before the equivalent lint cleanup.
- Combined `audit --all` attempts encountered connector stall-watchdog
  termination; both constituent stages passed independently after the
  gameplay corrections.
- `python tools/check_authority_docs.py`: PASS.
- `python ci.py gate --lint`: PASS after the lint cleanup.
- `python ci.py audit --core`: PASS again after the lint cleanup.

## Continued audit: natural generation and ordinary fieldwork

The continuation began from the verified previous commit. Reviewed the
natural resource generators, prospective occupancy, surface creation and
clock binding, rooted biomass lifecycle, terrain and movement, survival
assessment, neutral fast-forward admission, automatic pickup, exclusive
work, equipment custody, and the terminal client's observation and travel
paths. Felling's same-height adjacent reach rule prevents chopping the
trunk underneath the player; no speculative change to fast-forward was
made on that basis.

Confirmed a stale natural chunk authorization: surface parcels are dated
to world tick zero, but the planned original perennial crop depends on
whether the first senescence has passed. The existing spring clock guard
masked the issue in wet chunks. A springless, seed-derived chunk plan
validated before the first senescence and committed after a real simulation
boundary incorrectly succeeded. The regression reproduced that failure.
The natural generation commit now rejects a still-available original
perennial crop after its expiry, before applying any owner. Fresh validation
produces dormant roots with the same physical biomass. Commits before the
boundary and commits of already reconciled roots remain admissible when
other owner guards permit them.

Shrub, understory, loose material, exposed copper and spring generation
now share one local candidate clearance rule. It checks provisional
stone footing, clear cells, trunks, full-cell structures, stationary
custody, earlier reservations, available resources and dormant living roots.
Removed the redundant separate fluid-location check in understory:
the stationary occupancy index already includes fluids. Seed-specific
candidate order, commodity and mass policies remain in their existing
owners. No new save field, compatibility adapter or dependency.

### Natural-world replay

Ran the production terminal client with `--natural --seed 1837596006
--habitat damp`, no authored starter trail and no fixture-provided
equipment. The following ordinary command sequence reached finite timber,
stone, a working fire, and a spring, then used a hand-built pick to
excavate adjacent terrain:

```text
trek e 1 steady
trek s 8 steady
trek e 2 steady
begin gather 1 2030.42
wait finish
begin craft 8
wait finish
trek w 7 steady
begin gather 1 5000
wait finish
craft 7
wait finish
assemble 6
take 1
equip 1
build-fire
feed-fire 1 2
wait 7200
trek e 7 steady
trek s 1 steady
trek e 2 steady
trek s 4 steady
drink 1
wait finish
trek n 1 steady
break down repeat 3
inventory
look down
quit
```

The 2,030.42 g timber source was exhausted exactly. The 5,910.16 g
stone source retained 910.16 g. Handle and tool-head crafting preserved
the 1,000 g batch totals in their component and chip outputs. The pick
was assembled, picked up and equipped in ordinary custody. The fire
consumed 500 g of chips for 5 MJ; at the weather interruption, 4.2872 MJ
remained and warmth had recovered to 100%. The spring at
(-212,-9,-246) decreased from 9,418.445 mL to 9,168.445 mL after drinking,
with hydration restored to 100%. Three 200 g cuts at (-212,-10,-247)
removed 600 g of terrain and transferred exactly that mass into the
pack; the block retained 2,649,400 g. The pick was still carried and
showed wear. Vitality remained 100% through this opening slice.
A tree blocked a direct two-metre climb route to the spring, and the
player reached it via an alternate normal foothold. No full progression
or late-game playthrough is claimed.

The replay used the terminal binary from the previous verified code
before the generation admission edits; it is evidence of ordinary
opening play, not a proof of the new transaction guard. The new exact
regression reproduced the stale crop commit and passed after the fix.

Verification after the continued production changes:

- `python tools/run_test.py delayed_natural_plan_cannot_restore_original_fruit_across_senescence`: reproduced failure, then PASS after the crop guard.
- `python ci.py quick`: PASS, four checks.
- `python ci.py audit --core`: PASS, 2,767 tests.
- `python ci.py gate --lint`: PASS.
- `python ci.py audit --gameplay`: PASS, 194 tests; settlement sample,
  ordinary-exact-local-after-disclosed-bootstrap evidence, roots
  `0x70CA1B6DAA2BD7BD/0x9577CFF5A085CB2F`.
- `python tools/check_authority_docs.py`: PASS.


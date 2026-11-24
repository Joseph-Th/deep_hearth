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

## Further survival and custody review

Reviewed manual-production suspension and retained-site validation, carried
equipment and stockpile movement guards, attention admission and commit
snapshots, direct consumption, food preservation, and structural floor
collision. Retained production endpoints are already constrained to the same
site; failed but embodied full-cell members still provide collision. Neither
path warranted a speculative behavior change.

The multi-portion meal defect involved inspecting stack mass and remote
custody before checking actor attention. The dead-player regression
reproduced a MassExceedsStack result instead of PlayerDead. Exact-lot and
multi-portion meal validation now share the same timing, attention and
source admission, then resolve exact selections and reserve consumption.
The regression covers oversized local and remote selections against both
live memory and trusted-load snapshots and verifies no mutation.

Natural dry-scrub replay, seed 24318, using the previous verified terminal
binary:

```text
trek w 3 steady
trek n 2 steady
begin gather 1 5000
wait finish
trek e 4 steady
trek s 1 steady
gather 1 500 repeat 3
begin craft 109 3 from 2
wait finish
eat 3
wait finish
begin craft 29
wait finish
trek n 1 steady
place-store 6
stash 1 3 1000
withdraw 1 1 250
inventory
quit
```

Collected 5,000 g stone, leaving 356.688 g at the source, and 1,500 g
wild nuts, leaving 248.795 g. Sorting took 43 minutes 12 seconds and
returned 1,350 g preserved nuts and 150 g rejected fragments. Eating
152.134 g restored the food reserve to 100%, leaving 1,197.866 g.
Shaping 3,000 g stone returned a 2,400 g crock body and 600 g chips.
Storage admission correctly blocked placement on the rooted food patch;
moving one voxel north admitted the crock. Depositing 1,000 g and
withdrawing 250 g left 750 g stored and 447.866 g carried. The displayed
spoilage estimates were 10,797 hours 12 minutes in the crock and
4,318 hours 53 minutes outside it, consistent with slower storage
exposure, not a reset of food age. Vitality remained 100% throughout.
This is a natural opening and preservation route, not a seasonal
survival or complete progression claim.

Verification:

- `python tools/run_test.py dead_player_full_reserve_use_rejects_before_reporting_no_meal_or_drink_needed`: reproduced failure; PASS after shared meal admission.
- `python ci.py quick`: PASS, four checks.
- `python ci.py audit --core`: PASS, 2,767 tests.
- `python ci.py gate --lint`: PASS.
- `python tools/check_authority_docs.py`: PASS.
- `python ci.py audit --gameplay`: PASS, 194 tests; settlement sample,
  ordinary-exact-local-after-disclosed-bootstrap evidence, roots
  `0x4DEEB6EE20A41495/0xA3ECAEFC23973508`.

## Further resident priority and occupancy review

Reviewed hydration, den care, optional work, patrol admission, watch movement,
scavenging route selection and saved continuation, including the crossed
inventory and fluid settlement boundaries. Also inspected production
completion and availability, fast-forward, energy illumination and local
warmth, and owner serialization for additional actionable defects.

A due, fed scout could steal from a junction stockpile rather than follow a
viable drinking route: optional industrial actions preceded the water-route
decision. A normal simulation regression reproduced the wrong position at
tick 985. The same ordering could interrupt the second edge of a water trip.
The correction selects the viable water trip after care and watch movement,
before fitting or extraction, and shares the choice with movement. The
regression verifies both outbound edges, actual drinking, unchanged copper
stocks, strict load validity, matter and fluid accounting, and saved replay.

Scout resource planners could also target the exposed post occupied by the
watchman even though physical movement would reject it. Scout route and
movement admission now share the occupancy restriction while the watchman
uses the common physical clearance rule and its separate scout exclusion.
The den remains shared. The regression extends an actual ration-funded
outer picket with equally adequate tip and side water sources and verifies
that the side route wins without changing state. The scenario uses the
production registry rather than the reduced machinery-only test registry.
No persisted fields or compatibility layers were added.

Verification:

- `python ci.py quick`: PASS, four checks.
- `cargo test --locked -j 4 --lib 'kobolds::'`: PASS, 397 tests after correcting the water fixture.
- `python ci.py audit --all`: PASS, 2,768 core and 194 gameplay tests; workshop
  controlled-capability sample, roots `0xCB176D10BF784E28/0xEF09378E58C34B2B`.
  An earlier combined run was terminated by the connector stall watchdog;
  the completed repeat is the broad evidence for this change.

No new interactive playthrough or graphical inspection was performed in
this pass. The changed resident contracts are exercised by production
simulation ticks and the broad gameplay checkpoint.


# Technical Design

This document records technical decisions that are intentionally outside `GAME_DESIGN.md`.
`AGENTS.md` remains the coding and architecture authority when the documents overlap.

## 1. Architectural Goal

Deep Hearth is built as a deterministic headless simulation with explicit adapter boundaries.
Gameplay state and rules must remain usable without a renderer, window, input device, network
connection, or storage backend.

This is a foundation for a long-lived simulation, not an engine-specific vertical slice.

## 2. Dependency Direction

The dependency direction is inward:

```text
content builders -> registries -> simulation systems -> core state
                                            ^
                                            |
                                  adapters translate at edges
```

- `core` owns domain-neutral runtime primitives and `AppState`.
- `registry` owns immutable definition aggregates and lookup contracts.
- `content` authors built-in immutable definitions in Rust.
- `simulation` owns the visible top-level tick pipeline.
- `persistence` owns versioned save envelopes and state-load validation, but not filesystem IO or a
  particular byte encoding.
- Future gameplay subsystems own their records, indexes, validation tokens, plans, and system
  functions. They plug into the canonical simulation pipeline instead of creating alternate loops.

Adapters may depend on the simulation core. The simulation core must not depend on adapters.

## 3. Determinism

Authoritative simulation behavior is deterministic for the same immutable registries, persisted
state, explicit inputs, and external snapshots.

- Runtime randomness is owned by `RandomState` in `AppState` and fully persisted.
- PRNG and stream-derivation algorithms have explicit versioned identities.
- Typed `RngStreamId` values create independent streams derived from the world seed; advancing one
  subsystem stream cannot shift another subsystem's sequence.
- `ProbabilityPpm` represents normalized probability with integer parts per million.
- Bounded random choices use rejection sampling rather than modulo reduction, avoiding distribution
  bias. Zero-percent and one-hundred-percent decisions do not consume RNG state.
- Ordering that can affect results uses deterministic collections or explicit sorting. Hash
  iteration order must never decide simulation outcomes.
- Authoritative physical quantities use integer representations and checked arithmetic.
- Parallel work may be introduced only when reduction and commit order remain deterministic.

## 4. Simulation Time

`SimulationTick` is absolute authoritative time. `TickSpan` is a distinct relative duration type, so
an absolute tick cannot be passed accidentally where a duration is required. The built-in core
starts at 20 ticks per second; this is a technical cadence, not a promise that every subsystem
updates at 20 Hz.

`PeriodicSchedule` provides deterministic clock-derived phase scheduling for static slow systems
such as ecology, soil, weather, migration, and settlement economics without introducing callbacks or
hidden mutable countdown state. Dynamic scheduled work such as production remains explicit persisted
records with dedicated indexes.

## 5. State and Records

`AppState` is the root of generated mutable state that must survive restart boundaries. It currently
owns the world seed, authoritative clock, independent deterministic RNG streams, finite-energy
stores, equipment records, structural records, inventory, and production. New subsystems add
explicit owned state rather than turning `AppState` into a bag of unrelated maps.

Runtime records use typed persistent IDs. Each subsystem owns its record collections and synchronized
indexes; callers receive read-only views and canonical systems retain mutation access.

- `InventoryState` owns stockpiles, persistent material lots, generated stockpile/lot IDs, derived
  commodity totals, cached stored mass, inbound reservations, and an owner revision.
- `EnergyState` owns finite energy stores, generated store IDs, and an owner revision.
- `EquipmentState` owns maintainable equipment instances, their structural support assignment, a
  synchronized support-to-equipment reverse index, generated equipment IDs, and an owner revision.
- `StructureState` owns structural members, support/dependent indexes, source-separated loads, damage,
  generated member IDs, and an owner revision.
- `ProductionState` owns active jobs, generated job IDs, a due-tick index, and an owner revision.
- Validated transaction tokens bind to the exact owner revisions they checked, preventing stale
  commits after intervening mutation.

## 6. Mutation Model

Consequential mutations happen in canonical system functions. A fallible operation validates every
precondition before mutation. Multi-resource mutations use consumed validated tokens. Systems that
read broad state but write a narrow result use decide/apply pairs.

Top-level tick order remains visible in one function. Subsystems do not hide gameplay mutations in
callbacks, event handlers, record methods, or engine lifecycle hooks.

## 7. Persistence

The core defines a versioned semantic save envelope while deliberately leaving byte encoding and
storage to adapters. The current save schema is version 14. Authored identity/physics compatibility
is tracked separately by `RegistrySchemaVersion`; the built-in registry schema is currently version
5. Core gravity is part of that immutable registry contract because changing it changes persisted
structural consequences even when authored IDs are unchanged.

`SaveMetadata` can be decoded without decoding the current `AppState` shape. A future adapter can
therefore inspect an old schema first and route it to a version-specific DTO and explicit migration.
Do not deserialize arbitrary legacy payloads directly into the newest state and inspect the version
afterward.

A current-schema load must:

1. Validate save and registry schema versions.
2. Validate every subsystem's local persisted invariants.
3. Resolve every persisted authored/runtime reference.
4. Validate cross-owner reservations, lifecycle, provenance, and in-process conservation.
5. Reject corrupted state before returning `AppState` to runtime use.

Persistence tests cover deterministic continuation, mixed composition, independent RNG continuation,
tampered RNG roots, tampered in-process consumed mass, stable immediate JSON reserialization,
equipment structural support/load agreement, and in-flight jobs surviving later process requirement
rebalancing from their committed snapshots.

Filesystem layout, compression, atomic writes, cloud storage, and released-save migration
implementations remain adapter work.

## 8. World, Spatial, and Engine Architecture

Persistent world references use chunk-agnostic 64-bit voxel coordinates. `VoxelCoord`,
`VoxelDelta`, `ColumnCoord`, and validated half-open `VoxelBounds` provide checked spatial arithmetic
without selecting chunk dimensions, storage encoding, ECS layout, or streaming policy.

Chunk shape, renderer, input, physics engine, threading, and networking remain deliberately
unselected. Domain records must not assume an ECS, scene graph, renderer object, or chunk layout.

## 9. Performance Policy

Performance begins with ownership and access patterns rather than premature micro-optimization.

- Authoritative records and indexes remain compact, private, and deterministic.
- Production uses `BTreeMap<SimulationTick, BTreeSet<ProductionJobId>>` so due work does not require
  scanning all active jobs.
- Stockpiles maintain cheap derived mass/commodity caches and update them atomically with lot state.
- Output capacity is reserved at process start so completion does not discover a late full-destination
  failure.
- Compatible newly created lot fragments coalesce deterministically to prevent unbounded tiny-lot
  growth while established lot IDs remain stable.
- Save/load validation is exhaustive, but the base tick executes only cheap invariants. Periodic
  exhaustive audits in soak tests preserve corruption coverage without making every tick O(world).
- Spatial/chunk architecture must be justified by workload measurements before selection.

## 10. Validation Policy

Every change must keep formatting clean, `cargo check` silent, Clippy warning-free with `-D warnings`,
and all tests passing. Release builds retain integer overflow checks.

The deterministic headless soak runs 10,000 canonical ticks with repeated production and transfers.
It runs twice from the same seed and requires complete final `AppState` equality. It also performs
periodic exhaustive state audits, recomputes global matter ownership and requires conservation, and
enforces a material-lot fragmentation ceiling.

## 11. Authoritative Physical Quantities

Simulation quantities participating in conservation or engineering calculations use explicit integer
units:

- `Mass`: milligrams (`u64`);
- `AggregateMass`: world-scale milligrams (`u128`);
- `Temperature`: nonnegative absolute millikelvin (`u32`);
- `Energy`: nanojoules (`u128`);
- `Pressure`: pascals (`u64`);
- `Area`: square millimeters (`u64`);
- `Acceleration`: micrometers per second squared (`u64`);
- `Force`: millinewtons (`u128`);
- `Power`: picowatts (`u128`), with a microwatt convenience constructor;
- `Torque`: micronewton-meters (`u64`);
- `AngularSpeed`: microradians per second (`u64`);
- `ElectricPotential`: microvolts (`u64`);
- `ElectricCurrent`: microamperes (`u64`);
- `ElectricalResistance`: microohms (`u64`);
- `Volume`: microliters (`u64`);
- `VolumetricFlow`: microliters per second (`u64`).

Arithmetic that can exceed authoritative storage is checked. Floating point is not used by the
implemented authoritative physical calculations.

## 12. Materials and Physical Forms

Materials are immutable definitions with typed IDs and grouped density, thermal, mechanical, and
electrical properties. Physical forms such as log, lump, ore, concentrate, and ingot have typed
`FormId` definitions. `CommodityKey` combines one material and one form for coarse indexing.

`MaterialComposition` is a canonical normalized mass-fraction profile. Components are sorted by
material ID, use integer parts per million, and total exactly 1,000,000 ppm. Duplicate materials,
zero fractions/IDs, invalid totals, and noncanonical deserialized order are rejected. This supports
ore grade, alloy ratios, carbon content, and later contamination without authoring a material ID for
every mixture.

Density-based material volume is computed from mass and composition with conservative upward
rounding at the microliter boundary. Sensible heat is weighted by composition and authored specific
heat; the calculator refuses to cross a constituent melting point, requiring phase change to be
modeled explicitly.

The initial material content includes wood, charcoal, copper, slag, and foundational forms. It is
architecture content, not the complete gameplay catalog.

## 13. Inventory and Matter Ownership

Material lots are the authoritative stored-matter representation. Each `MaterialLotRecord` owns a
persistent lot ID, stockpile owner, exact mass, a `MaterialLotProfile` (commodity, absolute
temperature, normalized composition), and a creation provenance range.

Stockpiles maintain derived deterministic indexes/caches for lot IDs, per-commodity mass, total
stored mass, capacity, and reserved inbound mass. `validate_transfer_bulk` plus the consumed
`ValidatedTransferBulk::commit` performs revision-bound two-stockpile movement. Partial transfers
split lots in stable ID order without averaging physical properties away. Newly created compatible
fragments can coalesce into the lowest-ID compatible destination lot.

There is intentionally no public arbitrary-deposit API. Tests can seed matter through `#[cfg(test)]`
fixtures; real matter creation must eventually originate from canonical extraction, biology, trade,
or another explicit source system.

`calculate_matter_accounting` recomputes implemented world matter from authoritative records. It
counts inventory lots plus in-flight production output snapshots. Consumed-input traces are history,
not a second owner, and reserved inbound capacity is space rather than matter.

## 14. Timed Production

Production separates static requirements from operation-specific physical outcomes.

`ProcessDefinition` contains stable identity, composition-aware material inputs, and typed capability
requirements. It does not contain a fixed duration, fixed output yield, or generic technology tier.
Capability references and physical value kinds are validated when registries are assembled.

`ProcessResolution` contains the resolved duration and exact output lot specifications for one
specific operation. It has no public arbitrary constructor. Future metallurgy, thermal, equipment,
tooling, labor, and skill systems must resolve a physical plan before the canonical start transaction
can accept it.

Production is closed-mass in the implemented core: resolved output mass must equal authored input
mass. Slag, tailings, wastewater, gas, and similar losses must therefore be explicit material streams
instead of hidden yield loss.

At start, selected input slices are removed from inventory and the job becomes the matter owner via
its committed output snapshot. Each durable job also preserves consumed mass and consumed-input
physical/provenance traces without retaining dangling source lot IDs. Loaded jobs validate trace mass
and output mass against that persisted conservation snapshot rather than rederiving them from a later
edited process definition.

Destination output capacity is reserved at start. Completion uses a deterministic due-tick plan and
converts reserved capacity into actual output lots before removing the job/index entry.

The built-in production registry remains empty until real physical authorization systems can resolve
gameplay operations faithfully.

## 15. Capabilities, Maintenance, Energy, Flow, and Mechanical Power

Capabilities are typed physical requirements rather than generic progression levels. Current value
kinds include presence, mass, temperature, energy, pressure, force, power, torque, angular speed,
electrical quantities, volume/flow, and equipment condition. Each requirement states `AtLeast` or
`AtMost` threshold semantics. `CapabilityProfile` owns deterministic nominal/static values, while
`CapabilitySource` lets runtime-adjusted providers satisfy the same evaluator without materializing
temporary maps. The built-in capability registry remains empty until concrete gameplay providers are
authored.

Maintenance uses normalized `Condition` with authored warning and critical thresholds. Equipment
definitions may additionally author piecewise-linear response curves for individual typed
capabilities. Each curve owns an explicit failed-condition endpoint and interpolates toward the
definition's nominal capability at pristine condition using overflow-safe integer arithmetic that
rounds toward the degraded endpoint. Runtime equipment providers resolve these effective values on
demand without allocation. Uncurved capabilities remain nominal. Presence-only capabilities cannot
use continuous condition curves because the capability model has no numeric absence state; any
future capability-disable behavior must be an explicit discrete policy. Pure wear and repair plans
still clamp at physical bounds without deleting the owning record; failure probabilities, repair
resources, and broader ownership policies remain subsystem-specific.

The current engineering modules provide scalar conservation foundations without prematurely choosing
network topology:

- picowatt power integrates across ticks into nanojoules plus an explicit carried remainder;
- volumetric flow integrates into microliters plus an explicit carried remainder;
- microvolt times microampere electrical power is exact at the picowatt scale;
- resistive voltage drop returns whole microvolts plus a validated picovolt remainder;
- micronewton-meter torque times microradian/second angular speed is exact in picowatts;
- rotational operating points can be checked independently against torque, speed, and power limits;
- mechanical efficiency splits input into useful output and explicit loss without overflowing
  full-width power;
- canonical rational transmission ratios transform torque/speed conservatively, with authored loss
  separated from integer-resolution loss so gearing cannot create power through rounding.

Future persistent network owners must preserve carried remainders when integrating incrementally so
small rates are not lost to repeated truncation. The mechanical scalar layer intentionally chooses
no shaft graph, belt routing, flywheel inertia model, slip state, clutch lifecycle, or network solver.

## 16. Equipment Structural Support

Equipment records own an optional `StructuralElementId` support assignment, while `EquipmentState`
maintains the synchronized reverse index used for support-local aggregation and removal checks. A
mount/unmount is a revision-bound cross-owner transaction: the equipment owner changes both support
views atomically while the structural owner changes only `StructuralLoadKind::Equipment`. The load is
derived from the aggregate mass of all equipment indexed on the element under registry-authored
gravity, then structural analysis resolves cracking or collapse before commit.

The equipment load cause is not writable through the public generic structural-load API. This keeps
one authoritative source for the derived value. Equipment load validation first audits both directions
of the persisted support index, including empty, missing, unknown, and mismatched entries. Exhaustive
cross-owner validation then independently recomputes mounted aggregate mass and required force,
rejects missing/planned support references, and requires every structural member's stored equipment
load to agree exactly. Structural removal is rejected while equipment still references the member.
Unmounting from failed debris is permitted so cleanup does not require resurrecting the structure;
unloading never clears persisted crack/failure state.

Mount/unmount also respects active production occupancy. A machine cannot move while an in-flight job
owns it, and support/maintenance commits recheck that derived production occupancy immediately before
mutation because job start does not increment the equipment owner revision. Provider resolution
requires any assigned structural support to remain active. For mounted equipment, the resulting use
token binds both the equipment and structural owner revisions plus the exact support assignment;
process start validation and commit reject intervening structural changes before consuming matter or
energy. A collapsed support therefore cannot authorize new production through a stale resolution. An
operation that was already committed retains its durable matter, energy, equipment-condition,
duration, and output snapshot; interrupting or partially recovering such work after a later support
failure remains deferred until production has an explicit interruption/cancellation owner rather than
silently destroying committed resources.

## 17. Cross-Subsystem Runtime Invariants and Boundaries

`validate_loaded_state(registries, state)` validates local owners plus cross-system relationships,
including registry references, lot provenance, generated ID cursors, due-index membership, stockpile
cache agreement, capacity/reservation agreement, job lifecycle, consumed-input references, and
in-process matter conservation, equipment support references, and mounted-equipment structural-load
agreement. Operation-specific thermal audits recompute condition-sensitive equipment capabilities
from the persisted provider-condition snapshot so an in-flight job's physical duration contract
remains reproducible after load.

The foundation intentionally leaves unresolved choices unresolved. Deferred areas include chunk
storage/streaming, renderer/ECS/physics/networking, canonical extraction sources, thermal fields and
phase change, combustion/emissions, real equipment/tool/worker capability providers, production
resolvers, persistent mechanical networks/inertia/slip, steam/boilers, electrical
topology/transformers/protection, hydrology and fluid networks, agriculture, ecology/genetics,
creatures/workers, settlements/logistics/trade, and released save-file migrations/storage adapters.

New systems must integrate through owned records, immutable definitions, typed IDs/quantities,
canonical mutations, dedicated errors, persistence semantics, invariant coverage, and behavioral
soak tests. Do not introduce shortcut recipes or generic technology tiers where the game design
requires physical authorization.

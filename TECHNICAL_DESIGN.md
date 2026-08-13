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
owns the world seed, authoritative clock, independent deterministic RNG streams, inventory owner, and
production owner. New subsystems add explicit owned state rather than turning `AppState` into a bag
of unrelated maps.

Runtime records use typed persistent IDs. Each subsystem owns its record collections and synchronized
indexes; callers receive read-only views and canonical systems retain mutation access.

- `InventoryState` owns stockpiles, persistent material lots, generated stockpile/lot IDs, derived
  commodity totals, cached stored mass, inbound reservations, and an owner revision.
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
storage to adapters. The current save schema is version 9. Authored identity compatibility is tracked
separately by `RegistrySchemaVersion`; the built-in registry schema is currently version 1.

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
tampered RNG roots, tampered in-process consumed mass, stable immediate JSON reserialization, and
in-flight jobs surviving later process requirement rebalancing from their committed snapshots.

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
- `Power`: picowatts (`u128`), with a microwatt convenience constructor;
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

## 15. Capabilities, Maintenance, Energy, and Flow

Capabilities are typed physical requirements rather than generic progression levels. Current value
kinds include presence, mass, temperature, energy, pressure, power, electrical quantities,
volume/flow, and equipment condition. Each requirement states `AtLeast` or `AtMost` threshold
semantics. `CapabilityProfile` is currently a transient provider/view value and is intentionally not
persisted until a real equipment, tool, structure, or worker owner exists. The built-in capability
registry therefore remains empty.

Maintenance uses normalized `Condition` with authored warning and critical thresholds. Pure wear and
repair plans clamp at physical bounds without deleting the owning record; degradation curves,
failure probabilities, repair resources, and ownership remain subsystem-specific.

The current engineering modules provide scalar conservation foundations without prematurely choosing
network topology:

- picowatt power integrates across ticks into nanojoules plus an explicit carried remainder;
- volumetric flow integrates into microliters plus an explicit carried remainder;
- microvolt times microampere electrical power is exact at the picowatt scale;
- resistive voltage drop returns whole microvolts plus a validated picovolt remainder.

Future persistent network owners must preserve carried remainders when integrating incrementally so
small rates are not lost to repeated truncation.

## 16. Cross-Subsystem Runtime Invariants and Boundaries

`validate_loaded_state(registries, state)` validates local owners plus cross-system relationships,
including registry references, lot provenance, generated ID cursors, due-index membership, stockpile
cache agreement, capacity/reservation agreement, job lifecycle, consumed-input references, and
in-process matter conservation.

The foundation intentionally leaves unresolved choices unresolved. Deferred areas include chunk
storage/streaming, renderer/ECS/physics/networking, canonical extraction sources, thermal fields and
phase change, combustion/emissions, real equipment/tool/worker capability providers, production
resolvers, mechanical power, steam/boilers, electrical topology/transformers/protection, hydrology
and fluid networks, agriculture, ecology/genetics, creatures/workers, settlements/logistics/trade,
and released save-file migrations/storage adapters.

New systems must integrate through owned records, immutable definitions, typed IDs/quantities,
canonical mutations, dedicated errors, persistence semantics, invariant coverage, and behavioral
soak tests. Do not introduce shortcut recipes or generic technology tiers where the game design
requires physical authorization.

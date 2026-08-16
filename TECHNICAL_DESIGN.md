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
stores, finite fluid stores, equipment records, structural records, finite geological deposits,
acquired geological knowledge, inventory, and production. New subsystems add explicit owned state
rather than turning `AppState` into a bag of unrelated maps.

Runtime records use typed persistent IDs. Each subsystem owns its record collections and synchronized
indexes; callers receive read-only views and canonical systems retain mutation access. State owner
modules keep records, indexes, and owner mutation primitives together while descendant
`state/validation.rs` modules perform exhaustive persistence audits against those private fields. This
keeps load validation close to the owner without widening mutation visibility or mixing audit code
into the runtime state surface.

- `InventoryState` owns stockpiles, persistent material lots, generated stockpile/lot IDs, derived
  commodity totals, cached stored mass, inbound reservations, persisted phase/temperature containment
  profiles, optional structural support assignments, the synchronized support-to-stockpile reverse
  index, and an owner revision.
- `EnergyState` owns finite energy stores, generated store IDs, and an owner revision. Store behavior
  is defined by immutable carrier, capacity, and independent input/output power envelopes.
- `FluidState` owns finite homogeneous fluid stores, generated store IDs, exact volume and
  temperature, optional structural support assignments, a synchronized support-to-store reverse
  index, and an owner revision. Empty stores carry no residual fluid identity.
- `EquipmentState` owns maintainable equipment instances, their structural support assignment, a
  synchronized support-to-equipment reverse index, generated equipment IDs, and an owner revision.
- `StructureState` owns structural members, exact embodied material traces/mass, support/dependent
  indexes, source-separated loads including derived self-weight, damage, generated member IDs, and an
  owner revision.
- `GeologyState` owns finite generated deposits, their exact remaining matter profile and bounds,
  generated deposit IDs, depletion lifecycle, and an owner revision.
- `GeologicalKnowledgeState` owns acquired observations, generated observation IDs, immutable spatial
  evidence records, a synchronized material-to-observation index, and an owner revision. It does not
  own or reference exact deposit identities.
- `ProductionState` owns active jobs, generated job IDs, a due-tick index, synchronized exclusive
  energy-store-to-job and equipment-to-job occupancy indexes, a stockpile-to-job-set occupancy index,
  and an owner revision.
- Validated transaction tokens bind to the exact owner revisions they checked, preventing stale
  commits after intervening mutation.

## 6. Mutation Model

Consequential mutations happen in canonical system functions. A fallible operation validates every
precondition before mutation. Multi-resource mutations use consumed validated tokens. Systems that
read broad state but write a narrow result use decide/apply pairs.

Top-level tick order remains visible in one function. Subsystems do not hide gameplay mutations in
callbacks, event handlers, record methods, or engine lifecycle hooks. Production execution keeps one
public facade while separating process-start admission from in-flight availability/completion work.
Thermal process execution similarly separates immutable resolver registration, sensible-heating
runtime resolution, and persisted-job replay validation behind one subsystem facade.

## 7. Persistence

The core defines a current-schema semantic save envelope while deliberately leaving byte encoding and
storage to adapters. `CURRENT_SAVE_SCHEMA_VERSION` in `src/persistence/mod.rs` is the sole owner of
the accepted save schema version. Authored identity/physics compatibility is tracked separately by
`RegistrySchemaVersion`; the built-in content registry owns its current value in `src/content/mod.rs`.
Core gravity, material phase/fusion semantics, physical form and particle-state policies, fluid
identity/density definitions, directional energy-store semantics, and operation-specific resolver
identities are part of that immutable registry contract because changing them can alter persisted
physical consequences even when authored IDs are unchanged.

Persistence is deliberately current-schema-only. `LoadedSaveEnvelope` represents the one payload
shape this build supports, and `into_state` rejects any save-schema or registry-schema mismatch before
returning runtime state. Historical save DTOs, migration paths, compatibility shims, and legacy
decoders are intentionally not retained.

A current-schema load must:

1. Validate save and registry schema versions.
2. Validate every subsystem's local persisted invariants.
3. Resolve every persisted authored/runtime reference.
4. Validate cross-owner reservations, lifecycle, provenance, and in-process conservation.
5. Reject corrupted state before returning `AppState` to runtime use.

Persistence tests cover deterministic continuation, mixed composition, independent RNG continuation,
tampered RNG roots, tampered in-process consumed mass, stable immediate JSON reserialization,
stockpile phase/temperature containment, stockpile structural support/index/load agreement,
fluid-store references/conservation plus support/index/density-derived-load agreement, structural
embodied-mass/self-weight/phase agreement, equipment structural support/load agreement, directional
energy source/sink ownership plus production energy-occupancy-index agreement, and
operation-specific sensible-heating/melting/casting replay from committed physical traces.
Comminution jobs are likewise recomputed from exact consumed-material, equipment, and energy traces,
including authored form transition, admissible feed-size envelope, particle-size distribution,
mass-specific work, carrier, condition-sensitive throughput, power-limited duration, and
post-operation wear. Current-schema lot,
output, and consumed-trace validation also rejects particle-size state that disagrees with the
authored form policy. Screening jobs independently recompute their aperture partition, typed stream
identities, exact output distributions, finite work, duration, and condition outcome.

Filesystem layout, compression, atomic writes, and cloud storage remain adapter work. Historical
save-schema migration is intentionally unsupported.

## 8. World, Spatial, and Engine Architecture

Persistent world references use chunk-agnostic 64-bit voxel coordinates. `VoxelCoord`,
`VoxelDelta`, `ColumnCoord`, and validated half-open `VoxelBounds` provide checked spatial arithmetic
without selecting chunk dimensions, storage encoding, ECS layout, or streaming policy.

Chunk shape, renderer, input, physics engine, threading, and networking remain deliberately
unselected. Domain records must not assume an ECS, scene graph, renderer object, or chunk layout.

Renderer-neutral visual definitions live in the immutable texture registry. A texture is a 32x32
tile of one-byte indexed texels: the high nibble selects one of at most 16 texture-local palette
ramps and the low nibble selects one of 16 authored shades. Four hue/luminance anchors expand into a
complete ramp during registry construction, so shadows and highlights may shift hue without storing
RGBA per texel. Block appearances map all six cube faces explicitly. Object appearances map ordered
mesh material slots explicitly. Commodity and equipment bindings resolve those appearances without
placing renderer objects or mutable visual state in `AppState`.

`TextureRegistry::bake_texture_array()` is the adapter boundary. It sorts authored IDs, deduplicates
indexed patterns independently from palette rows, produces a complete 32/16/8/4/2/1 discrete mip chain,
and returns dense `TextureId`, block-appearance, and object-appearance lookups. Every texture draw
descriptor contains a pattern layer and palette-row pair packed into one shader-facing `u32`; block
faces and object material slots are already resolved to those descriptors before meshing. Mips choose
the majority local ramp with stable slot-order tie-breaking, then average only that ramp's shade
positions; palette indices are never numerically blended. The upload contract is:

```text
slot     = indexed_texel >> 4
shade    = clamp((indexed_texel & 15) + lighting_delta, 0, 15)
ramp_id  = palette_rows[palette_row * 16 + slot]
rgba     = palette_colors[ramp_id * 16 + shade]
```

Indexed mip levels require integer `R8_UINT` storage and nearest/point sampling. Linear filtering of
the index texture is invalid; filtering, if desired, occurs after palette resolution in an adapter
strategy that preserves index identity. Texture definitions are immutable presentation content and
are not persisted by the headless simulation, so changing their color or detail does not alter the
registry compatibility schema used to validate authoritative saves.

`TEXTURE_SIDE` and `TEXTURE_MIP_LEVEL_COUNT` are the single source of truth for both baking and WGSL
sampling. The shader content builder injects those Rust-owned values into the common shader library;
surface and alpha-aware shadow passes therefore use the exact uploaded base size and maximum mip
without independently authored constants. Moving from 16x16 to 32x32 quadruples base texels but not
texture fetches per fragment. Built-in pattern and palette-row deduplication keeps the complete six-
level indexed upload and lookup tables within 16 KiB.

Renderer-neutral WGSL definitions live in the immutable shader registry. Libraries and executable
programs have typed IDs and validated acyclic library-only dependencies. Startup assembly walks those
dependencies in stable order, emits each source module once, and bakes executable programs into a
dense ID lookup for an adapter to compile and cache. The registry carries entry-point names,
premultiplied-alpha/depth/color-target pipeline requirements, compute workgroup sizes, and explicit
worst-case work budgets. Shader source is presentation content and does not enter `AppState` or the
save compatibility schema.

The built-in suite joins the indexed texture path to a linear HDR lighting path: one 16x16 tiled
compute pass deterministically retains at most 32 point lights per tile from at most 512 stable-ordered
candidates; surfaces add palette-ramp lighting, ambient occlusion, four-tap sun shadows, warm block
light, local lights, and height fog. Directional shadows use two bounded pipelines selected from each
baked descriptor's alpha mode: opaque casters use a vertex-only, zero-sample depth program, while
cutout casters share surface UV/key locations and the indexed mip sampler for accurate silhouettes.
Separate programs provide three-wave analytic water with depth-based absorption/refraction and foam,
three-layer procedural billboard smoke, a procedural cloud/star sky, four-read half-resolution bloom,
and ACES-fit display mapping. Depth reconstruction uses WGSL's 0-to-1 clip convention. The exact
frame order, resource formats, vertex layouts, uniform semantics, and binding contract are documented
in `assets/shaders/README.md`; the renderer backend still owns resource allocation, synchronization,
draw ordering, and platform pipeline creation.

## 9. Performance Policy

Performance begins with ownership and access patterns rather than premature micro-optimization.

- Authoritative records and indexes remain compact, private, and deterministic.
- Geological knowledge indexes observations by material so a regional assessment does not scan
  unrelated material evidence. Full bidirectional index verification remains an exhaustive load/audit
  check rather than a per-tick invariant.
- Production uses `BTreeMap<SimulationTick, BTreeSet<ProductionJobId>>` so due work does not require
  scanning all active jobs.
- Production keeps synchronized `EnergyStoreId -> ProductionJobId` and
  `EquipmentId -> ProductionJobId` occupancy indexes plus a
  `StockpileId -> BTreeSet<ProductionJobId>` index. Repeated finite-energy, equipment, and stockpile
  availability checks therefore use deterministic keyed lookup instead of scanning all active jobs.
  Exhaustive validation reconstructs every expected index from durable job traces and rejects
  disagreement.
- Stockpiles maintain cheap derived mass/commodity caches and update them atomically with lot state.
- Fluid stores index support membership bidirectionally, so transfer-time structural recomputation
  visits only stores sharing affected supports rather than scanning all hydraulic storage.
- Output capacity is reserved at process start so completion does not discover a late full-destination
  failure.
- Compatible newly created lot fragments coalesce deterministically to prevent unbounded tiny-lot
  growth while established lot IDs remain stable.
- Save/load validation is exhaustive, but the base tick executes only cheap invariants. Periodic
  exhaustive audits in soak tests preserve corruption coverage without making every tick O(world).
- Texture mesh construction resolves stable texture, block-appearance, and object-appearance IDs
  through bounded dense lookups. Block faces and object material slots are prebaked draw descriptors
  carrying only a texture-array layer and palette row. Indexed tiles cost one byte per texel, custom
  mip levels remain indexed, repeated patterns and palette assignments are deduplicated independently,
  and shared ramps occupy one small RGBA lookup table.
- Shader hot paths carry declared work ceilings enforced at registry construction. Built-ins cap a
  surface at seven texture reads and 32 tile-local lights, procedural effects at three fixed noise
  layers, cutout shadows at three indexed reads, opaque shadows at zero reads, bloom at four
  half-resolution reads, and post processing at five reads. A logarithmic workgroup prefix scan
  compacts lights without nondeterministic atomic allocation, smoke rejects empty billboard pixels
  before procedural noise, and indoor/unlit surface fragments skip shadow taps. The unique shipped
  WGSL source suite has a tested 48 KiB maximum. Naga is a test-only dependency, so shader validation
  adds no graphics library or runtime weight to the shipping crate.
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
- `Length`: micrometers (`u64`);
- `Acceleration`: micrometers per second squared (`u64`);
- `Force`: millinewtons (`u128`);
- `Power`: picowatts (`u128`), with a microwatt convenience constructor;
- `Torque`: micronewton-meters (`u64`);
- `AngularSpeed`: microradians per second (`u64`);
- `ElectricPotential`: microvolts (`u64`);
- `ElectricCurrent`: microamperes (`u64`);
- `ElectricalResistance`: microohms (`u64`);
- `Volume`: microliters (`u64`);
- `AggregateVolume`: world-scale microliters (`u128`);
- `MassSpecificEnergy`: nanojoules per milligram (`u64`);
- `MassFlow`: milligrams per second (`u64`);
- `VolumetricFlow`: microliters per second (`u64`).

Arithmetic that can exceed authoritative storage is checked. Floating point is not used by the
implemented authoritative physical calculations.

## 12. Materials and Physical Forms

Materials are immutable definitions with typed IDs and grouped density, thermal, mechanical, and
electrical properties. Physical forms such as log, lump, ore, crushed material, concentrate, ingot,
and molten matter have typed `FormId` definitions, an explicit `MaterialPhase`, and an authored
particle-size-state policy. `CommodityKey` combines one material and one form for coarse indexing.
Thermal definitions may author an exact solid/liquid fusion point and latent heat.

`ParticleSizeRange` stores validated nonzero minimum and maximum particle diameters using typed
`Length`. `ParticleSizeDistribution` stores one or more nonoverlapping resolved size classes with
canonical relative mass weights. Class weights are reduced to one canonical ratio, and a one-class
distribution is the conservative representation of an unresolved size envelope: it records bounds
without pretending to know how mass is distributed inside them. Forms marked as requiring
particulate state must carry a distribution; forms marked untracked must not. Screening may partition
only classes lying wholly on one side of the authored aperture. A cut through an unresolved class or
a weighted partition that is not exactly representable at the current whole-milligram mass resolution
is rejected instead of fabricating a yield.

`MaterialComposition` is a canonical normalized mass-fraction profile. Components are sorted by
material ID, use integer parts per million, and total exactly 1,000,000 ppm. Duplicate materials,
zero fractions/IDs, invalid totals, and noncanonical deserialized order are rejected. This supports
ore grade, alloy ratios, carbon content, and later contamination without authoring a material ID for
every mixture.

Density-based material volume is computed from mass and composition with conservative upward
rounding at the microliter boundary. Sensible heat is weighted by composition and authored specific
heat. Phase-aware validation requires solid matter to remain at or below its authored fusion point
and liquid matter to remain at or above it. Pure-material fusion energy is represented explicitly as
latent heat. Mixed liquid material is intentionally rejected until alloy/solution phase diagrams are
modeled rather than deriving a fictitious weighted melting point.

The initial material content includes wood, charcoal, copper, slag, and foundational forms. It is
architecture content, not the complete gameplay catalog.

## 13. Inventory, Geological Matter, and Matter Ownership

Material lots are the authoritative stored-matter representation. Each `MaterialLotRecord` owns a
persistent lot ID, stockpile owner, exact mass, a `MaterialLotProfile` (commodity, absolute
temperature, normalized composition, and optional form-governed weighted particle-size distribution),
and a creation provenance range. Lot coalescing compares the complete profile, so different
particle-size distributions cannot be averaged away by inventory compaction.

Stockpiles maintain derived deterministic indexes/caches for lot IDs, per-commodity mass, total
stored mass, capacity, reserved inbound mass, and a persisted containment profile declaring accepted
material phases plus maximum material temperature. A stockpile may also own one optional structural
support assignment. `InventoryState` maintains the synchronized support-to-stockpile reverse index;
`StructuralLoadKind::StoredMatter` is derived from the aggregate stored mass of all stockpiles on a
support, converted to force once under registry-authored gravity. Aggregating mass before conversion
avoids per-container rounding creating artificial weight. Reserved inbound capacity is space, not
matter, and therefore contributes no structural load until physical output becomes authoritative.

`validate_transfer_bulk` plus the consumed `ValidatedTransferBulk::commit` performs revision-bound
movement between distinct stockpiles. A same-stockpile request is rejected as an invalid transfer;
source-equals-destination production reservations use their separate production transaction path.
If either stockpile is structurally supported, transfer validation computes
both final stored masses and analyzes the complete final stored-matter load arrangement under one
structural revision before matter moves. The transaction remains bound to that structural revision
even when aggregate mass-to-force rounding leaves the numeric load unchanged. Partial transfers split
lots in stable ID order without averaging physical properties away. Newly created compatible
fragments can coalesce into the lowest-ID compatible destination lot. Every canonical ingress and
production-output reservation rechecks destination containment.

Cross-owner systems that physically inspect exact lot slices before deciding an outcome use a
crate-private exact-relocation transaction rather than `validate_transfer_bulk`. It consumes the
already-bound `ConsumptionSelection`, preserves each selected profile and provenance exactly, assigns
distinct deterministic IDs for multiple partial slices, validates destination containment/capacity,
and plans source/destination stored-matter loads together. Reserved inbound capacity participates in
the space check but never in structural weight. This primitive does not select material itself and is
therefore not an arbitrary gameplay movement API.

There is intentionally no public arbitrary inventory-deposit API. Tests can seed inventory through
`#[cfg(test)]` fixtures. World generation has a separately named geological source boundary that
admits validated finite deposits into `GeologyState`; it is not a player extraction path and does not
write inventory directly. Its `GeneratedDepositSpec` is opaque and has no production/public
constructor until a real regional geological generator can authorize one, so exposing the admission
function does not expose arbitrary matter creation. `AppState` likewise does not publicly expose
authoritative deposit enumeration: external/player-facing adapters receive acquired geological
knowledge instead of a hidden-truth escape hatch.

`calculate_matter_accounting` recomputes implemented world matter from authoritative records. It
counts remaining geological deposit mass, embodied structural mass, inventory lots, and in-flight
production output snapshots. Consumed-input traces are history unless they are explicitly owned as
structural embodiment; reserved inbound capacity is space rather than matter.

`GeologicalDepositRecord` stores a chunk-agnostic `VoxelBounds`, exact initial and remaining mass,
commodity/form identity, absolute temperature, normalized composition, generated tick, and a
validated available/depleted lifecycle. Natural geological ownership accepts only solid forms that do
not require processed particle-size state; crushed/ground particulate belongs to later material-
processing owners rather than being admitted as an under-specified natural deposit. It does not
prescribe ore-body generation algorithms, terrain voxel storage, prospecting visibility, or mining
geometry. Overlapping geological bounds are
therefore not prohibited by this foundation; a future geological model may use overlapping records
for distinct structures or mineralization rather than forcing a premature one-record-per-voxel rule.

Geological extraction is a revision-bound cross-owner transaction. An opaque `ExtractionResolution`
must already exist before validation, so this foundation does not expose free instant mining. The
transaction checks finite remaining mass and destination capacity, binds geology and inventory
revisions, and also binds structure when the destination stockpile is supported. It then moves the
exact mass, composition, form, temperature, and provenance into a material lot while applying the
destination's final stored-matter load atomically. Inventory exposes only a crate-private validated
ingress primitive for explicit source owners, preserving the existing prohibition on arbitrary
gameplay matter insertion.

Explicit modeled-energy accounting includes supported material sensible plus latent thermal energy
still owned by geological deposits, structural members, inventory, and in-process matter, together
with finite energy stores and energy retained by active production work. Extraction, construction,
deconstruction, sensible heating, pure-material melting, and casting therefore change ownership
without changing the modeled total when all represented sources and sinks are included.

## 14. Prospecting Knowledge

Authoritative geological deposits and acquired knowledge are deliberately separate owners. Exact
deposit identity, bounds, remaining mass, and composition are simulation truth; player-facing
prospecting state contains only observations that a physical survey resolver has authorized.

`GeologicalObservationRecord` stores a chunk-independent spatial footprint, evidence provenance,
observation tick, and a canonical material-sorted list of bounded abundance estimates in integer
parts per million. Evidence kinds such as surface exposure, panning, core samples, assays, magnetic,
electrical, and seismic surveys are provenance labels rather than technology levels. Accuracy is
represented by the actual abundance interval and spatial footprint supplied by the physical resolver.

An opaque `ProspectingResolution` has no public constructor. Future sampling, drilling, laboratory,
tool, labor, and instrument systems must determine what was measured before
`validate_record_prospecting` can create a revision-bound commit token. Persisting knowledge never
consults hidden deposits and therefore cannot become a magic reveal path.

`assess_geological_knowledge` reads only acquired evidence for one material and region. It clips
relevant observation footprints to the query and intersects hard abundance bounds only when every
relevant observation shares a nonempty common locality. Evidence from disjoint subregions is marked
spatially incomparable rather than being averaged or reported as a false contradiction. Genuine
incompatible bounds over a common locality remain an explicit conflict. The broader evidence envelope
is retained, the common evidence region is exposed, and the most precise supporting observation is
selected deterministically by abundance width, spatial footprint, recency, then ID.
`build_geological_knowledge_map` produces stable material-ID-ordered regional projections and omits
materials whose observations do not intersect the requested region. Evidence relevance therefore
does not imply a fabricated uniform whole-region ore grade.

## 15. Timed Production

Production separates static requirements from operation-specific physical outcomes.

`ProcessDefinition` contains stable identity, composition-aware material inputs, and typed capability
requirements. It does not contain a fixed duration, fixed output yield, or generic technology tier.
Capability references and physical value kinds are validated when registries are assembled.

`ProcessResolution` contains the resolved duration and exact output lot specifications for one
specific operation plus optional finite energy/equipment outcomes owned by that operation. It has no
public arbitrary constructor. Physical resolvers must produce the plan before the canonical start
transaction can accept it. Implemented thermal resolvers cover phase-aware sensible heating,
pure-material melting, and pure-material casting. The ore-processing foundation additionally resolves
selected-batch comminution: a crusher/grinder assigns an authored particulate diameter envelope while
preserving each distinct input trace's mass, composition, and temperature, derives duration from
condition-sensitive `MassFlow`, enforces maximum batch mass, and requires exact authored
`MassSpecificEnergy` from a finite source of the required carrier. Coarse untracked input can establish
its first explicit envelope. Already-particulate input must reduce maximum diameter without increasing
minimum diameter, so a grinder can perform a same-form `crushed -> crushed` transition without
inventing a second material form merely to encode fineness. A comminution definition may additionally
author an admissible particulate feed envelope. Every selected input trace must lie wholly inside that
range before the output reduction is evaluated, and the registry requires constrained processes to
use particulate input forms and to admit at least the authored reducing output relationship. This
models mill operating/feed limits as physical process semantics instead of hard-coded machine IDs or
technology tiers. Authoritative duration is the slower of
equipment throughput and source output power, so weak power infrastructure reduces throughput and
increases active-tick wear. The canonical jaw crusher establishes one conservative 500-10000 um class.
A separate grinding mill uses distinct typed capabilities to reduce that same-form material to two
equal-weight classes, 500-2000 um and 2001-4000 um. Screening remains a separate resolver because it
may classify only size classes wholly on one side of its aperture; direct crusher output therefore
fails the 2 mm screen while grinder output can be partitioned exactly. A constrained fine-grinding
pass accepts only the resulting 2001-4000 um oversize stream and reduces it to 500-2000 um. Routing
that output back into the undersize stockpile therefore gives a closed preparation circuit in which
only oversize pays the additional energy and wear. Separation by composition, recovery, chemical
smelting, alloying, tooling, labor, and skill remain separate future resolvers.

`ResolvedComminution` exposes the exact observed equipment condition and predicted post-operation
condition alongside throughput-limited duration, energy-limited duration, required work energy,
available power, effective processing rate, and typed bottleneck. Player-facing decision layers can
therefore compare slower resource-conserving operation against faster lower-wear operation from the
same authoritative resolution that will later be committed, rather than duplicating wear or duration
math in an adapter.

Production is closed-mass in the implemented core: resolved output mass must equal authored input
mass. Slag, tailings, wastewater, gas, and similar losses must therefore be explicit material streams
instead of hidden yield loss.

At start, selected input slices are removed from inventory and the job becomes the matter owner via
its committed output snapshot. Each durable job also preserves consumed mass and consumed-input
physical/provenance traces without retaining dangling source lot IDs. Energy supplied from a finite
source becomes an explicit consumed-energy trace. Energy released by an operation remains an explicit
job-owned trace until authoritative completion transfers it into the reserved finite sink. Loaded
jobs validate these traces and operation-specific physics rather than trusting mutable caller input.

Destination output capacity is reserved at start. Completion uses a deterministic due-tick plan and
converts reserved capacity into actual output lots before removing the job/index entry.

Supported stockpile weight follows authoritative matter ownership rather than reservations. Starting
a job removes the consumed input mass from its source stockpile's derived `StoredMatter` load when the
job becomes the matter owner. In-flight matter is not simultaneously counted as stockpile weight.
When jobs complete, all output mass arriving at the same destination tick is aggregated first and one
final support-load plan is analyzed before output ingress, so simultaneous completions cannot produce
order-dependent structural results. Starting new work requires any supported destination stockpile to
have an active support. Once output capacity is durably reserved, however, a later support collapse
does not invalidate the already-owned in-flight matter: completion may deposit that reserved output
onto the failed debris and records its resulting `StoredMatter` load. This avoids an otherwise
unrecoverable occupied-job state while preserving the rule that failed supports accept no new inbound
work.

The general built-in gameplay production registry remains intentionally sparse until physical
authorization systems exist for each operation. Canonical content currently registers the jaw
crusher's ore-comminution process, a same-form grinding process, a 2 mm dry-screening process, and the
selective oversize fine-grinding pass plus the pure-copper melt/cast path because concrete equipment
and finite energy owners exist for those operations. Crusher, grinder, and screen each have distinct
typed throughput/batch capabilities, condition-sensitive throughput, active-tick wear, and exact
mechanical work requirements. The chain does not fabricate concentration: crushing preserves one
unresolved class, grinding authors a finer
resolved size distribution, screening changes only size-class ownership, and the constrained regrind
returns only coarse material to the fine profile while preserving material composition and
temperature. Test registries continue to exercise additional resolver boundaries
without turning them into unrestricted recipe shortcuts. Comminution and screening reject absent,
insufficient, or wrong-carrier energy rather than granting free processing, and no concentration,
chemical separation, or smelting process is registered until a physical resolver can justify its mass
partition and chemistry.

## 16. Capabilities, Maintenance, Energy, Flow, and Mechanical Power

Capabilities are typed physical requirements rather than generic progression levels. Current value
kinds include presence, mass, mass flow, temperature, energy, pressure, force, power, torque, angular
speed, electrical quantities, volume/flow, and equipment condition. Each requirement states `AtLeast`
or `AtMost` threshold semantics. `CapabilityProfile` owns deterministic nominal/static values, while
`CapabilitySource` lets runtime-adjusted providers satisfy the same evaluator without materializing
temporary maps. The built-in capability registry contains only capabilities backed by canonical
workshop equipment and physical resolvers; unrelated future capability families remain unauthored.

Maintenance uses normalized `Condition` with authored warning and critical thresholds. Equipment
definitions may additionally author piecewise-linear response curves for individual typed
capabilities. Each curve owns an explicit failed-condition endpoint and interpolates toward the
definition's nominal capability at pristine condition using overflow-safe integer arithmetic that
rounds toward the degraded endpoint. Runtime equipment providers resolve these effective values on
demand without allocation. Uncurved capabilities remain nominal. Presence-only capabilities cannot
use continuous condition curves because the capability model has no numeric absence state; any
future capability-disable behavior must be an explicit discrete policy. Pure wear plans clamp at the
physical failed bound without deleting the owning record.

Equipment repair is a separate conserved cross-owner transaction rather than a public condition
increment. `EquipmentRepairResolution` has no public constructor. A future physical maintenance
resolver must bind a specific equipment instance together with the equipment owner revision and
observed pre-repair `Condition`, a strictly improved final `Condition`, an exact inventory
`ConsumptionSelection`, and an explicit spent-material stockpile. Transaction validation first
rejects a stale equipment snapshot, then resolves the equipment definition, rejects active production
occupancy, binds the resulting owner revisions, and delegates the exact matter movement to the
inventory relocation primitive. Commit rechecks equipment revision, condition, and production
occupancy before allowing any material mutation, then relocates the exact selected matter and finally
applies the infallible condition improvement. If the maintenance source
or spent destination is structurally supported, both final `StoredMatter` loads are analyzed in the
same validated relocation. The current boundary intentionally preserves spent material identity,
temperature, composition, particulate state, and provenance; replacement-part consumption,
lubrication chemistry, salvage/waste transformation, tools, workers, skill, duration, access, and
maintenance-process automation remain responsibilities of future physical resolvers rather than
being guessed inside the transaction.

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

Finite energy stores are persistent runtime owners with immutable carrier/capacity definitions and
independent maximum input/output power. Runtime allocation creates empty capacity only. A validated
energy supply reserves one store's output capability for an active production job; a validated
energy sink similarly reserves input capacity. An active job cannot simultaneously share a source or
sink with another job. `ProductionState` maintains that exclusivity in a synchronized keyed occupancy
index used by hot-path energy validation. Released process energy is applied only during authoritative
completion after all participating owner revisions have been rechecked, so a stale energy mutation
cannot partially solidify material, wear equipment, remove a job, or deposit heat.

Store-to-store relocation has a separate atomic storage boundary for an already physically resolved
same-carrier transfer. `EnergyTransferResolution` deliberately has no public constructor. Validation
requires distinct stores, source output capability, destination input capability, identical carriers,
unreserved endpoints, sufficient source energy, finite destination capacity, and an available energy
revision. The consumed commit token binds both the energy and production owner revisions plus exact
endpoint energy snapshots, then subtracts and adds the identical quantity under one energy revision.
This boundary does not choose a path, integrate transfer power over time, convert carriers, model
losses, or generate energy. Those decisions remain responsibilities of future electrical, mechanical,
thermal-distribution, and generation owners.

Finite fluid storage is a separate conservation boundary. `FluidDefinition` binds an authored fluid
identity to an underlying material identity and a nonzero constant bulk density in kilograms per
cubic meter. Density is fluid-specific rather than borrowed from the underlying material's solid
density and is part of registry compatibility because it changes persistent structural consequences.
`FluidStoreRecord` owns finite volume, temperature, and one optional structural support assignment;
`FluidState` owns the synchronized support-to-store reverse index. Runtime allocation creates empty
capacity only.

`StructuralLoadKind::Fluid` is exclusively owned by the fluid integration. For each structural
support, exact `volume_uL * density_kg_per_m3` numerators are summed across every supported store
before one conservative ceiling conversion to aggregate milligrams and then one gravity conversion to
force. This keeps support weight independent of how the same fluid volume is partitioned among tanks.
Mount and unmount are revision-bound fluid/structure transactions. New support assignments and any
transfer that increases a support's aggregate fluid weight require an active member; the resulting
load may crack or collapse that member through ordinary structural analysis. Draining failed debris,
redistributing fluid between stores on the same failed support when aggregate weight does not rise,
and unmounting failed stores remain legal so cleanup cannot require resurrecting a structure.
Structural removal is rejected while the member remains referenced by any fluid store.

The storage transaction moves an already physically resolved volume atomically, but
`FluidTransferResolution` has no public constructor, so the storage layer cannot authorize pathless
movement. Transfer validation computes both stores' final contents and all affected support loads in
one plan, binding the structural revision even when a same-support transfer leaves aggregate force
numerically unchanged. Commit applies structural consequences before the infallible owner mutation,
and `FluidTransferOutcome` exposes any resulting structural analysis. Transfers refuse unlike
identities or temperatures instead of inventing mixture chemistry or thermal equilibration. Pressure,
gravity-driven routing, channels, pumps, temperature/pressure-dependent density, surface water,
groundwater, and a mass/volume phase bridge remain future owners.

Thermal phase-change production builds on these boundaries. Pure-material melting combines sensible
heating to the fusion point with authored latent heat and requires a liquid-capable destination.
Pure-material casting performs the inverse, including sensible cooling of superheated liquid and
latent heat release into an explicit finite thermal sink. Both persist exact input/output/energy and
equipment-condition outcomes and share their runtime physics with exhaustive save validation.

Future persistent network owners must preserve carried remainders when integrating incrementally so
small rates are not lost to repeated truncation. The mechanical scalar layer intentionally chooses
no shaft graph, belt routing, flywheel inertia model, slip state, clutch lifecycle, or network solver.

## 17. Structural Construction Matter

Structural planning is deliberately separate from material ownership. `add_structural_element`
creates a planned geometry/reference record with no embodied matter, and activation is rejected until
construction matter has been committed. The core does not derive mass from `VoxelBounds` times
cross-section because bounds are only an occupancy envelope and the current profile does not define a
member axis, solid length, joinery, or cutting waste. Inventing those assumptions would make later
geometry and construction systems incompatible with persisted consequences.

An opaque `StructuralConstructionResolution` owns an exact inventory selection produced by a future
physical construction resolver. `validate_structural_construction` binds inventory and structural
owner revisions, requires a still-planned unmaterialized target, and currently requires every trace
to be pure matter matching the member's authored material. This purity restriction is intentional:
current structural capacity uses one authored material's strength and cannot safely assign full pure
strength to contaminated or composite matter. Composition-aware structural mechanics can relax this
only when they define the corresponding physical capacity model.

Construction transfers the exact selected mass, temperature, composition, and provenance from
inventory into persisted structural embodiment and requires the embodied form to be solid.
`StructuralLoadKind::SelfWeight` is then derived from the committed aggregate mass under
registry-authored gravity. Self-weight is a structure-owned load channel and is rejected by the
public generic load mutation API, preventing callers from forging or erasing the member's own weight.
If the construction source stockpile is structurally supported, its final stored-matter load is
resolved before the material leaves inventory. Exhaustive load validation recomputes embodied trace
mass, phase, and self-weight independently.

Direct generic removal is rejected for any member that still owns matter. An opaque
`StructuralDeconstructionResolution` instead validates a destination stockpile and prepares both a
structural removal and exact trace-preserving inventory ingress. Commit rechecks both owner revisions,
removes the member through normal structural cascade analysis, then restores every embodied trace to
inventory without changing its physical profile or provenance. If the recovery destination is
supported, member removal and the destination's final stored-matter load are analyzed together in one
structural overlay and committed under one revision, avoiding an artificial order-dependent
intermediate structure. Failed debris follows the same conservation boundary. Future dismantling and
demolition resolvers may produce explicit salvage, debris, or waste streams, but they must balance the
member's committed mass rather than deleting it.

Construction and deconstruction tests verify cross-owner stale-token atomicity, exact matter and
modeled sensible-energy conservation, direct-deletion prevention, mixed-composition rejection under
the current strength model, persistence corruption rejection, and a deterministic 1,000-cycle
inventory-to-structure-to-inventory soak.

## 18. Stockpile Structural Support

Stockpile records own an optional `StructuralElementId` support assignment, while `InventoryState`
maintains the synchronized support-to-stockpile reverse index. Mount/unmount is a revision-bound
inventory/structure transaction. Mounting requires an active structural target and writes only the
inventory-owned `StructuralLoadKind::StoredMatter` contribution. All stockpile masses assigned to one
support are aggregated before gravity conversion. The generic structural-load API rejects direct
`StoredMatter` writes, preserving inventory as the sole source of truth.

Adding stored matter can crack or collapse a support through normal structural analysis. A support
cannot be removed while any stockpile still references it. Failed debris can be unloaded so cleanup
does not require resurrecting the member; unloading removes the derived load but does not clear crack
or failure state. Failed supports reject newly initiated inbound matter. Output already reserved by a
production job while the support was valid may complete after a later collapse because the job owns
that matter and the occupied stockpile cannot be relocated mid-operation. Stockpiles participating as
a production source or destination cannot be moved to a different support while that job is active.

Every canonical operation that changes stockpile stored mass participates in the same ownership
invariant: bulk transfer, production start/completion, geological extraction, structural
construction/deconstruction, and test/bootstrap material seeding. Cross-owner tokens bind structural
revisions whenever supported matter is involved, including zero-force-delta changes where conservative
rounding leaves the stored contribution unchanged. Multi-stockpile or simultaneous-output changes use
one batch load plan so no transient ordering can fabricate or omit weight. Save validation audits both
support-index directions and independently recomputes each structural member's required stored-matter
load.

## 19. Equipment Structural Support

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

## 20. Cross-Subsystem Runtime Invariants and Boundaries

`validate_loaded_state(registries, state)` validates local owners plus cross-system relationships,
including registry references, lot provenance, phase state, and particle-size policy, generated ID
cursors, due-index membership, stockpile cache/containment/reservation agreement, finite fluid
definitions/capacity/support indexes and independently derived fluid structural weight,
job lifecycle, consumed-input references, energy source/sink ownership, and in-process matter/energy
conservation, geological deposit references/lifecycle/provenance/solid nonparticulate form, geological observation
references/order/provenance and both directions of the material evidence index, structural embodied
trace mass/material/composition/provenance/solid phase, consolidated-form eligibility, and self-weight
agreement, equipment support references and mounted-equipment structural-load agreement, stockpile
support references and both directions of the inventory support index, independently derived
stored-matter structural-load agreement, and fluid support references in both directions. Operation-
specific thermal audits recompute sensible heating, melting, casting,
condition-sensitive equipment outcomes, and released heat from persisted physical traces. Comminution
audits likewise recompute the exact form and particle-size result, so an in-flight job's contract
remains reproducible after load.

The foundation intentionally leaves unresolved choices unresolved. Deferred areas include chunk
storage/streaming, renderer/ECS/physics/networking, regional geological generation, physical
prospecting resolvers and mining authorization/rates/waste streams, richer construction geometry and
demolition/salvage resolvers, environmental thermal fields/heat transport, vaporization, mixed/alloy
phase diagrams, combustion/emissions, chemical smelting/reduction, forging/machining, real
equipment/tool/worker capability providers, persistent mechanical networks/inertia/slip,
steam/boilers, electrical topology/transformers/protection, pressure/gravity-resolved hydrology and
fluid networks, agriculture, ecology/genetics, creatures/workers, settlements/logistics/trade, and
save-file storage adapters. Historical save-schema migration remains intentionally unsupported.

New systems must integrate through owned records, immutable definitions, typed IDs/quantities,
canonical mutations, dedicated errors, persistence semantics, invariant coverage, and behavioral
soak tests. Do not introduce shortcut recipes or generic technology tiers where the game design
requires physical authorization.

# Status

## Current Foundation

- Headless deterministic Rust simulation core with no renderer or engine dependency.
- Immutable registry aggregate with separate authored-ID compatibility version.
- Typed absolute `SimulationTick`, relative `TickSpan`, and deterministic periodic phase scheduling.
- Persisted independent RNG streams derived from the world seed, normalized integer probability,
  and unbiased bounded random selection.
- Explicit authoritative integer quantities for mass, aggregate mass, temperature, energy,
  pressure, area, length, acceleration, force, power, torque, angular speed, voltage, current,
  resistance, volume, aggregate volume, and volumetric flow.
- Typed material/form definitions with density, thermal, mechanical, and electrical properties,
  explicit solid/liquid form phase, and optional authored fusion temperature/latent heat.
- Canonical normalized mass-fraction composition for ores, alloys, and mixed material lots, including
  validated deserialization and composition-aware material inputs.
- Density-based conservative material-volume calculation, composition-weighted sensible heat, exact
  pure-material fusion latent heat, and phase-consistent thermal-state validation. Solid matter may
  reach but not exceed its authored fusion boundary; liquid matter must remain at or above it. Mixed
  liquid compositions are refused until real alloy/solution phase diagrams exist rather than being
  assigned an invented weighted melting point.
- Persistent material lots with mass, temperature, composition, ownership, and provenance ranges.
- Capacity-aware stockpiles with derived commodity totals, cached mass, inbound reservations,
  revision-bound atomic transfers, deterministic splitting, compatible-fragment coalescing, and a
  persisted material-containment envelope for accepted solid/liquid phases and maximum temperature.
  The convenience stockpile allocator remains solid-only. Every deposit, ingress, transfer, future
  production output, and exhaustive save audit rechecks phase and temperature compatibility.
- Persistent finite geological deposits with chunk-independent bounds, exact initial/remaining mass,
  material form, normalized composition, temperature, generation provenance, depletion lifecycle,
  generated IDs, owner revision, and exhaustive registry/state validation. Authoritative deposit
  enumeration remains crate-private. World-generation admission accepts an opaque generated-deposit
  plan with no production/public constructor, so player-facing adapters cannot bypass prospecting or
  use the source boundary as a matter-spawn API. Geological ownership is explicitly solid-only and
  independently revalidated on load.
- Revision-bound geological extraction transfers exact conserved matter into inventory through a
  crate-private validated ingress primitive. Extraction binds both geology and inventory revisions,
  rejects stale or over-capacity commits without partial mutation, preserves physical material
  profiles, and exposes no public constructor for mining authorization before tool/labor/geometry
  physics exist.
- Persistent geological knowledge is separate from authoritative deposit truth. Prospecting
  observations own stable IDs, spatial footprints, evidence provenance, bounded material-abundance
  estimates, observation time, a revision, and a synchronized material-to-observation index.
  Recording is revision-bound and atomic; physical survey systems must resolve an opaque prospecting
  result before knowledge can be persisted.
- Read-only geological assessment combines only acquired evidence. Hard measurement bounds are
  intersected only when all relevant observations share a common spatial overlap; disjoint evidence
  is explicitly spatially incomparable rather than being turned into false precision or a false
  conflict. Genuine contradictory evidence at a common locality remains visible, and precision
  ranking uses quantitative abundance width and spatial footprint instead of technology tiers.
  Regional geological-map projections are deterministic and omit materials known only outside the
  requested area.
- Closed-mass timed production with explicit fixed-feed versus selected-batch input policies,
  deterministic exact-lot binding, durable consumed-input traces, operation-specific resolved output
  snapshots, revision-bound start tokens, and due-tick indexing. Physical resolvers consume the same
  exact lot selection they inspected rather than reselecting equivalent-looking matter at commit.
- Typed authored capability requirements with physical value kinds and registry-reference validation.
  Built-in capability and production registries remain intentionally empty until real providers and
  physical resolvers exist.
- Continuous equipment `Condition`, authored maintenance warning/critical bands, and pure wear/repair
  plans without disposable durability semantics.
- Persistent maintainable equipment records with immutable physical mass and capability-provider
  definitions, revision-checked wear/repair application, provider resolution, registry-reference
  validation, save/load ownership, in-flight provider provenance, and exclusive operation occupancy.
  Definitions may author deterministic piecewise-linear condition curves per typed capability;
  effective values are resolved on demand without allocating temporary profiles, and pristine values
  remain the single nominal source of truth. Maintenance mutation is rejected while an active
  production job owns the equipment instance. Operation-specific production resolvers can persist an
  exact post-operation condition and completion applies wear atomically under the equipment owner's
  revision, with simultaneous due outcomes sharing one revision advance. Continuous condition curves
  reject presence-only capabilities; discrete capability loss remains an explicit future policy
  rather than fake numeric interpolation.
- Persistent equipment-to-structure support assignment with revision-bound two-owner mount/unmount
  transactions and a synchronized support-to-equipment reverse index. Mounted equipment mass is
  aggregated support-locally before gravity conversion, writes only the equipment-owned structural
  load channel, can crack or collapse its support through normal structural analysis, cannot be moved
  while occupied by production, and blocks removal of a support until unmounted. Failed debris can be
  unloaded without repairing or resurrecting it. A machine mounted on a failed support cannot
  authorize new work. Resolved mounted-equipment use binds both equipment and structural owner
  revisions through start validation and commit, while support and maintenance commits recheck
  production occupancy immediately before mutation. Exhaustive load validation audits both index
  directions and the independently derived structural force.
- Persistent finite-energy stores with typed electrical/thermal/mechanical carriers, immutable
  capacity and independent input/output power envelopes, monotonic runtime IDs/revisions, exact
  consumed- and released-energy provenance, and registry-aware persistence validation. Public runtime
  allocation creates empty stores only; arbitrary energy seeding remains test/bootstrap-only until a
  conserved generation owner exists. Source-only, sink-only, and bidirectional stores are explicit.
  Active jobs reserve every participating source or sink exclusively. Released process heat remains
  owned by the in-flight job and enters its finite sink only when completion becomes authoritative;
  stale sink revisions reject completion atomically before material output, wear, job removal, or
  energy mutation.
- Persistent structural members with typed material/profile references, grouped immutable geometry,
  explicit physical length independent of voxel bounds, cross-section, planned/active/failed
  lifecycle, exact embodied material traces and mass, persistent cracking, and synchronized
  forward/reverse support indexes with cycle rejection. Invalid zero length or cross-section cannot
  enter the normal allocation path, and active or failed members cannot exist without embodied
  construction matter.
- Revision-bound construction transfers an exact pre-resolved lot selection from inventory into one
  planned structural member, preserving composition, temperature, and provenance. Prismatic solid
  volume is derived from cross-section and physical length, while required pure-material mass is
  derived directly from exact geometry and authored density with one conservative milligram rounding
  boundary. Read-only material-requirement resolution exposes that physical requirement without
  authorizing construction. The conserved construction transaction rejects both under- and
  over-materialization, accepts only pure solid matter matching the member's authored material, and
  derives a structure-owned `SelfWeight` load from the committed mass and registry gravity.
  Persisted structural embodiment independently rechecks its solid phase. `SelfWeight` cannot be
  written through the generic load API.
- Materialized structural members cannot be deleted through generic removal. Revision-bound
  deconstruction validates destination capacity and both owners, removes the member only as part of a
  conserved recovery transaction, and returns every embodied trace to inventory without losing its
  physical history. Failed debris uses the same recovery boundary.
- Deterministic axial structural analysis using authored material compressive/tensile strength and
  exact strength-times-area force capacity, stable equal-load sharing, readable stable/strained/
  cracking/failed stages, cracked-capacity degradation, and synchronous overload/support-loss
  cascades.
- Source-separated structural load contributions for self-weight, permanent load, stored matter,
  equipment, fluid, snow, wind, and occupancy so independent owning systems cannot overwrite each
  other's causes. Self-weight and equipment load channels are exclusively owned by their source
  integrations; direct generic writes are rejected. Zero writable contributions are removed
  canonically.
- Revision-bound structural transactions for support linking/removal, activation, load updates, and
  unmaterialized-plan removal. Consequences are resolved before commit; materialized removal is
  routed through conserved deconstruction and rebuilt structures never reuse identity.
- Component-local structural mutation analysis uses a one-operation read overlay instead of cloning
  or rescanning unrelated structures, while exhaustive save audits still validate the full graph.
- Authored core gravity plus conservative exact single-record and aggregate mass-to-weight and
  pressure-times-area force conversions provide shared physical boundaries for storage, equipment,
  snow, fluid, soil, and wind integrations.
- Exact power-to-energy, flow-to-volume, electrical-power, and resistive-drop scalar calculations
  with explicit carried fractional remainders where repeated truncation would lose resources.
- Persistent finite fluid stores with monotonic runtime IDs/revisions, explicit capacity, homogeneous
  authored fluid identity, volume, temperature, and exhaustive persistence validation. Fluid
  definitions are registry-owned and cross-reference underlying material identity; the built-in fluid
  registry remains intentionally empty until phase-aware world fluid content exists. Public runtime
  allocation creates empty capacity only, so the storage owner cannot manufacture water or other
  fluid. Revision-bound transfer commits conserve exact aggregate volume and clear zero-volume
  identities canonically. Gameplay cannot construct a transfer resolution directly: gravity,
  pressure, channel, or pump systems must eventually authorize movement. The current conservative
  transfer path refuses unlike fluid identities or temperatures rather than silently inventing
  mixture chemistry or thermal equilibration.
- Read-only world-scale fluid-volume accounting aggregates beyond one store's `u64` range without
  trusting cached totals.
- Exact scalar rotational mechanics with micronewton-meter torque and microradian/second angular
  speed, typed torque/speed capabilities, independent torque/speed/power operating limits,
  normalized mechanical efficiency with explicit loss, and canonical rational transmission ratios.
  Ratio transforms conservatively round output torque/speed down and account any sub-unit remainder
  as loss instead of creating power. Shaft/belt network topology remains deliberately unchosen.
- Exact inverse power-duration calculation returns the minimum whole tick span that can supply an
  energy requirement, including authoritative-range overflow handling without floating point.
- Selected-batch sensible heating derives required energy from each selected lot's actual mass,
  composition, temperature, and authored phase; validates equipment heating power, maximum
  temperature, maximum batch mass, finite energy carrier, and discharge power; derives duration
  exactly; and preserves matter/composition/form in heated outputs. Solid sensible heating may reach
  but cannot cross fusion, while liquid sensible heating may continue upward from the fusion boundary
  without charging latent heat a second time. Runtime resolution and persisted-job validation share
  the same phase-aware calculation.
- Pure-material melting is a real physical production resolver rather than a recipe shortcut. It
  requires selected solid matter of one pure material, derives exact sensible heat to the authored
  fusion point plus exact latent heat, checks furnace power/temperature/batch limits and finite energy
  supply, derives duration and equipment wear, and commits a pure molten output at the fusion
  boundary. Molten output cannot start unless its destination explicitly accepts liquid matter at the
  required temperature. Mixed/alloy melting remains blocked until phase diagrams exist.
- Pure-material casting/solidification resolves the inverse transfer. It requires selected pure
  liquid matter, derives sensible cooling to the fusion point plus latent heat release, checks mold or
  cooling-equipment power/temperature/batch limits, reserves a finite thermal-energy sink with an
  explicit input-power envelope, and commits a solid output at the fusion boundary. Released heat is
  persisted with the in-flight job, is replay-validated from the consumed liquid traces, and is moved
  into the sink atomically only at completion.
- Read-only global matter accounting across geological deposits, embodied structural matter,
  inventory, and in-process matter ownership.
- Read-only explicit modeled-energy accounting across finite stores, supported material sensible plus
  latent thermal energy in geological/structural/inventory/in-process ownership, energy supplied to
  active jobs, and released heat retained by in-flight phase-change work. Geological extraction,
  structural construction/deconstruction, sensible heating, pure-material melting, and casting all
  preserve the modeled total across ownership changes.
- Canonical top-level tick pipeline with cheap per-tick invariants and exhaustive save/load audits.
- Persistence semantic schema 22 and authored registry compatibility schema 10 with metadata
  preflight, registry-aware state validation, structural topology/damage audits, energy/equipment
  ownership validation, directional energy-source/sink reservation and capacity audits, embodied
  structural matter/self-weight/phase audits, geometry/density-to-mass recomputation,
  equipment-support/load agreement audits, exclusive-resource double-book detection,
  operation-specific sensible-heating/melting/casting recomputation including post-operation
  condition outcomes and released heat, stable in-flight conservation snapshots, and deterministic
  continuation tests.
- Chunk-independent 64-bit voxel coordinates and validated spatial bounds without choosing chunk
  dimensions or streaming policy.
- Deterministic 10,000-tick mixed-system soak with repeated production/transfers, varying structural
  snow load on a persistently cracked supported deck, full-state replay equality, periodic exhaustive
  audits, matter-conservation checks, and lot-fragmentation ceiling.
- Deterministic 5,000-tick real sensible-heating soak with repeated exact lot resolution, finite
  energy depletion, equipment/energy reservations, duration-derived equipment wear, periodic
  exhaustive audits, matter conservation, modeled-energy conservation, and replay-identical final
  state.
- Deterministic 2,000-step geological extraction soak with exact finite depletion, compatible-lot
  coalescing, periodic exhaustive audits, matter and modeled sensible-energy conservation, and
  replay-identical final state.
- Deterministic 2,000-observation prospecting soak with synchronized material indexes, periodic
  exhaustive audits, stable persistence continuation, and replay-identical final state.
- Deterministic 1,000-cycle construction/deconstruction soak repeatedly moves one finite material
  batch between inventory and active structures whose geometry resolves to that exact density-based
  quantity, with periodic exhaustive audits, matter and modeled sensible-energy conservation, and
  replay-identical final state.
- Deterministic 2,000-transfer fluid-storage soak repeatedly moves one finite homogeneous fluid volume
  through multiple stores, with periodic exhaustive state audits, aggregate-volume conservation, and
  replay-identical final state.
- Deterministic 500-operation pure-material melting soak repeatedly transfers one finite copper batch
  from solid inventory through exact sensible-plus-latent heating into molten storage, with periodic
  exhaustive audits, matter and modeled-energy conservation, finite energy depletion, equipment wear,
  and replay-identical final state.
- Deterministic 300-operation pure-material casting soak repeatedly transfers finite molten copper
  into solid ingots while accumulating the exact released latent heat in a bounded thermal sink, with
  periodic exhaustive audits, matter and modeled-energy conservation, and replay-identical final
  state.
- Current debug validation suite: 258 passing tests with `cargo check` silent and
  Clippy warnings denied.
- Release profile keeps integer overflow checks enabled.

## Deliberately Deferred

- Renderer, input, audio, UI, engine/ECS selection, physics implementation, networking, and general
  threading architecture.
- Concrete voxel/chunk storage, world generation, spatial indexes, chunk dimensions, and streaming.
- Regional geological generation algorithms and host-rock relationships, voxel-level terrain matter
  ownership, and physical prospecting resolvers for surface evidence, panning, sampling, drilling,
  assays, and geophysical instruments. The knowledge owner records resolved uncertainty but does not
  infer hidden deposits or manufacture survey accuracy.
- Physical mining authorization including tools, labor, access geometry, extraction rate, recovery,
  waste rock, tailings, drainage, and risk. The current finite-deposit owner and transfer transaction
  are conservation foundations, not a mining gameplay shortcut.
- Thermal fields, environmental heat transport/losses, vaporization/boiling, mixed-material and
  alloy/solution phase diagrams, combustion, fuel networks, and emissions. Pure-material solid/liquid
  fusion and finite explicit thermal sinks are modeled; an implicit environment is deliberately not
  used as an infinite heat source or sink.
- Concrete equipment/tool/worker content, richer voxel/container equipment placement beyond a
  structural support owner, repair material consumption, discrete capability-disable policies, and
  authored gameplay-specific degradation curves.
- Richer physical construction and demolition resolution: member orientation/end geometry,
  joints/connections, cutting and placement waste, tools, labor, duration, salvage fractions, debris
  transformation, and non-identity-preserving demolition outputs. Current prismatic geometry resolves
  solid volume and density-based material quantity, but that quantity foundation deliberately does
  not pretend unresolved joinery, process, labor, or tooling requirements authorize construction.
- Structural bending, shear, torsion, buckling, connection/joint capacity, terrain-support inference,
  and automatic voxel-geometry load paths. Current structural profiles model explicit axial load
  paths rather than pretending those unsolved mechanics are already represented.
- Automatic bindings from inventory mass, fluid contents, snow/weather, wind, and terrain pressure
  into their source-separated structural load contributions. Structural self-weight and mounted
  equipment weight now write their own aggregate contributions canonically; the other owners remain
  deferred.
- Additional production resolvers beyond sensible heating and pure-material melt/cast, including ore
  preparation and chemical smelting/reduction, alloying, forging/working, machining/tool wear,
  labor/skill, chemistry, and environmental constraints. Gameplay processes remain unregistered until
  their corresponding physical gates exist.
- Persistent mechanical-power networks and shaft/belt layout, rotational inertia/flywheels, slip and
  clutch state, steam/boilers, electrical networks, transformers, protection, and distribution
  topology, plus conserved primary energy-generation paths for finite stores. Directional finite
  input/output transfer and process-released-heat sinks now exist, but no free charging/generation API
  is exposed.
- Pressure/gravity-resolved hydrology topology, terrain/surface/groundwater ownership, precipitation
  and runoff, pumps, irrigation, sanitation, wastewater, contamination/water-quality mixtures, and a
  phase-aware bridge between conserved material mass and hydraulic fluid volume. The current finite
  fluid owner is deliberately a storage/conservation boundary, not a pathless movement or water-spawn
  shortcut.
- Agriculture, soil processes, ecology, genetics, creatures, workers, settlements, logistics, trade,
  economy, migration, and other gameplay systems.
- Save-file encoding/storage, compression, atomic filesystem writes, cloud storage, and released-save
  migration implementations.
- Spatial/world performance benchmarks required before final chunk and streaming architecture.

## Foundation Direction

New gameplay work should add one owning subsystem at a time with immutable registry definitions,
typed persistent IDs and quantities, dedicated errors, canonical mutations, explicit invariants,
persistence semantics, and behavioral/soak coverage. Cross-system integration belongs in the visible
simulation pipeline or an explicitly named integration module. Do not add simplified production
recipes or generic technology tiers before the physical systems that authorize them exist.

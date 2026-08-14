# Status

## Current Foundation

- Headless deterministic Rust simulation core with no renderer or engine dependency.
- Immutable registry aggregate with separate authored-ID compatibility version.
- Typed absolute `SimulationTick`, relative `TickSpan`, and deterministic periodic phase scheduling.
- Persisted independent RNG streams derived from the world seed, normalized integer probability,
  and unbiased bounded random selection.
- Explicit authoritative integer quantities for mass, aggregate mass, temperature, energy,
  pressure, area, acceleration, force, power, torque, angular speed, voltage, current, resistance,
  volume, and volumetric flow.
- Typed material/form definitions with density, thermal, mechanical, and electrical properties.
- Canonical normalized mass-fraction composition for ores, alloys, and mixed material lots, including
  validated deserialization and composition-aware material inputs.
- Density-based conservative material-volume calculation and composition-weighted sensible-heat
  calculation that refuses to cross phase boundaries implicitly.
- Persistent material lots with mass, temperature, composition, ownership, and provenance ranges.
- Capacity-aware stockpiles with derived commodity totals, cached mass, inbound reservations,
  revision-bound atomic transfers, deterministic splitting, and compatible-fragment coalescing.
- Persistent finite geological deposits with chunk-independent bounds, exact initial/remaining mass,
  material form, normalized composition, temperature, generation provenance, depletion lifecycle,
  generated IDs, owner revision, and exhaustive registry/state validation. Authoritative deposit
  enumeration remains crate-private. World-generation admission accepts an opaque generated-deposit
  plan with no production/public constructor, so player-facing adapters cannot bypass prospecting or
  use the source boundary as a matter-spawn API.
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
  production job owns the equipment instance. Continuous condition curves reject presence-only
  capabilities; discrete capability loss remains an explicit future policy rather than fake numeric
  interpolation.
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
  capacity and discharge-power envelopes, monotonic runtime IDs/revisions, exact consumed-energy
  provenance, and registry-aware persistence validation. Public runtime allocation creates empty
  stores only; arbitrary energy seeding remains test/bootstrap-only until a conserved generation or
  charging owner exists. Active jobs reserve a source store's discharge capability exclusively.
- Persistent structural members with typed material/profile references, voxel bounds, cross-section,
  planned/active/failed lifecycle, exact embodied material traces and mass, persistent cracking, and
  synchronized forward/reverse support indexes with cycle rejection. Active or failed members cannot
  exist without embodied construction matter.
- Revision-bound construction transfers an exact pre-resolved lot selection from inventory into one
  planned structural member, preserving composition, temperature, and provenance. The current
  single-material strength model accepts only pure matter matching the member's authored material,
  and derives a structure-owned `SelfWeight` load from actual committed mass and registry gravity.
  `SelfWeight` cannot be written through the generic load API.
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
- Exact scalar rotational mechanics with micronewton-meter torque and microradian/second angular
  speed, typed torque/speed capabilities, independent torque/speed/power operating limits,
  normalized mechanical efficiency with explicit loss, and canonical rational transmission ratios.
  Ratio transforms conservatively round output torque/speed down and account any sub-unit remainder
  as loss instead of creating power. Shaft/belt network topology remains deliberately unchosen.
- Exact inverse power-duration calculation returns the minimum whole tick span that can supply an
  energy requirement, including authoritative-range overflow handling without floating point.
- First real physical production resolver: selected-batch sensible heating derives required energy
  from each selected lot's actual mass, composition, and temperature; validates equipment heating
  power, maximum temperature, and maximum batch mass; validates finite energy carrier and discharge
  power; derives duration exactly; and preserves matter/composition in heated outputs. Sensible
  heating refuses to cross unresolved material phase boundaries instead of inventing latent heat.
- Read-only global matter accounting across geological deposits, embodied structural matter,
  inventory, and in-process matter ownership.
- Read-only explicit modeled-energy accounting across finite stores, geological, structural, and
  inventory sensible heat, in-process sensible heat, and energy supplied to active jobs. Geological
  extraction, structural construction/deconstruction, and sensible heating preserve the modeled total
  across ownership changes.
- Canonical top-level tick pipeline with cheap per-tick invariants and exhaustive save/load audits.
- Persistence semantic schema 17 and authored registry compatibility schema 5 with metadata
  preflight, registry-aware state validation, structural topology/damage audits, energy/equipment
  ownership validation, embodied structural matter/self-weight audits, equipment-support/load
  agreement audits, exclusive-resource double-book detection, operation-specific thermal job
  recomputation, stable in-flight conservation snapshots, and deterministic continuation tests.
- Chunk-independent 64-bit voxel coordinates and validated spatial bounds without choosing chunk
  dimensions or streaming policy.
- Deterministic 10,000-tick mixed-system soak with repeated production/transfers, varying structural
  snow load on a persistently cracked supported deck, full-state replay equality, periodic exhaustive
  audits, matter-conservation checks, and lot-fragmentation ceiling.
- Deterministic 5,000-tick real sensible-heating soak with repeated exact lot resolution, finite
  energy depletion, equipment/energy reservations, periodic exhaustive audits, matter conservation,
  modeled-energy conservation, and replay-identical final state.
- Deterministic 2,000-step geological extraction soak with exact finite depletion, compatible-lot
  coalescing, periodic exhaustive audits, matter and modeled sensible-energy conservation, and
  replay-identical final state.
- Deterministic 2,000-observation prospecting soak with synchronized material indexes, periodic
  exhaustive audits, stable persistence continuation, and replay-identical final state.
- Deterministic 1,000-cycle construction/deconstruction soak repeatedly moves one finite material
  batch between inventory and active structures, with periodic exhaustive audits, matter and modeled
  sensible-energy conservation, and replay-identical final state.
- Current debug validation suite: 209 passing tests with `cargo check` silent and
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
- Thermal fields, environmental heat transport/losses, latent heat/phase transitions, combustion,
  fuel networks, and emissions. Current sensible heating is intentionally ideal transfer into
  material sensible heat because no thermal-environment owner exists yet.
- Concrete equipment/tool/worker content, richer voxel/container equipment placement beyond a
  structural support owner, repair material consumption, discrete capability-disable policies, and
  authored gameplay-specific degradation curves.
- Physical construction and demolition resolution: member-axis/solid geometry, derived material
  volume and quantity, joints/connections, cutting and placement waste, tools, labor, duration,
  salvage fractions, debris transformation, and non-identity-preserving demolition outputs. Current
  construction/deconstruction transactions conserve already-resolved matter but deliberately do not
  invent those unresolved physical requirements.
- Structural bending, shear, torsion, buckling, connection/joint capacity, terrain-support inference,
  and automatic voxel-geometry load paths. Current structural profiles model explicit axial load
  paths rather than pretending those unsolved mechanics are already represented.
- Automatic bindings from inventory mass, fluid contents, snow/weather, wind, and terrain pressure
  into their source-separated structural load contributions. Structural self-weight and mounted
  equipment weight now write their own aggregate contributions canonically; the other owners remain
  deferred.
- Real production resolvers beyond sensible heating, including metallurgy, tooling, labor/skill,
  chemistry, and environmental constraints. Gameplay processes remain unregistered until their
  corresponding physical gates exist.
- Persistent mechanical-power networks and shaft/belt layout, rotational inertia/flywheels, slip and
  clutch state, steam/boilers, electrical networks, transformers, protection, and distribution
  topology, plus conserved energy generation/charging paths for finite stores.
- Hydrology/fluid networks, pumps, irrigation, sanitation, wastewater, and water-quality ownership.
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

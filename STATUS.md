# Status

## Current Foundation

- Headless deterministic Rust simulation core with no renderer or engine dependency.
- Immutable registry aggregate with separate authored-ID compatibility version.
- Typed absolute `SimulationTick`, relative `TickSpan`, and deterministic periodic phase scheduling.
- Persisted independent RNG streams derived from the world seed.
- Consequential production, inventory, structural, equipment, energy, fluid, geology, and geological
  knowledge backing collections are private to their state owners. Synchronized indexes change only
  through owner methods that update each related collection in one mutation boundary.
- Wide runtime and coordination records are grouped by ownership concern rather than accumulating
  flat field lists: root runtime systems, production-job identity/schedule/resources/equipment,
  registry presentation domains, screening resolution constraints, completion revision contracts,
  and gameplay-harness inputs/reports each have explicit nested profiles. The resulting persistent
  layout is save schema 32 and remains current-schema-only; no historical layout shim or migration is
  retained.
- Explicit authoritative integer quantities for mass, aggregate mass, temperature, energy,
  pressure, area, length, acceleration, force, power, torque, angular speed, voltage, current,
  resistance, volume, aggregate volume, and volumetric flow.
- Typed material/form definitions with density, thermal, mechanical, and electrical properties,
  explicit solid/liquid form phase, explicit particulate-state policy, and optional authored fusion
  temperature/latent heat.
- Canonical normalized mass-fraction composition for ores, alloys, and mixed material lots, including
  validated deserialization and composition-aware material inputs.
- Density-based conservative material-volume calculation, composition-weighted sensible heat, exact
  pure-material fusion latent heat, and phase-consistent thermal-state validation. Solid matter may
  reach but not exceed its authored fusion boundary; liquid matter must remain at or above it. Mixed
  liquid compositions are refused until real alloy/solution phase diagrams exist rather than being
  assigned an invented weighted melting point.
- Persistent material lots with mass, temperature, composition, optional validated weighted
  particle-size distributions, ownership, and provenance ranges. Particle-size classes have
  canonical non-overlapping diameter bounds and relative mass weights; a single class preserves the
  conservative meaning of an unresolved size envelope without inventing an internal yield curve.
  Particle-size state is part of lot fungibility rather than a detached ore-processing annotation.
- Capacity-aware stockpiles with derived commodity totals, cached mass, inbound reservations,
  revision-bound atomic transfers between distinct stockpiles, deterministic splitting,
  compatible-fragment coalescing, and a persisted material-containment envelope for accepted
  solid/liquid phases and maximum temperature.
  Stockpile allocation requires that containment envelope explicitly; there is no compatibility
  allocator that silently chooses one. Every deposit, ingress, transfer, future production output,
  and exhaustive save audit rechecks phase and temperature compatibility.
- Persistent stockpile-to-structure support assignment with a synchronized support-to-stockpile
  reverse index. Inventory exclusively owns `StoredMatter` structural load: all supported stockpile
  masses are aggregated per member before gravity conversion, generic callers cannot write the load
  channel, stored matter can crack/collapse supports through normal analysis, and support removal is
  blocked until stockpiles are unmounted. Failed debris can be unloaded without repairing it.
  Newly initiated inbound matter requires an active support, while output already durably reserved by
  an in-flight production job may still complete after a later support collapse. Production occupancy
  prevents moving source/destination stockpiles while a job is active.
- Every canonical stockpile-mass mutation keeps stored-matter load synchronized in the same validated
  transaction: manual transfer, production start/completion, geological extraction, structural
  construction/deconstruction, and test/bootstrap ingress. Multi-stockpile transfers and simultaneous
  production completions use one deterministic batch structural plan, so results do not depend on an
  intermediate mutation order. Supported operations bind the structural revision even when aggregate
  force rounds to the same value. Reserved inbound capacity remains space and does not create weight.
  Production completion hands already-reserved output streams to an inventory-owned batch plan that
  allocates material-lot IDs, releases reservations, inserts/coalesces lots, and advances the inventory
  cursor/revision together; production no longer reads or writes inventory ID bookkeeping directly.
  Test/bootstrap lot seeding also delegates to the same validated material-ingress path as source-owned
  production transactions; raw lot insertion remains private to inventory's lot-mutation owner.
- Persistent finite geological deposits with chunk-independent bounds, exact initial/remaining mass,
  material form, normalized composition, temperature, generation provenance, depletion lifecycle,
  generated IDs, owner revision, and exhaustive registry/state validation. Authoritative deposit
  enumeration remains crate-private. World-generation admission accepts an opaque generated-deposit
  plan with no production/public constructor, so player-facing adapters cannot bypass prospecting or
  use the source boundary as a matter-spawn API. Geological ownership is explicitly solid-only,
  excludes processed particulate forms whose size state belongs to material-processing owners, and
  is independently revalidated on load.
- Revision-bound geological extraction transfers exact conserved matter into inventory through a
  crate-private validated ingress primitive. Extraction binds both geology and inventory revisions,
  rejects stale or over-capacity commits without partial mutation, preserves physical material
  profiles, updates a supported destination's derived stored-matter load atomically, and exposes no
  public constructor for mining authorization before tool/labor/geometry physics exist.
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
  snapshots, revision-bound start tokens, due-tick indexing, exclusive equipment occupancy indexing,
  and deterministic stockpile-to-job occupancy indexing. Repeated equipment and stockpile busy checks
  use keyed lookups rather than scanning every active job. Physical resolvers consume the same exact
  lot selection they inspected rather than reselecting equivalent-looking matter at commit.
- Typed authored capability requirements with physical value kinds and registry-reference validation.
  Canonical crusher and foundry content uses the same capability and production registries as runtime
  resolution; additional process content remains gated on corresponding physical providers and
  resolvers.
- Continuous equipment `Condition`, authored maintenance warning/critical bands, and pure wear plans
  without disposable durability semantics.
- Persistent maintainable equipment records with immutable physical mass and capability-provider
  definitions, revision-checked wear application, provider resolution, registry-reference validation,
  save/load ownership, in-flight provider provenance, and exclusive operation occupancy.
  Definitions may author deterministic piecewise-linear condition curves per typed capability;
  effective values are resolved on demand without allocating temporary profiles, and pristine values
  remain the single nominal source of truth. Maintenance mutation is rejected while an active
  production job owns the equipment instance. Operation-specific production resolvers can persist an
  exact post-operation condition and completion applies wear atomically under the equipment owner's
  revision, with simultaneous due outcomes sharing one revision advance. Continuous condition curves
  reject presence-only capabilities; discrete capability loss remains an explicit future policy
  rather than fake numeric interpolation.
- Resource-backed equipment maintenance replaces the former arbitrary condition-increase path.
  Equipment definitions can author one replacement commodity/mass and a service target in the normal
  condition band. Runtime maintenance resolution selects that exact conserved stock from an explicit
  source and binds it into the opaque repair resolution; registry construction validates the authored
  material/form references. Canonical validation relocates those exact traces into an explicit spent
  destination without changing composition, temperature, particle-size state, or provenance; source
  and destination `StoredMatter` structural loads are planned together, and both equipment and
  inventory revisions are rechecked before commit. Repair also rechecks derived production occupancy
  immediately before material moves because job start does not advance the equipment owner revision.
  The canonical jaw crusher currently consumes 50,000 mg of copper-ingot replacement stock and
  restores to 900,000 ppm condition. Tools, workers, service duration, access, and replacement/waste
  chemistry remain deliberately unresolved rather than being faked inside the transaction.
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
  Active jobs reserve every participating source or sink exclusively through a synchronized
  `EnergyStoreId`-to-job occupancy index, replacing repeated active-job scans with deterministic keyed
  lookup while exhaustive load validation reconstructs and checks the index. Released process heat
  remains owned by the in-flight job and enters its finite sink only when completion becomes
  authoritative; stale sink revisions reject completion atomically before material output, wear, job
  removal, or energy mutation.
- Atomic same-carrier finite-energy relocation now has an opaque physical-resolution boundary. The
  energy owner validates distinct endpoints, directional input/output capability, carrier equality,
  production occupancy, exact source quantity, destination capacity, and revision availability, then
  commits equal subtraction/addition under one energy revision after rechecking both energy and
  production owner revisions plus endpoint snapshots. No public resolution constructor exists, so
  storage cannot authorize pathless transfer, implicit carrier conversion, losses, or generation;
  future network owners must resolve those physical questions first.
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
  over-materialization, accepts only pure consolidated solid matter matching the member's authored
  material, rejects particulate forms until a real compaction/binder/sintering/casting path produces
  a load-bearing bulk form, and derives a structure-owned `SelfWeight` load from the committed mass
  and registry gravity.
  A supported source stockpile is unloaded in the same cross-owner transaction. Persisted structural
  embodiment independently rechecks its solid phase. `SelfWeight` cannot be written through the
  generic load API.
- Materialized structural members cannot be deleted through generic removal. Revision-bound
  deconstruction validates destination capacity and both owners, removes the member only as part of a
  conserved recovery transaction, and returns every embodied trace to inventory without losing its
  physical history. If recovery targets a supported stockpile, removal and final stored-matter load
  are analyzed together under one structural revision. Failed debris uses the same recovery boundary.
- Deterministic axial structural analysis using authored material compressive/tensile strength and
  exact strength-times-area force capacity, stable equal-load sharing, readable stable/strained/
  cracking/failed stages, cracked-capacity degradation, and synchronous overload/support-loss
  cascades.
- Source-separated structural load contributions for self-weight, permanent load, stored matter,
  equipment, fluid, snow, wind, and occupancy so independent owning systems cannot overwrite each
  other's causes. Self-weight, stored-matter, and equipment load channels are exclusively owned by
  their source integrations; direct generic writes are rejected. Zero writable contributions are
  removed canonically.
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
  authored fluid identity, volume, temperature, optional structural support assignment, and a
  synchronized support-to-store reverse index. Fluid definitions are registry-owned, cross-reference
  underlying material identity, and author constant bulk density for exact weight projection; the
  built-in fluid registry remains intentionally empty until phase-aware world fluid content exists.
  Public runtime allocation creates empty capacity only, so the storage owner cannot manufacture
  water or other fluid. Revision-bound transfer commits conserve exact aggregate volume and clear
  zero-volume identities canonically. Supported transfers resolve both stores' final contents and all
  affected `Fluid` structural loads as one cross-owner plan. Volume-times-density is aggregated across
  every store on a support before one conservative milligram rounding boundary and gravity conversion,
  preventing per-container rounding from fabricating weight. Increasing aggregate fluid weight
  requires an active support and can crack or collapse it through normal structural analysis; draining,
  same-support redistribution without added weight, and unmounting from failed debris remain possible.
  The `Fluid` load channel is exclusively fluid-owned, and structural removal is blocked until stores
  are unmounted. Gameplay still cannot construct a transfer resolution directly: gravity, pressure,
  channel, or pump systems must eventually authorize movement. The current conservative transfer path
  refuses unlike fluid identities or temperatures rather than silently inventing mixture chemistry or
  thermal equilibration.
- Read-only world-scale fluid-volume accounting aggregates beyond one store's `u64` range without
  trusting cached totals.
- Exact scalar rotational mechanics with micronewton-meter torque and microradian/second angular
  speed, typed torque/speed capabilities, independent torque/speed/power operating limits,
  normalized mechanical efficiency with explicit loss, and canonical rational transmission ratios.
  Ratio transforms conservatively round output torque/speed down and account any sub-unit remainder
  as loss instead of creating power. Shaft/belt network topology remains deliberately unchosen.
- Exact inverse power-duration calculation returns the minimum whole tick span that can supply an
  energy requirement, including authoritative-range overflow handling without floating point.
- Typed material mass throughput in milligrams per second plus exact whole-tick duration resolution
  provides a shared rate foundation for crushers, grinders, conveyors, and later continuous material
  equipment without abusing batch mass or floating-point rates.
- Selected-batch comminution is the first ore-processing resolver. It accepts exact solid lot slices,
  requires authored equipment throughput, maximum batch mass, energy carrier, and exact
  mass-specific work, then reserves that work from a finite energy source. Each comminution
  definition authors a validated weighted particle-size distribution. Coarse untracked feed may
  acquire its first explicit size state, while already-particulate feed must strictly reduce the
  distribution envelope without coarsening represented fines. Input and output forms may therefore
  be identical for real grinding distinctions. Mass, normalized composition, and temperature remain
  exact. Runtime equipment condition can derate throughput, while finite source output power can
  independently bottleneck the operation; authoritative duration uses the slower limit and therefore
  drives active-tick wear. Resolved comminution exposes independent throughput- and energy-limited
  durations, a typed current bottleneck, and exact condition-before/condition-after projections.
  Persisted jobs recompute the exact authored particle distribution, work energy, carrier,
  power-limited duration, and wear from their committed traces. The canonical jaw crusher remains a
  single unresolved 500-10000 um class rather than fabricating a within-envelope mass curve without
  authored data. Canonical content now also includes a separate grinding mill and same-form grinding
  process with its own typed throughput/batch capabilities. Grinding reduces that crusher envelope to
  two explicit equal-weight classes, 500-2000 um and 2001-4000 um, while preserving mass,
  composition, temperature, and form. The grinder therefore adds physically useful particle-size
  information without pretending to concentrate ore or relabel the material. Comminution definitions
  may also author an admissible particulate feed envelope. Runtime resolution and persistence replay
  both reject selected feed outside that operating range, allowing physically distinct mill passes to
  be represented without hard-coding equipment IDs or introducing arbitrary process unlocks.
- Selected-batch dry screening is a reusable ore-processing resolver with an exact authored aperture,
  typed undersize/oversize output streams, runtime equipment throughput and maximum-batch limits,
  finite work energy, power-limited duration, and active-tick wear. Screening aggregates identical
  physical input profiles before converting class weights to whole-milligram stream masses so lot
  fragmentation cannot change yield. A class wholly at or below the aperture is undersize and a
  class wholly above it is oversize; an aperture intersecting an unresolved class is rejected rather
  than assigned an invented split. A weighted class partition that would require fractional
  milligrams is also rejected at the current mass resolution rather than silently reclassifying the
  remainder into the wrong stream. Persisted screening jobs recompute stream identities, exact
  outputs, energy, duration, and equipment condition. Canonical content now registers a workshop dry
  screen and a 2 mm dry-screening process with separate throughput and batch capabilities, finite
  mechanical work, condition-sensitive throughput, and active-tick wear. Direct crusher-to-screen
  processing still fails because the jaw crusher emits one unresolved 0.5-10 mm class. The canonical
  grinding mill is now the physical bridge: its 0.5-2 mm and 2.001-4 mm classes lie wholly on opposite
  sides of the screen aperture, allowing exact routed undersize/oversize ownership. A second
  fine-grinding operation accepts only the 2.001-4 mm screen oversize and reduces it to the same
  0.5-2 mm profile as the undersize stream. This creates a selective closed-loop preparation circuit:
  already-fine material avoids the extra grinding work and wear while oversize can be recycled.
- Production output ownership is stream-based rather than destination-global. Resolvers assign typed
  operation-local stream IDs, resolution canonicalizes stream order by ID, and durable jobs preserve
  each physically inseparable stream's identity, exact lot specifications, and routed destination.
  Start validation binds routes by stream ID, validates every destination, aggregates inbound capacity
  reservations per stockpile under one inventory revision, and completion deposits every stream from
  one deterministic plan. Sensible-heating, melt/cast, and comminution resolvers remain explicitly
  single-stream; screening owns stable undersize and oversize streams.
- Ore-processing and thermal resolver registries have exclusive process ownership, preventing one
  process ID from silently acquiring two incompatible physical interpretations.
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
- Support-dependent production can suspend rather than complete magically when its equipment loses an
  active structural support. A suspended job keeps consumed matter and energy as authoritative
  work-in-process, grants no output or completion wear, leaves simulation time free to advance, and
  retains the exact remaining active process time. Structural equipment movement is the narrow
  recovery exception to normal job occupancy, allowing the machine to be relocated while the job
  still owns it; once active support is restored, the canonical tick pipeline reschedules completion
  from the exact remaining active time. The immutable active process duration remains the physics and
  wear-audit contract even when wall-clock completion moves because of downtime. Other occupied
  resources remain exclusive and report an explicit `AwaitingResume` release horizon rather than a
  stale pre-failure due tick.
- Canonical top-level tick pipeline with cheap per-tick invariants and exhaustive save/load audits.
- Current-schema-only persistence and authored registry compatibility, with accepted version values
  owned by `CURRENT_SAVE_SCHEMA_VERSION` and the built-in content registry rather than duplicated in
  status documentation. Loading performs registry-aware state validation, structural topology/damage
  audits, energy/equipment ownership validation, directional energy-source/sink reservation, production
  energy/equipment/stockpile occupancy and capacity audits, embodied structural
  matter/self-weight/phase audits, geometry/density-to-mass recomputation,
  equipment-support/load agreement audits, stockpile-support/index/stored-matter-load agreement
  audits, fluid-support/index/density-derived-load agreement audits, exclusive-resource double-book
  detection,
  particle-size distribution policy/state audits, typed production-stream identity/routing and
  per-destination reservation audits, operation-specific sensible-heating/melting/casting,
  comminution, and screening recomputation including admissible comminution feed envelopes, exact
  output particle classes and stream partitioning, post-operation condition outcomes and released
  heat, stable in-flight conservation
  snapshots, production active-duration/suspension scheduling, due-index exclusion while suspended,
  suspension-provider identity, and deterministic continuation tests. Suspended-job round trips
  preserve work-in-process exactly; adversarial saves that reinsert a suspended job into the due
  index, forge its paused due tick, or claim more remaining work than the operation's active duration
  are rejected. Suspension timestamps later than the authoritative clock and empty production
  due-index buckets are also rejected as noncanonical state. The cheap per-tick invariant suite
  verifies running/suspended job scheduling and suspension time against the authoritative clock and
  due index.
- Chunk-independent 64-bit voxel coordinates and validated spatial bounds without choosing chunk
  dimensions or streaming policy.
- Renderer-neutral immutable texture registry with hue-shift-capable 16-shade palette ramps,
  one-byte 32x32 indexed texels, strict opaque/cutout/blend validation, explicit six-face block
  appearances, ordered object material-slot appearances, and validated material-form/equipment
  bindings. The deterministic startup baker produces stable dense texture descriptors, independently
  deduplicates indexed patterns and palette rows for cheap recolors, and generates discrete
  32/16/8/4/2/1 mip chains without averaging palette indices. Block faces and object material slots are
  prebaked to draw descriptors so hot meshing does not revisit authored maps. Built-in visual content
  uses multi-scale structure rather than uniform speckle: timber grain, knots and growth-ring cracks;
  charcoal fractures; copper veins; beveled panels and rivets; slag pores; molten flow and crust;
  aggregate clasts; worn-metal scratches and rust; sooted brick inclusions; and beveled cutout mesh.
  The complete built-in indexed upload, including all six mip levels and palette lookup tables, stays
  within 16 KiB and below half the bytes of its equivalent deduplicated RGBA texel mip chain.
- Renderer-neutral immutable WGSL registry with typed IDs, validated acyclic shared-library graphs,
  deterministic dependency assembly, dense startup program lookup, explicit render/compute entry
  points, fixed-function blend/depth/color-target requirements, portable workgroup limits, and
  audited per-invocation work budgets. Nine built-in programs cover indexed HDR surfaces, stable
  16x16 tiled point-light culling, separate zero-sample opaque and alpha-aware cutout directional
  shadows, three-wave depth-aware water, three-layer procedural soft-particle smoke, a procedural
  cloud/star sky, four-read half-resolution bloom, and ACES-fit post processing with grading,
  vignette, and dither. The cutout shadow path shares the surface path's injected texture dimensions,
  discrete mip selection, and mesh UV/key locations; baked alpha mode selects the appropriate shadow
  pipeline without per-fragment branching. Surface lighting combines palette shade selection,
  ambient occlusion, warm block light, up to 32 local lights, four-tap sun shadows, and height fog.
  A logarithmic workgroup prefix scan compacts light candidates; overflowing tiles retain the first
  32 stable-ordered overlaps without atomic allocation flicker. The unique WGSL suite is held below
  48 KiB and every assembled program is parsed and semantically validated in the default-off
  `test-shader-validation` lane, leaving ordinary core test builds and the default shipping crate free
  of the Naga dependency.
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
- Deterministic 2,000-transfer finite-energy soak repeatedly relocates one conserved same-carrier
  energy total between stores, with periodic exhaustive state audits, exact energy conservation, and
  replay-identical final state.
- Deterministic 2,000-transfer fluid-storage soak repeatedly moves one finite homogeneous fluid volume
  through multiple stores, with periodic exhaustive state audits, aggregate-volume conservation, and
  replay-identical final state.
- Deterministic 1,000-transfer supported-fluid soak moves one finite homogeneous fluid volume between
  separately supported stores, recomputing both density-derived structural loads on every transfer,
  with periodic exhaustive audits, aggregate-volume conservation, and replay-identical final state.
- Deterministic 500-cycle maintenance soak repeatedly applies wear, commits an exact resource-backed
  repair, and returns the same finite test material for the next transaction cycle, with periodic
  exhaustive audits, matter and modeled-energy conservation, and replay-identical final state. The
  return transfer is a transaction stress fixture, not a gameplay claim that spent parts self-renew.
- Deterministic 500-operation pure-material melting soak repeatedly transfers one finite copper batch
  from solid inventory through exact sensible-plus-latent heating into molten storage, with periodic
  exhaustive audits, matter and modeled-energy conservation, finite energy depletion, equipment wear,
  and replay-identical final state.
- Deterministic 300-operation pure-material casting soak repeatedly transfers finite molten copper
  into solid ingots while accumulating the exact released latent heat in a bounded thermal sink, with
  periodic exhaustive audits, matter and modeled-energy conservation, and replay-identical final
  state.
- Deterministic 300-operation comminution soak repeatedly crushes one finite mixed-composition ore
  batch through condition-sensitive equipment, with periodic exhaustive persistence audits, exact
  matter conservation, exact finite work-energy depletion, bounded lot coalescing, accumulated
  equipment wear, and replay-identical final state.
- Deterministic 300-operation dry-screening soak repeatedly partitions one finite resolved
  mixed-composition particulate batch into routed undersize/oversize streams, with an in-flight
  save/load continuation, periodic exhaustive audits, exact matter conservation, exact finite work
  depletion, accumulated equipment wear, bounded output-lot coalescing, and replay-identical final
  state.
- Deterministic 1,000-transfer supported-stockpile soak repeatedly moves one finite material lot
  between separately supported stockpiles, updating both derived structural loads on every transfer,
  with periodic exhaustive audits and replay-identical final state.
- The headless copper-workshop gameplay harness consumes `build_registries()` directly rather than
  maintaining shadow equipment/process definitions. Canonical built-in content includes the jaw
  crusher, grinding mill, dry screen, electric furnace, cooled casting mold, two mechanical drive
  envelopes, electrical buffer, thermal sink, ore crushing, same-form grinding, exact dry screening,
  pure-copper melting, and pure-copper casting used by the harness. Normal exercise-mode runs combine
  five deterministic anchor seeds with three organic exploratory seeds generated from a fresh replay
  root. `DEEP_HEARTH_GAMEPLAY_VARIATION_SEED` reproduces any organic set from an exact decimal or hex
  root, and `DEEP_HEARTH_GAMEPLAY_SEEDS` accepts exact decimal or hex seed lists for reproduction or
  wider sweeps. Explicit seed lists fail on malformed entries rather than silently dropping them. The
  anchors guarantee stable comparison and all three operating priorities; balance-dependent outcomes
  are reported rather than frozen into aggregate pass/fail coverage. Every run reports its exact root
  and replay seeds. The exercise source lives under `tests/gameplay_harness/` as a dedicated integration
  target rather than library code, so harness-only edits rebuild the dedicated target against the
  cached core library instead of invalidating the feature-enabled library or compiling the crate
  unit-test harness. Seed/configuration contracts share that one specialized target instead of creating
  another Cargo artifact. Routine harness tests keep success output captured; the report lane emits
  replay inputs, compact outcome/system summaries, and an explicit exercised/bootstrap/deferred scope
  line, while `DEEP_HEARTH_GAMEPLAY_VERBOSE` enables the detailed decision trace.
  The compact report exposes sampled ore/delivery input ranges alongside completed work orders,
  terminal causes, delivery-informed control decisions, structural/WIP recovery, maintenance services,
  system pressure, and bottleneck prevalence
  so each fresh sample is useful as gameplay feedback rather than only a pass/fail result. Starting
  conditions vary ore grade, batch size, crusher condition, one finite crusher-service replacement
  stock, two competing structural bays, real background stored cargo, a scheduled supported-stockpile
  delivery, and finite mechanical work reserves. Batch size and initial condition are derived from
  current authored crusher capabilities and maintenance bands. Support geometry spans a broad ordinary
  utilization range derived from current material strength and crusher weight; background cargo and
  delivery mass scale from current equipment/material quantities, while delivery timing is selected
  within a horizon derived from a real resolved batch duration. Bootstrap-only matter/energy seeding
  and structural materialization are isolated in one setup module and cannot be called by the acting
  policy. After setup the timed structural disruption is an actual `validate_transfer_bulk` transaction
  into a mounted stockpile, so inventory owns the resulting `StoredMatter` load and normal support
  analysis owns any strain, cracking, failure, suspension, or recovery. The harness chooses the
  transfer tick; this does not claim an implemented logistics scheduler. Wider seed sweeps remain
  explicit diagnostic exercises rather than a fixed gate or frozen balance claim. Each seed also
  selects one bounded operating priority: conserve high-power
  reserve, protect projected equipment condition, or minimize batch completion time. These priorities
  choose only among legal resolver outputs and never override critical-condition, maintenance, support,
  energy, or ownership gates. The low-power drive is seeded with exactly enough work for the planned
  order, while the high-power drive remains an optional scarce reserve rather than a hidden completion
  requirement. When projected wear would cross the critical boundary the policy resolves and commits
  real authored maintenance if replacement stock remains, then reevaluates the power choice with
  restored condition. Replacement stock is finite and spent matter remains owned rather than
  disappearing. Lack of usable stored work is reported separately from maintenance supply exhaustion.
  The scheduled delivery can occur during production. If its inventory-owned load merely strains the
  active support, the committed job can finish and the player may then relocate; if the support fails,
  the production job suspends with exact remaining active time and conserved work-in-process. Recovery
  can relocate the occupied machine and resume that work, or leave it visibly stranded when no
  surviving bay can carry the crusher. Failed structural damage remains persistent either way. Output
  exposes delivery mass/target/timing, player priority, remaining work reserve, condition band, support
  state, completed work before delivery, suspended/stranded work-in-process, contained copper floor,
  and crushed ore particle-size
  classes rather than reducing experience coverage to booleans. Ore grade therefore has an honest
  conserved-value effect even though it cannot yet change a downstream processing choice. The harness
  identifies the missing concentration/smelting bridge. A separate ore-preparation capability probe
  derives a legal mixed-ore batch from current authored equipment limits and screen-class
  representability, then runs canonical crushing, grinding, screening, and any nonzero oversize
  regrind. Direct crusher-to-screen and crusher-to-fine-grind availability are reported as current
  observations rather than frozen requirements. Routing follows the resolver's actual nonzero output
  streams, particle checks compare against authored process distributions/apertures, and the probe
  checks stage-by-stage persistence invariants, resolved energy use, equipment wear, composition
  preservation, and whole-chain matter conservation without requiring an arbitrary zero-energy final
  state. Pure-copper melt/cast uses a seed-varied legal batch derived from the authored furnace/mold
  limits and is exercised once as a
  separately labeled downstream
  capability probe, not repeated per scenario or presented as a continuous ore-to-metal loop. Matter,
  equipment, initial energy, and structural bays remain explicit setup fixtures until their physical
  acquisition/construction authorizers exist; experienced post-setup mutations use canonical runtime
  transactions. Gameplay-harness support is split by responsibility across bootstrap, configuration,
  execution contracts, probe setup, reporting, and deterministic seed-mixing modules instead of
  accumulating all support in the main scenario controller. Seed/configuration behavior is covered by
  focused named tests rather than one aggregated boolean contract test, and custom replay lists are
  reported distinctly from generated organic scenarios. Isolated unit-test registry builders share one
  test-only domain assembler and no longer inherit unrelated canonical gameplay content as that content
  expands.
- Runtime state owners keep records, synchronized indexes, and owner mutation primitives in their
  state modules while descendant validation modules own exhaustive persistence audits without widening
  private mutation access. Production execution is organized behind one canonical facade with separate
  start-admission and in-flight completion modules; thermal process code likewise separates immutable
  resolver registration, sensible-heating resolution, and persistence replay validation. Inventory
  fixture/bootstrap helpers now live in a dedicated conditional support module instead of the
  production transaction module. Public bulk stockpile transfer performs deterministic selection and
  then delegates admission, split-ID planning, structural-load planning, and commit to the same exact
  relocation pipeline used by physical resolvers, removing the former parallel transfer mutation path.
- `TESTING.md` and `.cargo/config.toml` expose maintained fast, soak, gameplay, shader, full, release,
  lint, check, and documentation lanes. Long-horizon soaks are explicit ignored unit tests, so fast and
  soak execution reuse one default-feature unit-test artifact instead of triggering separate feature
  builds. The ordinary Clippy lane checks production library code only; `test-fast` then compiles and
  executes the full default-feature unit-test target, avoiding an all-target Clippy build immediately
  before the same large test target is compiled for execution. The all-target/all-feature Clippy lane
  remains explicit hardening. The large gameplay exercise is integration-test source rather than
  library code, and the Naga parser dependency remains behind its dedicated test feature. GitHub CI runs
  format/lint, combined core/soak tests, gameplay, and shader validation in parallel with a shared
  dependency cache, source-aware per-lane target caches, and superseded-run cancellation instead of one
  serial release-sized gate. Pull requests use a unit-tested fail-safe changed-path classifier to skip
  documentation-only and known-unrelated specialized builds before installing Rust or restoring build
  caches; pushes to `main` still run every lane. Test binaries and one-shot validation binaries omit
  debug symbols to reduce codegen/link time without changing ordinary dev-profile debugging behavior.
- Current default validation keeps `cargo check` silent and Clippy warnings denied.
- Project lint policy denies wildcard enum match arms, keeping project-owned enum handling exhaustive
  as variants evolve instead of relying on review to catch silent fallback behavior.
- Boolean fields, parameters, and predicate APIs follow the project `is_`/`has_`/`can_` vocabulary;
  the current save schema uses those same names directly and retains no historical Serde rename shim.
- Release profile keeps integer overflow checks enabled.

## Deliberately Deferred

- Renderer backend, window/input, audio, UI, engine/ECS selection, physics implementation,
  networking, and general threading architecture. Compact texture upload and bounded WGSL lighting
  contracts are implemented, but no graphics API backend, mesh/chunk format, GPU resource-lifetime
  policy, pipeline-cache implementation, or device-specific quality tier is selected.
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
- Broader equipment/tool/worker content beyond the canonical crusher, grinding mill, dry screen,
  furnace, and casting mold; richer voxel/container equipment placement beyond a structural support
  owner; repair tools/labor/duration/access, richer spare-part suitability, replacement and waste
  transformations, discrete capability-disable policies, and broader authored maintenance/degradation
  profiles. The jaw crusher now has a real replacement-stock maintenance resolver, but that narrow
  service does not pretend unresolved tooling, labor, time, or chemistry already exist.
- Richer physical construction and demolition resolution: member orientation/end geometry,
  joints/connections, cutting and placement waste, tools, labor, duration, salvage fractions, debris
  transformation, and non-identity-preserving demolition outputs. Current prismatic geometry resolves
  solid volume and density-based material quantity, but that quantity foundation deliberately does
  not pretend unresolved joinery, process, labor, or tooling requirements authorize construction.
- Structural bending, shear, torsion, buckling, connection/joint capacity, terrain-support inference,
  and automatic voxel-geometry load paths. Current structural profiles model explicit axial load
  paths rather than pretending those unsolved mechanics are already represented.
- Automatic bindings from snow/weather, wind, and terrain pressure into their source-separated
  structural load contributions. Structural self-weight, supported inventory matter, mounted
  equipment weight, and supported fluid weight now write their own aggregate contributions
  canonically; the remaining owners remain deferred.
- Additional production resolvers and gameplay content beyond sensible heating, pure-material
  melt/cast, conservative crushing/grinding, and canonical dry screening, including non-ideal
  screening efficiency/blinding/wet-feed effects, richer mill media/loading physics, washing,
  gravity/flotation separation, explicit
  recovery/tailings physics, chemical smelting/reduction, alloying, forging/working, machining/tool
  wear, labor/skill, chemistry, and environmental constraints. Weighted particle-size classes and
  typed multi-stream ownership now support exact conservative classification where the authored
  aperture lies between resolved classes, but the simulation still refuses to invent a split through
  an unresolved class. The canonical crush/grind/screen/pure-melt/pure-cast processes are registered;
  additional gameplay processes remain unregistered until their corresponding physical gates exist.
- Persistent mechanical-power networks and shaft/belt layout, rotational inertia/flywheels, slip and
  clutch state, steam/boilers, electrical networks, transformers, protection, and distribution
  topology, plus conserved primary energy-generation paths for finite stores. Directional finite
  input/output envelopes, process-released-heat sinks, and an opaque conserved same-carrier storage
  relocation boundary now exist, but no topology resolver can construct that transfer and no free
  charging/generation API is exposed.
- Pressure/gravity-resolved hydrology topology, terrain/surface/groundwater ownership, precipitation
  and runoff, pumps, irrigation, sanitation, wastewater, contamination/water-quality mixtures,
  temperature/pressure-dependent fluid properties, and a phase-aware bridge between conserved
  material mass and hydraulic fluid volume. The current finite fluid owner now contributes real
  support weight but remains a storage/conservation boundary, not a pathless movement or water-spawn
  shortcut.
- Agriculture, soil processes, ecology, genetics, creatures, workers, settlements, logistics, trade,
  economy, migration, and other gameplay systems.
- Save-file encoding/storage, compression, atomic filesystem writes, and cloud storage. Historical
  save-schema migration is intentionally unsupported rather than deferred.
- Spatial/world performance benchmarks required before final chunk and streaming architecture.

## Foundation Direction

New gameplay work should add one owning subsystem at a time with immutable registry definitions,
typed persistent IDs and quantities, dedicated errors, canonical mutations, explicit invariants,
persistence semantics, and behavioral/soak coverage. Cross-system integration belongs in the visible
simulation pipeline or an explicitly named integration module. Do not add simplified production
recipes or generic technology tiers before the physical systems that authorize them exist.

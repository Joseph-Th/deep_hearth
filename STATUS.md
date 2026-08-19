# Status

This document answers one question: **what capability exists in the current runtime?** Use
[`README.md`](README.md) to find the owning subsystem. `ARCHITECTURE.md` and `TECHNICAL_DESIGN.md` own
implementation contracts; `GAME_DESIGN.md` owns intended gameplay. Status does not record implementation
history or future design detail.

## Implemented

### Runtime core

- Headless deterministic Rust simulation with no renderer or engine dependency.
- Immutable validated registries for authored definitions and a separate authored compatibility version.
- `AppState` owns generated runtime state; subsystem owners keep authoritative records and synchronized
  indexes private behind canonical operations.
- Persisted independent RNG streams derive from the world seed. Result-affecting ordering is stable.
- Typed absolute time, relative durations, deterministic periodic schedules, and an authored calendar.
- Checked integer quantities cover the modeled physical units used by matter, energy, structures,
  fluids, electrical calculations, and mechanical calculations.
- Current-schema persistence reconstructs derived indexes and validates subsystem plus cross-owner
  invariants before loaded state is trusted.

### Matter, materials, and inventory

- Materials and forms have typed identities, physical properties, explicit phase, and particle-state
  policy. Pure-material fusion temperature and latent heat are supported where authored.
- Material composition is canonical normalized mass fraction. Mixed lots preserve composition rather
  than collapsing to a synthetic material identity.
- Persistent lots own mass, temperature, composition, particle-size state when required, provenance,
  stockpile ownership, and storage exposure used by perishability.
- Stockpiles enforce capacity, phase and temperature containment, inbound reservations, and optional
  preservation behavior. Food storage changes future spoilage rate without erasing accumulated age.
- Inventory maintains deterministic commodity routing and compatible lot coalescing while preserving
  physical distinctions that affect gameplay.
- Stockpile-to-stockpile movement requires an opaque material-transfer resolution from a physical or
  logistics owner. Inventory validates custody and storage but cannot authorize pathless movement.
- Supported stockpiles contribute inventory-owned `StoredMatter` structural load. Matter-changing
  transactions keep inventory ownership and structural load synchronized atomically.
- World matter accounting covers implemented geological, inventory, structural, equipment, biological,
  and in-process ownership.

### Geology, mining, and prospecting knowledge

- Finite geological deposits persist bounds, material profile, remaining mass, provenance, and
  depletion state. Natural deposits are solid ownership and do not expose player-facing hidden truth.
- Hand mining is the gameplay extraction boundary. It requires a real tool, exclusive player labor,
  authored throughput/batch/hardness capability, destination capacity, and wear. Extracted matter is
  conserved through mining work-in-process and an explicit output claim.
- Primitive progression includes stone tools, mineralized ore, separate native-copper occurrences,
  cold-worked copper reinforcement, and condition-sensitive mining improvements.
- Geological knowledge is a separate persisted owner containing only acquired observations. Records
  store spatial evidence and bounded abundance estimates without referencing exact hidden deposits.
- Read-only assessment combines overlapping evidence deterministically, preserves contradictions, and
  marks spatially disjoint evidence incomparable where a single bound would be misleading.
- Knowledge recording requires an opaque `ProspectingResolution`. Physical survey generation itself is
  not implemented.

### Production, player work, and survival

- Timed production is closed-mass, deterministic, persisted, and revision-bound. Jobs own consumed
  matter and modeled energy while in flight, reserve output capacity at start, and commit resolved
  output streams at completion.
- Manual shaping uses the same conserved timed-production foundation as machine work and supports
  integral repeated batches without discounting matter, time, wear, or exertion.
- `PlayerWorkState` is exclusive across manual crafting, mining, and direct player power. Admission
  requires enough metabolic energy and hydration for the scheduled work plus basal upkeep.
- Direct manual power converts survival-costed player labor through a real equipment capability into a
  finite compatible energy store and applies equipment wear at completion.
- Primitive progression includes hand shaping, composite tool assembly, mining, native-copper tool
  reinforcement, material-backed mechanical work storage, hand-crank charging, and a player-built
  primitive crusher capable of autonomous comminution while player labor is occupied elsewhere.
- Player survival tracks metabolic energy, hydration, vitality, and recent Grain/Fruit/Protein
  nutrition. Basal depletion and active-work exertion run in the canonical tick.
- Authored food has finite freshness; preservation affects future exposure. Meals can combine explicit
  food selections atomically. Eating preserves matter ownership in biological accounting.
- Authored water is finite and drinkable. Drinking preserves fluid-volume ownership in biological
  accounting.

### Capabilities, equipment, and maintenance

- Capabilities are typed physical requirements and values, including throughput, mass, temperature,
  power, torque, speed, electrical quantities, volume/flow, and condition.
- Equipment is persistent, condition-bearing, mass-bearing, and optionally assembled from exact
  material/provenance traces. Equipment can be occupied exclusively by production, mining, or direct
  player-power work as applicable.
- Authored condition curves can derate numeric capabilities. Failed productive equipment reaches zero
  productive rate where that curve is authored.
- Maintenance consumes an exact authored replacement commodity and produces a distinct conserved spent
  material form while restoring an authored condition target. It cannot run through active occupancy.
- Additive equipment upgrades preserve identity, accumulated condition, and existing material traces
  while adding exact authored matter.
- Idle, unmounted, pristine assembled equipment can be disassembled back into its exact embodied
  traces. Worn-equipment salvage and maintenance-scrap recovery are not implemented.
- Mounted equipment contributes an equipment-owned structural load and requires active support for new
  work.

### Energy, structures, fluids, and physical scalars

- Finite energy stores have typed carriers, capacity, directional power envelopes, persistent identity,
  and exclusive production occupancy. Material-backed stores own exact embodied matter.
- Same-carrier energy relocation requires an opaque physical resolution. Energy storage cannot create
  power, convert carriers, or authorize pathless transfer.
- Empty, idle material-backed stores can be disassembled to their exact embodied traces. Any stored
  energy blocks disassembly. Energy-store placement/support is not implemented.
- Structural members persist geometry, lifecycle, damage, support relationships, embodied material,
  and source-separated loads. Deterministic axial analysis produces stable, strained, cracking, and
  failed states with support-loss cascades.
- Structural construction and deconstruction conserve exact material traces. Self-weight is derived
  from embodied matter. Stockpile, equipment, and fluid weight are owned by their respective source
  integrations.
- Finite homogeneous fluid stores track identity, volume, temperature, capacity, and optional support.
  Transfers conserve exact volume and require an opaque physical resolution; unlike fluids or
  temperatures are not mixed implicitly.
- Water exists as authored finite fluid and can be consumed by survival. Hydrology, channels, pumps,
  and world water generation are not implemented.
- Scalar foundations cover exact power/energy integration, flow/volume integration, electrical power
  and resistance, rotational torque/speed/power, mechanical efficiency, rational transmission ratios,
  and mass throughput/duration.

### Ore processing and thermal production

- Selected-batch crushing and grinding preserve mass, composition, temperature, and exact lot
  selection while applying authored particle-size results, equipment throughput/batch limits, finite
  work energy, power-limited duration, and condition wear.
- Grinding can refine particle-size state without changing material form. Authored feed envelopes can
  reject physically unsuitable particulate input.
- Dry screening partitions resolved particle classes into typed undersize and oversize streams at an
  authored aperture. Unresolved classes that cross the aperture and nonrepresentable fractional-mass
  partitions are rejected.
- Canonical ore preparation supports crushing, grinding, exact screening, and selective oversize
  regrinding. It does not concentrate ore or change composition.
- Production supports typed multi-stream routing with deterministic destination reservations and
  completion.
- Sensible heating, pure-material melting, and pure-material casting use selected real matter,
  authored thermal properties, finite equipment power, finite energy sources/sinks, exact phase
  boundaries, and explicit latent heat.
- Mixed/alloy melting and chemical smelting are not implemented.
- Production tied to supported equipment can suspend on support loss, retain exact work-in-process and
  remaining active time, and resume after valid structural recovery.

### Persistence, spatial primitives, and renderer-neutral assets

- The canonical tick pipeline keeps its execution order explicit and uses cheap runtime invariants;
  exhaustive persistence validation owns full graph/index/physics audits.
- Persistence supports the current save and registry schemas and validates deterministic continuation,
  ownership, reservations, structural integration, and operation-specific in-flight physics.
- Spatial foundations provide checked chunk-independent voxel coordinates and bounds. Chunk dimensions,
  storage, and streaming are not selected.
- Immutable renderer-neutral texture content supports indexed 32x32 tiles, palette ramps, block-face
  and object-slot appearance bindings, deterministic baking, deduplication, and discrete mip chains.
- Immutable renderer-neutral WGSL content supports deterministic library assembly and bounded shader
  definitions for surfaces, tiled lights, shadows, water, smoke, sky, bloom, and post processing.
- No graphics backend, GPU resource manager, scene system, or platform renderer is implemented.

### Verification coverage

- Deterministic unit, integration, soak, persistence, conservation, and gameplay-harness coverage exists
  for the implemented ownership and production paths.
- Gameplay evaluation covers workshop operation, primitive progression, survival provisioning, ore
  preparation, and pure-copper foundry capability with deterministic pass/fail cases plus a separate
  exploratory report.
- Test selection, replay controls, assertion policy, and local CI are owned by [`TESTING.md`](TESTING.md).


## Not implemented

The following capabilities are outside the current runtime boundary:

- graphics backend, window/input/audio integration, ECS selection, networking, and general engine
  integration;
- voxel/chunk storage, terrain/world generation, streaming, and world-scale spatial indexing;
- regional geological generation, voxel ore topology, and physical prospecting actions such as
  panning, sampling, drilling, assays, and geophysical surveys;
- mechanized excavation, mine access/haulage/drainage/ground control, recovery fractions, waste rock,
  and tailings ownership;
- environmental heat fields and transport, vaporization, combustion, fuels, emissions, mixed/alloy
  phase diagrams, mineral concentration, chemical smelting/reduction, alloying, forging, and machining;
- general worn-equipment salvage, maintenance-scrap recovery, repair labor/tools/time/access, and
  richer maintenance chemistry;
- structural bending, shear, torsion, buckling, joints/connections, terrain support, construction
  labor/tooling/waste, and non-exact demolition/salvage;
- shaft/belt power networks, rotational inertia/slip/clutches, steam systems, electrical topology,
  generation/distribution/protection, and energy-store spatial/support integration;
- hydrology, groundwater/surface water, channels, pumps, irrigation, wastewater, sanitation, fluid
  mixing, and pressure/temperature-dependent fluid properties;
- agriculture, soil simulation, ecology, genetics, creatures, hunting/combat, non-player workers,
  settlements, logistics, trade, economy, and migration;
- save-file encoding/storage, filesystem atomicity, compression, and cloud storage adapters.

Persistence supports only the current save schema. New capabilities must add their own physical owner,
canonical mutation path, persistence semantics, and invariant coverage before `STATUS.md` lists them as
implemented.

# Status

This page is the authority for runtime scope. It answers whether a capability is reachable in ordinary play,
implemented for controlled evaluation, or absent. Product direction does not establish implementation; use
[`README.md`](README.md) for project routing and [`GAME_DESIGN.md`](GAME_DESIGN.md) for intended future play.

## Ordinary play

Current progression:

`local clues -> coarse-to-fine prospecting -> stone tools -> evidence-gated hand mining -> scarce-copper choice -> primitive power and processing -> second reinforcement`

| Area | Reachable capability |
| --- | --- |
| Survival and labor | Metabolic energy, hydration, vitality, Grain/Fruit/Protein nutrition, perishable food whose freshness responds to storage preservation state, multiple Protein foods that trade dietary energy/hydration against shelf life, finite drinking water, activity-dependent exertion, and exclusive player work. Player haulage/construction of preservation storage is not yet an ordinary-play logistics system. |
| Materials and inventory | Typed materials/forms, exact composition and particle state, finite stockpiles/lots, containment, preservation, reservations, provenance, temperature, deterministic coalescing, and represented-matter accounting. |
| Prospecting and knowledge | Survival-costed regional reconnaissance, local transects, field inspection, and detailed survey trade footprint, time, and precision to produce persisted bounded evidence without exposing hidden deposit identity or exact hidden state. Local transects cover a small multi-voxel area more efficiently than repeated point inspections while remaining area evidence rather than an exact-target reveal. |
| Mining | Evidence-gated hand mining through opaque targets with tool capability, hardness/batch/throughput limits, wear, reserved output capacity, geology-owned work-in-process, and explicit terminal output claim. |
| Crafting and equipment | Production-backed shaping, persistent equipment, assembly, additive upgrades, condition-dependent capability, occupancy, installation, disassembly/recovery, aggregate material maintenance, and exact embodied-component service. Primitive service exchanges a complete craftable worn component for fresh matter while preserving equipment identity and unrelated upgrades. |
| Primitive power | Material-backed finite energy storage and survival-costed manual power through portable equipment. The crude stone flywheel passively loses rotation, so stored hand work supports near-term automation but cannot be banked indefinitely; the actor recharges from observable remaining work when a planned operation would otherwise be underfunded. |
| Primitive processing | Crushing and finite-recovery native-copper sorting use finite work, equipment capability, wear, exact selected matter, and physical outputs; delegated work releases player attention. Primitive sorting recovers 90% of liberated copper with zero gangue carry into the native-metal stream, while unrecovered copper and arbitrary residue-compatible gangue remain in composition-bearing crushed residue instead of disappearing or requiring gangue-specific recipes. Gangue-hosted residue cannot be looped through the same primitive sorter. Later concentration may accept sufficiently fine composition-bearing crushed feed regardless of commodity host; that better process emits a distinct particulate tailings form, so one concentration pass does not feed the identical process indefinitely. The reachable loop also services worn primitive equipment from normally crafted replacement components, so scarce copper upgrades can remain long-lived investments instead of forcing full rebuilds. |

## Implemented infrastructure

These systems are authoritative runtime support even where ordinary acquisition is incomplete.

| Area | Implemented capability |
| --- | --- |
| Core | Deterministic headless simulation, immutable versioned registries, generated `AppState`, persisted RNG streams, typed time, checked integer physical quantities, explicit tick order. |
| Persistence | Current save schema only; trusted load rebuilds derived indexes and validates local and cross-owner invariants. Encoding/storage are adapter concerns. |
| Production | Timed closed-mass jobs, exact selected inputs, reserved outputs, persisted work-in-process, multi-stream routing, revision-bound completion, support-aware suspension/resume. |
| Energy and fluids | Finite typed-carrier energy stores with directional power limits and optional passive loss; finite homogeneous fluid stores with exact withdrawal and support-aware load. No generic inter-store transfer. |
| Structures | Material-backed members, contact-constrained support topology, axial analysis, source-owned loads, damage, and failure cascades. Player construction is absent. |
| Physical scalars | Typed mass, temperature, pressure, area, length, acceleration, force, power, energy, volume, mass-specific energy, and mass flow with checked integer arithmetic. |
| Spatial and presentation | Checked chunk-independent voxel coordinates, deterministic renderer-neutral texture baking, deterministic WGSL assembly. No graphics backend. |

## Capability-only evaluation

These systems execute through controlled gameplay-harness setup but lack an ordinary-play acquisition path.

| Surface | Evaluated capability |
| --- | --- |
| Workshop | Installed industrial machines under finite stored work, survival pressure, wear, maintenance, structural support, suspension/recovery, and policy-dependent choices. |
| Ore preparation | Installed crushing, grinding, dry screening, oversize regrinding, and copper concentration with exact constituent accounting and physical tailings. Concentration accepts fully liberated 500..=2,000 um particulate feed. |
| Foundry | Installed sensible heating, pure-material melting/casting, finite energy, equipment limits, phase boundaries, latent heat, finite heat recovery, and passive sink rejection. |

## Absent scope

| Area | Boundary |
| --- | --- |
| Engine/platform | Graphics backend, window/input/audio integration, ECS, networking, platform integration, general engine shell. |
| World representation | Voxel/chunk storage, terrain generation, streaming, world-scale spatial indexing, runtime discovery of clue locations. |
| Logistics | General transport authorization, pathing, carrying/haulage time, transport labor/energy, world-space delivery. Harness delivery is controlled setup infrastructure. |
| Advanced geology/mining | Regional generation, voxel ore topology, sampling, drilling, assays, geophysics, mechanized excavation, access, haulage, drainage, ground control, recovery fractions, waste rock, tailings transport/impoundment. |
| Thermal/chemical industry | Environmental heat transport beyond explicit sink loss, vaporization, combustion, fuels/emissions, mixed/alloy phase behavior, smelting/reduction, alloying, forging, machining, broader separation. |
| Maintenance/structures | Maintenance labor/tools/access, bespoke salvage, player construction/deconstruction, demolition/salvage physics, bending, shear, torsion, buckling, joints, terrain support. |
| Power networks | Generic energy transfer, shaft/belt networks, inertia/slip/clutches, steam, electrical topology, generation, distribution, protection, spatial network integration. |
| Hydrology | Generic fluid transport, surface/ground water, channels, pumps, irrigation, wastewater, sanitation, mixing, pressure/temperature-dependent fluid properties. |
| Ecology and society | Agriculture, soil, ecology, genetics, creatures, hunting/combat, workers, settlements, trade, economy, migration. |
| Industrial acquisition | Ordinary acquisition for industrial machines, industrial energy systems, and supporting infrastructure. |
| Save storage adapters | Save-file encoding/storage, filesystem atomicity, compression, cloud storage. |

A capability enters implemented scope only with an authoritative owner, canonical runtime path, required
persistence semantics, invariant coverage, and executable verification.

# Status

This page defines current runtime scope. It is the authority for whether a capability is ordinarily
reachable, implemented only for controlled evaluation, or absent. Product intent in
[`GAME_DESIGN.md`](GAME_DESIGN.md) does not establish implementation.

Use [`README.md`](README.md) for routing, [`ARCHITECTURE.md`](ARCHITECTURE.md) for engineering rules,
[`TECHNICAL_DESIGN.md`](TECHNICAL_DESIGN.md) for subsystem contracts, and [`TESTING.md`](TESTING.md) for
verification.

## Ordinary-play reachable

Current progression:

`local clues -> prospect -> stone tools -> evidence-gated hand mining -> scarce-copper choice -> primitive power and processing -> second reinforcement`

| Area | Current capability |
| --- | --- |
| Survival and labor | Metabolic energy, hydration, vitality, Grain/Fruit/Protein nutrition, perishable food, preservation, finite drinking water, activity-dependent exertion, and one exclusive player-work owner. |
| Materials and inventory | Typed materials/forms, exact composition and particle state, finite stockpiles/lots, containment, preservation, reservations, provenance, temperature, deterministic coalescing, and exact represented-matter accounting. |
| Prospecting and knowledge | Survival-costed field inspection and detailed field survey produce persisted bounded evidence without exposing hidden deposit identity or exact hidden state. |
| Mining | Evidence-gated hand mining through opaque targets with real tools, hardness/batch/throughput limits, wear, reserved destination capacity, in-flight ownership, and explicit output claim. |
| Crafting and equipment | Production-backed manual shaping, persistent embodied equipment, assembly, additive upgrades, condition-dependent capabilities, occupancy, installation, disassembly/recovery, and material-consuming maintenance. |
| Primitive power | Material-backed finite energy storage and survival-costed manual power through real portable equipment. |
| Primitive processing | Crushing and native-copper separation use finite work, equipment capability, wear, exact selected matter, and physical outputs. Delegated work returns player attention. |

## Implemented runtime support

These systems are active runtime infrastructure, but not every object or operation they can represent is
ordinarily acquirable.

| Area | Current capability |
| --- | --- |
| Core | Deterministic headless simulation, immutable versioned registries, generated `AppState`, persisted RNG streams, typed time, checked integer physical quantities, and explicit tick order. |
| Persistence | Current save schema only. Trusted load rebuilds derived indexes and validates local and cross-owner invariants before returning state. Encoding/storage remain adapter concerns. |
| Production | Timed closed-mass jobs with exact selected inputs, reserved outputs, persisted work-in-process, multi-stream routing, revision-bound completion, and support-aware suspension/resume. |
| Energy and fluids | Finite typed-carrier energy stores with directional power limits, exact optional passive dissipation into unmodeled loss domains, and finite homogeneous fluid stores with exact withdrawal and support-aware load. No generic inter-store transfer exists. |
| Structures | Material-backed members with contact-constrained support topology, axial analysis, source-owned loads, damage, and failure cascades. Controlled setup can materialize valid members; player construction is absent. |
| Physical scalars | Typed throughput, mass, temperature, pressure, power, torque, speed, electrical, flow, volume, efficiency, transmission, and operating-limit calculations. |
| Spatial and presentation | Checked chunk-independent voxel coordinates, deterministic renderer-neutral texture baking, and deterministic WGSL assembly. No graphics backend. |

## Capability-only evaluation

These production systems are executable through controlled gameplay-harness setup. Their ordinary-play
acquisition path is not implemented.

| Surface | Evaluated capability |
| --- | --- |
| Workshop | Installed industrial machines operate under finite stored work, survival pressure, wear, maintenance, structural support, suspension/recovery, and policy-dependent choices. |
| Ore preparation | Installed crushing, grinding, dry screening, oversize regrinding, and generalized copper concentration preserve exact constituent accounting and produce physical tailings. Concentration requires fully liberated 500..=2,000 um particulate feed rather than accepting primary coarse crusher output directly. |
| Foundry | Installed sensible heating, pure-material melting, and pure-material casting use finite energy, equipment limits, phase boundaries, latent heat, finite heat recovery, and passive heat rejection from the workshop sink. |

## Not implemented

| Area | Boundary |
| --- | --- |
| Engine/platform | Graphics backend, window/input/audio integration, ECS, networking, platform integration, general engine shell. |
| World representation | Voxel/chunk storage, terrain generation, streaming, world-scale spatial indexing, runtime discovery of clue locations. |
| Logistics | Ordinary stockpile transport authorization, pathing, carrying/haulage time, transport labor/energy, world-space delivery. Controlled harness delivery is setup infrastructure only. |
| Advanced geology/mining | Regional generation, voxel ore topology, sampling, drilling, assays, geophysics, mechanized excavation, access, haulage, drainage, ground control, recovery fractions, waste rock, tailings transport/impoundment. |
| Thermal/chemical industry | Spatial/environmental heat transport beyond explicit passive sink loss, vaporization, combustion, fuels/emissions, mixed/alloy phase behavior, smelting/reduction, alloying, forging, machining, and separation beyond the current liberated-copper model. |
| Rich maintenance/structures | Maintenance labor/tools/access, bespoke salvage, player construction/deconstruction, construction waste, demolition/salvage physics, bending, shear, torsion, buckling, joints, terrain support. |
| Power networks | Generic store-to-store energy transfer, shaft/belt networks, inertia/slip/clutches, steam, electrical topology, generation, distribution, protection, spatial network integration. |
| Hydrology | Generic inter-store fluid transport, surface/ground water, channels, pumps, irrigation, wastewater, sanitation, mixing, pressure/temperature-dependent fluid properties. |
| Ecology and society | Agriculture, soil, ecology, genetics, creatures, hunting/combat, workers, settlements, trade, economy, migration. |
| Industrial acquisition | Ordinary-play acquisition for industrial machines, industrial energy systems, and the infrastructure needed to reach capability-only scenarios. |
| Save storage adapters | Save-file encoding/storage, filesystem atomicity, compression, cloud storage. |

A capability moves into implemented scope only when it has an authoritative owner, one canonical runtime
path, required persistence semantics, invariant coverage, and executable verification.

# Status

This is the runtime capability inventory. Use [`README.md`](README.md) for repository routing,
[`ARCHITECTURE.md`](ARCHITECTURE.md) for engineering rules,
[`TECHNICAL_DESIGN.md`](TECHNICAL_DESIGN.md) for technical contracts, and
[`GAME_DESIGN.md`](GAME_DESIGN.md) for the forward design target.

## Implemented runtime

| Area | Capability |
| --- | --- |
| Core | Deterministic headless simulation with immutable versioned registries, generated `AppState`, persisted RNG streams, typed time, checked integer physical quantities, and explicit tick order. |
| Persistence | Current save schema only. Trusted load rebuilds derived indexes and validates local and cross-owner invariants before returning state. Encoding and storage are adapter concerns. |
| Matter and inventory | Typed materials/forms, exact composition and particle state, finite stockpiles and lots, containment/preservation, reservations, deterministic coalescing, opaque-authorized transfer, provenance, temperature, and exact represented-matter accounting. |
| Geology and knowledge | Finite hidden deposits plus persisted player observations with bounded abundance evidence. Hidden deposit identity and exact hidden state are not player-facing. |
| Prospecting | Survival-costed one-voxel field inspection for coarse evidence and a slower detailed field survey for narrower evidence. Both use exclusive player labor and persist observations. |
| Mining | Evidence-gated hand mining through opaque targets, real tools, hardness/batch/throughput limits, wear, destination reservation, in-flight ownership, and explicit output claim. |
| Production and crafting | Timed closed-mass jobs with exact selected inputs, output reservations, persisted work-in-process, multi-stream routing, revision-bound completion, suspension/resume, and production-backed manual shaping. |
| Survival and labor | Metabolic energy, hydration, vitality, recent Grain/Fruit/Protein nutrition, perishable food, preservation, finite drinking water, exertion costs, and one exclusive player-work owner. |
| Equipment and maintenance | Persistent embodied equipment, condition-dependent capabilities, assembly, additive upgrades, occupancy, fixed/portable installation, pristine disassembly, worn recovery, and material-consuming maintenance. |
| Energy and manual power | Finite typed-carrier energy stores with directional power limits and opaque transfer; material-backed store construction; survival-costed player power through real portable equipment. |
| Structures and fluids | Material-backed structural members with axial analysis, loads, damage, failure cascades, construction/deconstruction, plus finite homogeneous fluid stores with support-aware load. |
| Physical capability scalars | Typed throughput, mass, temperature, pressure, power, torque, speed, electrical, flow, volume, efficiency, transmission, and operating-limit calculations. |
| Ore processing | Crushing, grinding, dry screening, oversize regrinding, strict binary native-copper separation, and generalized copper concentration from liberated multi-gangue feed with finite work, equipment limits, wear, physical tailings, and exact constituent accounting. |
| Thermal production | Sensible heating, pure-material melting, and pure-material casting with finite energy, equipment limits, phase boundaries, and latent heat. |
| Primitive progression | Visible local clues -> coarse/refined prospecting -> opaque mining targets -> stone tools -> hand mining -> scarce copper choice between extraction and stored-work rate -> flywheel/crusher -> native-copper separation -> second reinforcement. Delegated processing returns player attention and produces progression material. |
| Industrial capability evaluation | Workshop, ore-preparation, and pure-copper foundry harnesses exercise already-installed industrial systems. They do not establish runtime acquisition of those systems. |
| Spatial and assets | Checked chunk-independent voxel coordinates; deterministic renderer-neutral texture baking and WGSL assembly. No graphics backend. |
| Verification | Unit, persistence, conservation, soak, and gameplay coverage through local tooling. [`TESTING.md`](TESTING.md) owns commands and harness rules. |

## Not implemented

| Area | Boundary |
| --- | --- |
| Engine/platform | Graphics backend, window/input/audio integration, ECS choice, networking, platform integration, general engine shell. |
| World representation | Voxel/chunk storage, terrain generation, streaming, world-scale spatial indexing, and runtime discovery of clue locations. |
| Advanced geology/mining | Regional geological generation, voxel ore topology, panning, physical sampling, drilling, assays, geophysics, mechanized excavation, access, haulage, drainage, ground control, recovery fractions, waste rock, and tailings transport/impoundment beyond the particulate tailings lots produced by current concentration. |
| Thermal/chemical industry | Environmental heat transport, vaporization, combustion, fuels/emissions, mixed/alloy phase behavior, concentration methods beyond the current liberated-copper separator model, smelting/reduction, alloying, forging, and machining. |
| Rich maintenance and structures | Repair labor/tools/access, bespoke salvage, maintenance scrap recovery, bending, shear, torsion, buckling, joints, terrain support, construction labor/waste, and fractional demolition streams. |
| Power networks | Shaft/belt networks, inertia/slip/clutches, steam systems, electrical topology, generation, distribution, protection, and spatial network integration. |
| Hydrology | Ground/surface water, channels, pumps, irrigation, wastewater, sanitation, fluid mixing, and pressure/temperature-dependent fluid properties. |
| Ecology and society | Agriculture, soil simulation, ecology, genetics, creatures, hunting/combat, workers, settlements, logistics, trade, economy, and migration. |
| Industrial acquisition | Ordinary-play acquisition for industrial machines and industrial energy systems, plus the processing infrastructure required to reach the bootstrapped industrial harnesses. |
| Save storage adapters | Save-file encoding/storage, filesystem atomicity, compression, cloud storage. |

List a capability here only when it has an authoritative owner, a canonical runtime path, persistence
semantics where required, invariant coverage, and executable verification.

# Status

**Role:** Current runtime scope and reachability authority.

Use [`README.md`](README.md) for routing,
[`GAME_DESIGN.md`](GAME_DESIGN.md) for intended experience, and [`DIRECTION.md`](DIRECTION.md) for future
priority. Source presence and controlled fixtures do not establish ordinary reachability.

## Ordinary play

Current progression:

`local clues -> prospecting -> mining -> native-copper dressing -> primitive mechanization -> copper upgrades -> settlement mechanization -> first foundry -> settlement batch foundry`

| Area | Reachable capability |
| --- | --- |
| Survival and labor | Metabolic energy, hydration, vitality, nutrition, perishability, timed consumption, exertion, preservation, and exclusive player work. |
| Materials and inventory | Typed commodities, exact lots, finite stockpiles/reservations, provenance, temperature, storage effects, recovery, and matter accounting. |
| Prospecting and mining | Reconnaissance, physical sampling, bounded geological evidence, unsampled localized attempts, tool-dependent extraction, and output claim into inventory. Hidden deposit identity and exact reserve remain non-public. |
| Crafting and equipment | Manual shaping, wear, service, recovery, primitive machines, copper upgrades, field instruments, specialist wire drawing, and settlement workshop upgrades. |
| Power | Human-powered providers, finite mechanical stores, direct treadle generation, and finite electrical storage. Generic power networks are absent. |
| Processing | Primitive crushing, grinding, sizing, concentration, scavenging, native-copper cleanup, copper forming, and the ordinary first/settlement foundry route. Compound-ore reduction is absent. |

## Current integration frontier

These are current graph boundaries, not priorities.

| Implemented side | Missing ordinary edge |
| --- | --- |
| Settlement batch foundry | Industrial-scale foundry acquisition, support, and high-power infrastructure. |
| Persistent local custody | Player movement, haulage, delivery/path cost, and ordinary world-source acquisition. |
| Structural physics | General player construction/deconstruction authorization. |
| Equipment maintenance | Maintenance tools and general transport to service sites. |
| Finite energy stores | Routed mechanical/electrical transmission and conversion networks. |
| Finite fluid stores | Routed transport, pumping, mixing, and pressure networks. |
| Industrial workshop/ore/foundry execution | Ordinary acquisition and construction of industrial infrastructure. |
| Checked coordinates and geological evidence | Runtime world/chunk ownership, terrain access, and clue-location discovery. |

[`DIRECTION.md`](DIRECTION.md) owns which boundary should be closed next.

## Implemented infrastructure

These systems have authoritative runtime owners and canonical production paths even where ordinary acquisition is
incomplete.

| Area | Implemented capability |
| --- | --- |
| Core | Deterministic headless simulation, validated immutable registries, generated `AppState`, typed time, checked physical quantities, and explicit tick order. |
| Persistence | Current-schema trusted load with deterministic derived-index rebuild and whole-state validation; byte storage is adapter-owned. |
| Production | Timed closed-mass jobs, exact inputs, output reservations/routing, persisted work-in-process, and support-aware suspension/resume. |
| Logistics | Persistent player/carried custody and world locations for stockpiles, equipment, energy stores, and fluid stores; local pickup/drop and access checks. |
| Loose surface matter | Persistent finite world-space resource records, exact-local actor observation, commodity-bound timed same-voxel gathering, destination reservation, survival cost, depletion, interruption cleanup, persistence, and conservation. Built-in methods distinguish loose stone, fallen timber, berry foraging, and clay-rich earth. Source generation remains controlled-fixture-only rather than ordinary world generation. |
| Energy and fluids | Finite typed energy stores with power/loss limits; finite homogeneous fluid stores with exact withdrawal, exact-player-local observation, and survival-filtered local drink-source discovery. Hydrology and ordinary fluid-source generation remain absent. |
| Storage recovery | Timed dismantling of material-backed storage with preservation checkpointing and exact body recovery. |
| Structures | Material-backed members, support topology, loads, damage, and failure cascades. |
| Spatial and presentation | Checked voxel coordinates plus deterministic renderer-neutral texture and shader assembly. |

## Capability-only evaluation

These surfaces execute through canonical runtime paths after controlled setup, but their infrastructure is not
ordinarily acquirable.

| Surface | Evaluated capability |
| --- | --- |
| Workshop | Industrial machinery under finite stored work, survival pressure, wear, maintenance, support, suspension, recovery, and actor policy. |
| Ore preparation | Industrial crushing, grinding, screening, regrinding, and concentration with exact constituent accounting. |
| Foundry | Industrial copper heating, melting, casting, remelting, finite energy, phase boundaries, heat recovery, and sink loss. |

## Absent scope

| Area | Boundary |
| --- | --- |
| Wilderness opening | Ordinary fresh-world wood/stone/forage/water acquisition, fire and warmth, shelter construction, and one continuous empty-inventory first-day loop. Existing survival, gathering, crafting, storage, thermal, and structural owners are supporting infrastructure, not proof that this opening is reachable. |
| Engine/platform | Graphics backend, window/input/audio integration, ECS, networking, and general engine shell. |
| World representation | Voxel/chunk storage, terrain generation, streaming, world-scale indexing, and runtime clue-location discovery. |
| Logistics | Movement/pathing, haulage, general delivery/transport, ordinary resource-source acquisition, container/hotbar presentation, fluid transport, and mounted-production transport/site geometry. |
| Advanced geology/mining | Regional generation, voxel ore topology, deep/laboratory/geophysical exploration, mechanized excavation, mine access, drainage, ground control, and waste/tailings logistics. |
| Thermal/chemical industry | Environmental heat transport, vaporization, combustion, fuels/emissions, mixed/alloy phase behavior, reduction/smelting, alloying, forging, machining, and broad recycling. |
| Maintenance/structures | Maintenance tools, general construction/deconstruction, demolition/salvage physics, bending, shear, torsion, buckling, joints, and terrain support. |
| Power networks | Generic transfer, shafts/belts, advanced mechanical transmission, scalable electrical distribution/protection, steam, and spatial network integration. |
| Hydrology | General fluid transport, surface/ground water, channels, pumps, irrigation, wastewater, sanitation, mixing, and pressure-dependent behavior. |
| Ecology and society | Agriculture, soil/ecology/genetics, creatures, hunting/combat, workers, settlements, trade, economy, and migration. |
| Industrial acquisition | Ordinary acquisition for industrial machinery, industrial energy systems, and supporting infrastructure. |
| Save storage adapters | Save-file encoding/storage, filesystem atomicity, compression, and cloud storage. |

Implemented means an authoritative owner, canonical runtime path, required persistence semantics, invariant
coverage, and executable verification. Ordinary reachability additionally requires a normal-play acquisition path.

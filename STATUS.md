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
| Prospecting and mining | Reconnaissance, sampling, bounded evidence, localized attempts, tool-dependent extraction, and inventory claim. Hidden deposit identity and exact reserve remain non-public. |
| Crafting and equipment | Manual shaping, wear, service, recovery, primitive machines, copper upgrades, field instruments, specialist wire drawing, and settlement workshop upgrades. |
| Power | Human-powered providers, finite mechanical stores, direct treadle generation, and finite electrical storage. Generic power networks are absent. |
| Processing | Primitive crushing, grinding, sizing, concentration, scavenging, native-copper cleanup, copper forming, and the ordinary first/settlement foundry route. Compound-ore reduction is absent. |
| Opening wilderness acquisition | Seeded five-edge graph with forage, optional water, and separate stone/timber leaves; edge-gated observation/movement and load-sensitive travel. A worksite can build a finite stone-ring campfire from local stone and staged timber; co-located heat restores warmth. General terrain/pathfinding, remote discovery, hydrology, shelter, and weather are absent. |
| Structures | Grounded adjacent members use carried matter and timed work; load-bearing starts on completion. |

## Current integration frontier

These are current graph boundaries, not priorities.

| Implemented side | Missing ordinary edge |
| --- | --- |
| Settlement batch foundry | Industrial-scale foundry acquisition, support, and high-power infrastructure. |
| Persistent local custody | General terrain/access generation, routed travel/haulage, delivery/path cost, and remote/general discovery. |
| Structural physics | Non-grounded construction, joints/panels, deconstruction/salvage, and terrain support. |
| Equipment maintenance | Maintenance tools and general transport to service sites. |
| Finite energy stores | Routed mechanical/electrical transmission and conversion networks. |
| Finite fluid stores | Routed transport, pumping, mixing, and pressure networks. |
| Industrial workshop/ore/foundry execution | Ordinary acquisition and construction of industrial infrastructure. |
| Checked coordinates and geological evidence | Runtime world/chunk ownership, terrain access, and clue-location discovery. |

[`DIRECTION.md`](DIRECTION.md) owns which boundary should be closed next.

## Implemented infrastructure

These systems have authoritative owners and canonical runtime paths even where ordinary acquisition is incomplete.

| Area | Implemented capability |
| --- | --- |
| Core | Deterministic headless simulation, validated immutable registries, generated `AppState`, typed time, checked physical quantities, and explicit tick order. |
| Persistence | Current-schema trusted load with deterministic derived-index rebuild and whole-state validation; byte storage is adapter-owned. |
| Production | Timed closed-mass jobs, exact inputs, output reservations/routing, persisted work-in-process, and support-aware suspension/resume. |
| Logistics | Persistent player/carried custody, locations, traversable neighbor edges, timed load-sensitive steps, exact-local observation, storage allocation, pickup/drop, and access checks. |
| Loose surface matter | Finite world records with edge-gated orientation, exact-local gathering, reservation, survival cost, depletion, interruption cleanup, and conservation. Fresh worlds distribute forage/stone/timber; terrain/ecology and remote discovery remain absent. |
| Energy and fluids | Finite typed stores with power/loss limits. Authored local combustion moves exact fuel matter to a terminal conservation account and finite thermal storage; co-located passive heat affects warmth. Fluids support exact withdrawal/local observation; fresh worlds place finite water. General heat transport and hydrology remain absent. |
| Storage recovery | Timed dismantling of material-backed storage with preservation checkpointing and exact body recovery. |
| Structures | Support/load/failure physics plus grounded hand-setting from exact material. |
| Spatial and presentation | Checked voxel coordinates plus deterministic renderer-neutral texture and shader assembly. |

## Capability-only evaluation

These surfaces execute canonically after controlled setup, but their infrastructure is not ordinarily acquirable.

| Surface | Evaluated capability |
| --- | --- |
| Workshop | Industrial machinery under finite stored work, survival pressure, wear, maintenance, support, suspension, recovery, and actor policy. |
| Ore preparation | Industrial crushing, grinding, screening, regrinding, and concentration with exact constituent accounting. |
| Foundry | Industrial copper heating, melting, casting, remelting, finite energy, phase boundaries, heat recovery, and sink loss. |

## Absent scope

| Area | Boundary |
| --- | --- |
| Wilderness opening | Travel/discovery beyond the bounded graph, shelter/weather protection, light, and one continuous empty-inventory first-day loop. The authored graph is not general pathfinding, hydrology, climate, or enclosure simulation. |
| Engine/platform | Graphics backend, window/input/audio integration, ECS, networking, and general engine shell. |
| World representation | Voxel/chunk storage, terrain generation, streaming, world-scale indexing, and runtime clue-location discovery. |
| Logistics | General access generation, multi-step routing, haulage, delivery/transport, remote acquisition, container/hotbar presentation, fluid transport, and mounted-production geometry. |
| Advanced geology/mining | Regional generation, voxel ore topology, deep/laboratory/geophysical exploration, mechanized excavation, mine access, drainage, ground control, and waste/tailings logistics. |
| Thermal/chemical industry | Environmental heat transport, vaporization, general combustion/fuel chemistry/emissions, mixed/alloy phases, reduction/smelting, alloying, forging, machining, and broad recycling. The campfire is not a general combustion or atmospheric model. |
| Maintenance/structures | Maintenance tools, construction/deconstruction/salvage, bending, shear, torsion, buckling, joints/panels, and terrain support. |
| Power networks | Generic transfer, shafts/belts, advanced mechanical transmission, scalable electrical distribution/protection, steam, and spatial network integration. |
| Hydrology | General fluid transport, surface/ground water, channels, pumps, irrigation, wastewater, sanitation, mixing, and pressure-dependent behavior. |
| Ecology and society | Agriculture, soil/ecology/genetics, creatures, hunting/combat, workers, settlements, trade, economy, and migration. |
| Industrial acquisition | Ordinary acquisition for industrial machinery, industrial energy systems, and supporting infrastructure. |
| Save storage adapters | Save-file encoding/storage, filesystem atomicity, compression, and cloud storage. |

Implemented means an authoritative owner, canonical runtime path, required persistence semantics, invariant
coverage, and executable verification. Ordinary reachability additionally requires a normal-play acquisition path.

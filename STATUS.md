# Status

**Role:** Current runtime scope and reachability authority.

Use [`README.md`](README.md) for routing, [`GAME_DESIGN.md`](GAME_DESIGN.md) for intent, and
[`DIRECTION.md`](DIRECTION.md) for priority. Fixtures do not establish ordinary reachability.

## Ordinary play

Current progression:

`local clues -> prospecting -> mining -> native-copper dressing -> primitive mechanization -> copper upgrades -> settlement mechanization -> first foundry -> settlement batch foundry`

| Area | Reachable capability |
| --- | --- |
| Survival and labor | Metabolic energy, hydration, vitality, nutrition, perishability, timed consumption, exertion, preservation, and exclusive player work. |
| Materials and inventory | Typed commodities, exact lots, finite stockpiles/reservations, provenance, temperature, storage effects, and matter accounting. |
| Prospecting and mining | Reconnaissance, sampling, bounded evidence, localized attempts, tool-dependent extraction, and inventory claim. Hidden deposits/reserves remain non-public. |
| Crafting and equipment | Manual shaping, wear, service, primitive machines, copper upgrades, field instruments, wire drawing, and workshop upgrades. |
| Power | Human-powered providers, finite mechanical stores, direct treadle generation, and finite electrical storage. Generic power networks are absent. |
| Processing | Crushing, grinding, sizing, concentration, scavenging, native-copper cleanup, copper forming, and first/settlement foundry routes. Compound-ore reduction is absent. |
| Opening wilderness acquisition | Seeded forage/water/stone/timber graph with edge-gated observation, load-sensitive travel, and a finite local-heating campfire. General terrain play, remote discovery, hydrology, shelter, and weather are absent. |
| Structures | Grounded adjacent members use carried matter and timed work; load-bearing starts on completion. |

## Current integration frontier

These are current graph boundaries, not priorities.

| Implemented side | Missing ordinary edge |
| --- | --- |
| Settlement batch foundry | Industrial-scale foundry acquisition, support, and high-power infrastructure. |
| Generated chunks + terrestrial full-cube body physics | Demand streaming/access, richer collision/contact, routed haulage/path cost, and remote discovery. |
| Structural physics | Non-grounded construction, joints/panels, deconstruction/salvage, and terrain support. |
| Equipment maintenance | Maintenance tools and general transport to service sites. |
| Finite energy stores | Routed mechanical/electrical transmission and conversion networks. |
| Finite fluid stores | Routed transport, pumping, mixing, and pressure networks. |
| Industrial workshop/ore/foundry execution | Ordinary acquisition and construction of industrial infrastructure. |
| Checked coordinates + compact chunk ownership + geological evidence | Terrain access/collision, regional geology-to-chunk composition, and clue-location discovery. |

[`DIRECTION.md`](DIRECTION.md) owns which boundary should be closed next.

## Implemented infrastructure

These systems have authoritative owners and canonical paths even where ordinary acquisition is incomplete.

| Area | Implemented capability |
| --- | --- |
| Core | Deterministic simulation; persisted 20 Hz `ControlTick`; exact 72:1 slow `SimulationTick`; active body physics cannot be skipped by slow stepping. |
| Persistence | Current-schema trusted load with deterministic derived-index rebuild and whole-state validation; byte storage is adapter-owned. |
| Production | Timed closed-mass jobs, exact inputs, output reservations/routing, persisted work-in-process, and support-aware suspension/resume. |
| Logistics | Fixed-point body/carrying, 20 Hz walk/sprint/crouch/jump, full-cube collision, load/speed-scaled exertion, legacy off-terrain steps, pickup/drop, and access checks. |
| Loose surface matter | Finite world records with edge-gated orientation, local gathering, reservation, survival cost, depletion, cleanup, and conservation. Fresh worlds distribute forage/stone/timber. |
| Energy and fluids | Finite typed stores with power/loss limits. Combustion moves exact fuel matter to a terminal account and finite thermal storage; local passive heat affects warmth. Fluids support exact withdrawal/local observation and finite opening water. |
| Storage recovery | Timed dismantling of material-backed storage with preservation checkpointing and exact body recovery. |
| Structures | Support/load/failure physics plus grounded hand-setting from exact material. |
| Block world | Persistent 16x16 palette-compressed chunks, represented mass, seeded terrain, generated-vs-air reads, body-safe generation, and full-cube collision. Streaming, richer shapes, mutation, composition, and block actions remain absent. |
| Spatial and presentation | Checked voxel coordinates plus deterministic renderer-neutral texture and shader assembly. |

## Capability-only evaluation

These surfaces execute canonically after controlled setup but are not ordinarily acquirable.

| Surface | Evaluated capability |
| --- | --- |
| Workshop | Industrial machinery under finite stored work, survival pressure, wear, maintenance, support, suspension, recovery, and actor policy. |
| Ore preparation | Industrial crushing, grinding, screening, regrinding, and concentration with exact constituent accounting. |
| Foundry | Industrial copper heating, melting, casting, remelting, finite energy, phase boundaries, heat recovery, and sink loss. |

## Absent scope

| Area | Boundary |
| --- | --- |
| Wilderness opening | Travel beyond the bounded graph, shelter/weather, light, and one continuous empty-inventory first-day loop. The graph is not pathfinding, hydrology, climate, or enclosure simulation. |
| Engine/platform | Graphics backend, window/input/audio integration, ECS, networking, and general engine shell. |
| World representation | Demand streaming, indexing, height/light summaries, mutation, partial/rich collision/access, geology/biome/ecology composition, and clue discovery. Compact chunks, seeded terrain, terrestrial fixed-point locomotion, full-cube collision, and 20 Hz control time exist. |
| Logistics | General access generation, multi-step routing, haulage, delivery/transport, remote acquisition, container/hotbar presentation, fluid transport, and mounted-production geometry. |
| Advanced geology/mining | Regional generation, voxel ore topology, deep/laboratory/geophysical exploration, mechanized excavation, mine access, drainage, ground control, and waste/tailings logistics. |
| Thermal/chemical industry | Environmental heat transport, vaporization, general combustion/fuel chemistry/emissions, mixed/alloy phases, reduction/smelting, alloying, forging, machining, and broad recycling. The campfire is not a general combustion or atmospheric model. |
| Maintenance/structures | Maintenance tools, construction/deconstruction/salvage, bending, shear, torsion, buckling, joints/panels, and terrain support. |
| Power networks | Generic transfer, shafts/belts, advanced mechanical transmission, scalable electrical distribution/protection, steam, and spatial network integration. |
| Hydrology | General fluid transport, surface/ground water, channels, pumps, irrigation, wastewater, sanitation, mixing, and pressure-dependent behavior. |
| Ecology and society | Agriculture, soil/ecology/genetics, creatures, hunting/combat, workers, settlements, trade, economy, and migration. |
| Industrial acquisition | Ordinary acquisition for industrial machinery, industrial energy systems, and supporting infrastructure. |
| Save storage adapters | Save-file encoding/storage, filesystem atomicity, compression, and cloud storage. |

Implemented means an authoritative owner, canonical path, persistence semantics, invariants, and executable proof.
Ordinary reachability additionally requires a normal-play acquisition path.

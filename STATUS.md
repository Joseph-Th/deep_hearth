# Status

**Role:** Current runtime scope and reachability authority.

Use [`README.md`](README.md) for routing, [`GAME_DESIGN.md`](GAME_DESIGN.md) for intent, and [`DIRECTION.md`](DIRECTION.md) for priority. Fixtures do not prove ordinary reachability.

## Ordinary play

Progression:

`local clues -> prospecting -> mining -> native-copper dressing -> primitive mechanization -> copper upgrades -> settlement mechanization -> first foundry -> settlement batch foundry`

| Area | Reachable capability |
| --- | --- |
| Survival and labor | Metabolic energy, hydration, vitality, nutrition, perishability, timed stack-aware consumption, exertion, preservation, and exclusive player work. |
| Materials and inventory | Typed commodities, exact lots, finite stockpiles/reservations, provenance, temperature, storage effects, and matter accounting. |
| Prospecting and mining | Reconnaissance, sampling, bounded evidence, tool-dependent extraction, and inventory claim. Hidden reserves remain non-public. |
| Crafting and equipment | Manual shaping, wear, service, primitive machines, copper upgrades, field instruments, wire drawing, and workshop upgrades. |
| Power | Human-powered providers, finite mechanical stores, direct treadle generation, and finite electrical storage. Generic power networks are absent. |
| Processing | Crushing, grinding, sizing, concentration, scavenging, native-copper cleanup, copper forming, and first/settlement foundry routes. Compound-ore reduction is absent. |
| Opening wilderness acquisition | Seeded forage/water/stone/timber graph, load-sensitive travel, and finite campfire heat. General terrain play, remote discovery, hydrology, shelter, and weather are absent. |
| Structures | Grounded adjacent members atomically consume carried stacks into timed work; load-bearing starts on completion. |

## Current integration frontier

Current graph boundaries, not priorities:

| Implemented side | Missing ordinary edge |
| --- | --- |
| Settlement batch foundry | Industrial-scale foundry acquisition, support, and high-power infrastructure. |
| Generated world + body + targeting + excavation | Inventory/container/crafting UI gestures, plus general placement, residency, richer contact, and remote discovery. |
| Structural physics | Non-grounded construction, joints/panels, deconstruction/salvage, and terrain support. |
| Equipment maintenance | Maintenance tools and service-site transport. |
| Finite energy stores | Routed mechanical/electrical transmission and conversion networks. |
| Finite fluid stores | Routed transport, pumping, mixing, and pressure networks. |
| Industrial workshop/ore/foundry execution | Ordinary industrial infrastructure acquisition/construction. |
| Checked coordinates + compact chunk ownership + geological evidence | Terrain access/collision, regional geology-to-chunk composition, and clue-location discovery. |

[`DIRECTION.md`](DIRECTION.md) owns which boundary should be closed next.

## Implemented infrastructure

These systems have authoritative owners and canonical paths even where ordinary acquisition is incomplete.

| Area | Implemented capability |
| --- | --- |
| Core | Deterministic simulation; persisted 20 Hz `ControlTick`; exact 72:1 slow `SimulationTick`; active body physics cannot be skipped by slow stepping. |
| Persistence | Current-schema trusted load with deterministic derived-index rebuild and whole-state validation; byte storage is adapter-owned. |
| Production | Timed closed-mass jobs, exact inputs, output reservations/routing, persisted work-in-process, and support-aware suspension/resume. |
| Crafting | Carried recipe/batch crafting selects exact lots privately; meaningful stack choice stays explicit. |
| Logistics | Fixed-point body/carrying, 20 Hz locomotion/collision, load-scaled exertion, bounded steps, equipment pickup/drop, nine-slot hotbar, carried/local inventory+equipment projection, max-fit quick-move, and access checks. |
| Loose surface matter | Finite gathering/drop custody; exact-local drop projection; partial capacity-filling auto-pickup; natural opportunities stay deliberate. |
| Energy and fluids | Finite typed stores with power/loss limits. Combustion moves exact fuel matter to a terminal account and finite thermal storage; local passive heat affects warmth. Fluids support exact withdrawal/local observation and finite opening water. |
| Storage recovery | Timed dismantling of material-backed storage with preservation checkpointing and exact body recovery. |
| Structures | Support/load/failure plus atomic carried-stack hand-setting and top-face timber-post use. |
| Block world | Mass-backed chunks, demand, collision, 4.5 m targeting, 20 Hz excavation, conserved fragments, selected use/drop, storage/post placement, and inventory/hotbar shell. Adapter UI, general placement, richer shapes, and residency remain absent. |
| Spatial and presentation | Checked voxel coordinates, canonical cube faces, fixed-point view directions, and deterministic renderer-neutral texture/shader assembly. |

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
| World representation | Residency, indexing, height/light summaries, placement/updates, partial shapes, geology/biome/ecology, and clue discovery. Terrain demand, locomotion, targeting, and conserved partial excavation exist. |
| Logistics | General access generation, routing/haulage/transport, remote acquisition, adapter inventory UI, fluid transport, and mounted-production geometry. |
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

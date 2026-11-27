# Status

**Role:** Runtime scope and reachability. Fixtures do not prove ordinary reachability.
See [`README.md`](README.md) for routing.

## Ordinary play

Progression:

`local clues -> prospecting -> mining -> native-copper dressing -> primitive mechanization -> copper upgrades -> settlement mechanization -> first foundry -> settlement batch foundry -> cast parts -> powered helve`

| Area | Reachable capability |
| --- | --- |
| Survival and labor | Spring start; 90-day seasons (30-day months), 360-day year. Metabolism, hydration, diet, spoilage, timed intake/work, exertion, weather, roofs and windbreaks. |
| Materials and inventory | Conserved lots/storage, reservations, preservation, residues and spoil bays. Insulated stone/timber cache (75 kg, 4x); ventilated fuel rack (150 kg, ambient aging). |
| Prospecting and mining | Weathered 10%-grade copper ore needs a stone pick, survey, mining and hand dressing; reserves stay private. |
| Crafting and equipment | Nut sorting (500 g to 450 g 180-day staples + 50 g waste); tuber curing (500 g to 400 g 90-day staples + 100 g waste). Rejects spoiled input. Flint/copper tools, kiln, molds, maintenance and waste storage. |
| Power | Crank, treadle, flywheels, buffers and straight timber shafts (50 W; 20% loss/roller). No gears or electric networks. |
| Processing | Ore dressing, local crusher batch choices and unattended work; casting, PbS+Fe smelting, kiln firing and finite cooling. No general reduction or passive cooling. |
| Opening wilderness acquisition | Scouts forage, drink, steal and warn; thirst limits raids. Firelight/boxes deter theft; loot is recoverable. Tracks, sounds and staggers; injuries scenario-only. |
| Natural starts | Random dry/mixed/damp hills, ore and raw matter, species-specific tree growth, four wild foods and seasonal fungal mycelia; no guaranteed supplies. |
| Structures | Timed carried-stack posts, framing and a 6.5 kg cover panel; activation on completion. |

## Current integration frontier

Missing ordinary edges:

| Implemented side | Missing ordinary edge |
| --- | --- |
| Settlement batch foundry | Industrial-scale foundry acquisition, support, and high-power infrastructure. |
| Generated world + body + targeting | Graphical inventory/crafting UI, broad placement, residency and remote discovery. |
| Structural physics | Broader panels/joints, cascade demolition, and terrain support. |
| Player equipment service/recovery | Maintenance-tool requirements and service-site transport beyond exact-local carried stock. |
| Finite energy stores | Branching, gears, electrical distribution and conversion beyond straight timber shaft runs. |
| Finite fluid stores | Horizontal/remote transport, pumping, chemical mixing and pressure networks. |
| Industrial workshop/ore/foundry execution | Raw acquisition, placed workshop parts, located ore bins and multi-stage autonomous processing. |
| Checked coordinates + compact chunks + geology | Regional geology-to-terrain and broader clue discovery. |

[`DIRECTION.md`](DIRECTION.md) owns which boundary should be closed next.

## Implemented infrastructure

These canonical owners may lack ordinary acquisition.

| Area | Implemented capability |
| --- | --- |
| Core | Persisted 20 Hz `ControlTick` + exact 72:1 `SimulationTick`; slow-step only when no control-rate consequence can occur. |
| Persistence | Current-schema trusted load with deterministic derived-index rebuild and whole-state validation; byte storage is adapter-owned. |
| Production | Timed conserved work, routed outputs, actor-local powered comminution batch choices with capacity-limited outputs and stale-cohort rejection, metal remelting/casting and iron upkeep. PbS+Fe yields lead and FeS matte. |
| Crafting | Timed exact-lot processing, including iron head finishing with recoverable chips, equipment admission and conserved outputs. |
| Logistics | 20 Hz body collision, carrying/load exertion, falling injury, equipment and inventory access, nine-slot hotbar, buffered actions. |
| Loose surface matter | Finite gathering and drops; small fresh emergency forage can spend vitality, other work requires reserves. Pickup respects capacity, heat, delay and work; timber loads deliberately. Local drop details shown. Deep residue obstructs storage footing. |
| Energy and fluids | Finite energy/losses, shafts, campfire, drinking; exact local/downhill pouring forbids bare-hand fluid >353.15 K at touched ends. |
| Storage recovery | Timed dismantling of material-backed storage with preservation checkpointing and exact body recovery. |
| Structures | Support/load/failure; timed posts, framing, cover and grounded timber roller; timed primary-break recovery. |
| Block world | Terrain, mineral provinces, biome trees, shrubs, finite tubers and seasonal fungal fruiting, springs, ruins, excavation; finite water. |
| Spatial and presentation | Checked coordinates, baked textures/shaders, terrain and seasonal tree meshes; read-only desktop preview, daylight, shadows and PNG capture. Text play supports gameplay and saves. |

## Capability-only evaluation

These surfaces execute canonically after controlled setup but are not ordinarily acquirable.

| Surface | Evaluated capability |
| --- | --- |
| Workshop | Industrial machinery under finite stored work, survival pressure, wear, maintenance, support, suspension, and recovery. |
| Ore preparation | Finite crushing, grinding, concentration and cassiterite/galena tailings scavenging. Magnetite needs an electric separator; deep mining and transport remain player work. |
| Foundry | Industrial copper heating, melting, casting, remelting, finite energy, phase boundaries, heat recovery, and sink loss. |

## Absent scope

| Area | Boundary |
| --- | --- |
| Wilderness opening | Full open-ended journey, precipitation, pathfinding, flowing water, propagated lighting, general enclosures and complete first-day loop. Terrain, weather and springs exist. |
| Engine/platform | Playable graphical client, audio, ECS, networking, and general engine shell. |
| World representation | Residency, general terrain placement, partial shapes, regional geology/biomes and discovery beyond local clues. |
| Logistics | General access generation, routing/haulage/transport, remote acquisition, graphical inventory UI, fluid transport, and mounted-production geometry. |
| Advanced geology/mining | Mixed-grade world ore, regional topology, laboratory surveys, mechanized mining, drainage, ground control and tailings transport. |
| Thermal/chemical industry | Environmental heat, vaporization, general combustion/emissions, alloy phases, reduction/smelting, alloying, forging, machining and recycling. Campfire is not an atmospheric model. |
| Maintenance/structures | Maintenance tools, broader panels/shapes, cascade salvage, bending/shear/torsion/buckling, and terrain support. |
| Power networks | Shaft branches, belts, gearing, electrical distribution/protection, steam and transmission beyond straight contiguous timber roller runs. |
| Hydrology | General fluid transport, surface/ground water, channels, pumps, irrigation, wastewater, sanitation, chemical blending/contamination, and pressure-dependent behavior. |
| Ecology and society | Tree planting, agriculture, lethal combat, mobile armed pursuit, temple physiology, relics, recruitment, settlements, trade, migration, breached enclosures and extended raids. |
| Industrial acquisition | Ordinary acquisition for industrial machinery, industrial energy systems, and supporting infrastructure. |
| General save storage adapters | Beyond text-play JSON save/load: platform save browser, atomic durability, compression and cloud storage. |

Implemented means an authoritative owner, canonical path, persistence semantics, invariants, and executable proof.
Ordinary reachability additionally requires a normal-play acquisition path.

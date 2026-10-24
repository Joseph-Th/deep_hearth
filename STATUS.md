# Status

**Role:** Current runtime scope and reachability authority.

Use [`README.md`](README.md) for routing, [`GAME_DESIGN.md`](GAME_DESIGN.md) for intent, and [`DIRECTION.md`](DIRECTION.md) for priority. Fixtures do not prove ordinary reachability.

## Ordinary play

Progression:

`local clues -> prospecting -> mining -> native-copper dressing -> primitive mechanization -> copper upgrades -> settlement mechanization -> first foundry -> settlement batch foundry -> cast parts -> powered helve`

| Area | Reachable capability |
| --- | --- |
| Survival and labor | Metabolism, hydration, vitality, diet, spoilage, timed intake, exertion, preservation, warmth and exclusive work. |
| Materials and inventory | Exact lots, finite storage/reservations, conservation, preservation, classified residues and waste-only spoil bays. |
| Prospecting and mining | Player-facing survey choices, bounded evidence, tool-dependent extraction, and inventory claim. Hidden reserves remain non-public. |
| Crafting and equipment | Manual/powered shaping, tool wear/service, upgrades, field instruments and machines. Physical waste routes to finite local storage; full bins block work. |
| Power | Human drives, mechanical stores, treadle generation, and plate/wound/rack electrical buffers. No power networks. |
| Processing | Graded tin/iron/lead ores; cassiterite, hematite and galena dressing with mechanical tailings recovery. Copper casting and PbS+Fe lead smelting; no general reduction. |
| Opening wilderness acquisition | Scouts raid, forage and track; light and guards deter them. Recovered ingots or sizeable iron scraps improve conserved tools; clappers repair two-hop alarms. Hearing respects air paths and concealment; no combat. |
| Structures | Timed carried-stack posts/framing plus a 6.5 kg cover panel; activation occurs on completion. |

## Current integration frontier

Current graph boundaries, not priorities:

| Implemented side | Missing ordinary edge |
| --- | --- |
| Settlement batch foundry | Industrial-scale foundry acquisition, support, and high-power infrastructure. |
| Generated world + body + targeting | Inventory/crafting UI, placement, residency, richer contact and remote discovery. |
| Structural physics | Broader panels/joints, cascade demolition, and terrain support. |
| Player equipment service/recovery | Maintenance-tool requirements and service-site transport beyond exact-local carried stock. |
| Finite energy stores | Routed mechanical/electrical transmission and conversion networks. |
| Finite fluid stores | Horizontal/remote transport, pumping, chemical mixing and pressure networks. |
| Industrial workshop/ore/foundry execution | Ordinary industrial infrastructure acquisition/construction. |
| Checked coordinates + compact chunk ownership + geological evidence | Terrain access/collision, regional geology-to-chunk composition, and clue-location discovery. |

[`DIRECTION.md`](DIRECTION.md) owns which boundary should be closed next.

## Implemented infrastructure

These systems have authoritative owners and canonical paths even where ordinary acquisition is incomplete.

| Area | Implemented capability |
| --- | --- |
| Core | Persisted 20 Hz `ControlTick` + exact 72:1 `SimulationTick`; slow-step only when no control-rate consequence can occur. |
| Persistence | Current-schema trusted load with deterministic derived-index rebuild and whole-state validation; byte storage is adapter-owned. |
| Production | Timed conserved work, routed outputs, metal remelting/casting and iron tool upkeep. PbS+Fe precipitation makes molten lead and FeS matte; recovered lead can be cast and remelted. Gas-forming reactions are reference-only. |
| Crafting | Timed exact-lot processing, including iron head finishing with recoverable chips, equipment admission and conserved outputs. |
| Logistics | 20 Hz body collision, carrying/load exertion, falling injury, equipment and inventory access, nine-slot hotbar, buffered actions. |
| Loose surface matter | Finite gathering/drop custody; empty-hand local natural gathering; partial capacity-filling drop pickup. Deep exposed residue layers obstruct ground-storage footing until cleared. |
| Energy and fluids | Finite power/loss-limited energy, campfire heat, drinking, and exact local/downhill fluid transfer with player choice previews. |
| Storage recovery | Timed dismantling of material-backed storage with preservation checkpointing and exact body recovery. |
| Structures | Support/load/failure; interaction-reach stack construction including real posts/framing and a supported cover panel; primary-break recovery. |
| Block world | Mass-backed eight-mineral geology, temple rooms, seven assemblies and solid caste bodies, with locally coordinated archive and drive damage watches. Excavation and targeting exist; placement, UI, residency absent. |
| Spatial and presentation | Checked voxel coordinates, canonical cube faces, fixed-point view directions, and deterministic renderer-neutral texture/shader assembly. |

## Capability-only evaluation

These surfaces execute canonically after controlled setup but are not ordinarily acquirable.

| Surface | Evaluated capability |
| --- | --- |
| Workshop | Industrial machinery under finite stored work, survival pressure, wear, maintenance, support, suspension, recovery, and actor policy. |
| Ore preparation | Finite crushing, grinding, concentration and cassiterite/galena tailings scavenging. Magnetite needs an electric separator; deep mining and transport remain player work. |
| Foundry | Industrial copper heating, melting, casting, remelting, finite energy, phase boundaries, heat recovery, and sink loss. |

## Absent scope

| Area | Boundary |
| --- | --- |
| Wilderness opening | Travel beyond the bounded graph, weather/climate, pathfinding, hydrology, enclosure, broad lighting, and complete first-day branches. Limited camp/refuge exists. |
| Engine/platform | Graphics backend, window/input/audio integration, ECS, networking, and general engine shell. |
| World representation | Residency, world summaries, general placement and block updates, partial shapes, regional geology, biome/ecology and clue discovery. |
| Logistics | General access generation, routing/haulage/transport, remote acquisition, adapter inventory UI, fluid transport, and mounted-production geometry. |
| Advanced geology/mining | Mixed-grade world ore, regional generation, voxel ore topology, laboratory/geophysical surveys, mechanized mining, drainage, ground control and tailings transport. |
| Thermal/chemical industry | Environmental heat transport, vaporization, general combustion/fuel chemistry/emissions, mixed/alloy phases, reduction/smelting, alloying, forging, machining, and broad recycling. The campfire is not a general combustion or atmospheric model. |
| Maintenance/structures | Maintenance tools, broader panels/shapes, cascade salvage, bending/shear/torsion/buckling, and terrain support. |
| Power networks | Generic transfer, shafts/belts, advanced mechanical transmission, scalable electrical distribution/protection, steam, and spatial network integration. |
| Hydrology | General fluid transport, surface/ground water, channels, pumps, irrigation, wastewater, sanitation, chemical blending/contamination, and pressure-dependent behavior. |
| Ecology and society | Agriculture, ecology, creature collision/combat/pursuit, broader temple-caste duties and physiology, active relic restoration, settlements, trade, economy, migration, and raids beyond the opening patrol. |
| Industrial acquisition | Ordinary acquisition for industrial machinery, industrial energy systems, and supporting infrastructure. |
| Save storage adapters | Save-file encoding/storage, filesystem atomicity, compression, and cloud storage. |

Implemented means an authoritative owner, canonical path, persistence semantics, invariants, and executable proof.
Ordinary reachability additionally requires a normal-play acquisition path.

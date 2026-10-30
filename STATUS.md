# Status

**Role:** Current runtime scope and reachability authority.

[`README.md`](README.md) routes tasks; [`GAME_DESIGN.md`](GAME_DESIGN.md) defines intent; [`DIRECTION.md`](DIRECTION.md) defines priority. Fixtures do not prove reachability.

## Ordinary play

Progression:

`local clues -> prospecting -> mining -> native-copper dressing -> primitive mechanization -> copper upgrades -> settlement mechanization -> first foundry -> settlement batch foundry -> cast parts -> powered helve`

| Area | Reachable capability |
| --- | --- |
| Survival and labor | Metabolism, hydration, vitality, diet, spoilage, timed intake, exertion, preservation, warmth and exclusive work. |
| Materials and inventory | Exact lots, finite storage/reservations, conservation, preservation, classified residues and waste-only spoil bays. |
| Prospecting and mining | Opening hard-copper seam requires acquired survey evidence and a reinforced pick. Ore is finite and must be hauled; reserves stay private. |
| Crafting and equipment | Flint/copper tools, kiln, cooling bed, fired ceramic casting mold, maintenance and upgrades; finite waste storage. |
| Power | Hand crank, treadle, finite flywheels, electrical buffers. A built 195 g timber roller transfers up to 50 W between opposed mechanical stores at 80% efficiency, obeying occupancy and endpoint limits. No multi-voxel routing or electric networks. |
| Processing | Ore dressing, casting, PbS+Fe smelting; fueled ceramic firing, finite-sink cooling, safe-temperature assembly and food storage. No general reduction or passive cooling. |
| Opening wilderness acquisition | Scouts forage, warn and steal drops; machine activity attracts raids on exposed stock. Presence, light and field boxes deter theft; loot is recoverable. Player clubs have nonlethal contact, wear and exertion. Enemy strikes require stationary fitted tools (owner scenario only). |
| Structures | Timed carried-stack posts/framing plus a 6.5 kg cover panel; activation occurs on completion. |

## Current integration frontier

Missing ordinary edges:

| Implemented side | Missing ordinary edge |
| --- | --- |
| Settlement batch foundry | Industrial-scale foundry acquisition, support, and high-power infrastructure. |
| Generated world + body + targeting | Inventory/crafting UI, placement, residency, richer contact and remote discovery. |
| Structural physics | Broader panels/joints, cascade demolition, and terrain support. |
| Player equipment service/recovery | Maintenance-tool requirements and service-site transport beyond exact-local carried stock. |
| Finite energy stores | Shaft runs, branching, gearing, electrical distribution and conversion beyond the single timber roller. |
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
| Production | Timed conserved work, routed outputs, metal remelting/casting and iron upkeep. PbS+Fe produces molten lead and FeS matte; lead recasts. Gas reactions are reference-only. |
| Crafting | Timed exact-lot processing, including iron head finishing with recoverable chips, equipment admission and conserved outputs. |
| Logistics | 20 Hz body collision, carrying/load exertion, falling injury, equipment and inventory access, nine-slot hotbar, buffered actions. |
| Loose surface matter | Finite gathering and drops, empty-hand local gathering, capacity-limited pickup. Deep residue obstructs ground-storage footing. |
| Energy and fluids | Finite energy and losses, autonomous one-hop shaft transfer, campfire heat, drinking, and local/downhill fluid transfer. |
| Storage recovery | Timed dismantling of material-backed storage with preservation checkpointing and exact body recovery. |
| Structures | Support/load/failure; timed posts, framing, cover and grounded timber roller; timed primary-break recovery. |
| Block world | Eight ores, temple rooms, excavation and targeting. Lazy generated trees yield conserved logs from timed, wearing stone/copper adze work. Juvenile relocation is capability-only; no saplings, leaves or residency. |
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
| Wilderness opening | Travel beyond bounded graph, weather/climate, pathfinding, hydrology, general enclosures/lighting and full first-day branches. Limited camp/refuge exists. |
| Engine/platform | Graphics backend, window/input/audio integration, ECS, networking, and general engine shell. |
| World representation | Residency, general placement, partial shapes, regional geology, biomes and clues. |
| Logistics | General access generation, routing/haulage/transport, remote acquisition, adapter inventory UI, fluid transport, and mounted-production geometry. |
| Advanced geology/mining | Mixed-grade world ore, regional generation, voxel ore topology, laboratory/geophysical surveys, mechanized mining, drainage, ground control and tailings transport. |
| Thermal/chemical industry | Environmental heat, vaporization, general combustion/emissions, alloy phases, reduction/smelting, alloying, forging, machining and recycling. Campfire is not an atmospheric model. |
| Maintenance/structures | Maintenance tools, broader panels/shapes, cascade salvage, bending/shear/torsion/buckling, and terrain support. |
| Power networks | Multi-cell shafts/belts, gearing, electrical distribution/protection, steam and spatial integration beyond one-roller transfer. |
| Hydrology | General fluid transport, surface/ground water, channels, pumps, irrigation, wastewater, sanitation, chemical blending/contamination, and pressure-dependent behavior. |
| Ecology and society | Tree planting, agriculture, lethal combat, mobile armed pursuit, temple physiology, relics, recruitment, settlements, trade, migration, breached enclosures and extended raids. |
| Industrial acquisition | Ordinary acquisition for industrial machinery, industrial energy systems, and supporting infrastructure. |
| Save storage adapters | Save-file encoding/storage, filesystem atomicity, compression, and cloud storage. |

Implemented means an authoritative owner, canonical path, persistence semantics, invariants, and executable proof.
Ordinary reachability additionally requires a normal-play acquisition path.

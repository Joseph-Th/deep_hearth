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
| Power | Hand crank, treadle, flywheels and buffers. Straight built timber shafts couple opposed mechanical stores at up to 50 W, losing 20% per roller. No gearboxes or electric networks. |
| Processing | Ore dressing and local powered crusher choices from visible exact-lot feed, machine, charge and output bins; timed unattended execution. Casting, PbS+Fe smelting, kiln firing and finite cooling. No general reduction or passive cooling. |
| Opening wilderness acquisition | Scouts forage, drink, steal and warn. Light, presence and boxes deter raids; stolen loot remains recoverable. Audible steps/splashes/hauling and running-triggered alarms. Clubs make nonlethal contact. Enemy strikes need fitted tools (owner scenario only). |
| Structures | Timed carried-stack posts/framing plus a 6.5 kg cover panel; activation occurs on completion. |

## Current integration frontier

Missing ordinary edges:

| Implemented side | Missing ordinary edge |
| --- | --- |
| Settlement batch foundry | Industrial-scale foundry acquisition, support, and high-power infrastructure. |
| Generated world + body + targeting | Inventory/crafting UI, placement, residency, richer contact and remote discovery. |
| Structural physics | Broader panels/joints, cascade demolition, and terrain support. |
| Player equipment service/recovery | Maintenance-tool requirements and service-site transport beyond exact-local carried stock. |
| Finite energy stores | Branching, gears, electrical distribution and conversion beyond straight timber shaft runs. |
| Finite fluid stores | Horizontal/remote transport, pumping, chemical mixing and pressure networks. |
| Industrial workshop/ore/foundry execution | Raw acquisition and player placement of powered workshop parts and located ore bins; multi-stage autonomous ore processing. |
| Checked coordinates + compact chunk ownership + geological evidence | Terrain access/collision, regional geology-to-chunk composition, and clue-location discovery. |

[`DIRECTION.md`](DIRECTION.md) owns which boundary should be closed next.

## Implemented infrastructure

These systems have authoritative owners and canonical paths even where ordinary acquisition is incomplete.

| Area | Implemented capability |
| --- | --- |
| Core | Persisted 20 Hz `ControlTick` + exact 72:1 `SimulationTick`; slow-step only when no control-rate consequence can occur. |
| Persistence | Current-schema trusted load with deterministic derived-index rebuild and whole-state validation; byte storage is adapter-owned. |
| Production | Timed conserved work, routed outputs, actor-local powered comminution batch choices with capacity-limited outputs and stale-cohort rejection, metal remelting/casting and iron upkeep. PbS+Fe yields lead and FeS matte. |
| Crafting | Timed exact-lot processing, including iron head finishing with recoverable chips, equipment admission and conserved outputs. |
| Logistics | 20 Hz body collision, carrying/load exertion, falling injury, equipment and inventory access, nine-slot hotbar, buffered actions. |
| Loose surface matter | Finite gathering and drops, empty-hand local gathering, capacity-limited pickup. Deep residue obstructs ground-storage footing. |
| Energy and fluids | Finite energy and losses, multi-cell straight timber shafts, campfire heat, drinking and local/downhill fluid transfer. |
| Storage recovery | Timed dismantling of material-backed storage with preservation checkpointing and exact body recovery. |
| Structures | Support/load/failure; timed posts, framing, cover and grounded timber roller; timed primary-break recovery. |
| Block world | Seeded ore, habitats, rooted forage, trees, springs, temples and excavation. Wild fruit withers and can ripen from finite roots; weather moves retained spring water. No full ecology or renderer. |
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
| Power networks | Shaft branches, belts, gearing, electrical distribution/protection, steam and transmission beyond straight contiguous timber roller runs. |
| Hydrology | General fluid transport, surface/ground water, channels, pumps, irrigation, wastewater, sanitation, chemical blending/contamination, and pressure-dependent behavior. |
| Ecology and society | Tree planting, agriculture, lethal combat, mobile armed pursuit, temple physiology, relics, recruitment, settlements, trade, migration, breached enclosures and extended raids. |
| Industrial acquisition | Ordinary acquisition for industrial machinery, industrial energy systems, and supporting infrastructure. |
| Save storage adapters | Save-file encoding/storage, filesystem atomicity, compression, and cloud storage. |

Implemented means an authoritative owner, canonical path, persistence semantics, invariants, and executable proof.
Ordinary reachability additionally requires a normal-play acquisition path.

# Status

This page is the authority for current runtime scope and reachability. Use [`README.md`](README.md) for
project routing, [`GAME_DESIGN.md`](GAME_DESIGN.md) for intended player experience, and
[`DIRECTION.md`](DIRECTION.md) for future integration priority. Source presence or controlled-harness execution
does not by itself make a capability ordinarily reachable.

## Ordinary play

Current progression:

`local clues -> coarse-to-fine prospecting -> physical sampling -> evidence-gated mining -> hand/primitive native-copper recovery -> copper upgrades -> reinforcement/recovery`

Primitive grinding, sizing, concentration, and scavenging are ordinarily executable from retained processing residue, but concentrate currently has no ordinary reduction/smelting sink. They therefore form a reachable integration frontier rather than a rational next step in the selected current-player loop.

| Area | Reachable capability |
| --- | --- |
| Survival and labor | Metabolic energy, hydration, vitality, three nutrition categories, perishability, timed consumption, exertion, and exclusive player work. Storage transitions preserve prior food age. Authored enclosures trade usable capacity, preservation strength, embodied material, and construction attention. |
| Materials and inventory | Typed commodities, exact composition/particle state, finite lots/stockpiles, reservations, provenance, preservation, temperature, deterministic coalescing, enclosures, and matter accounting. Enclosure dismantling is timed exclusive work that checkpoints exposure, restores ambient storage, and returns the exact body. Salvaged bodies rework through authored recovery routes. |
| Prospecting and knowledge | Reconnaissance, transects, and inspection are equipment-free. Detailed physical sampling requires an authored hammer and yields bounded abundance; definite presence also yields a conservative hardness band. A fully localized single body can additionally yield a conservative resource-mass band at authored resolution; partial or ambiguous bodies reveal no reserve estimate, and exact reserve/deposit identity remain hidden. Aggregate methods stay area-bounded, indexed evidence covers only surveyed cells, and an authored copper upgrade reduces sampling wear and unlocks an indexed multi-voxel survey. |
| Mining | Evidence-gated hand extraction requires acquired hardness evidence; admission uses its conservative upper bound and never reveals exact hidden resistance. Authored picks trade extraction rate, batch mass, and hardness limit; copper variants extend throughput while a lighter reinforced pick covers hard rock. |
| Crafting and equipment | Manual shaping/joinery, persistent equipment, wear, and exact-component service/recovery. Worn disassembly scraps only the wear component; other components stay intact. Boards retain a hand fallback; authored adzes and copper upgrades trade throughput, yield recovery, and attention. The saw bench requires an authored timber frame plus a replaceable copper blade and improves board recovery. A copper-free timber treadle forging hammer uses direct player labor to accelerate reinforcement shaping, scrap rework, and saw-blade cold-working without changing yield; copper screen perforation remains hand-only because no punch/die component is authored. Reinforcement extends extraction, sampling, woodworking, power, and processing equipment through authored routes. |
| Primitive power | Manual generation feeds lossy flywheels. Authored crank, treadle, and settlement walking-wheel providers trade build mass, metabolic efficiency, wear, and output power. Authored timber, stone, copper-banded, paired, and timber-framed stone-bank stores trade stored-work capacity, input limits, drag, and build mass. The 5 kJ settlement bank is deliberately sized to cover the largest authored settlement comminution batch while retaining the same short coast-time class as the smaller flywheels. All remain work buffers, not batteries. |
| Primitive processing | Portable crusher, quern, riddle/screens, and separator provide crush -> grind -> size -> regrind -> concentrate. Authored copper upgrades improve batch mass and throughput while preserving service components. Copper-era settlement equipment adds a timber-framed comminution mill that combines crushing and grinding, plus a timber ore-dressing table that combines sizing and gravity separation. Both trade substantially more embodied timber/stone for larger batches and consolidated capability while shared equipment occupancy preserves the lighter specialist machines as parallel-work alternatives. The matched settlement flywheel bank makes the comminution mill's full authored crushing, grinding, fine-grinding, and tailings-regrind batch envelope ordinarily powerable. First-pass tailings retain copper; finer regrinding enables one lower-grade scavenger pass at added work, energy, and wear, producing terminal exhausted tailings. |

## Current integration frontier

These are current graph boundaries, not future priorities. They identify where an otherwise implemented or
player-relevant flow stops today. [`DIRECTION.md`](DIRECTION.md) owns which boundary should be closed next.

| From | Missing edge | Current consequence |
| --- | --- | --- |
| Prepared ore / concentrate | reduction or smelting into pure metal | Prepared ore can be mechanically liberated and concentrated with material-backed primitive equipment, and native copper can be recovered, but concentrate cannot enter the foundry as foundry-ready pure copper. The foundry therefore remains a capability island rather than the continuation of the ore-preparation chain. |
| Local inventory custody | world-space carrying, haulage, delivery, access, and path cost | Matter can move through explicit local owner transitions, but there is no general player/world transport authority. Controlled harness delivery does not establish ordinary logistics. |
| Structural physics and material embodiment | ordinary player construction/deconstruction authorization | Structures can own conserved members, support, load, damage, and failure, but ordinary play cannot yet construct the general structural graph. |
| Physical equipment maintenance | world-space access and maintenance-tool requirements | Service already occupies exclusive player work, consumes authored survival exertion and replacement matter, and recovers condition only at completion; generic spatial access/tool authorization is still absent. |
| Finite energy stores | routed mechanical/electrical transmission or conversion | Stores have exact capacity, power limits, passive loss, and process integration, but no generic physical network moves energy between endpoints. |
| Finite fluid stores | routed transport, pumping, mixing, or pressure network | Stores have exact volume, temperature, withdrawal, and structural load, but fluid movement beyond canonical consumption/egress remains absent. |
| Capability-level industrial machinery and energy infrastructure | ordinary acquisition/construction routes | Workshop, ore-preparation, and foundry execution can be evaluated after controlled setup but their required industrial infrastructure is not normally obtainable. |
| Checked persistent coordinates and bounded geological evidence | runtime voxel world, clue-location discovery, terrain access | Spatial identity and evidence semantics exist without a world/chunk owner; controlled scenarios supply locations that ordinary runtime world generation cannot yet provide. |

## Implemented infrastructure

These systems have authoritative runtime owners and executable production paths even where ordinary acquisition
is incomplete.

| Area | Implemented capability |
| --- | --- |
| Core | Deterministic headless simulation, immutable validated registries, generated `AppState`, persisted RNG streams, typed time, checked integer physical quantities, and explicit tick order. |
| Persistence | Current schema only. Trusted load rebuilds derived indexes and validates the complete supported runtime graph. Encoding and storage are adapter concerns. |
| Production | Timed closed-mass jobs, exact selected inputs, reserved outputs, persisted work-in-process, multi-stream routing, revision-bound completion, and support-aware suspension/resume. |
| Energy and fluids | Finite typed-carrier energy stores with directional power limits and optional passive loss; finite homogeneous fluid stores with exact withdrawal and support-aware structural load. Generic inter-store transfer is absent. |
| Storage recovery | Material-backed stockpile enclosures have an exact timed dismantling action owned by exclusive player work, with checkpointed exposure, ambient-storage restoration, and exact body recovery. |
| Structures | Material-backed members, contact-constrained support topology, axial analysis, source-owned loads, damage, and failure cascades. General player construction is absent. |
| Spatial and presentation | Checked chunk-independent voxel coordinates, deterministic renderer-neutral texture baking, and deterministic WGSL assembly. No graphics backend is included. |

## Capability-only evaluation

These systems execute through canonical runtime paths after controlled gameplay-harness setup, but ordinary
play cannot yet acquire their required infrastructure.

| Surface | Evaluated capability |
| --- | --- |
| Workshop | Installed industrial machinery under finite stored work, survival pressure, wear, maintenance, structural support, suspension/recovery, and actor policy. |
| Ore preparation | Installed industrial crushing, grinding, screening, regrinding, and concentration with exact constituent accounting/tailings. The same physics are ordinarily reachable through slower primitive providers; this surface is the industrial throughput benchmark. |
| Foundry | Installed sensible heating, copper-bound pure-material melting/casting, remelting of ingot, reinforcement, native copper, and copper scrap, finite energy, equipment limits, phase boundaries, latent heat, heat recovery, and passive sink loss. Ore and concentrate still require an unimplemented reduction/smelting stage. |

## Absent scope

| Area | Boundary |
| --- | --- |
| Engine/platform | Graphics backend, window/input/audio integration, ECS, networking, platform integration, and general engine shell. |
| World representation | Voxel/chunk storage, terrain generation, streaming, world-scale spatial indexing, and runtime clue-location discovery. |
| Logistics | General transport authorization, pathing, carrying/haulage time, transport labor/energy, and world-space delivery. Controlled harness delivery is setup infrastructure only. |
| Advanced geology/mining | Regional generation, voxel ore topology, sampling/drilling/assays/geophysics, mechanized excavation, access, haulage, drainage, ground control, waste-rock handling, and tailings transport/impoundment. |
| Thermal/chemical industry | Environmental heat transport beyond explicit sink loss, vaporization, combustion, fuels/emissions, mixed/alloy phase behavior, reduction/smelting, alloying, forging, machining, broader separation, wood/stone chip recovery, and broader non-copper/non-stone scrap recycling. |
| Maintenance/structures | Maintenance tool requirements and world-space access, general structural construction/deconstruction, demolition/salvage physics, bending, shear, torsion, buckling, joints, and terrain support. |
| Power networks | Generic energy transfer, shafts/belts, inertia/slip/clutches, steam, electrical generation/distribution/protection, and spatial network integration. |
| Hydrology | Generic fluid transport, surface/ground water, channels, pumps, irrigation, wastewater, sanitation, mixing, and pressure-dependent fluid behavior. |
| Ecology and society | Agriculture, soil, ecology, genetics, creatures, hunting/combat, workers, settlements, trade, economy, and migration. |
| Industrial acquisition | Ordinary acquisition for industrial machines, industrial energy systems, and supporting infrastructure. |
| Save storage adapters | Save-file encoding/storage, filesystem atomicity, compression, and cloud storage. |

A capability is implemented only when it has an authoritative owner, canonical runtime path, required
persistence semantics, invariant coverage, and executable verification. Ordinary reachability additionally
requires an acquisition path available to normal play.

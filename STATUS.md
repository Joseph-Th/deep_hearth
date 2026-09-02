# Status

This page is the authority for current runtime scope and reachability. Use [`README.md`](README.md) for
project routing, [`GAME_DESIGN.md`](GAME_DESIGN.md) for intended player experience, and
[`DIRECTION.md`](DIRECTION.md) for future integration priority. Source presence or controlled-harness execution
does not by itself make a capability ordinarily reachable.

## Ordinary play

Current progression:

`local clues -> coarse-to-fine prospecting -> stone tools -> evidence-gated hand mining -> scarce-copper investment -> hand processing or primitive mechanization -> further reinforcement and recovery`

| Area | Reachable capability |
| --- | --- |
| Survival and labor | Metabolic energy, hydration, vitality, Grain/Fruit/Protein nutrition, perishability, timed eating/drinking, exertion, and exclusive player work. Storage transitions preserve prior food age. Timber choices span a 10 kg rough box at 1.25x, a 20 kg chest at 2x, a 50 kg bulk crate at 1.5x, a 20 kg double-wall chest at 3x, and an 8 kg pantry at 4x, costing 2/3/4/5/6 kg raw timber and 150/230/290/370/440 attention ticks. A 6 kg carved stone crock provides 2.5x preservation for 3 kg stone and 180 ticks, adding a timber-free small-reserve route. |
| Materials and inventory | Typed commodities, exact composition and particle state, finite stockpiles/lots, reservations, provenance, preservation, temperature, deterministic coalescing, storage enclosures, and represented-matter accounting. Preservation enclosures dismantle as timed exclusive work, checkpoint preservation exposure, restore ambient storage, and return their exact bodies. Detached timber bodies can be reused intact or salvaged to boards plus explicit wood-chip loss; the carved stone crock can be reused intact or salvaged to reworkable stone scrap. Pure stone service scrap can be reknapped into tooling plus chips; clean copper scrap can be cold-worked back into reinforcement. |
| Prospecting and knowledge | Regional reconnaissance, local transects, field inspection, and detailed survey produce persisted bounded evidence without exposing hidden deposits. |
| Mining | Evidence-gated hand extraction with hardness, batch, throughput, wear, reservation, and claim semantics. The heavy quarry pick trades more stone/timber for 35 g/s and 500 g soft-rock batches versus the starter pick's 20 g/s and 200 g, while both remain capped at 500 MPa. Its copper variant reaches 45 g/s, 750 g, and 600 MPa; the lighter reinforced pick remains the 750 MPa hard-rock specialist. |
| Crafting and equipment | Manual shaping/joinery, persistent assembly/upgrades, condition-dependent capability, occupancy, disassembly/recovery, and timed exact-component service. The same 20 g copper reinforcement can improve the stone pick, quarry pick, hand crank, crusher, or separator while preserving identity and prior wear; service retains reinforcement and worn disassembly returns copper scrap. Pure stone service scrap can be reknapped, and a copper-free timber treadle assembles from existing flywheel, board, and handle commodities. |
| Primitive power | Portable manual generation feeds finite mechanical stores with passive loss. The stone crank provides 50 W; the heavier copper-free timber treadle provides 100 W with better metabolic efficiency and lower wear; the copper crank remains the compact 150 W option. Storage choices are stone 500 J, copper-banded 750 J at 150 W input/0.05 W loss, or copper-free paired stone 1,000 J at 100 W input/0.1 W loss. Exact disassembly returns embodied material. |
| Primitive processing | Hand breaking and sorting provide a zero-machine ore-processing route. Crusher/separator equipment trades construction, stored work, and wear for higher throughput, better recovery, and returned player attention. Copper-reinforced variants increase both throughput and safe single-batch capacity; wear reduces both productive rate and safe batch size before failure. Unrecovered target material remains in residue; primitive sorting cannot be repeated on its own gangue-hosted residue. |

## Current integration frontier

These are current graph boundaries, not future priorities. They identify where an otherwise implemented or
player-relevant flow stops today. [`DIRECTION.md`](DIRECTION.md) owns which boundary should be closed next.

| From | Missing edge | Current consequence |
| --- | --- | --- |
| Prepared ore / concentrate | reduction or smelting into pure metal | Ordinary progression can recover native copper but cannot turn prepared copper-bearing ore or concentrate into foundry-ready pure copper. The foundry therefore remains a capability island rather than the continuation of the ore-preparation chain. |
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
| Storage recovery | Material-backed stockpile enclosures have an exact timed dismantling action owned by exclusive player work. Completion checkpoints current preservation exposure, restores ambient storage, and returns embodied traces to inventory with their material state and provenance intact; survival exertion is charged across the authored service interval. |
| Structures | Material-backed members, contact-constrained support topology, axial analysis, source-owned loads, damage, and failure cascades. General player construction is absent. |
| Spatial and presentation | Checked chunk-independent voxel coordinates, deterministic renderer-neutral texture baking, and deterministic WGSL assembly. No graphics backend is included. |

## Capability-only evaluation

These systems execute through canonical runtime paths after controlled gameplay-harness setup, but ordinary
play cannot yet acquire their required infrastructure.

| Surface | Evaluated capability |
| --- | --- |
| Workshop | Installed industrial machinery under finite stored work, survival pressure, wear, maintenance, structural support, suspension/recovery, and actor policy. |
| Ore preparation | Installed crushing, grinding, dry screening, regrinding, and copper concentration with exact constituent accounting and physical tailings. |
| Foundry | Installed sensible heating, copper-bound pure-material melting/casting, remelting of ingot, reinforcement, native copper, and copper scrap, finite energy, equipment limits, phase boundaries, latent heat, heat recovery, and passive sink loss. Ore and concentrate still require an unimplemented reduction/smelting stage. |

## Absent scope

| Area | Boundary |
| --- | --- |
| Engine/platform | Graphics backend, window/input/audio integration, ECS, networking, platform integration, and general engine shell. |
| World representation | Voxel/chunk storage, terrain generation, streaming, world-scale spatial indexing, and runtime clue-location discovery. |
| Logistics | General transport authorization, pathing, carrying/haulage time, transport labor/energy, and world-space delivery. Controlled harness delivery is setup infrastructure only. |
| Advanced geology/mining | Regional generation, voxel ore topology, sampling/drilling/assays/geophysics, mechanized excavation, access, haulage, drainage, ground control, waste-rock handling, and tailings transport/impoundment. |
| Thermal/chemical industry | Environmental heat transport beyond explicit sink loss, vaporization, combustion, fuels/emissions, mixed/alloy phase behavior, reduction/smelting, alloying, forging, machining, broader separation, wood/stone chip recovery, wood-scrap recovery, and broader non-copper/non-stone scrap recycling. |
| Maintenance/structures | Maintenance tool requirements and world-space access, general structural construction/deconstruction, demolition/salvage physics, bending, shear, torsion, buckling, joints, and terrain support. |
| Power networks | Generic energy transfer, shafts/belts, inertia/slip/clutches, steam, electrical generation/distribution/protection, and spatial network integration. |
| Hydrology | Generic fluid transport, surface/ground water, channels, pumps, irrigation, wastewater, sanitation, mixing, and pressure-dependent fluid behavior. |
| Ecology and society | Agriculture, soil, ecology, genetics, creatures, hunting/combat, workers, settlements, trade, economy, and migration. |
| Industrial acquisition | Ordinary acquisition for industrial machines, industrial energy systems, and supporting infrastructure. |
| Save storage adapters | Save-file encoding/storage, filesystem atomicity, compression, and cloud storage. |

A capability is implemented only when it has an authoritative owner, canonical runtime path, required
persistence semantics, invariant coverage, and executable verification. Ordinary reachability additionally
requires an acquisition path available to normal play.

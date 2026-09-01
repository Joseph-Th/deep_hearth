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
| Survival and labor | Metabolic energy, hydration, vitality, Grain/Fruit/Protein nutrition, food perishability, bounded timed eating/drinking, activity-dependent exertion, and exclusive player work. Food freshness can be projected read-only through one future authored storage transition, preserving exact prior exposure while making the candidate transition time and later assessment horizon explicit; construction legality remains separately validated. Timber can be shaped and joined into two material-backed provisions enclosures without changing prior food age: the lidded chest costs 3 kg raw timber and 230 attention ticks for 2x future preservation, while the double-wall chest costs 5 kg and 370 ticks for 3x preservation at the same 20 kg usable capacity. |
| Materials and inventory | Typed commodities, exact composition and particle state, finite stockpiles/lots, reservations, provenance, preservation, temperature, deterministic coalescing, storage enclosures, and represented-matter accounting. Detached timber enclosure bodies can be reused intact or manually salvaged back into boards plus explicit wood-chip residue, allowing ordinary storage to be reconfigured without deleting embodied timber. Pure stone working-component scrap can be reknapped in 1 kg batches into 0.8 kg reusable stone tooling plus 0.2 kg chips; mixed-temperature or contaminated scrap remains ineligible without the missing thermal/material owner. Clean copper scrap can be manually cold-worked back into reinforcement in ordinary play; the capability-level foundry can additionally remelt copper scrap and other pure-copper forms. |
| Prospecting and knowledge | Regional reconnaissance, local transects, field inspection, and detailed survey produce persisted bounded evidence without exposing hidden deposits. |
| Mining | Evidence-gated hand extraction with tool capability, hardness, batch and throughput limits, wear, reserved output capacity, geology-owned work-in-process, and explicit output claim. |
| Crafting and equipment | Manual shaping/joinery, persistent equipment, assembly, additive upgrades, condition-dependent capability, occupancy, installation, disassembly/recovery, maintenance, and exact component service. The same 20 g copper reinforcement can improve the stone pick, hand crank, toggle crusher, or rocking separator while preserving equipment identity and prior wear; component service preserves the reinforcement and worn disassembly returns it as copper scrap. Accumulated pure stone service scrap can re-enter the same working-component economy through slower manual reknapping, reducing repeated dependence on fresh stone lumps without making repair lossless or free. |
| Primitive power | Portable manual generation into finite mechanical storage. Stored mechanical work dissipates passively, so powered work plans from remaining stored energy. An empty, idle stone flywheel can accept the same 20 g copper reinforcement used by primitive equipment, preserving store identity and transfer behavior while increasing reserve capacity from 500 J to 750 J; exact disassembly returns the reinforcement for reuse. |
| Primitive processing | Hand breaking and sorting provide a zero-machine ore-processing route. Crusher/separator equipment trades construction, stored work, and wear for higher throughput, better recovery, and returned player attention. Copper-reinforced variants increase both throughput and safe single-batch capacity; wear reduces both productive rate and safe batch size before failure. Unrecovered target material remains in residue; primitive sorting cannot be repeated on its own gangue-hosted residue. |

## Current integration frontier

These are current graph boundaries, not future priorities. They identify where an otherwise implemented or
player-relevant flow stops today. [`DIRECTION.md`](DIRECTION.md) owns which boundary should be closed next.

| From | Missing edge | Current consequence |
| --- | --- | --- |
| Prepared ore / concentrate | reduction or smelting into pure metal | Ordinary progression can recover native copper but cannot turn prepared copper-bearing ore or concentrate into foundry-ready pure copper. The foundry therefore remains a capability island rather than the continuation of the ore-preparation chain. |
| Local inventory custody | world-space carrying, haulage, delivery, access, and path cost | Matter can move through explicit local owner transitions, but there is no general player/world transport authority. Controlled harness delivery does not establish ordinary logistics. |
| Structural physics and material embodiment | ordinary player construction/deconstruction authorization | Structures can own conserved members, support, load, damage, and failure, but ordinary play cannot yet construct the general structural graph. |
| Physical equipment maintenance | maintenance labor, tools, and access | Replacement material and equipment-condition transitions are modeled; the missing player-work/access layer means service itself is not yet a full world action. |
| Finite energy stores | routed mechanical/electrical transmission or conversion | Stores have exact capacity, power limits, passive loss, and process integration, but no generic physical network moves energy between endpoints. |
| Finite fluid stores | routed transport, pumping, mixing, or pressure network | Stores have exact volume, temperature, withdrawal, and structural load, but fluid movement beyond canonical consumption/egress remains absent. |
| Capability-level industrial machinery and energy infrastructure | ordinary acquisition/construction routes | Workshop, ore-preparation, and foundry execution can be evaluated after controlled setup but their required industrial infrastructure is not normally obtainable. |
| Conserved storage-enclosure dismantling | timed/labor-authorized player dismantling | The physical custody transition exists and preserves storage history/matter, but ordinary play does not yet pay or authorize the dismantling work itself. |
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
| Storage recovery | Material-backed stockpile enclosures have an exact conserved dismantling transition that checkpoints current preservation exposure, restores ambient storage, and returns embodied traces to inventory with their material state and provenance intact. This is physical transition authority, not yet a timed/labor-authorized player dismantling action. |
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
| Maintenance/structures | Maintenance labor/tools/access, general structural construction/deconstruction, demolition/salvage physics, bending, shear, torsion, buckling, joints, and terrain support. |
| Power networks | Generic energy transfer, shafts/belts, inertia/slip/clutches, steam, electrical generation/distribution/protection, and spatial network integration. |
| Hydrology | Generic fluid transport, surface/ground water, channels, pumps, irrigation, wastewater, sanitation, mixing, and pressure-dependent fluid behavior. |
| Ecology and society | Agriculture, soil, ecology, genetics, creatures, hunting/combat, workers, settlements, trade, economy, and migration. |
| Industrial acquisition | Ordinary acquisition for industrial machines, industrial energy systems, and supporting infrastructure. |
| Save storage adapters | Save-file encoding/storage, filesystem atomicity, compression, and cloud storage. |

A capability is implemented only when it has an authoritative owner, canonical runtime path, required
persistence semantics, invariant coverage, and executable verification. Ordinary reachability additionally
requires an acquisition path available to normal play.

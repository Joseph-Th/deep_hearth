# Status

This page is the authority for current runtime scope and reachability. Use [`README.md`](README.md) for
project routing, [`GAME_DESIGN.md`](GAME_DESIGN.md) for intended player experience, and
[`DIRECTION.md`](DIRECTION.md) for future integration priority. Source presence or controlled-harness execution
does not by itself make a capability ordinarily reachable.

## Ordinary play

Current progression:

`local clues -> coarse-to-fine prospecting -> physical sampling -> evidence-gated mining -> hand/primitive processing -> liberation/sizing/concentration -> copper upgrades -> reinforcement/recovery`

| Area | Reachable capability |
| --- | --- |
| Survival and labor | Metabolic energy, hydration, vitality, three nutrition categories, perishability, timed consumption, exertion, and exclusive player work. Storage transitions preserve prior food age. Timber preservation choices are rough box 10 kg/1.25x/2 kg/150t, chest 20 kg/2x/3 kg/230t, bulk crate 50 kg/1.5x/4 kg/290t, double-wall chest 20 kg/3x/5 kg/370t, and pantry 8 kg/4x/6 kg/440t. The stone crock is 6 kg/2.5x/3 kg stone/180t. |
| Materials and inventory | Typed commodities, exact composition/particle state, finite lots/stockpiles, reservations, provenance, preservation, temperature, deterministic coalescing, enclosures, and matter accounting. Enclosure dismantling is timed work that checkpoints exposure, restores ambient storage, and returns the exact body. Timber bodies salvage to boards plus chips; worn wood scrap can be lossily reworked into handle or board stock; the crock becomes stone scrap; stone scrap reknaps to tooling plus chips; clean copper scrap cold-works to reinforcement. |
| Prospecting and knowledge | Reconnaissance, transects, and inspection are equipment-free. Detailed physical sampling is 48t/voxel at 25,000 ppm and requires a 650 g hammer; definite target presence also yields a conservative 50 MPa hardness band. A 20 g copper upgrade preserves wear, halves sampling wear, and unlocks a 72t indexed four-voxel survey at the same resolution. Uncertain cells expose no hardness, aggregate methods stay area-bounded, indexed evidence covers only surveyed cells, and deposit identity remains hidden. |
| Mining | Evidence-gated hand extraction requires acquired hardness evidence; admission uses its conservative upper bound and never reveals exact hidden resistance. The heavy quarry pick provides 35 g/s and 500 g batches at 500 MPa versus the starter pick's 20 g/s and 200 g; its copper variant reaches 45 g/s, 750 g, and 600 MPa, while the lighter reinforced pick remains the 750 MPa hard-rock specialist. |
| Crafting and equipment | Manual shaping/joinery, persistent equipment, wear, and exact-component service/recovery. Worn disassembly scraps only the wear component; other components stay intact. Boards retain a 50t hand fallback; a 1 kg stone adze gives 10 g/s at the same 800/200 board/chip yield, doubled by a 20 g copper upgrade. A 1.854 kg saw bench uses 1.6 kg boards, a 200 g timber member, and a replaceable 54 g copper blade; 60 g native copper leaves 6 g scrap. Its required 40 g/s route yields 900 g boards +100 g chips per 1 kg log. Reinforcement also upgrades the pick, quarry pick, geological hammer, crank, crusher, separator, and quern; a blank can instead become an 18 g screen plate +2 g scrap. |
| Primitive power | Manual generation feeds lossy flywheels. Stone crank: 50 W; all-timber treadle: 100 W with better metabolic efficiency; copper crank: 150 W. Storage is timber 300 J/100 W input, stone 500 J/150 W input, copper-banded stone 750 J/150 W input, or paired stone 1,000 J/150 W input. Timber trades bulk material for stone; stone stores more work per build mass. All remain short work buffers, not batteries. |
| Primitive processing | Portable crusher, quern, riddle/screens, and separator provide crush -> grind -> size -> regrind -> concentrate. The copper-free 1.6 kg timber riddle handles 250 g at 1.25 g/s with a replaceable 1.4 kg panel; an 18 g copper plate upgrades the same tool to 500 g at 2.5 g/s while preserving that service component. The quern upgrades with 20 g copper from 1.2 to 1.8 g/s and 500 to 750 g. First-pass tailings retain copper; finer regrinding enables one lower-grade scavenger pass at added work, energy, and wear, producing terminal exhausted tailings. |

## Current integration frontier

These are current graph boundaries, not future priorities. They identify where an otherwise implemented or
player-relevant flow stops today. [`DIRECTION.md`](DIRECTION.md) owns which boundary should be closed next.

| From | Missing edge | Current consequence |
| --- | --- | --- |
| Prepared ore / concentrate | reduction or smelting into pure metal | Ordinary progression can now mechanically liberate and concentrate copper-bearing ore with material-backed primitive equipment, in addition to recovering native copper, but cannot reduce or smelt that concentrate into foundry-ready pure copper. The foundry therefore remains a capability island rather than the continuation of the ore-preparation chain. |
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

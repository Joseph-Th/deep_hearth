# Status

This page is the authority for current runtime scope and reachability. Use [`README.md`](README.md) for
project routing, [`GAME_DESIGN.md`](GAME_DESIGN.md) for intended player experience, and
[`DIRECTION.md`](DIRECTION.md) for future integration priority. Source presence or controlled-harness execution
does not by itself make a capability ordinarily reachable.

## Ordinary play

Current progression:

`local clues -> coarse-to-fine prospecting -> physical sampling -> evidence-gated mining -> hand/tool-assisted native-copper dressing -> primitive mechanization -> copper upgrades -> settlement mechanization -> reinforcement/recovery`

Primitive grinding, sizing, concentration, scavenging, and final concentrate cleaning are ordinary. Authored ore represents liberated copper as elemental copper, so rich concentrate mechanically cleans to `FORM_NATIVE_METAL`; current copper needs no reduction step. The frontier is ordinary foundry infrastructure.

| Area | Reachable capability |
| --- | --- |
| Survival and labor | Metabolic energy, hydration, vitality, three nutrition categories, perishability, timed consumption, exertion, and exclusive player work. Storage transitions preserve prior food age. Authored enclosures trade usable capacity, preservation strength, embodied material, and construction attention. |
| Materials and inventory | Typed commodities, exact composition/particle state, finite lots/stockpiles, reservations, provenance, preservation, temperature, deterministic coalescing, enclosures, and matter accounting. Enclosure dismantling is timed exclusive work that checkpoints exposure, restores ambient storage, and returns the exact body. Salvaged bodies rework through authored recovery routes. |
| Prospecting and knowledge | Reconnaissance, transects, and inspection are equipment-free. Detailed physical sampling requires an authored hammer and yields bounded abundance; definite presence also yields a conservative hardness band. A fully localized single body can additionally yield a conservative resource-mass band at authored resolution; partial or ambiguous bodies reveal no reserve estimate, and exact reserve/deposit identity remain hidden. Aggregate methods stay area-bounded, indexed evidence covers only surveyed cells, and an authored copper upgrade reduces sampling wear and unlocks an indexed multi-voxel survey. |
| Mining | Evidence-gated hand extraction requires acquired hardness evidence; admission uses its conservative upper bound and never reveals exact hidden resistance. Authored picks trade extraction rate, batch mass, and hardness limit; copper variants extend throughput while a lighter reinforced pick covers hard rock. |
| Crafting and equipment | Manual shaping/joinery has persistent wear and exact-component service/recovery. Boards retain a hand fallback; adzes/frame saw trade attention and recovery, while the treadle hammer speeds copper working. Dedicated lathe and grindstone branches speed round timber and improve stone-service recovery; powered upgrades delegate both from finite work. A stone-flywheel pump drill is required for copper sizing plates and wears a recoverable stone bit. Its timber spindle upgrade reuses the worn drill and can spend finite mechanical work to pierce plates without player attention; sash sawmill and helve hammer similarly upgrade earlier tools. |
| Primitive power | Manual generation feeds lossy flywheels. Crank, treadle, and walking-wheel providers trade build mass, metabolic efficiency, wear, and output. Timber, stone, copper-banded, paired, and settlement-bank stores trade capacity, input limits, drag, and build mass. The same finite stores feed ordinary sawmill, helve, drill, lathe, and toolroom grinding directly; no shaft/belt network is implied. A 480 J rotor or 270 J service cycle fits the 500 J stone flywheel. The 5 kJ bank covers the largest authored settlement comminution batch. All remain work buffers, not batteries. |
| Primitive processing | Bare hands remain the zero-investment dressing fallback. A stone cobbing hammer speeds breaking; a timber cobbing/picking bench speeds breaking and visible-copper sorting. Both are wearing direct-labor tools and do not change recovery. Crusher, quern, riddle/screens, and separator provide crush -> grind -> size -> regrind -> concentrate with finite mechanical work and higher sustained throughput. Copper upgrades improve batch mass and throughput while preserving service components. Settlement equipment combines crushing/grinding in a timber comminution mill and sizing/gravity separation in an ore-dressing table; heavier embodied construction buys larger batches and consolidated capability while specialist machines retain parallel-work value. The settlement flywheel bank powers the mill's full authored batch envelope. Finer tailings regrinding enables one lower-grade scavenger pass at added work, energy, and wear, producing terminal exhausted tailings. |

## Current integration frontier

These are current graph boundaries, not future priorities. They identify where an otherwise implemented or
player-relevant flow stops today. [`DIRECTION.md`](DIRECTION.md) owns which boundary should be closed next.

| From | Missing edge | Current consequence |
| --- | --- | --- |
| Ordinary native copper | ordinary foundry infrastructure and electrical supply | Native copper is valid cold-work/melting feed, but foundry hardware and electrical energy are unobtainable. Ordinary power is mechanical and no mechanical-to-electrical conversion path exists. The furnace's 2 MW heating capability is a transfer ceiling, not a minimum operating floor. |
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
| Core | Deterministic headless simulation, immutable validated registries, generated `AppState`, typed time, checked integer physical quantities, and explicit tick order. No runtime stochastic owner is currently implemented. |
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
| Foundry | Installed pure-copper heating/melting/casting, remelting, finite energy, equipment limits, phase boundaries, latent heat, heat recovery, and sink loss. Ordinary cleanup already supplies accepted native-copper feed; capability-only because foundry infrastructure is not ordinarily obtainable. Reduction remains absent for future compound ores. |

## Absent scope

| Area | Boundary |
| --- | --- |
| Engine/platform | Graphics backend, window/input/audio integration, ECS, networking, platform integration, and general engine shell. |
| World representation | Voxel/chunk storage, terrain generation, streaming, world-scale spatial indexing, and runtime clue-location discovery. |
| Logistics | General transport authorization, pathing, carrying/haulage time, transport labor/energy, and world-space delivery. Controlled harness delivery is setup infrastructure only. |
| Advanced geology/mining | Regional generation, voxel ore topology, sampling/drilling/assays/geophysics, mechanized excavation, access, haulage, drainage, ground control, waste-rock handling, and tailings transport/impoundment. |
| Thermal/chemical industry | Environmental heat transport beyond explicit sink loss, vaporization, combustion, fuels/emissions, mixed/alloy phase behavior, reduction/smelting, alloying, forging, machining, broader separation, broader wood/stone chip recovery, and broader non-copper/non-stone scrap recycling. |
| Maintenance/structures | Maintenance tool requirements and world-space access, general structural construction/deconstruction, demolition/salvage physics, bending, shear, torsion, buckling, joints, and terrain support. |
| Power networks | Generic energy transfer, shafts/belts, inertia/slip/clutches, steam, electrical generation/distribution/protection, and spatial network integration. |
| Hydrology | Generic fluid transport, surface/ground water, channels, pumps, irrigation, wastewater, sanitation, mixing, and pressure-dependent fluid behavior. |
| Ecology and society | Agriculture, soil, ecology, genetics, creatures, hunting/combat, workers, settlements, trade, economy, and migration. |
| Industrial acquisition | Ordinary acquisition for industrial machines, industrial energy systems, and supporting infrastructure. |
| Save storage adapters | Save-file encoding/storage, filesystem atomicity, compression, and cloud storage. |

A capability is implemented only when it has an authoritative owner, canonical runtime path, required
persistence semantics, invariant coverage, and executable verification. Ordinary reachability additionally
requires an acquisition path available to normal play.

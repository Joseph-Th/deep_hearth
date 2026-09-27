# Status

This page owns runtime scope and reachability. Use [`README.md`](README.md) for routing,
[`GAME_DESIGN.md`](GAME_DESIGN.md) for player experience, and [`DIRECTION.md`](DIRECTION.md) for future
priority. Source presence or controlled setup does not prove ordinary reachability.

## Ordinary play

Current progression:

`local clues -> prospecting -> direct or sampled mining -> native-copper dressing -> primitive mechanization -> copper upgrades -> settlement mechanization -> reinforcement/recovery -> first electrical foundry casting/recovery -> settlement batch foundry`

Primitive grinding, sizing, concentration, scavenging, and concentrate cleaning are ordinary. Rich elemental-copper concentrate cleans to `FORM_NATIVE_METAL` without reduction. A copper-wound treadle, finite electrical buffer, stone crucible, and mold provide the first low-throughput melt/cast recovery; ingot cold-works to reinforcement, while native copper is cheaper for small orders. Worked copper can then be hammered into conductor winding or, after investing in a stone-die timber drawbench, drawn with lower attention and no trimming loss. A flywheel drawbench upgrade preserves that lossless transform while delegating repeated drawing to finite stored mechanical work. Conductor stock supports an additive 150 W double-wound treadle dynamo, 60 kJ electrical buffer, four-pot 80 g crucible furnace, four-cavity 80 g mold, and 60 kJ thermal sink. That settlement package reduces repeated charging and batch-handling starts without adding a generic power network. Industrial foundry scale remains frontier.

| Area | Reachable capability |
| --- | --- |
| Survival and labor | Metabolic energy, hydration, vitality, nutrition, perishability, timed consumption, exertion, and exclusive player work. Storage transitions preserve prior food age. Stone/timber preservation is ordinary early infrastructure; a small copper-banded crock is an ordinary post-copper option that trades worked metal for stronger preservation rather than more capacity. |
| Materials and inventory | Typed commodities, exact lot state, finite stockpiles/reservations, provenance, preservation, temperature, enclosures, recovery, and matter accounting. |
| Prospecting and knowledge | Equipment-free reconnaissance plus physical sampling with bounded evidence; exact reserve and deposit identity remain hidden. A copper-reinforced hammer can perform indexed per-voxel channel work. A timber/copper channel-sampling frame produces tighter aggregate grade evidence across a broader exposure but deliberately supplies neither local hardness nor reserve scale. A heavier hand-driven tripod core drill provides slower single-voxel confirmation with tighter abundance, hardness, and resource-scale bands. |
| Mining | Localized targets can be attempted unsampled; sampling exposes a conservative hardness band before commitment. Picks trade hardness reach, batch mass, and rate. |
| Crafting and equipment | Manual shaping/joinery, wear, service, recovery, primitive machine branches, powered upgrades, lossless specialist wire drawing with a finite-work flywheel upgrade, conductor-winding fabrication, field instrumentation, and settlement batch-foundry upgrades are ordinary through the copper/settlement workshop line. |
| Primitive power | Crank, treadle, walking wheel, finite mechanical stores, the first copper-wound treadle dynamo, and its 150 W double-wound upgrade are ordinary direct providers. No shaft/belt/wiring network is implied. |
| Primitive processing | Hand dressing through primitive crushing, grinding, sizing, regrinding, concentration, scavenging, and settlement comminution/dressing are ordinary with finite work and wear. |

## Current integration frontier

These are current graph boundaries, not future priorities. They identify where an otherwise implemented or
player-relevant flow stops today. [`DIRECTION.md`](DIRECTION.md) owns which boundary should be closed next.

| From | Missing edge | Current consequence |
| --- | --- | --- |
| Settlement batch foundry | industrial-scale foundry acquisition/support/high-power supply | Ordinary play reaches both portable low-throughput copper melt/cast recovery and an additive 80 g settlement batch line; this does not make industrial foundry infrastructure ordinary. |
| Player/world custody | movement, haulage, delivery, path cost, and ordinary world-source acquisition | Logistics persists player, stockpile, equipment, energy-store, and fluid-store locations and requires exact local custody for direct player actions. General movement/transport and gathering remain absent. |
| Structural physics and material embodiment | ordinary player construction/deconstruction authorization | Structures can own conserved members, support, load, damage, and failure, but ordinary play cannot yet construct the general structural graph. |
| Physical equipment maintenance | maintenance-tool requirements and transport to service sites | Service enforces exact-local equipment/material endpoints for both mounted and unmounted equipment; maintenance tools and general transport remain absent. |
| Finite energy stores | routed mechanical/electrical transmission or conversion | Stores have exact capacity, power limits, passive loss, and process integration, but no generic physical network moves energy between endpoints. |
| Finite fluid stores | routed transport, pumping, mixing, or pressure network | Stores have exact volume, temperature, withdrawal, and structural load, but fluid movement beyond canonical consumption/egress remains absent. |
| Capability-level industrial machinery and energy infrastructure | ordinary acquisition/construction routes | Controlled setup can evaluate industrial workshop, ore, and foundry execution; ordinary acquisition is absent. |
| Checked coordinates and bounded geological evidence | runtime voxel world, clue discovery, terrain access | Spatial identity/evidence exist without a world/chunk owner; controlled scenarios still supply locations. |

## Implemented infrastructure

These systems have authoritative runtime owners and executable production paths even where ordinary acquisition
is incomplete.

| Area | Implemented capability |
| --- | --- |
| Core | Deterministic headless simulation, immutable validated registries, generated `AppState`, typed time, checked integer physical quantities, and explicit tick order. No runtime stochastic owner is currently implemented. |
| Persistence | Current schema only. Trusted load rebuilds derived indexes and validates the complete supported runtime graph. Encoding and storage are adapter concerns. |
| Production | Timed closed-mass jobs, exact inputs, reserved/routed outputs, persisted work-in-process, support-aware suspension/resume, and same-site admission for explicitly located endpoints. |
| Logistics | Persistent player/carried custody plus stockpile, equipment, energy-store, and fluid-store voxels; support-independent stockpile/equipment locations; same-voxel pickup/drop; exact-local player access; production-site coherence; and trusted-load replay of live spatial obligations. Located stockpiles, equipment, and fluid stores must remain inside any assigned structural support. |
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
| Foundry | Installed industrial pure-copper heating/melting/casting, remelting, finite energy, phase boundaries, heat recovery, and sink loss remain the capability-only throughput benchmark. Ordinary play separately reaches the portable first electrical foundry and its additive 80 g settlement batch upgrades through the same owners. Reduction remains absent for compound ores. |

## Absent scope

| Area | Boundary |
| --- | --- |
| Engine/platform | Graphics backend, window/input/audio integration, ECS, networking, platform integration, and general engine shell. |
| World representation | Voxel/chunk storage, terrain generation, streaming, world-scale spatial indexing, and runtime clue-location discovery. |
| Logistics | Player movement/pathing, haulage cost, general delivery/transport, container/hotbar presentation, ordinary resource-source acquisition, fluid transport, mounted-production site geometry, and mounted-to-mounted equipment transport. Local custody/access and explicitly located production-site coherence are implemented. |
| Advanced geology/mining | Regional generation, voxel ore topology, deep drilling beyond the ordinary shallow portable core survey, laboratory assays, geophysics, mechanized excavation, access, haulage, drainage, ground control, waste-rock handling, and tailings transport/impoundment. |
| Thermal/chemical industry | Environmental heat transport beyond explicit sink loss, vaporization, combustion, fuels/emissions, mixed/alloy phase behavior, reduction/smelting, alloying, forging, machining, broader separation, broader wood/stone chip recovery, and broader non-copper/non-stone scrap recycling. |
| Maintenance/structures | Maintenance tool requirements, general structural construction/deconstruction, demolition/salvage physics, bending, shear, torsion, buckling, joints, and terrain support. Equipment maintenance already enforces exact-local equipment/material access, including mounted equipment whose persisted location lies on its support. |
| Power networks | Generic transfer, shafts/belts, inertia/slip/clutches, steam, scalable electrical generation/distribution/protection, and spatial integration. The treadle dynamo is a direct provider, not a network. |
| Hydrology | Generic fluid transport, surface/ground water, channels, pumps, irrigation, wastewater, sanitation, mixing, and pressure-dependent fluid behavior. |
| Ecology and society | Agriculture, soil, ecology, genetics, creatures, hunting/combat, workers, settlements, trade, economy, and migration. |
| Industrial acquisition | Ordinary acquisition for industrial machines, industrial energy systems, and supporting infrastructure. |
| Save storage adapters | Save-file encoding/storage, filesystem atomicity, compression, and cloud storage. |

A capability is implemented only when it has an authoritative owner, canonical runtime path, required
persistence semantics, invariant coverage, and executable verification. Ordinary reachability additionally
requires an acquisition path available to normal play.

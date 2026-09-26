# Status

This page owns runtime scope and reachability. Use [`README.md`](README.md) for routing,
[`GAME_DESIGN.md`](GAME_DESIGN.md) for player experience, and [`DIRECTION.md`](DIRECTION.md) for future
priority. Source presence or controlled setup does not prove ordinary reachability.

## Ordinary play

Current progression:

`local clues -> prospecting -> direct or sampled mining -> native-copper dressing -> primitive mechanization -> copper upgrades -> settlement mechanization -> reinforcement/recovery -> first electrical foundry casting/recovery`

Primitive grinding, sizing, concentration, scavenging, and concentrate cleaning are ordinary. Rich elemental-copper concentrate cleans to `FORM_NATIVE_METAL` without reduction. A copper-wound treadle upgrade, finite energy stores, stone crucible, and mold provide 20 g melt/cast recovery; ingot cold-works to reinforcement, while native copper is cheaper for small orders. Industrial foundry scale remains frontier.

| Area | Reachable capability |
| --- | --- |
| Survival and labor | Metabolic energy, hydration, vitality, nutrition, perishability, timed consumption, exertion, and exclusive player work. Storage transitions preserve prior food age. |
| Materials and inventory | Typed commodities, exact lot state, finite stockpiles/reservations, provenance, preservation, temperature, enclosures, recovery, and matter accounting. |
| Prospecting and knowledge | Equipment-free reconnaissance plus physical sampling with bounded abundance/hardness evidence; exact reserve and deposit identity remain hidden. Indexed survey is an authored copper-era upgrade. |
| Mining | Localized targets can be attempted unsampled; sampling exposes a conservative hardness band before commitment. Picks trade hardness reach, batch mass, and rate. |
| Crafting and equipment | Manual shaping/joinery, wear, service, recovery, primitive machine branches, and powered upgrades are ordinary through the copper/settlement workshop line. |
| Primitive power | Crank, treadle, walking wheel, finite mechanical stores, and the copper-wound treadle electrical upgrade are ordinary direct providers. No shaft/belt/wiring network is implied. |
| Primitive processing | Hand dressing through primitive crushing, grinding, sizing, regrinding, concentration, scavenging, and settlement comminution/dressing are ordinary with finite work and wear. |

## Current integration frontier

These are current graph boundaries, not future priorities. They identify where an otherwise implemented or
player-relevant flow stops today. [`DIRECTION.md`](DIRECTION.md) owns which boundary should be closed next.

| From | Missing edge | Current consequence |
| --- | --- | --- |
| First foundry casting | industrial-scale foundry acquisition/support/high-power supply | Ordinary play reaches 20 g copper melt/cast recovery; the portable 100 W route does not make 2 MW-class industrial foundry infrastructure ordinary. |
| Player/world custody | movement, haulage, delivery, path cost, and ordinary world-source acquisition | Logistics persists player, stockpile, equipment, energy-store, and fluid-store locations and rejects known-remote direct actions. General movement/transport and gathering remain absent. |
| Structural physics and material embodiment | ordinary player construction/deconstruction authorization | Structures can own conserved members, support, load, damage, and failure, but ordinary play cannot yet construct the general structural graph. |
| Physical equipment maintenance | mounted-equipment site access and maintenance-tool requirements | Detached service enforces known-local equipment/material endpoints; mounted-site/tool rules remain absent. |
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
| Logistics | Persistent player/carried custody plus stockpile, detached-equipment, energy-store, and fluid-store voxels; same-voxel pickup/drop; known-remote access rejection; production-site coherence; and trusted-load replay of live spatial obligations. Fluid locations also constrain drinking and structural support. |
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
| Foundry | Installed industrial pure-copper heating/melting/casting, remelting, finite energy, phase boundaries, heat recovery, and sink loss remains the capability-only throughput benchmark. Ordinary play separately reaches a portable 20 g electrical foundry through the same owners. Reduction remains absent for future compound ores. |

## Absent scope

| Area | Boundary |
| --- | --- |
| Engine/platform | Graphics backend, window/input/audio integration, ECS, networking, platform integration, and general engine shell. |
| World representation | Voxel/chunk storage, terrain generation, streaming, world-scale spatial indexing, and runtime clue-location discovery. |
| Logistics | Player movement/pathing, haulage cost, general delivery/transport, container/hotbar presentation, ordinary resource-source acquisition, fluid transport, mounted-production site geometry, and mounted-to-mounted equipment transport. Local custody/access and explicitly located production-site coherence are implemented. |
| Advanced geology/mining | Regional generation, voxel ore topology, sampling/drilling/assays/geophysics, mechanized excavation, access, haulage, drainage, ground control, waste-rock handling, and tailings transport/impoundment. |
| Thermal/chemical industry | Environmental heat transport beyond explicit sink loss, vaporization, combustion, fuels/emissions, mixed/alloy phase behavior, reduction/smelting, alloying, forging, machining, broader separation, broader wood/stone chip recovery, and broader non-copper/non-stone scrap recycling. |
| Maintenance/structures | Maintenance tool requirements and mounted-equipment structural-site access, general structural construction/deconstruction, demolition/salvage physics, bending, shear, torsion, buckling, joints, and terrain support. Detached equipment maintenance already enforces known-local equipment/material access. |
| Power networks | Generic transfer, shafts/belts, inertia/slip/clutches, steam, scalable electrical generation/distribution/protection, and spatial integration. The treadle dynamo is a direct provider, not a network. |
| Hydrology | Generic fluid transport, surface/ground water, channels, pumps, irrigation, wastewater, sanitation, mixing, and pressure-dependent fluid behavior. |
| Ecology and society | Agriculture, soil, ecology, genetics, creatures, hunting/combat, workers, settlements, trade, economy, and migration. |
| Industrial acquisition | Ordinary acquisition for industrial machines, industrial energy systems, and supporting infrastructure. |
| Save storage adapters | Save-file encoding/storage, filesystem atomicity, compression, and cloud storage. |

A capability is implemented only when it has an authoritative owner, canonical runtime path, required
persistence semantics, invariant coverage, and executable verification. Ordinary reachability additionally
requires an acquisition path available to normal play.

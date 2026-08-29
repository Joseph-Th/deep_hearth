# Status

This page is the authority for current runtime scope and reachability. Use [`README.md`](README.md) for
project routing and [`GAME_DESIGN.md`](GAME_DESIGN.md) for intended player experience. Source presence or
controlled-harness execution does not by itself make a capability ordinarily reachable.

## Ordinary play

Current progression:

`local clues -> coarse-to-fine prospecting -> stone tools -> evidence-gated hand mining -> scarce-copper choice -> hand processing or primitive mechanization -> second reinforcement`

| Area | Reachable capability |
| --- | --- |
| Survival and labor | Metabolic energy, hydration, vitality, Grain/Fruit/Protein nutrition, food perishability, bounded timed eating/drinking, activity-dependent exertion, and exclusive player work. Timber can be shaped and joined into a material-backed provisions chest that improves future preservation without changing prior food age. |
| Materials and inventory | Typed commodities, exact composition and particle state, finite stockpiles/lots, reservations, provenance, preservation, temperature, deterministic coalescing, storage enclosures, and represented-matter accounting. Current-tier chips and worn-component scrap remain conserved but have no recovery route. |
| Prospecting and knowledge | Regional reconnaissance, local transects, field inspection, and detailed survey produce persisted bounded evidence without exposing hidden deposits. |
| Mining | Evidence-gated hand extraction with tool capability, hardness, batch and throughput limits, wear, reserved output capacity, geology-owned work-in-process, and explicit output claim. |
| Crafting and equipment | Manual shaping/joinery, persistent equipment, assembly, additive upgrades, condition-dependent capability, occupancy, installation, disassembly/recovery, maintenance, and exact component service. |
| Primitive power | Portable manual generation into finite mechanical storage. Stored mechanical work dissipates passively, so powered work plans from remaining stored energy. |
| Primitive processing | Hand breaking and sorting provide a zero-machine ore-processing route. Crusher/separator equipment trades construction, stored work, and wear for higher throughput, better recovery, and returned player attention. Unrecovered target material remains in residue; primitive sorting cannot be repeated on its own gangue-hosted residue. |

## Implemented infrastructure

These systems have authoritative runtime owners and executable production paths even where ordinary acquisition
is incomplete.

| Area | Implemented capability |
| --- | --- |
| Core | Deterministic headless simulation, immutable validated registries, generated `AppState`, persisted RNG streams, typed time, checked integer physical quantities, and explicit tick order. |
| Persistence | Current schema only. Trusted load rebuilds derived indexes and validates the complete supported runtime graph. Encoding and storage are adapter concerns. |
| Production | Timed closed-mass jobs, exact selected inputs, reserved outputs, persisted work-in-process, multi-stream routing, revision-bound completion, and support-aware suspension/resume. |
| Energy and fluids | Finite typed-carrier energy stores with directional power limits and optional passive loss; finite homogeneous fluid stores with exact withdrawal and support-aware structural load. Generic inter-store transfer is absent. |
| Structures | Material-backed members, contact-constrained support topology, axial analysis, source-owned loads, damage, and failure cascades. General player construction is absent. |
| Spatial and presentation | Checked chunk-independent voxel coordinates, deterministic renderer-neutral texture baking, and deterministic WGSL assembly. No graphics backend is included. |

## Capability-only evaluation

These systems execute through canonical runtime paths after controlled gameplay-harness setup, but ordinary
play cannot yet acquire their required infrastructure.

| Surface | Evaluated capability |
| --- | --- |
| Workshop | Installed industrial machinery under finite stored work, survival pressure, wear, maintenance, structural support, suspension/recovery, and actor policy. |
| Ore preparation | Installed crushing, grinding, dry screening, regrinding, and copper concentration with exact constituent accounting and physical tailings. |
| Foundry | Installed sensible heating, pure-material melting/casting, finite energy, equipment limits, phase boundaries, latent heat, heat recovery, and passive sink loss. |

## Absent scope

| Area | Boundary |
| --- | --- |
| Engine/platform | Graphics backend, window/input/audio integration, ECS, networking, platform integration, and general engine shell. |
| World representation | Voxel/chunk storage, terrain generation, streaming, world-scale spatial indexing, and runtime clue-location discovery. |
| Logistics | General transport authorization, pathing, carrying/haulage time, transport labor/energy, and world-space delivery. Controlled harness delivery is setup infrastructure only. |
| Advanced geology/mining | Regional generation, voxel ore topology, sampling/drilling/assays/geophysics, mechanized excavation, access, haulage, drainage, ground control, waste-rock handling, and tailings transport/impoundment. |
| Thermal/chemical industry | Environmental heat transport beyond explicit sink loss, vaporization, combustion, fuels/emissions, mixed/alloy phase behavior, reduction/smelting, alloying, forging, machining, broader separation, and chip/scrap recycling. |
| Maintenance/structures | Maintenance labor/tools/access, general structural construction/deconstruction, demolition/salvage physics, bending, shear, torsion, buckling, joints, and terrain support. |
| Power networks | Generic energy transfer, shafts/belts, inertia/slip/clutches, steam, electrical generation/distribution/protection, and spatial network integration. |
| Hydrology | Generic fluid transport, surface/ground water, channels, pumps, irrigation, wastewater, sanitation, mixing, and pressure-dependent fluid behavior. |
| Ecology and society | Agriculture, soil, ecology, genetics, creatures, hunting/combat, workers, settlements, trade, economy, and migration. |
| Industrial acquisition | Ordinary acquisition for industrial machines, industrial energy systems, and supporting infrastructure. |
| Save storage adapters | Save-file encoding/storage, filesystem atomicity, compression, and cloud storage. |

A capability is implemented only when it has an authoritative owner, canonical runtime path, required
persistence semantics, invariant coverage, and executable verification. Ordinary reachability additionally
requires an acquisition path available to normal play.

# Status

This document answers one question: what capability exists in the current runtime? Use
[`README.md`](README.md) for repository routing, [`ARCHITECTURE.md`](ARCHITECTURE.md) for engineering law,
[`TECHNICAL_DESIGN.md`](TECHNICAL_DESIGN.md) for implemented contracts, and
[`GAME_DESIGN.md`](GAME_DESIGN.md) for intended future gameplay.

## Implemented

| Area | Current runtime boundary |
| --- | --- |
| Core simulation | Headless deterministic Rust simulation; immutable versioned registries; `AppState` runtime ownership; persisted independent RNG streams; typed time/calendar; checked integer physical quantities; explicit tick order. |
| Persistence | Current save schema only. Load rebuilds derived indexes and validates local plus cross-owner invariants before returning state. Encoding and filesystem storage are adapter concerns. |
| Materials | Typed materials/forms with physical properties, phase, composition, particle state, provenance, temperature, and freshness/exposure where applicable. Pure-material fusion temperature and latent heat are modeled. |
| Inventory | Finite stockpiles, exact lots, capacity/containment/preservation, inbound reservations, deterministic routing/coalescing, opaque-authorized material transfer, supported-stockpile structural load, exact represented matter accounting. |
| Geology | Finite persisted deposits with spatial bounds, material profile, excavation hardness, provenance, remaining mass, and depletion. Hidden deposit truth is not exposed through player-facing knowledge. |
| Prospecting knowledge | Persisted acquired observations with spatial evidence and bounded abundance estimates; deterministic assessment of overlapping, contradictory, and disjoint evidence. A survival-costed one-voxel field inspection now acquires uncertain surface evidence through exclusive player labor; advanced sampling/survey methods remain future work. |
| Mining | Evidence-gated hand mining with opaque geological targets, real tools, exclusive player labor, throughput/batch/hardness capability, wear, destination reservation, in-flight ownership, and explicit output claim. Hidden deposit identity, exact remaining mass, exact target hardness, and pre-claim output composition are not player-facing mining inputs. |
| Production | Deterministic timed closed-mass jobs with exact selected inputs, reserved output capacity, persisted in-flight matter/energy, typed multi-stream routing, revision-bound start/completion, and support-loss suspension/resume. |
| Manual crafting | Timed production-backed shaping with integral repeated batches; matter, time, exertion, and wear scale with the requested amount. |
| Survival | Metabolic energy, hydration, vitality, Grain/Fruit/Protein nutrition, basal depletion, work exertion, perishable food, preservation, exact selected meals, and finite drinkable water. Vitality recovery is limited by the weakest recent dietary category, making balanced provisioning materially useful without directly punishing narrow diets; sub-ppm recovery is persisted across eligible ticks so integer vitality does not create artificial rate cliffs, while the read-only assessment exposes the rounded diet-supported recovery rate. Consumed matter/fluid remain in terminal conservation accounting. |
| Player labor | One exclusive `PlayerWorkState` across manual crafting, field prospecting, mining, and direct player power. Admission requires sufficient projected metabolic energy and hydration. |
| Capabilities | Typed physical requirements/values for throughput, mass, temperature, power, torque, speed, electrical quantities, flow, volume, and condition. |
| Equipment | Persistent mass-bearing condition-bearing equipment, exact assembly traces, occupancy, condition-dependent capability curves, additive upgrades, pristine disassembly, worn same-material recovery where authored, and fixed-vs-portable installation policy. |
| Maintenance | Exact replacement material is consumed and reformed into a conserved spent material form while condition is restored to the authored target. Active occupancy blocks maintenance. |
| Manual power | Survival-costed labor through a real portable unmounted power provider into a finite compatible energy store, with duration, efficiency, store limits, and equipment wear enforced. |
| Energy | Finite typed-carrier stores with capacity and directional power limits; exclusive production occupancy; opaque-authorized same-carrier transfer; material-backed construction/disassembly. |
| Structures | Material-backed members with geometry, topology, self-weight, source-separated loads, damage, active/cracked/failed lifecycle, axial tension/compression analysis, support-loss cascades, conserved construction/deconstruction, and damaged recovery forms. |
| Fluids | Finite homogeneous stores with identity, volume, temperature, capacity, optional support, exact volume accounting, and opaque-authorized transfer. Fluid structural load uses authored material density. |
| Mechanical/electrical scalars | Exact power/energy integration, flow/volume integration, electrical power/resistance, torque/speed/power, efficiency, transmission ratio, and operating limits. |
| Ore processing | Crushing, grinding, dry screening, selective oversize regrinding, and an authored primitive native-copper separation step. Comminution/screening preserve exact mass/composition/temperature and particle state; separation accepts liberated two-constituent copper/stone feed, derives pure copper and crushed-stone streams from the selected composition, conservatively retains sub-milligram target rounding in residue, preserves residue particle state, and consumes finite mechanical work with equipment limits, duration, and wear. This is not generalized mineral concentration. |
| Thermal production | Phase-aware sensible heating, pure-material melting, and pure-material casting with selected real matter, authored thermal properties, finite energy sources/sinks, equipment power/temperature limits, and latent heat. |
| Primitive progression | Player-performed local field inspection turns visible clue regions into uncertain persisted evidence and then opaque mining targets, followed by stone tool shaping/assembly, hand mining, scarce native-copper reinforcement of extraction or stored-work rate, material-backed flywheel storage, hand-crank charging, a player-built primitive crusher, and a player-built rocking separator. Only one reinforcement is directly supplied by the native-copper seam; the other must be recovered from composition-bearing crushed ore, so delegated processing creates both returned player attention and a concrete material progression path. Upgrade order changes which affordance arrives first, then both branches converge through crusher-to-separator processing. The maintained progression evaluation separately reports automation attention payback and the separator's additional material-progression setup cost. World-scale discovery of clue locations remains outside the current world-representation boundary. |
| Industrial gameplay evaluation | Bootstrapped workshop, ore-preparation, and pure-copper foundry capability harnesses. These prove installed-system behavior, not end-to-end industrial acquisition. |
| Spatial foundations | Checked chunk-independent voxel coordinates and bounds. |
| Renderer-neutral assets | Immutable texture definitions/baking and deterministic WGSL library/program assembly with typed identities and bounded work. No graphics backend. |
| Verification | Unit, persistence, conservation, soak, and gameplay coverage with local CI. [`TESTING.md`](TESTING.md) owns command selection and harness rules. |

## Not implemented

| Area | Missing capability |
| --- | --- |
| Engine/platform | Graphics backend, window/input/audio integration, ECS choice, networking, platform integration, general engine shell. |
| World representation | Voxel/chunk storage, terrain/world generation, streaming, world-scale spatial indexing. |
| Geological world systems | Regional geological generation, voxel ore topology, world-scale clue discovery, and advanced prospecting such as panning, material sampling, drilling, assays, or geophysics. Local non-destructive field inspection is implemented. |
| Mining infrastructure | Mechanized excavation, access/haulage/drainage/ground control, recovery fractions, waste rock, tailings ownership. |
| Thermal/chemical industry | Environmental heat transport, vaporization, combustion/fuels/emissions, mixed/alloy phase behavior, generalized mineral concentration beyond the authored native-copper/stone primitive separator, chemical smelting/reduction, alloying, forging, machining. |
| Rich maintenance | Bespoke salvage fractions, maintenance-scrap recovery, repair labor/tools/time/access, richer maintenance chemistry. |
| Advanced structures | Bending, shear, torsion, buckling, joints/connections, terrain support, construction labor/tooling/waste, fractional demolition streams. |
| Power networks | Shaft/belt networks, inertia/slip/clutches, steam systems, electrical topology, generation/distribution/protection, spatial/support integration for energy stores. |
| Hydrology | Ground/surface water, channels, pumps, irrigation, wastewater, sanitation, fluid mixing, pressure/temperature-dependent fluid properties. |
| Ecology and society | Agriculture, soil simulation, ecology, genetics, creatures, hunting/combat, workers, settlements, logistics, trade, economy, migration. |
| Industrial acquisition | Runtime paths for industrial machine acquisition, industrial power generation, generalized mixed-ore concentration/smelting, and the broader infrastructure needed to reach the bootstrapped industrial harnesses through ordinary play. |
| Save storage adapters | Save-file encoding/storage, filesystem atomicity, compression, cloud storage. |

New capability is listed here only after it has an authoritative owner, canonical runtime path,
persistence semantics where applicable, invariant coverage, and executable verification.

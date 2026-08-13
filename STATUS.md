# Status

## Current Foundation

- Headless deterministic Rust simulation core with no renderer or engine dependency.
- Immutable registry aggregate with separate authored-ID compatibility version.
- Typed absolute `SimulationTick`, relative `TickSpan`, and deterministic periodic phase scheduling.
- Persisted independent RNG streams derived from the world seed, normalized integer probability,
  and unbiased bounded random selection.
- Explicit authoritative integer quantities for mass, aggregate mass, temperature, energy,
  pressure, power, voltage, current, resistance, volume, and volumetric flow.
- Typed material/form definitions with density, thermal, mechanical, and electrical properties.
- Canonical normalized mass-fraction composition for ores, alloys, and mixed material lots, including
  validated deserialization and composition-aware material inputs.
- Density-based conservative material-volume calculation and composition-weighted sensible-heat
  calculation that refuses to cross phase boundaries implicitly.
- Persistent material lots with mass, temperature, composition, ownership, and provenance ranges.
- Capacity-aware stockpiles with derived commodity totals, cached mass, inbound reservations,
  revision-bound atomic transfers, deterministic splitting, and compatible-fragment coalescing.
- Closed-mass timed production with separate static requirements and operation-specific resolved
  output plans, durable consumed-input traces, committed output snapshots, and due-tick indexing.
- Typed authored capability requirements with physical value kinds and registry-reference validation.
  Built-in capability and production registries remain intentionally empty until real providers and
  physical resolvers exist.
- Continuous equipment `Condition`, authored maintenance warning/critical bands, and pure wear/repair
  plans without disposable durability semantics.
- Exact power-to-energy, flow-to-volume, electrical-power, and resistive-drop scalar calculations
  with explicit carried fractional remainders where repeated truncation would lose resources.
- Read-only global matter accounting across inventory and in-process matter ownership.
- Canonical top-level tick pipeline with cheap per-tick invariants and exhaustive save/load audits.
- Persistence semantic schema 9 with metadata preflight, registry-aware state validation, stable
  in-flight job conservation snapshots, and deterministic continuation tests.
- Chunk-independent 64-bit voxel coordinates and validated spatial bounds without choosing chunk
  dimensions or streaming policy.
- Deterministic 10,000-tick soak with repeated production/transfers, full-state replay equality,
  periodic exhaustive audits, matter-conservation checks, and lot-fragmentation ceiling.
- Current debug validation suite: 79 passing tests with `cargo check` silent and Clippy warnings denied.
- Release profile keeps integer overflow checks enabled.

## Deliberately Deferred

- Renderer, input, audio, UI, engine/ECS selection, physics implementation, networking, and general
  threading architecture.
- Concrete voxel/chunk storage, world generation, spatial indexes, chunk dimensions, and streaming.
- Canonical extraction/world-generation matter sources. Arbitrary matter insertion remains test-only.
- Thermal fields, heat transport, latent heat/phase changes, combustion, fuel networks, and emissions.
- Real equipment, tools, workers, structures, and their capability-provider ownership.
- Real production resolvers for metallurgy, equipment capability, tooling, labor/skill, energy, and
  environmental constraints. Gameplay processes remain unregistered until these gates exist.
- Mechanical-power networks, steam/boilers, electrical networks, transformers, protection, and
  distribution topology.
- Hydrology/fluid networks, pumps, irrigation, sanitation, wastewater, and water-quality ownership.
- Agriculture, soil, geology, ecology, genetics, creatures, workers, settlements, logistics, trade,
  economy, migration, and other gameplay systems.
- Save-file encoding/storage, compression, atomic filesystem writes, cloud storage, and released-save
  migration implementations.
- Spatial/world performance benchmarks required before final chunk and streaming architecture.

## Foundation Direction

New gameplay work should add one owning subsystem at a time with immutable registry definitions,
typed persistent IDs and quantities, dedicated errors, canonical mutations, explicit invariants,
persistence semantics, and behavioral/soak coverage. Cross-system integration belongs in the visible
simulation pipeline or an explicitly named integration module. Do not add simplified production
recipes or generic technology tiers before the physical systems that authorize them exist.

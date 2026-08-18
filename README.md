# Deep Hearth

Deep Hearth is a deterministic, systems-driven survival and industrialization game simulation.

The repository currently focuses on a headless Rust simulation core. Rendering, input, audio,
networking, platform integration, and save-file storage are adapters around that core rather than
owners of gameplay state or rules.

The renderer-neutral visual foundation provides hue-shaped palette ramps, one-byte indexed 32x32
tiles, explicit block-face and object material-slot appearances, and a deterministic startup bake
into deduplicated texture-array layers with discrete mipmaps. This is twice the linear texture
resolution of a conventional 16x16 voxel tile while retaining compact indexed storage. A future
graphics adapter can upload the baked `R8_UINT` texels and compact lookup tables without adding
image-decoding or graphics dependencies to the simulation core. A matching bounded WGSL suite adds
palette-aware HDR surfaces, zero-sample opaque shadows, accurate cutout shadows, stable 16x16 tiled
point lights, analytic water, procedural billboard smoke, a cloud-and-star sky, half-resolution
bloom, ACES-fit tone mapping, and fog. Shader libraries assemble deterministically at startup, and
every executable declares an auditable ceiling for texture reads, noise layers, lights, and loop
iterations.

The implemented foundation includes persisted independent RNG streams, exact integer engineering
quantities, composition-aware and phase-aware material lots, temperature/phase-constrained storage,
atomic inventory transactions, closed-mass durable production, finite geological matter ownership
with conservation-bound extraction, persistent prospecting evidence and uncertainty-aware geological
maps, typed capability requirements, maintenance condition, condition-sensitive equipment,
conserved structural construction/deconstruction with real self-weight, inventory and equipment weight
coupled into structural failure, directional finite energy stores, finite homogeneous fluid storage,
exact electrical/flow/rotational scalar mechanics, selected-batch crushing and same-form grinding with
finite work energy and condition-sensitive throughput, exact multi-stream dry screening with finite
work and conservative particle-class partitioning, feed-size-constrained selective regrinding, and
physical thermal production for sensible heating,
pure-material melting, and casting with explicit latent heat and finite heat sinks. Current-schema
persistence revalidates cross-owner conservation and operation-specific physics, and deterministic
soak coverage exercises the major ownership paths. Primitive progression now includes exclusive timed
player work, exertion-aware manual shaping, conserved composite stone-tool assembly, tool-gated finite
mining with condition-sensitive throughput/hardness/batch limits, distinct native-metal occurrences,
cold-worked additive copper reinforcement that preserves existing wear, materially constructed
flywheel work storage, direct manual charging, and a player-built primitive crusher that converts
accumulated hand work into a shorter mechanized comminution burst. Pristine equipment and empty
material-backed stores can reverse assembly into their exact embodied traces without resetting IDs;
worn-equipment salvage remains unresolved rather than becoming a free repair path. Physical
prospecting, mineralized-ore concentration and chemical smelting, alloy phase diagrams, environmental
heat transport, fluid networks, richer construction, agriculture/ecology, non-player workers, and
settlement systems remain unavailable until their real physical owners exist.

## Documentation authority

Read these documents before changing the project. Each question has one owning document.

| Question | Authority |
| --- | --- |
| What is this project and how do I run it? | `README.md` |
| What coding and architecture law must contributors follow? | `AGENTS.md` |
| How are tests, harnesses, and CI lanes organized? | `TESTING.md` |
| What is the implemented technical architecture and its deliberate boundaries? | `TECHNICAL_DESIGN.md` |
| What is currently implemented and what is deliberately deferred? | `STATUS.md` |
| What gameplay intent and progression is intended? | `GAME_DESIGN.md` |

## Task routing

Use this map to find the owner before reading implementation broadly. The table routes change
classes only; formulas, physical contracts, persistence semantics, and invariants remain in the
owning source and current architecture/design documents.

| Change class | Start with | Additional authority | First proving route |
| --- | --- | --- | --- |
| Authored definitions, recipes, capabilities, or registry validation | `src/content/`, `src/registry/`, `src/capability/` | `STATUS.md` when supported scope changes | Narrowest exact owner test via `cargo test-fast <qualified-test-name> -- --exact` |
| Core state, identity, time, deterministic RNG, or simulation order | `src/core/`, `src/simulation/` | `ARCHITECTURE.md` | Narrowest exact owner test; add `--soak` when long-horizon invariants change |
| Matter, material properties, composition, or inventory custody | `src/matter/`, `src/material/`, `src/inventory/` | `TECHNICAL_DESIGN.md` | Narrowest exact owner test |
| Equipment capability, condition, maintenance, or durable tool ownership | `src/equipment/`, `src/maintenance/`, `src/capability/` | `TECHNICAL_DESIGN.md` | Narrowest exact owner test |
| Production, crafting, mining, geology, or ore processing | `src/production/`, `src/crafting/`, `src/mining/`, `src/geology/`, `src/ore_processing/` | `TECHNICAL_DESIGN.md` | Narrowest exact owner test; add `--gameplay` when workshop behavior/content changes |
| Energy, electrical, rotational/mechanical, fluid, or thermal physics | `src/energy/`, `src/electrical/`, `src/mechanical/`, `src/fluid/`, `src/thermal/` | `TECHNICAL_DESIGN.md` | Narrowest exact owner test |
| Structural load/failure or spatial ownership | `src/structural/`, `src/spatial/` | `TECHNICAL_DESIGN.md` | Narrowest exact owner test; add `--soak` for long-horizon structural integration |
| Player labor, work lifecycle, exertion, or survival resources | `src/labor/`, `src/survival/` | `TECHNICAL_DESIGN.md`, `GAME_DESIGN.md` when progression intent changes | Narrowest exact owner test; add `--gameplay` when player workshop behavior changes |
| Save/load shape, durable validation, or deterministic continuation | `src/persistence/` plus the affected state owner | `ARCHITECTURE.md`, `STATUS.md` when compatibility/scope changes | Narrowest persistence/owner test; add `--soak` when continuation-sensitive behavior changes |
| Texture baking, WGSL, or renderer-neutral visual contracts | `src/texture/`, `src/shader/` | `TECHNICAL_DESIGN.md` | `cargo test-shaders` plus any affected owner test |
| Test lanes, harness behavior, CI, or documentation routing | `TESTING.md`, `.cargo/config.toml`, `ci.py` | Owning test/tool source | Smallest affected lane; add `python ci.py gate --docs` for documentation contracts |

If a change spans several rows, keep one canonical owner for each consequential fact and review the
cross-owner transaction in `ARCHITECTURE.md` rather than introducing a convenience mutation path.

The ordinary edit loop is intentionally small:

```text
cargo check-fast
cargo test-fast <qualified-test-name> -- --exact
```

Use `check-fast` for intermediate compile/type feedback without linking the unit-test harness. For
changed behavior, run the narrowest exact owner test and use the complete fast lane only at coherent
checkpoints. Before committing, run the local gate once; add the specialized lane for the contract you
changed:

```text
python ci.py gate
python ci.py gate --soak       # when long-horizon ownership/invariants changed
python ci.py gate --gameplay   # when workshop behavior/content changed
python ci.py gate --shaders    # when WGSL/shader assembly changed
```

`cargo test-check` remains available when an all-target compile-only diagnostic is useful, but the
ordinary pre-commit sequence already type-checks production code through Clippy and compiles the full
default-feature unit-test target through `cargo test-fast`.

`TESTING.md` owns lane selection, harness output, and local verification gates. GitHub Actions and
hosted runners are prohibited. Release hardening remains explicit:

```text
python ci.py hardening
cargo test-release
```

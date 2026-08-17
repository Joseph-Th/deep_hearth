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
player work, exertion-aware manual shaping, conserved composite stone-pick assembly, and tool-gated
finite mining with condition-sensitive throughput, hardness/batch limits, wear, reserved output, and
explicit claim. Physical prospecting, alloy phase diagrams and chemical smelting, environmental heat
transport, fluid networks, richer construction, agriculture/ecology, non-player workers, and
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

The ordinary edit loop is intentionally small:

```text
cargo test <qualified-test-name> -- --exact
cargo test-fast
```

Before committing, run formatting, production-library linting, and the fast unit-test lane; add the
specialized lane for the contract you changed:

```text
cargo fmt --check
cargo test-lint
cargo test-fast
cargo test-soak       # when long-horizon ownership/invariants changed
cargo test-gameplay   # when workshop behavior/content changed
cargo test-shaders    # when WGSL/shader assembly changed
```

`cargo test-check` remains available when an all-target compile-only diagnostic is useful, but the
ordinary pre-commit sequence already type-checks production code through Clippy and compiles the full
default-feature unit-test target through `cargo test-fast`.

`TESTING.md` owns lane selection, harness output, and local verification gates. GitHub Actions and
hosted runners are prohibited. Release hardening remains explicit:

```text
cargo test-lint-all
cargo test-release
cargo test-doc
```

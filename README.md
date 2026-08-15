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
pure-material melting, and casting with explicit latent heat and finite heat sinks. Versioned
persistence revalidates cross-owner conservation and operation-specific physics, and deterministic
soak coverage exercises the major ownership paths. Physical survey/mining authorization, alloy phase
diagrams and chemical smelting, environmental heat transport, fluid networks, richer construction,
and later labor/ecology/settlement systems remain unavailable until their real physical owners exist.

Read these documents before changing the project:

- `GAME_DESIGN.md` defines gameplay intent and progression.
- `AGENTS.md` defines coding and architecture law.
- `TECHNICAL_DESIGN.md` records implemented technical architecture and deliberate boundaries.
- `STATUS.md` records current implementation status and deferred work.

Required development validation:

```text
cargo fmt --check
cargo check --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked -j 2
```

Release hardening additionally runs:

```text
cargo test --release --locked -j 2
cargo doc --locked --no-deps
```

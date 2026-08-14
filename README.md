# Deep Hearth

Deep Hearth is a deterministic, systems-driven survival and industrialization game simulation.

The repository currently focuses on a headless Rust simulation core. Rendering, input, audio,
networking, platform integration, and save-file storage are adapters around that core rather than
owners of gameplay state or rules.

The implemented foundation includes persisted independent RNG streams, exact integer engineering
quantities, composition-aware and phase-aware material lots, temperature/phase-constrained storage,
atomic inventory transactions, closed-mass durable production, finite geological matter ownership
with conservation-bound extraction, persistent prospecting evidence and uncertainty-aware geological
maps, typed capability requirements, maintenance condition, condition-sensitive equipment,
conserved structural construction/deconstruction with real self-weight, inventory and equipment weight
coupled into structural failure, directional finite energy stores, finite homogeneous fluid storage,
exact electrical/flow/rotational scalar mechanics, selected-batch comminution with finite work energy
and condition-sensitive throughput, and physical thermal production for sensible heating,
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

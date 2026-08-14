# Deep Hearth

Deep Hearth is a deterministic, systems-driven survival and industrialization game simulation.

The repository currently focuses on a headless Rust simulation core. Rendering, input, audio,
networking, platform integration, and save-file storage are adapters around that core rather than
owners of gameplay state or rules.

The implemented foundation includes persisted independent RNG streams, exact integer engineering
quantities, composition-aware material lots, atomic inventory transactions, closed-mass durable
production, typed capability requirements, maintenance condition, thermal/volume calculations,
condition-sensitive equipment capabilities, equipment weight coupled into structural failure,
energy/electrical/fluid scalar integration, exact torque/speed mechanical transmission primitives,
versioned persistence, chunk-independent spatial types, global matter accounting, and deterministic
soak coverage. Built-in gameplay processes remain unregistered until their real physical
authorization systems exist.

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

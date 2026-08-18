# Tasks

Current executable work that may be picked up asynchronously. Keep entries short; remove completed work. `STATUS.md`/`CAPABILITIES.md` owns implemented truth and `ROADMAP.md`/`DIRECTION.md` owns future direction.

## T-0001 - Protect Markdown authority graph

- Area: documentation
- Next: Extend the existing --docs verification lane with a fast mechanical Markdown authority check: required current authority pages, local links, concrete repository routes/aliases, and README/STATUS/TESTING ownership relationships. Keep rustdoc as its own proof and avoid brittle prose linting or duplicating subsystem semantics.
- Paths: `TESTING.md`
- Verify: `python ci.py gate --docs && python ../tools/check_standards.py`
- Depends: none
- Basis: `755b8567d2380bfc1bbe41ae9a61e0446629d190`
- Reviewed: `2026-08-18T03:37:13Z`

## T-0002 - Make durable state deserialization lossless

- Area: persistence-admission
- Next: Make the current-only SaveEnvelope/LoadedSaveEnvelope contract reject semantic information loss before persistent invariant validation. Durable AppState owners contain many BTreeMap-backed authoritative and derived indexes, while ordinary derived Deserialize can accept repeated serialized map keys by overwriting an earlier member; nested persisted structs also generally accept unknown fields. Add format-neutral duplicate-rejecting deserializers for every durable map/set representation whose wire form permits repeated keys, and enforce current-shape unknown-field rejection across the persisted envelope/state graph (or one equivalent strict structural admission layer) without adding compatibility defaults. Preserve the existing save-schema and RegistrySchemaVersion checks, exhaustive validate_loaded_state replay, and deterministic continuation. Add raw JSON regressions through the supported Serde envelope for duplicate production job IDs, duplicate nested inventory/structure keys, unexpected top-level and nested current-schema fields, and unchanged valid round trips. The persistence owner is currently under unrelated active work, so anchor this task to a clean representative durable-state owner and refresh against src/persistence/mod.rs before implementation.
- Paths: `src/production/state.rs`
- Verify: `cargo test-fast persistence && python ci.py gate`
- Depends: none
- Basis: `11b88e9aff45e589c3bceb4fb382eeeffae8f8cc`
- Reviewed: `2026-08-18T08:01:46Z`

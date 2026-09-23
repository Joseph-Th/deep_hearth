# Rust diagnostics

`rust_diagnostics.py` is the project-owned entry point for optional Rust agent diagnostics. These
commands answer a named engineering question; they are not completion gates and do not replace the
proof lanes in [`../TESTING.md`](../TESTING.md).

The workspace owns the pinned compatible tool versions and installation workflow. The compatibility
vocabulary is `cargo modules structure`, `cargo mutants --list`, and `cargo expand`; the project-owned
wrappers below remain the preferred interface.

## Module ownership and dependency shape

Use cargo-modules when source ownership or module shape is unclear before editing:

`python tools/rust_diagnostics.py modules --focus survival`

The default is the bounded structure view
`cargo modules structure --lib --no-fns --no-traits --no-types --max-depth 4`, with bare focus
paths normalized to `crate::<path>`. Increase depth only when the owner boundary remains ambiguous.

Use `modules orphans --tests` to look for unlinked source when test-only modules count as linked.
Match the feature set being reviewed with `--features` or `--all-features`. Use
`modules dependencies --focus <owner>` when dependency direction is the question; dependency views default
to depth 1 because their DOT output grows much faster than the structure tree. The wrapper
intentionally does not expose crate-wide `--acyclic`: cargo-modules treats ordinary type/constructor
relationships as cycles in this crate, so that mode produces misleading architectural signal.

## Mutation testing

Use cargo-mutants after focused tests exist but their ability to constrain an important invariant,
transaction boundary, rejection path, or continuation rule is still uncertain.

Start by listing mutations:

`python tools/rust_diagnostics.py mutants src/survival/validation/direct_consumption.rs --re validate_pending_food_freshness`

Execution is deliberately opt-in and requires the mutation regex:

`python tools/rust_diagnostics.py mutants src/survival/validation/direct_consumption.rs --re validate_pending_food_freshness --run`

The wrapper keeps runs targeted, fixes execution at exactly two concurrent mutant jobs, never uses
`--in-place`, and writes logs beneath `target/agent-output/rust-diagnostics/mutants/`. The worker
count is intentionally not configurable. Mutation execution also holds one project-wide exclusive
lock: if another `mutants --run` invocation is active, a second invocation fails immediately instead
of multiplying Cargo/rustc/test process trees. The existing `.cargo/mutants.toml` keeps ignored
artifacts out of copied worktrees and caps lints so behavior-removing mutants reach tests.
`--skip-baseline` is appropriate only when the unchanged test surface was just proven separately.

Never invoke `cargo mutants` directly and never launch multiple `rust_diagnostics.py mutants --run`
commands in parallel. One targeted mutation execution at a time is the project limit. Mutation
testing is diagnostic evidence for one unresolved invariant, not an exhaustive audit strategy; stop
when that question is resolved and return to the normal proof lanes.

Do not optimize for a mutation score. Inspect only mutations that distinguish the contract being
changed. A surviving relevant mutant is evidence that the focused proof is weak; an unrelated
survivor is not a reason to add artificial tests.

## Macro and derive expansion

Use cargo-expand only when generated Rust materially affects the question, such as serialization,
derive behavior, or a macro-generated API:

`python tools/rust_diagnostics.py expand survival::state::direct_consumption::PendingEating`

An item expansion can omit derive-generated sibling impls. In that case expand the containing
module and filter the output:

`python tools/rust_diagnostics.py expand survival::state::direct_consumption --grep PendingEating`

The wrapper uses the locked dependency graph, disables color, and can print only regex-matched
windows so generated code remains evidence-sized.

## Workflow placement

Use these diagnostics during investigation or focused proof selection:

1. cargo-modules before editing when ownership is unclear.
2. cargo-expand before reasoning about generated behavior.
3. cargo-mutants after focused tests exist and test adequacy is genuinely uncertain.
4. Return to the normal `TESTING.md` owner/boundary/continuation/system proof ladder for completion.

Do not add these diagnostics to `quick`, `gate`, or `audit`; they are question-driven tools, not
universal health metrics.

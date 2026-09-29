# Rust diagnostics

**Role:** Optional Rust diagnostics reference.

Use `rust_diagnostics.py` only for a named uncertainty that source reading or focused tests do not resolve
cheaply. These commands are diagnostic, not completion gates; [`../TESTING.md`](../TESTING.md) owns proof.

## Module ownership and dependency shape

Use module diagnostics when ownership or dependency shape remains unclear:

`python tools/rust_diagnostics.py modules --focus survival`

The default is a bounded structure view. Increase depth only when the owner boundary remains ambiguous. Use
`modules dependencies --focus <owner>` for dependency direction; dependency views default to depth 1 because
their DOT output grows much faster than the structure tree. Use `modules orphans --tests` when unlinked test
source is the question, and match relevant features with `--features` or `--all-features`. The wrapper omits
crate-wide `--acyclic` because ordinary type and constructor relationships produce misleading cycle signal here.

## Mutation testing

Use mutation diagnostics only when a focused test's ability to constrain an important invariant, transaction,
rejection path, or continuation rule is still uncertain.

Start by listing mutations:

`python tools/rust_diagnostics.py mutants src/survival/validation/direct_consumption.rs --re validate_pending_food_freshness`

Execution is opt-in and requires the mutation regex:

`python tools/rust_diagnostics.py mutants src/survival/validation/direct_consumption.rs --re validate_pending_food_freshness --run`

The wrapper owns isolation, logs, concurrency limits, and mutation configuration. Run one targeted mutation
execution at a time. Use `--skip-baseline` only when the unchanged test surface was just proven separately.
Inspect only mutations that distinguish the contract being changed; mutation score is not a quality metric.

## Macro and derive expansion

Use cargo-expand only when generated Rust materially affects the question, such as serialization,
derive behavior, or a macro-generated API:

`python tools/rust_diagnostics.py expand survival::state::direct_consumption::PendingEating`

If an item view omits a needed derive-generated sibling impl, expand the containing module and filter it:

`python tools/rust_diagnostics.py expand survival::state::direct_consumption --grep PendingEating`

Keep expansion output focused on the item or regex window that answers the question.

## Workflow placement

Use module inspection before editing when ownership is unclear, expansion before reasoning about material macro
behavior, and mutation testing only after focused tests exist. Return to [`../TESTING.md`](../TESTING.md) for
completion. Do not add diagnostics to `quick`, `gate`, or `audit`.

# Deep Hearth Agent Guide

**Applicable profiles:** Universal; Stateful Application; Deterministic System; Automated Behavior Evaluation
**BCA policy:** ratchet

**Rust agent diagnostics:** advisory. Use bounded, uncertainty-driven commands from [`tools/README.md`](tools/README.md); they are not BCA or completion gates. [`TESTING.md`](TESTING.md) owns verification.

This file owns project execution. Workspace [`../AGENTS.md`](../AGENTS.md) owns coordination;
[`README.md`](README.md) owns project routing.

## Cold start

1. Preserve unrelated working-tree changes.
2. Read [`STATUS.md`](STATUS.md) before assuming a capability is implemented or reachable.
3. Use [`README.md`](README.md) to find the owning subsystem, canonical boundary, contract, and focused proof.
4. Read the owning source and adjacent tests, plus only the authority document needed for the change.

Read [`DIRECTION.md`](DIRECTION.md) only for future sequencing/integration choices. Read
[`GAMEPLAY_EVALUATION.md`](GAMEPLAY_EVALUATION.md) only for automated-player behavior/evidence policy.

## Operating protocol

Before editing, assign one control coordinate:

1. **Authority:** which truth layer owns the claim: intent, direction, current reality, implemented contract,
   concrete source, or evidence?
2. **Owner:** which subsystem owns the consequential generated fact?
3. **Stage:** observe, resolve/decide, validate/authorize, commit/apply, continue, report outcome, or audit?
4. **Flow:** which matter, energy, fluid, labor, information, support, capacity, identity, reservation, or time
   edges cross the change?
5. **Proof:** owner, boundary, continuation, system/gameplay, or exploratory evidence?

A feature name is not an address. Start with the routed owner and crossed edge; widen only when a canonical
operation, durable record, trusted-load rule, tick phase, or failing proof shows another owner participates.
Search by canonical operation, durable identity, typed error, and adjacent proof rather than by every file sharing
the feature noun.

Attach new behavior to the existing owner and control grammar whenever possible. Add only the definition, state,
observation, authorization, mutation, outcome, persistence, and proof surfaces required by the new semantics. Do
not introduce a parallel manager, cache, status flag, helper API, or test-only path for a fact that already has an
owner.

## Guardrails

- Change consequential behavior only through its authoritative subsystem and canonical operation. Tests, harnesses, and tools reuse production paths.
- Preserve deterministic continuation, strict trusted-load validation, checked physical arithmetic, typed ownership, and exact represented matter, fluid, and energy accounting.
- Core systems perform no implicit external IO; adapters own external effects.
- Remove obsolete code and stale documentation. Do not add compatibility scaffolding, test-only public APIs, fake callers, or broad warning suppressions without an active contract.
- Verification is local. Do not add or depend on hosted CI.
- Mutation testing is advisory and serialized. Use only `tools/rust_diagnostics.py`; never call
  `cargo mutants` directly, bypass its fixed two-worker limit, or parallelize `mutants --run`. Use it
  only for one named unresolved invariant, not as a general audit.

## Completion

Use [`TESTING.md`](TESTING.md) for the smallest complete proof. `bca.toml` and [`TESTING.md`](TESTING.md)
own the BCA ratchet. For documentation-only changes, run `python tools/check_authority_docs.py`.
Before handoff, review the task-scoped diff and update only the authority pages whose contracts changed.

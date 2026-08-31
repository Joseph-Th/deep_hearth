# Deep Hearth Agent Guide

**Applicable profiles:** Universal; Stateful Application; Deterministic System; Automated Behavior Evaluation
**BCA policy:** ratchet

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

Treat the repository as one linked control system. Before editing, assign one **control coordinate**:

1. **Authority:** which truth layer owns the claim: intent, direction, current reality, implemented contract,
   concrete source, or evidence?
2. **Owner:** which subsystem owns the consequential generated fact?
3. **Stage:** observe, resolve/decide, validate/authorize, commit/apply, continue, report outcome, or audit?
4. **Flow:** which matter, energy, fluid, labor, information, support, capacity, identity, reservation, or time
   edges cross the change?
5. **Proof:** owner, boundary, continuation, system/gameplay, or exploratory evidence?

The task is that coordinate plus the desired delta; a feature name is not an address. Descend the
[`README.md`](README.md) abstraction ladder only until uncertainty resolves, then verify bottom-up.

Keep investigation evidence-bounded: start with current scope, one owner, and crossed edges; expand only when a
canonical operation, durable record, trusted-load validator, tick phase, or failing proof shows another owner
participates. Read transformation/resolver code only for changed physical derivation. Stop when intent, reality,
owner, control path, flows, and distinguishing proof are known; widen again only on contradictory evidence.

Search by authority heading, owner, canonical operation, durable identity, typed error, and adjacent proof rather
than by every file sharing a feature noun.

Changes should be accretive: attach new behavior to an existing owner and control grammar where one exists. A
new concept should add the minimum necessary definition, authoritative state, observation, authorization,
mutation, outcome, persistence, and proof surfaces. Do not create a parallel manager, helper API, cache, status
flag, or test-only path for a fact that already has an owner.

## Guardrails

- Change consequential behavior only through its authoritative subsystem and canonical operation. Tests, harnesses, and tools reuse production paths.
- Preserve deterministic continuation, strict trusted-load validation, checked physical arithmetic, typed ownership, and exact represented matter, fluid, and energy accounting.
- Core systems perform no implicit external IO; adapters own external effects.
- Remove obsolete code and stale documentation. Do not add compatibility scaffolding, test-only public APIs, fake callers, or broad warning suppressions without an active contract.
- Verification is local. Do not add or depend on hosted CI.

## Completion

Use [`TESTING.md`](TESTING.md) for the smallest complete proof. `bca.toml` and [`TESTING.md`](TESTING.md)
own the BCA ratchet. For documentation-only changes, run `python tools/check_authority_docs.py`.
Before handoff, review the task-scoped diff and update only the authority pages whose contracts changed.

# Deep Hearth Agent Guide

**Applicable profiles:** Universal; Stateful Application; Deterministic System; Automated Behavior Evaluation
**BCA policy:** ratchet

This file owns project execution rules. Workspace [`../AGENTS.md`](../AGENTS.md) owns cross-project
coordination. [`README.md`](README.md) is the project routing hub.

## Cold start

1. Preserve unrelated working-tree changes.
2. Read [`STATUS.md`](STATUS.md) before assuming a capability is implemented or reachable.
3. Use [`README.md`](README.md) to find the owning subsystem, canonical boundary, contract, and focused proof.
4. Read the owning source and adjacent tests, plus only the authority document needed for the change.

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

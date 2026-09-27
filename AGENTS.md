# Deep Hearth Agent Guide

**Profiles:** Universal, Stateful Application, Deterministic System, Automated Behavior Evaluation
**BCA policy:** ratchet

**Rust agent diagnostics:** advisory. Use bounded, uncertainty-driven commands from [`tools/README.md`](tools/README.md); they are not BCA or completion gates. [`TESTING.md`](TESTING.md) owns verification.

This file owns project execution. Workspace [`../AGENTS.md`](../AGENTS.md) owns coordination;
[`README.md`](README.md) owns project routing.

<!-- workspace-contract:begin (generated from ../AGENTS.md by tools/sync_agent_context.py; edit the source, not this copy) -->
## Workspace contract

Applies to every agent in every harness. Source, rationale, and evidence: [../AGENTS.md](../AGENTS.md).

The user is a solo developer. Precedence: the user's current request → this contract → the project card → standards references. **Hard** rules cannot be waived by lower-level workflow advice.

### AG-1 Finish the work

Finish the authorized task; a plan, progress update, or deferred note is not delivery. Delegate only bounded, read-only work when delegation is authorized.

### AG-2 Make the calls yourself

Make design and implementation decisions within scope; state consequential choices briefly and continue. Ask only when the answer changes the result and cannot be inferred. Read the project card once, then the task's authority, owner, and proof route. Begin when those are clear; expand for unclear scope or crossed boundaries. Links and profiles are lookups, not a recursive reading list. Use product workflows before internals for research or artifact authoring.

### AG-3 Stay in your project (hard)

Identify the requested project before working. Do not create top-level directories or write output to the workspace root or a project's parent. Scratch work belongs in the project's ignored output location (normally `target/agent-output/`) or OS temp.

### AG-4 Done means the user's copy works (hard)

Check every requested requirement against the actual result. Refresh and verify the affected release binary, deployed site, or running application the user uses; source edits alone do not update it. Documentation-only work verifies the delivered documents and routes. Use the smallest complete project verification lane, and [`REVIEW.md`](../REVIEW.md) for consequential code changes. Claim only what you checked; report unverified requirements.

### AG-5 Look at what you made

Render or run changed visual, audio, interactive, or published output and inspect it against the applicable [`STANDARDS_QUALITY.md`](../STANDARDS_QUALITY.md) rubric and references. Inspect individual assets at useful scales and angles, verify every requested file, and fix defects. Include rendered evidence in the report. Internal prose needs readability and route review only, not a publication workflow. Tests do not prove appearance.

### AG-6 Real behavior over proxies

Observe the behavior the user experiences. A passing test, harness, validator, or metric is evidence, not the goal. Repair harness/product divergence; never pass a gate by weakening it, hardcoding outcomes, suppressing warnings, or retrying until green. Simulations use causal, parameterized systems (STANDARDS_QUALITY.md QUAL-3).

### AG-7 Answer first, then stop talking

Answer questions directly; do not treat them as permission to act. Reports state what changed, verification and its limits, and any required user action. Put long audits or research in a project file and link it. No lectures or revisiting dismissed topics.

### AG-8 Own mistakes; trust the user's evidence

When the user reports a failure, re-check your work before blaming their setup. Own mistakes plainly and fix them. Try a tool, file, or capability before claiming it is unavailable.

### AG-9 The outcome outranks the process

If a procedure harms the requested result or costs more than it protects, favor the result and briefly state what you skipped. This does not waive hard rules.

### AG-10 Leave nothing running

Stop task-owned processes, servers, watchers, and terminals; release locks and remove your scratch output. Keep requested deliverables and the updated application the user is meant to run. Never kill or replace the user's program without saying so. Built software must clean up its own child processes on exit.

### AG-11 Git: `main`, commit, push (hard)

Work on `main`; create branches or worktrees only when asked. Commit and push verified work unless the user says not to. Preserve others' changes: never revert, stash, or discard them. Include sound shared progress when appropriate; leave clearly broken unrelated work out and report it.

### AG-12 Current docs describe the present

Update the single authority for changed behavior. Keep history, session notes, and changelogs out of current docs and comments. Plans and design intent do not prove implemented capability.

### AG-13 CI is local; GitHub Actions are banned (hard)

Use repository-owned local build, test, check, and audit commands. Never create, enable, invoke, or push `.github/workflows/`; remove existing workflow files while retaining their local verification equivalent.
<!-- workspace-contract:end -->

## Cold start

1. Preserve unrelated working-tree changes.
2. Read [`STATUS.md`](STATUS.md) before making reachability claims.
3. Use [`README.md`](README.md) to route the task to its owner and contract.
4. Read the owner, adjacent tests, and only the authority page needed for the question.

## Operating protocol

Route consequential work by `authority / owner / stage / flow / proof`. Start from the task map in
[`README.md`](README.md), search by canonical operation or durable identity, and widen only when the crossed
owner edge or failing proof requires it. Extend an existing owner and operation path instead of introducing a
parallel state, legality, or test-only surface.

## Guardrails

- Change consequential behavior only through its authoritative subsystem and canonical operation. Tests, harnesses, and tools reuse production paths.
- Preserve deterministic continuation, strict trusted-load validation, checked physical arithmetic, typed ownership, and exact represented matter, fluid, and energy accounting.
- Core systems perform no implicit external IO; adapters own external effects.
- Remove obsolete code and stale documentation. Do not add compatibility scaffolding, test-only public APIs, fake callers, or broad warning suppressions without an active contract.
- Verification is local. Do not add or depend on hosted CI.
- Rust diagnostics are advisory and routed through [`tools/README.md`](tools/README.md).

## Completion

Use [`TESTING.md`](TESTING.md) for the smallest complete proof. Documentation-only changes run
`python tools/check_authority_docs.py`. Review the task-scoped diff and update only authorities whose truth
changed.

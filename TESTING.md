# Testing

This page owns test organization, gameplay-harness contracts, and local verification. Use
[`README.md`](README.md) for routing and [`STATUS.md`](STATUS.md) for runtime scope.

Use the smallest lane that completely proves the changed contract.

## Fast path

| Need | Command |
| --- | --- |
| Documentation/contracts | `python tools/check_authority_docs.py` |
| Build-free edit loop | `python ci.py quick` |
| Production compile | `cargo check-fast` |
| Production build gate | `python ci.py gate` |
| Fast lib-only Clippy | `cargo lint-fast` |
| List tests without building | `python tools/run_test.py --list [substring]` |
| Type-check an integration target without linking | `python tools/run_test.py --check --target <integration-target>` |
| Run one exact unit/integration test | `python tools/run_test.py <qualified-name-or-unique-substring>` |
| Show one selected test's captured stdout | `python tools/run_test.py --verbose <qualified-name-or-unique-substring>` |
| Run one owner/subsystem test group | `python tools/run_test.py --suite <qualified-prefix-or-substring>` |
| Gameplay harness contracts | `python ci.py gate --gameplay contracts` |
| Focused gameplay | `python ci.py gate --gameplay {workshop,survival,progression,ore,foundry}` |
| Core audit | `python ci.py audit --core` |
| Gameplay audit | `python ci.py audit --gameplay` |
| Core + gameplay audit | `python ci.py audit --all` |
| Broad production/gameplay Clippy | `python ci.py gate --lint` |
| Shader validation | `python ci.py gate --shaders` |
| Rustdoc | `python ci.py gate --rustdoc` |
| Long-horizon soak | `python ci.py gate --soak` |
| Gameplay exploration report | `python ci.py report` |
| Changed-source BCA review | `python ci.py bca [--path <scope>] [--since <revision>]` |
| Current BCA hotspot review | `python ci.py bca --hotspots [--path <scope>] [--since <revision>]` |
| Agent Rust diagnostics | `python tools/rust_diagnostics.py --help` |

`quick` is build-free. `gate` runs one build lane and does not repeat `quick`; specialized flags replace its
default compile. `audit` checkpoints add `quick` to the selected runtime surface.

During edits, use `cargo check-fast`, `cargo lint-fast`, or `run_test.py --check`, then one exact/suite or focused proof.
Reuse warm artifacts. Gameplay gates/audits keep fixed anchors plus one fresh bounded organic case per probe.
Roots are replay evidence; explicit roots replace them. `report` samples more cases.
Contracts remain deterministic.

Without `--target`, `run_test.py` resolves tests from source without Cargo and chooses the smallest complete
explicit target. Pin `--target` only to reuse a warm failed binary or force an integration boundary. `--check`
always requires an explicit integration target. Exact tests are quiet by default; `--verbose` implies `--nocapture`.
Detailed gameplay narration is report-only.

Library unit tests intentionally use the ordinary feature-minimal `cfg(test)` graph. This keeps exact tests,
owner suites, and `audit --core` on one reusable incremental artifact. Gameplay integration targets alone enable
`test-gameplay`; the combined `audit --all` pays for that broader feature graph only at an explicit checkpoint.

## Evidence ladder

Stop at the smallest evidence level that distinguishes the changed contract:

1. **Owner proof:** exact local success/rejection, lifecycle, arithmetic, or invariant behavior.
2. **Boundary proof:** one crossed ownership/custody edge, including stale-state rejection and atomicity where
   applicable.
3. **Continuation proof:** save/load replay or schedule continuation when future state depends on the change.
4. **System proof:** a focused cross-owner or gameplay scenario when the result depends on interaction rather
   than one owner alone.
5. **Exploration/audit:** bounded broader sampling only when looking for unknown interactions or at explicit
   checkpoints.

Define the owner contract before broad audit; local proof is insufficient for cross-owner/player claims. Treat
verification as a proof graph: reuse stronger evidence. A proof receipt names contract, owner/edge, command/test,
result, replay input, and relevant freshness basis, mapped to the task control coordinate.

A new production projection/envelope that replaces caller reconstruction must agree with canonical semantics on
representative feasible, limiting, infeasible, and stale cases.

### Diagnostic contract

Diagnostics should route directly to the controlling owner/action with stable identities, expected/actual
quantities/lifecycle, and replay input when relevant. Behavior evidence preserves world/policy seeds, observable
decision inputs, and canonical blocker/outcome; conservation/persistence failures name owners and discrepancy.

### Failure triage map

Start from the semantic failure class, not from the broadest command. Widen only when the evidence crosses
another owner or runtime boundary.

| Symptom / claim | Inspect first | Minimum distinguishing evidence |
| --- | --- | --- |
| Command rejected unexpectedly | resolver/validator and typed error | exact error fields plus authoritative preconditions |
| Rejected command changed state | validated commit and owned reservations/indexes | promised pre/post atomic surface equality |
| Validated command became stale | token dependencies and revisions | one intervening mutation per dependency plus typed stale rejection |
| Wrong matter/fluid/energy total | owning custody edge and accounting helper | first owner-by-owner discrepancy |
| Save fails trusted load | `LoadedSaveEnvelope::into_state`, then named validator | exact schema or typed validation error on the smallest state |
| Save continuation diverges | persisted schedules/provider traces and explicit inputs | first differing authoritative outcome/tick |
| Unexpected tick result | relevant `TickOutcome` and phase decision/apply | first differing tick plus driving pre-tick owner state |
| Production lifecycle wrong | job, resolution, availability, provider/support | remaining time, suspension reason, due tick, matching outcome |
| Mining extraction/claim mismatch | job, deposit, reservation, claim validator | lifecycle, mass bound, ready custody, reservation, claimed mass |
| Actor did nothing/chose poorly | observation, candidates, blockers, policy | explicit no-action/failure classification and replay seeds |
| Capability unreachable | [`STATUS.md`](STATUS.md) and acquisition topology | owner execution proof separated from ordinary acquisition |
| Gameplay result changed | affected focused scope | replay seeds, observations, choice, typed result, conserved totals |
| Caller searches nearby feasible requests | owning resolver/envelope | monotonic bound agreement without copied formulas |
| Complexity regressed | BCA changed-source review and owner shape | specific branch/ownership cost, not only a score |

Repair direct tooling failures before behavioral probes. Use `python ci.py gate --gameplay <scope>` for gameplay
failures. For an individual failure, prefer target-free `run_test.py`.

## Complexity review

`bca.toml` and `.bca-baseline.toml` own the cognitive-complexity ratchet used by `python ci.py quick`.
New or worsened over-threshold cognitive complexity fails the ratchet; other BCA metrics are advisory.

Use `python ci.py bca` for changed code and `python ci.py bca --hotspots` for current hotspots. Metrics are
diagnostic: refactor for clearer ownership/control flow, not to game the baseline.

### Rust agent diagnostics

Use `python tools/rust_diagnostics.py` only for a named uncertainty; [`tools/README.md`](tools/README.md) owns its operating details. Its bounded ownership view wraps `cargo modules structure --lib --no-fns --no-traits --no-types --max-depth 4`. Mutation, module, and expansion results are diagnostics, not completion gates.

## Unit tests

Unit-test bodies live beside their owner in `*_tests.rs` or `mod_tests.rs` and are included with
`#[cfg(test)] #[path = "..."] mod tests;`.

Assertions prove durable contracts:

- rejection: typed error and unchanged authoritative state when atomicity matters;
- success: resulting identity, quantity, lifecycle, relationship, ownership, or other durable state;
- receipt sufficiency: when the operation creates/chooses continuation identity or landing state, prove the
  returned outcome matches the durable owner result; do not require an outcome when caller-known identity is
  already sufficient;
- conservation: totals across authoritative owners;
- persistence: serialized continuation and trusted-load admission for state that survives load;
- authored values: read from registries instead of duplicating balance constants.

Avoid error prose, wall-clock, incidental order/count, and copied balance assertions. Prefer owner relationships.
Generators prove bounded variation unless an exact value is owned. Harnesses match meaningful typed errors and
fall back for unexpected variants. Interaction bugs reproduce interacting states.

Soak tests are ignored tests whose qualified name includes `soak`. Use them only when repeated ownership,
persistence, conservation, or numerical accumulation adds evidence that focused tests cannot provide.

## Gameplay evaluation

Automated-player boundaries/evidence semantics live in [`GAMEPLAY_EVALUATION.md`](GAMEPLAY_EVALUATION.md); read
it only for gameplay-harness behavior or interpretation.

Focused gameplay targets are compile surfaces, not contract collections. Each exposes one gate/probe and only its
support. Supported CI gameplay runs fixed anchors plus fresh bounded variation; direct Cargo uses fallback roots.
Explicit roots replay cases. Cheap cross-cutting contracts belong in `gameplay_contracts`; broad contracts use the
consolidated
`gameplay_audit` target. The default report emits compact measured summaries without a second CI-owned
interpretation layer; use `python ci.py report --verbose` for replayable preservation, woodworking, fieldwork,
and other episode detail.
Aggregate time uses the registry-derived clock. The broad audit includes `fieldwork_probe::batch_capped_mining_finishes_the_requested_order` for
ordinary extraction-order continuation; the fieldwork exploration episode remains report-driven.

## Completion

Run only the lanes required by the changed contract. Broad audits are explicit checkpoints, not default edit
loops. Verification is local; hosted CI is outside the project contract.

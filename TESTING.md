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
| List tests without building | `python tools/run_test.py --list [substring]` |
| Type-check a test target without linking | `python tools/run_test.py --check <qualified-name-or-unique-substring>` |
| Run one exact unit/integration test | `python tools/run_test.py <qualified-name-or-unique-substring>` |
| Run one owner/subsystem test group | `python tools/run_test.py --suite <qualified-prefix-or-substring>` |
| Focused gameplay | `python ci.py gate --gameplay {workshop,survival,progression,ore,foundry}` |
| Core audit | `python ci.py audit --core` |
| Gameplay audit | `python ci.py audit --gameplay` |
| Core + gameplay audit | `python ci.py audit --all` |
| Clippy | `python ci.py gate --lint` |
| Shader validation | `python ci.py gate --shaders` |
| Rustdoc | `python ci.py gate --rustdoc` |
| Long-horizon soak | `python ci.py gate --soak` |
| Gameplay exploration report | `python ci.py report` |
| Changed-source BCA review | `python ci.py bca [--path <scope>] [--since <revision>]` |
| Current BCA hotspot review | `python ci.py bca --hotspots [--path <scope>] [--since <revision>]` |

`quick` is build-free. `gate` runs one build lane and does not repeat `quick`; specialized flags replace its
default compile. `audit` checkpoints add `quick` to the selected runtime surface.

While code is unstable, use `cargo check-fast`; integration tests can use `run_test.py --check`. Then run one
exact/suite or focused gameplay proof. Reuse warm artifacts. Keep one test profile and shared support shape; no
prebuild or all-target repair-loop compile. Focused targets compile only declared harness modules; build-free
contracts catch missing dependencies. Gameplay gates keep maintained cases plus one fresh replayable organic
case; `report` broadens the sample.

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
representative feasible, limiting, infeasible, and stale cases. The harness is not its oracle.

### Diagnostic contract

Diagnostics should route directly to the controlling owner/action with stable identities, expected/actual
quantities/lifecycle, and replay input when relevant. Behavior evidence preserves world/policy seeds, observable
decision inputs, and canonical blocker/outcome; conservation/persistence failures name owners and discrepancy.

### Failure triage map

Start from the semantic failure class, not from the broadest available command. The first row that explains the
observed failure defines the initial read/test cone; widen only when that evidence crosses another boundary.

| Symptom / claim | Inspect first | Minimum distinguishing evidence | Widen when |
| --- | --- | --- | --- |
| Command rejected unexpectedly | operation resolver/validator and its dedicated typed error | exact error variant/fields plus the relevant immutable definitions and authoritative precondition record | the error wraps or names another owner boundary |
| Rejected command changed state | validated commit path and transaction-owned IDs/indexes/reservations | exact pre/post equality for the promised atomic surface | rollback spans another owner or scheduled work |
| Previously validated command became stale | `Validated*` token fields and every mutable dependency it binds | one focused intervening mutation per dependency class plus typed stale rejection | a dependency can change without the token's revision/identity noticing it |
| Wrong matter/fluid/energy total | owning custody edge plus `calculate_matter_accounting`, `calculate_fluid_volume_accounting`, or `calculate_explicit_energy_accounting` | exact owner-by-owner discrepancy before/after the one transition | the discrepancy first appears only after a later tick or load |
| Save fails trusted load | `LoadedSaveEnvelope::into_state`, then the named local/cross-owner validator | exact schema or typed `StateValidationError` / owner validation error; reproduce from the smallest corrupted/current-schema state | reconstruction changes data before the failing invariant or several owners disagree |
| Save loads but continuation diverges | persisted job/schedule/RNG/provider traces plus operation-specific replay validator | equal pre-save state semantics, exact replay input, and first differing authoritative outcome/tick | divergence starts in a different phase than the persisted work owner |
| Unexpected tick result | `TickOutcome` field for the effect, then `advance_tick` phase decision/apply pair | first tick where expected/actual outcome differs and the pre-tick owner revisions/state that drive that phase | another phase mutates a shared dependency before apply |
| Production completed/suspended/resumed incorrectly | `ProductionJobRecord`, `ProcessResolution`, availability changes, provider/support state | job identity, remaining active time, suspension reason, scheduled completion, and matching `TickOutcome` change | support, energy, labor, or destination reservation is the actual blocker |
| Mining extraction/claim mismatch | `MiningJobRecord`, geological remaining mass, destination reservation, claim validator | job lifecycle, before/post-extraction mass bound, ready custody, reserved inbound, claimed lot mass | another mining job or inventory mutation legitimately changed shared state |
| Actor did nothing or chose poorly | decision window: legitimate observation, generated candidates, production blockers, actor policy | classify as unobserved, generator gap, policy gate, validation gate, information gap, execution failure, inconsequential, dormant, or insufficient data | the classification itself requires hidden diagnostic truth |
| Capability appears implemented but cannot be reached | [`STATUS.md`](STATUS.md) current integration frontier plus acquisition graph/catalog contract | prove owner/canonical execution separately from ordinary acquisition | the missing edge is claimed reachable by current status/content |
| Gameplay result changed | focused scope matching the affected player-visible loop | exact replay seeds, observable decision inputs, selected action, typed result, relevant conserved totals | the behavior depends on another gameplay scope or an intentionally broad cross-system checkpoint |
| Caller probes nearby requests for one feasible bound | owning resolver and its capability/resource/lifetime limits | prove monotonicity and envelope agreement without copied formulas | alternatives are physically distinct or search order is itself under evaluation |
| Complexity or maintainability regression | BCA changed-source review and owner/control-flow shape | identify the specific branch/ownership cost, not only a numeric score | refactoring would cross ownership or public API boundaries |

Repair direct tooling failures before behavioral probes. Use `python ci.py gate --gameplay <scope>` rather than
reconstructing gameplay Cargo flags or treating a zero-match default catalog as absence.

## Complexity review

`bca.toml` and `.bca-baseline.toml` own the cognitive-complexity ratchet used by `python ci.py quick`.
New or worsened over-threshold cognitive complexity fails the ratchet; other BCA metrics are advisory.

Use `python ci.py bca` for changed code and `python ci.py bca --hotspots` for current hotspots. Metrics are
diagnostic: refactor for clearer ownership/control flow, not to game the baseline.

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

Avoid error-prose, wall-clock, incidental-order/count, or unowned balance assertions. Prefer one canonical-path
regression naming the invariant; interaction bugs reproduce the interacting states.

Soak tests are ignored tests whose qualified name includes `soak`. Use them only when repeated ownership,
persistence, conservation, or numerical accumulation adds evidence that focused tests cannot provide.

## Gameplay evaluation

Automated-player boundaries/evidence semantics live in [`GAMEPLAY_EVALUATION.md`](GAMEPLAY_EVALUATION.md); read
it only for gameplay-harness behavior or interpretation.

## Completion

Run only the lanes required by the changed contract. Broad audits are explicit checkpoints, not default edit
loops. Verification is local; hosted CI is outside the project contract.

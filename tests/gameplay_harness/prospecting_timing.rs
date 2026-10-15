//! Canonical completion stepping for already-admitted prospecting work.

use deep_hearth::core::state::AppState;
use deep_hearth::equipment::EquipmentId;
use deep_hearth::geology::{
    FieldProspectingOutcome, GeologicalEvidenceKind, player_prospecting_options,
};
use deep_hearth::labor::{ProspectingMethodId, ProspectingWork};
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;
use deep_hearth::spatial::VoxelBounds;

use super::tick_observation::{TickEventAllowance, assert_tick_events_within};

/// Selects one prospecting technique using only the player-facing prospecting catalog.
///
/// Exact-local actors may use only a currently usable local instrument instance. Explicitly
/// movement-abstracted capability/proxy witnesses have no admitted logistics player, so they may
/// match the disclosed instrument's authored definition but still do not select by method ID.
pub(super) fn select_visible_prospecting_method(
    registries: &Registries,
    state: &AppState,
    region: VoxelBounds,
    evidence: GeologicalEvidenceKind,
    equipment: Option<EquipmentId>,
    context: &'static str,
) -> ProspectingMethodId {
    let options = player_prospecting_options(registries, state, region)
        .unwrap_or_else(|error| panic!("gameplay harness {context} catalog failed: {error}"));
    let candidates = options
        .iter()
        .filter(|option| option.evidence() == evidence)
        .filter(|option| match equipment {
            None => !option.requires_equipment(),
            Some(equipment) => {
                if state.logistics().player().is_some() {
                    option
                        .instruments()
                        .iter()
                        .any(|instrument| instrument.equipment() == equipment)
                } else {
                    state
                        .equipment()
                        .get_equipment(equipment)
                        .is_some_and(|record| {
                            option
                                .accepted_instrument_definitions()
                                .contains(&record.definition())
                        })
                }
            }
        })
        .collect::<Vec<_>>();
    let best_key = candidates
        .iter()
        .map(|option| (option.duration().value(), option.abundance_uncertainty_ppm()))
        .min()
        .unwrap_or_else(|| {
            panic!(
                "gameplay harness {context} found no actor-visible {:?} survey for the chosen region/instrument",
                evidence
            )
        });
    let mut best = candidates.into_iter().filter(|option| {
        (
            option.duration().value(),
            option.abundance_uncertainty_ppm(),
        ) == best_key
    });
    let selected = best
        .next()
        .unwrap_or_else(|| unreachable!("best prospecting key came from a candidate"));
    assert!(
        best.next().is_none(),
        "gameplay harness {context} has multiple prospecting techniques tied on actor-visible attention and abundance quality; add a real player preference instead of using method identity"
    );
    selected.method()
}

/// Completes one admitted prospecting action by following its authoritative work schedule.
pub(super) fn complete_prospecting_work(
    registries: &Registries,
    state: &mut AppState,
    work: ProspectingWork,
    context: &'static str,
) -> FieldProspectingOutcome {
    assert!(
        state.tick() >= work.started_at() && state.tick() < work.completes_at(),
        "gameplay harness {context} requires active prospecting before its admitted completion"
    );
    let mut completion = None;
    while state.tick() < work.completes_at() {
        let outcome = advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("gameplay harness {context} tick failed: {error}"));
        if state.tick() < work.completes_at() {
            assert_eq!(
                outcome.field_prospecting(),
                None,
                "gameplay harness {context} observed prospecting before its admitted completion"
            );
        } else {
            completion = outcome.field_prospecting();
        }
        assert_tick_events_within(
            &outcome,
            TickEventAllowance {
                field_prospecting: true,
                ..TickEventAllowance::default()
            },
            context,
        );
    }
    completion.unwrap_or_else(|| {
        panic!("gameplay harness {context} prospecting produced no completion outcome")
    })
}

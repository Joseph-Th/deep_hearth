//! Adaptive multi-site recovery after the initial localized deposit cannot satisfy player demand.

use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::{AppState, validate_loaded_state};

use super::campaign::decide_fieldwork_survey_strategy;
use super::extraction::{FieldworkExtractionOrder, execute_fieldwork_extraction};
use super::preparation::upgrade_sampling_hammer;
use super::review::FieldworkEpisodeReview;
use super::survey::{
    FieldworkSurveyStrategy, QUATERNARY_CHANNEL_START_X, SECONDARY_CHANNEL_START_X,
    TERTIARY_CHANNEL_START_X, localize_target,
};

pub(super) struct InitialShortfallRecovery {
    pub(super) strategy: FieldworkSurveyStrategy,
    pub(super) upgrade_ticks: u64,
    pub(super) projected_point_search_ticks: u64,
    pub(super) projected_indexed_search_ticks: Option<u64>,
    pub(super) baseline_search_ticks: u64,
    pub(super) realized_attention_delta: i128,
    pub(super) sites_visited: u64,
    pub(super) search_ticks: u64,
    pub(super) extraction_ticks: u64,
    pub(super) additional_extracted: Mass,
    pub(super) fulfilled: Mass,
    pub(super) remaining: Mass,
    pub(super) terminal: &'static str,
}

#[derive(Clone, Copy)]
struct InitialShortfallRun {
    sites_visited: u64,
    search_ticks: u64,
    extraction_ticks: u64,
    additional_extracted: Mass,
    fulfilled: Mass,
    remaining: Mass,
}

fn run_initial_shortfall_sites(
    review: &FieldworkEpisodeReview<'_>,
    state: &mut AppState,
    strategy: FieldworkSurveyStrategy,
) -> InitialShortfallRun {
    let mut remaining = review
        .requested
        .checked_sub(review.extraction.extracted)
        .unwrap_or_else(|| unreachable!("initial fieldwork extraction cannot exceed its order"));
    assert!(
        !remaining.is_zero(),
        "initial shortfall recovery requires unfinished player demand"
    );
    let mut sites_visited = 0_u64;
    let mut search_ticks = 0_u64;
    let mut extraction_ticks = 0_u64;
    let mut additional_extracted = Mass::ZERO;
    for start_x in [
        SECONDARY_CHANNEL_START_X,
        TERTIARY_CHANNEL_START_X,
        QUATERNARY_CHANNEL_START_X,
    ] {
        if remaining.is_zero() {
            break;
        }
        let search_started_at = state.tick().value();
        let localization = localize_target(
            review.registries,
            state,
            review.sampling_hammer,
            review.channel_voxels,
            start_x,
            strategy,
        );
        search_ticks = search_ticks
            .checked_add(state.tick().value() - search_started_at)
            .unwrap_or_else(|| panic!("fieldwork multi-site recovery search time overflowed"));
        sites_visited += 1;

        let requested = remaining.min(localization.resource_mass.upper());
        assert!(
            !requested.is_zero(),
            "fieldwork multi-site recovery must resolve a nonzero local opportunity"
        );
        let extraction = execute_fieldwork_extraction(
            review.registries,
            state,
            FieldworkExtractionOrder {
                target: localization.target,
                destination: review.followup_destination,
                equipment: review.mining_equipment,
                requested,
                batch_limit: review.estimate.batch,
            },
        );
        assert!(
            !extraction.extracted.is_zero(),
            "fieldwork multi-site recovery must make productive progress"
        );
        extraction_ticks = extraction_ticks
            .checked_add(extraction.ticks)
            .unwrap_or_else(|| panic!("fieldwork multi-site recovery extraction time overflowed"));
        additional_extracted = additional_extracted
            .checked_add(extraction.extracted)
            .unwrap_or_else(|| panic!("fieldwork multi-site recovery mass overflowed"));
        remaining = remaining
            .checked_sub(extraction.extracted)
            .unwrap_or_else(|| unreachable!("recovery extraction cannot exceed remaining demand"));
    }

    validate_loaded_state(review.registries, state)
        .unwrap_or_else(|error| panic!("fieldwork multi-site recovery state invalid: {error}"));
    let fulfilled = review
        .extraction
        .extracted
        .checked_add(additional_extracted)
        .unwrap_or_else(|| panic!("fieldwork multi-site fulfilled mass overflowed"));
    assert_eq!(fulfilled.checked_add(remaining), Some(review.requested));
    InitialShortfallRun {
        sites_visited,
        search_ticks,
        extraction_ticks,
        additional_extracted,
        fulfilled,
        remaining,
    }
}

pub(super) fn execute_initial_shortfall_recovery(
    review: &FieldworkEpisodeReview<'_>,
) -> InitialShortfallRecovery {
    let planned_sites = 3_u64;
    let survey_decision = decide_fieldwork_survey_strategy(
        review.registries,
        review.state,
        review.raw,
        review.parts,
        review.channel_voxels,
        planned_sites,
    );

    // Freeze policy from the actor-visible projection above. The matched point-search branch below
    // is diagnostic evidence only; its realized result must never reselect the committed strategy.
    let mut baseline_state = review.state.clone();
    let baseline = run_initial_shortfall_sites(
        review,
        &mut baseline_state,
        FieldworkSurveyStrategy::PointSearch,
    );
    let (selected, upgrade_ticks) =
        if survey_decision.selected_strategy == FieldworkSurveyStrategy::IndexedChannel {
            let mut selected_state = review.state.clone();
            let actual = upgrade_sampling_hammer(
                review.registries,
                &mut selected_state,
                review.raw,
                review.parts,
                review.sampling_hammer,
            );
            assert_eq!(Some(actual), survey_decision.projected_upgrade_ticks);
            (
                run_initial_shortfall_sites(
                    review,
                    &mut selected_state,
                    FieldworkSurveyStrategy::IndexedChannel,
                ),
                actual,
            )
        } else {
            (baseline, 0)
        };
    assert_eq!(
        selected.additional_extracted, baseline.additional_extracted,
        "survey strategy must not change recoverable matter in the matched shortfall campaign"
    );
    assert_eq!(
        selected.remaining, baseline.remaining,
        "survey strategy must not change shortfall terminal demand"
    );
    let selected_attention = selected
        .search_ticks
        .checked_add(upgrade_ticks)
        .unwrap_or_else(|| panic!("fieldwork shortfall selected attention overflowed"));

    InitialShortfallRecovery {
        strategy: survey_decision.selected_strategy,
        upgrade_ticks,
        projected_point_search_ticks: survey_decision.projected_point_search_ticks,
        projected_indexed_search_ticks: survey_decision.projected_indexed_search_ticks,
        baseline_search_ticks: baseline.search_ticks,
        realized_attention_delta: i128::from(baseline.search_ticks)
            - i128::from(selected_attention),
        sites_visited: selected.sites_visited,
        search_ticks: selected.search_ticks,
        extraction_ticks: selected.extraction_ticks,
        additional_extracted: selected.additional_extracted,
        fulfilled: selected.fulfilled,
        remaining: selected.remaining,
        terminal: if selected.remaining.is_zero() {
            "order-complete"
        } else {
            "local-search-area-exhausted"
        },
    }
}

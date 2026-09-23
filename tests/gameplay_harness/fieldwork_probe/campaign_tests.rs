//! Survey-campaign investment policy contracts.

use super::*;
use deep_hearth::content::build_registries;
use deep_hearth::core::quantity::Mass;

use super::super::preparation::assemble_sampling_hammer;
use super::super::world::build_fieldwork_world;

#[test]
fn maintained_fieldwork_witnesses_span_campaign_horizons() {
    assert_eq!(planned_future_sites(1), 1);
    assert_eq!(
        [
            planned_future_sites(2),
            planned_future_sites(3),
            planned_future_sites(6),
        ],
        [1, 3, 2],
        "maintained coverage must include a copper-capable three-site survey-investment witness"
    );
}

#[test]
fn survey_investment_requires_a_material_disclosed_attention_payoff() {
    assert_eq!(
        select_survey_strategy(102, Some(118)),
        FieldworkSurveyStrategy::PointSearch
    );
    assert_eq!(
        select_survey_strategy(204, Some(196)),
        FieldworkSurveyStrategy::PointSearch,
        "an eight-tick expected gain is too small to justify scarce-copper survey capital"
    );
    assert_eq!(
        select_survey_strategy(306, Some(274)),
        FieldworkSurveyStrategy::IndexedChannel,
        "a three-site campaign clears the minimum expected-return threshold"
    );
    assert_eq!(
        select_survey_strategy(196, Some(196)),
        FieldworkSurveyStrategy::PointSearch
    );
    assert_eq!(
        select_survey_strategy(204, None),
        FieldworkSurveyStrategy::PointSearch
    );
}

#[test]
fn three_site_survey_decision_respects_both_expected_return_and_upgrade_supply() {
    let registries = build_registries();
    let requested = Mass::from_milligrams(24_000_000);

    let mut funded =
        build_fieldwork_world(&registries, 1, requested, Mass::from_milligrams(4_500_000));
    assert!(
        funded.copper_rich,
        "maintained funded witness lost copper supply"
    );
    let _ = assemble_sampling_hammer(&registries, &mut funded.state, funded.raw, funded.parts);
    let funded_decision = decide_fieldwork_survey_strategy(
        &registries,
        &funded.state,
        funded.raw,
        funded.parts,
        funded.channel_voxels,
        3,
    );
    assert_eq!(
        funded_decision.selected_strategy,
        FieldworkSurveyStrategy::IndexedChannel
    );
    assert!(funded_decision.projected_upgrade_ticks.is_some());

    let mut unfunded =
        build_fieldwork_world(&registries, 2, requested, Mass::from_milligrams(4_500_000));
    assert!(
        !unfunded.copper_rich,
        "maintained unfunded witness unexpectedly gained copper supply"
    );
    let _ = assemble_sampling_hammer(
        &registries,
        &mut unfunded.state,
        unfunded.raw,
        unfunded.parts,
    );
    let unfunded_decision = decide_fieldwork_survey_strategy(
        &registries,
        &unfunded.state,
        unfunded.raw,
        unfunded.parts,
        unfunded.channel_voxels,
        3,
    );
    assert_eq!(
        unfunded_decision.selected_strategy,
        FieldworkSurveyStrategy::PointSearch
    );
    assert_eq!(unfunded_decision.projected_upgrade_ticks, None);
}

//! Survey-campaign investment policy contracts.

use super::*;
use deep_hearth::content::build_registries;
use deep_hearth::core::quantity::Mass;

use super::super::preparation::assemble_sampling_hammer;
use super::super::world::{FieldworkWorld, build_fieldwork_world};

fn has_visible_native_copper(world: &FieldworkWorld) -> bool {
    world
        .state
        .inventory()
        .get_stockpile(world.raw)
        .is_some_and(|stockpile| !stockpile.get_mass(world.native_copper).is_zero())
}

fn expected_campaign_horizons() -> std::collections::BTreeSet<u64> {
    let maximum = u64::try_from(FOLLOWUP_CHANNEL_STARTS.len())
        .unwrap_or_else(|_| unreachable!("bounded fieldwork site count fits u64"));
    (1..=maximum).collect()
}

#[test]
fn maintained_fieldwork_witnesses_span_campaign_horizons() {
    use super::super::super::focused_case::{FocusedProbeCase, FocusedProbeRole};

    let maintained = |seed| FocusedProbeCase::new(seed, None, FocusedProbeRole::MaintainedCoverage);
    let horizons = (0..128)
        .map(|seed| planned_future_sites(maintained(seed)))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        horizons,
        expected_campaign_horizons(),
        "maintained fieldwork coverage must exercise every bounded survey horizon"
    );
}

#[test]
fn organic_campaign_horizon_depends_on_behavior_seed_not_world_seed() {
    use super::super::super::focused_case::{FocusedProbeCase, FocusedProbeRole};

    let first_world = FocusedProbeCase::new(1, Some(3), FocusedProbeRole::OrganicVariation);
    let second_world = FocusedProbeCase::new(99, Some(3), FocusedProbeRole::OrganicVariation);
    assert_eq!(
        planned_future_sites(first_world),
        planned_future_sites(second_world)
    );

    let mut horizons = std::collections::BTreeSet::new();
    for behavior_seed in 0..128 {
        horizons.insert(planned_future_sites(FocusedProbeCase::new(
            1,
            Some(behavior_seed),
            FocusedProbeRole::OrganicVariation,
        )));
    }
    assert_eq!(horizons, expected_campaign_horizons());
}

#[test]
fn point_search_projection_accounts_for_barren_future_ground_without_hidden_world_frequency() {
    let registries = build_registries();
    let policy = FieldworkSurveyPolicy::baseline();
    assert_eq!(
        policy.productive_site_prior_ppm().checked_mul(2),
        Some(SITE_PRODUCTIVITY_SCALE_PPM),
        "baseline fieldwork policy must stay explicitly neutral rather than copy a generated success rate"
    );
    let channel_voxels = i64::try_from(
        registries
            .labor()
            .get_prospecting(PROSPECTING_LOCAL_TRANSECT)
            .map(|definition| definition.maximum_region_voxels())
            .unwrap_or_else(|| panic!("fieldwork local-transect definition disappeared")),
    )
    .unwrap_or_else(|_| panic!("fieldwork local-transect span exceeds coordinate range"));
    let channel_count = u64::try_from(CHANNEL_COUNT)
        .unwrap_or_else(|_| unreachable!("positive channel count fits u64"));
    let channel_voxel_count = u64::try_from(channel_voxels)
        .unwrap_or_else(|_| panic!("fieldwork campaign channel width must be positive"));
    let common_transects = channel_count
        .checked_mul(projected_prospecting_ticks(
            &registries,
            PROSPECTING_LOCAL_TRANSECT,
            horizontal_region(0, channel_voxels),
        ))
        .unwrap_or_else(|| panic!("fieldwork campaign transect duration overflowed"));
    let point = horizontal_region(0, 1);
    let inspection = projected_prospecting_ticks(&registries, PROSPECTING_FIELD_INSPECTION, point);
    let detailed =
        projected_prospecting_ticks(&registries, PROSPECTING_DETAILED_FIELD_SURVEY, point);
    let productive_doubled_variable = channel_voxel_count
        .checked_add(1)
        .and_then(|count| count.checked_mul(inspection))
        .and_then(|ticks| ticks.checked_add(detailed.checked_mul(2)?))
        .unwrap_or_else(|| panic!("fieldwork productive search duration overflowed"));
    let barren_doubled_variable = channel_voxel_count
        .checked_mul(inspection)
        .and_then(|ticks| ticks.checked_mul(2))
        .unwrap_or_else(|| panic!("fieldwork barren search duration overflowed"));
    let neutral_variable = productive_doubled_variable
        .checked_add(barren_doubled_variable)
        .unwrap_or_else(|| panic!("fieldwork neutral search duration overflowed"))
        .div_ceil(4);
    let expected = common_transects
        .checked_add(neutral_variable)
        .unwrap_or_else(|| panic!("fieldwork neutral search projection overflowed"));
    assert_eq!(
        expected_point_search_ticks(&registries, channel_voxels, policy),
        expected,
        "two transects plus a neutral mix of productive mean-search and full barren scan must own the forecast"
    );
}

#[test]
fn survey_investment_requires_a_material_disclosed_attention_payoff() {
    let policy = FieldworkSurveyPolicy::baseline();
    let indexed_ticks = 1_000_000_u64;
    let required_saved_ticks = u64::try_from(policy.minimum_return_ppm())
        .unwrap_or_else(|_| unreachable!("survey return policy fits u64"));
    assert_eq!(
        select_survey_strategy(indexed_ticks, Some(indexed_ticks + 1), policy),
        FieldworkSurveyStrategy::PointSearch
    );
    assert_eq!(
        select_survey_strategy(
            indexed_ticks + required_saved_ticks.saturating_sub(1),
            Some(indexed_ticks),
            policy,
        ),
        FieldworkSurveyStrategy::PointSearch,
        "a return below the policy floor must not justify scarce-copper survey capital"
    );
    assert_eq!(
        select_survey_strategy(
            indexed_ticks + required_saved_ticks,
            Some(indexed_ticks),
            policy,
        ),
        FieldworkSurveyStrategy::IndexedChannel,
        "meeting the disclosed return floor must justify the indexed survey"
    );
    assert_eq!(
        select_survey_strategy(indexed_ticks, Some(indexed_ticks), policy),
        FieldworkSurveyStrategy::PointSearch
    );
    assert_eq!(
        select_survey_strategy(indexed_ticks, None, policy),
        FieldworkSurveyStrategy::PointSearch
    );
}

#[test]
fn behavior_seed_varies_survey_investment_tolerance_without_changing_physics() {
    let baseline = FieldworkSurveyPolicy::baseline();
    let eager = FieldworkSurveyPolicy::from_behavior_seed(0);
    let cautious = FieldworkSurveyPolicy::from_behavior_seed(50_000);
    assert!(eager.minimum_return_ppm() < baseline.minimum_return_ppm());
    assert!(baseline.minimum_return_ppm() < cautious.minimum_return_ppm());
    assert_eq!(
        eager.productive_site_prior_ppm(),
        cautious.productive_site_prior_ppm(),
        "behavior entropy may vary return tolerance but not smuggle a different hidden-world success rate"
    );
    let indexed_ticks = 1_000_000_u64;
    let midpoint_return_ppm = (eager.minimum_return_ppm() + cautious.minimum_return_ppm()) / 2;
    let midpoint_saved_ticks = u64::try_from(midpoint_return_ppm)
        .unwrap_or_else(|_| unreachable!("survey return policy fits u64"));
    assert_eq!(
        select_survey_strategy(
            indexed_ticks + midpoint_saved_ticks,
            Some(indexed_ticks),
            eager
        ),
        FieldworkSurveyStrategy::IndexedChannel
    );
    assert_eq!(
        select_survey_strategy(
            indexed_ticks + midpoint_saved_ticks,
            Some(indexed_ticks),
            cautious,
        ),
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
        has_visible_native_copper(&funded),
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
        FieldworkSurveyPolicy::baseline(),
    );
    assert_eq!(
        funded_decision.selected_strategy,
        FieldworkSurveyStrategy::IndexedChannel
    );
    assert!(funded_decision.projected_upgrade_ticks.is_some());

    let mut unfunded =
        build_fieldwork_world(&registries, 2, requested, Mass::from_milligrams(4_500_000));
    assert!(
        !has_visible_native_copper(&unfunded),
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
        FieldworkSurveyPolicy::baseline(),
    );
    assert_eq!(
        unfunded_decision.selected_strategy,
        FieldworkSurveyStrategy::PointSearch
    );
    assert_eq!(unfunded_decision.projected_upgrade_ticks, None);
}

//! Matched future-site campaign used to evaluate geological information investment.

use deep_hearth::content::{
    PROSPECTING_DETAILED_FIELD_SURVEY, PROSPECTING_FIELD_INSPECTION,
    PROSPECTING_INDEXED_CHANNEL_SURVEY, PROSPECTING_LOCAL_TRANSECT,
};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::equipment::EquipmentId;
use deep_hearth::inventory::StockpileId;
use deep_hearth::labor::{ProspectingMethodId, project_prospecting_work};
use deep_hearth::registry::Registries;
use deep_hearth::spatial::VoxelBounds;

use super::super::focused_case::{FocusedProbeCase, FocusedProbeRole};
use super::super::seed::mix64;
use super::planning::project_sampling_hammer_upgrade_ticks;
use super::preparation::upgrade_sampling_hammer;
use super::survey::{CHANNEL_COUNT, FieldworkSurveyStrategy, horizontal_region, search_target};

#[derive(Clone, Copy)]
pub(super) struct FieldworkCampaignSite {
    pub(super) start_x: i64,
}

pub(super) struct FieldworkSurveyCampaignPlan<'a> {
    pub(super) raw: StockpileId,
    pub(super) parts: StockpileId,
    pub(super) hammer: EquipmentId,
    pub(super) channel_voxels: i64,
    pub(super) sites: &'a [FieldworkCampaignSite],
    pub(super) planned_sites: u64,
    pub(super) investment_policy: FieldworkSurveyPolicy,
}

struct CampaignExecutionPlan<'a> {
    hammer: EquipmentId,
    channel_voxels: i64,
    sites: &'a [FieldworkCampaignSite],
}

#[derive(Clone, Copy)]
struct CampaignRun {
    search_ticks: u64,
    first_search_ticks: u64,
    first_site_localized: bool,
    localized_sites: u64,
}

pub(super) struct FieldworkSurveyCampaignReview {
    pub(super) planned_sites: u64,
    pub(super) selected_strategy: FieldworkSurveyStrategy,
    pub(super) upgrade_available: bool,
    pub(super) upgrade_ticks: u64,
    pub(super) projected_point_search_ticks: u64,
    pub(super) projected_indexed_search_ticks: Option<u64>,
    pub(super) baseline_search_ticks: u64,
    pub(super) selected_search_ticks: u64,
    pub(super) realized_attention_delta: i128,
    pub(super) first_search_ticks: u64,
    pub(super) first_site_localized: bool,
    pub(super) localized_sites: u64,
    pub(super) barren_sites: u64,
    pub(super) investment_policy: FieldworkSurveyPolicy,
}

#[derive(Clone, Copy)]
pub(super) struct FieldworkSurveyDecision {
    pub(super) selected_strategy: FieldworkSurveyStrategy,
    pub(super) projected_upgrade_ticks: Option<u64>,
    pub(super) projected_point_search_ticks: u64,
    pub(super) projected_indexed_search_ticks: Option<u64>,
}

pub(super) const MINIMUM_SURVEY_INVESTMENT_RETURN_PPM: u128 = 100_000;
const ORGANIC_MINIMUM_SURVEY_RETURN_PPM: u128 = 75_000;
const ORGANIC_MAXIMUM_SURVEY_RETURN_PPM: u128 = 125_000;
const SITE_PRODUCTIVITY_SCALE_PPM: u128 = 1_000_000;
const NEUTRAL_PRODUCTIVE_SITE_PRIOR_PPM: u128 = SITE_PRODUCTIVITY_SCALE_PPM / 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct FieldworkSurveyPolicy {
    minimum_return_ppm: u128,
    productive_site_prior_ppm: u128,
}

impl FieldworkSurveyPolicy {
    pub(super) const fn baseline() -> Self {
        Self {
            minimum_return_ppm: MINIMUM_SURVEY_INVESTMENT_RETURN_PPM,
            productive_site_prior_ppm: NEUTRAL_PRODUCTIVE_SITE_PRIOR_PPM,
        }
    }

    pub(super) const fn from_behavior_seed(seed: u64) -> Self {
        let span = ORGANIC_MAXIMUM_SURVEY_RETURN_PPM - ORGANIC_MINIMUM_SURVEY_RETURN_PPM;
        Self {
            minimum_return_ppm: ORGANIC_MINIMUM_SURVEY_RETURN_PPM + (seed as u128) % (span + 1),
            // Unknown neighboring ground is not assigned the hidden world's generated success
            // frequency. Keep one explicit neutral prior while behavior entropy varies only the
            // actor's capital-return tolerance.
            productive_site_prior_ppm: NEUTRAL_PRODUCTIVE_SITE_PRIOR_PPM,
        }
    }

    pub(super) const fn minimum_return_ppm(self) -> u128 {
        self.minimum_return_ppm
    }

    pub(super) const fn productive_site_prior_ppm(self) -> u128 {
        self.productive_site_prior_ppm
    }
}

pub(super) fn planned_future_sites(case: FocusedProbeCase) -> u64 {
    let policy_seed = match case.role() {
        // Maintained cases intentionally attach stable policy to their maintained world identity.
        FocusedProbeRole::MaintainedAnchor | FocusedProbeRole::MaintainedCoverage => case.seed(),
        // Choice-rich organic/replay policy must be independent of physical world generation.
        FocusedProbeRole::OrganicVariation | FocusedProbeRole::ExplicitReplay => {
            case.required_behavior_seed("fieldwork survey campaign horizon")
        }
    };
    1 + mix64(policy_seed ^ 0x4649_454C_4443_41BE) % 3
}

fn projected_prospecting_ticks(
    registries: &Registries,
    method: ProspectingMethodId,
    region: VoxelBounds,
) -> u64 {
    project_prospecting_work(registries, method, region)
        .unwrap_or_else(|error| panic!("fieldwork campaign prospecting projection failed: {error}"))
        .duration()
        .value()
}

fn expected_point_search_ticks(
    registries: &Registries,
    channel_voxels: i64,
    policy: FieldworkSurveyPolicy,
) -> u64 {
    let channel_voxel_count = u64::try_from(channel_voxels)
        .unwrap_or_else(|_| panic!("fieldwork campaign channel width must be positive"));
    let channel_region = horizontal_region(0, channel_voxels);
    let point_region = horizontal_region(0, 1);
    let common_transects = u64::try_from(CHANNEL_COUNT)
        .unwrap_or_else(|_| unreachable!("positive channel count fits u64"))
        .checked_mul(projected_prospecting_ticks(
            registries,
            PROSPECTING_LOCAL_TRANSECT,
            channel_region,
        ))
        .unwrap_or_else(|| panic!("fieldwork campaign transect duration overflowed"));
    let inspection =
        projected_prospecting_ticks(registries, PROSPECTING_FIELD_INSPECTION, point_region);
    let detailed =
        projected_prospecting_ticks(registries, PROSPECTING_DETAILED_FIELD_SURVEY, point_region);
    // Within a selected channel no actor-visible fact favors one unsampled voxel over another, so
    // the productive-site projection uses the symmetric mean inspection count. Whether a new area
    // is productive at all is different: follow-up geology may legitimately be barren, and its
    // generated frequency is hidden. Weight productive and barren search costs only by the explicit
    // actor prior instead of silently assuming every future area contains copper.
    let productive_doubled_variable = channel_voxel_count
        .checked_add(1)
        .and_then(|count| count.checked_mul(inspection))
        .and_then(|ticks| ticks.checked_add(detailed.checked_mul(2)?))
        .unwrap_or_else(|| panic!("fieldwork campaign expected point-search duration overflowed"));
    let barren_variable = channel_voxel_count
        .checked_mul(inspection)
        .unwrap_or_else(|| panic!("fieldwork barren-site search duration overflowed"));
    let productive_prior = policy.productive_site_prior_ppm();
    assert!(productive_prior <= SITE_PRODUCTIVITY_SCALE_PPM);
    let barren_prior = SITE_PRODUCTIVITY_SCALE_PPM - productive_prior;
    let weighted_doubled_variable = u128::from(productive_doubled_variable)
        .checked_mul(productive_prior)
        .and_then(|productive| {
            u128::from(barren_variable)
                .checked_mul(2)
                .and_then(|barren| barren.checked_mul(barren_prior))
                .and_then(|barren| productive.checked_add(barren))
        })
        .unwrap_or_else(|| panic!("fieldwork campaign weighted search projection overflowed"));
    let variable_ticks = weighted_doubled_variable.div_ceil(SITE_PRODUCTIVITY_SCALE_PPM * 2);
    let variable_ticks = u64::try_from(variable_ticks)
        .unwrap_or_else(|_| panic!("fieldwork campaign search projection exceeds tick range"));
    common_transects
        .checked_add(variable_ticks)
        .unwrap_or_else(|| panic!("fieldwork campaign point-search duration overflowed"))
}

fn indexed_search_ticks(registries: &Registries, channel_voxels: i64) -> u64 {
    let channel_region = horizontal_region(0, channel_voxels);
    let common_transects = u64::try_from(CHANNEL_COUNT)
        .unwrap_or_else(|_| unreachable!("positive channel count fits u64"))
        .checked_mul(projected_prospecting_ticks(
            registries,
            PROSPECTING_LOCAL_TRANSECT,
            channel_region,
        ))
        .unwrap_or_else(|| panic!("fieldwork campaign transect duration overflowed"));
    common_transects
        .checked_add(projected_prospecting_ticks(
            registries,
            PROSPECTING_INDEXED_CHANNEL_SURVEY,
            channel_region,
        ))
        .unwrap_or_else(|| panic!("fieldwork campaign indexed-search duration overflowed"))
}

fn select_survey_strategy(
    projected_point_search_ticks: u64,
    projected_indexed_search_ticks: Option<u64>,
    policy: FieldworkSurveyPolicy,
) -> FieldworkSurveyStrategy {
    let Some(indexed) = projected_indexed_search_ticks else {
        return FieldworkSurveyStrategy::PointSearch;
    };
    let Some(saved) = projected_point_search_ticks.checked_sub(indexed) else {
        return FieldworkSurveyStrategy::PointSearch;
    };
    if saved == 0 {
        return FieldworkSurveyStrategy::PointSearch;
    }
    let return_ppm = u128::from(saved)
        .checked_mul(1_000_000)
        .and_then(|value| value.checked_div(u128::from(indexed)))
        .unwrap_or_else(|| panic!("fieldwork survey-investment return overflowed"));
    if return_ppm >= policy.minimum_return_ppm() {
        return FieldworkSurveyStrategy::IndexedChannel;
    }
    // The indexed hammer consumes a scarce copper reinforcement parcel. A merely positive
    // expected tick delta is not enough to justify that capital spend; small expeditions retain
    // the already-owned stone hammer until the disclosed campaign clears a material return.
    FieldworkSurveyStrategy::PointSearch
}

pub(super) fn decide_fieldwork_survey_strategy(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    parts: StockpileId,
    channel_voxels: i64,
    planned_sites: u64,
    policy: FieldworkSurveyPolicy,
) -> FieldworkSurveyDecision {
    assert!(
        planned_sites > 0,
        "fieldwork survey plan requires at least one future site"
    );
    let point_per_site = expected_point_search_ticks(registries, channel_voxels, policy);
    let indexed_per_site = indexed_search_ticks(registries, channel_voxels);
    let projected_point_search_ticks = point_per_site
        .checked_mul(planned_sites)
        .unwrap_or_else(|| panic!("fieldwork point campaign projection overflowed"));
    let projected_upgrade_ticks =
        project_sampling_hammer_upgrade_ticks(registries, state, raw, parts);
    let projected_indexed_search_ticks = projected_upgrade_ticks.map(|upgrade| {
        indexed_per_site
            .checked_mul(planned_sites)
            .and_then(|search| search.checked_add(upgrade))
            .unwrap_or_else(|| panic!("fieldwork indexed campaign projection overflowed"))
    });
    FieldworkSurveyDecision {
        selected_strategy: select_survey_strategy(
            projected_point_search_ticks,
            projected_indexed_search_ticks,
            policy,
        ),
        projected_upgrade_ticks,
        projected_point_search_ticks,
        projected_indexed_search_ticks,
    }
}

fn run_sites(
    registries: &Registries,
    state: &mut AppState,
    strategy: FieldworkSurveyStrategy,
    plan: &CampaignExecutionPlan<'_>,
) -> CampaignRun {
    let mut search_ticks = 0_u64;
    let mut first_search_ticks = None;
    let mut first_site_localized = None;
    let mut localized_sites = 0_u64;
    for (index, site) in plan.sites.iter().enumerate() {
        let search_started_at = state.tick().value();
        let localized = search_target(
            registries,
            state,
            plan.hammer,
            plan.channel_voxels,
            site.start_x,
            strategy,
        );
        localized_sites += u64::from(localized.is_some());
        let site_search_ticks = state.tick().value() - search_started_at;
        search_ticks = search_ticks
            .checked_add(site_search_ticks)
            .unwrap_or_else(|| panic!("fieldwork campaign search duration overflowed"));
        if index == 0 {
            first_search_ticks = Some(site_search_ticks);
            first_site_localized = Some(localized.is_some());
        }
    }
    validate_loaded_state(registries, state)
        .unwrap_or_else(|error| panic!("fieldwork campaign state invalid: {error}"));
    CampaignRun {
        search_ticks,
        first_search_ticks: first_search_ticks
            .unwrap_or_else(|| unreachable!("fieldwork campaign has at least one site")),
        first_site_localized: first_site_localized
            .unwrap_or_else(|| unreachable!("fieldwork campaign has at least one site")),
        localized_sites,
    }
}

pub(super) fn evaluate_fieldwork_survey_campaign(
    registries: &Registries,
    state: &AppState,
    plan: FieldworkSurveyCampaignPlan<'_>,
) -> FieldworkSurveyCampaignReview {
    let planned_sites = plan.planned_sites;
    let site_count = usize::try_from(planned_sites)
        .unwrap_or_else(|_| unreachable!("bounded fieldwork campaign fits usize"));
    assert!((1..=plan.sites.len()).contains(&site_count));
    let execution = CampaignExecutionPlan {
        hammer: plan.hammer,
        channel_voxels: plan.channel_voxels,
        sites: &plan.sites[..site_count],
    };
    let decision = decide_fieldwork_survey_strategy(
        registries,
        state,
        plan.raw,
        plan.parts,
        plan.channel_voxels,
        planned_sites,
        plan.investment_policy,
    );
    let selected_strategy = decision.selected_strategy;

    let mut baseline_state = state.clone();
    let baseline = run_sites(
        registries,
        &mut baseline_state,
        FieldworkSurveyStrategy::PointSearch,
        &execution,
    );
    let (selected, upgrade_ticks) = if selected_strategy == FieldworkSurveyStrategy::IndexedChannel
    {
        let mut selected_state = state.clone();
        let upgrade_ticks = upgrade_sampling_hammer(
            registries,
            &mut selected_state,
            plan.raw,
            plan.parts,
            plan.hammer,
        );
        assert_eq!(Some(upgrade_ticks), decision.projected_upgrade_ticks);
        let selected = run_sites(
            registries,
            &mut selected_state,
            FieldworkSurveyStrategy::IndexedChannel,
            &execution,
        );
        (selected, upgrade_ticks)
    } else {
        (baseline, 0)
    };
    let selected_attention = selected
        .search_ticks
        .checked_add(upgrade_ticks)
        .unwrap_or_else(|| panic!("fieldwork selected campaign attention overflowed"));
    assert_eq!(
        baseline.localized_sites, selected.localized_sites,
        "fieldwork survey strategies must agree on whether each searched area contains a resolvable target"
    );
    assert_eq!(
        baseline.first_site_localized, selected.first_site_localized,
        "fieldwork survey strategies must agree on whether the first future area contains a resolvable target"
    );
    FieldworkSurveyCampaignReview {
        planned_sites,
        selected_strategy,
        upgrade_available: decision.projected_upgrade_ticks.is_some(),
        upgrade_ticks,
        projected_point_search_ticks: decision.projected_point_search_ticks,
        projected_indexed_search_ticks: decision.projected_indexed_search_ticks,
        baseline_search_ticks: baseline.search_ticks,
        selected_search_ticks: selected.search_ticks,
        realized_attention_delta: i128::from(baseline.search_ticks)
            - i128::from(selected_attention),
        first_search_ticks: selected.first_search_ticks,
        first_site_localized: selected.first_site_localized,
        localized_sites: selected.localized_sites,
        barren_sites: planned_sites
            .checked_sub(selected.localized_sites)
            .unwrap_or_else(|| {
                unreachable!("localized campaign sites cannot exceed planned sites")
            }),
        investment_policy: plan.investment_policy,
    }
}

#[cfg(test)]
include_fieldwork_campaign_contract_tests!();

//! Matched future-site campaign used to evaluate geological information investment.

use deep_hearth::content::{
    PROSPECTING_DETAILED_FIELD_SURVEY, PROSPECTING_FIELD_INSPECTION,
    PROSPECTING_INDEXED_CHANNEL_SURVEY, PROSPECTING_LOCAL_TRANSECT,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::equipment::EquipmentId;
use deep_hearth::inventory::StockpileId;
use deep_hearth::registry::Registries;

use super::super::seed::mix64;
use super::extraction::{FieldworkExtractionOrder, execute_fieldwork_extraction};
use super::planning::project_sampling_hammer_upgrade_ticks;
use super::preparation::upgrade_sampling_hammer;
use super::survey::{CHANNEL_COUNT, FieldworkSurveyStrategy, localize_target};

#[derive(Clone, Copy)]
pub(super) struct FieldworkCampaignSite {
    pub(super) start_x: i64,
    pub(super) destination: StockpileId,
}

pub(super) struct FieldworkSurveyCampaignPlan<'a> {
    pub(super) raw: StockpileId,
    pub(super) parts: StockpileId,
    pub(super) hammer: EquipmentId,
    pub(super) mining_equipment: EquipmentId,
    pub(super) batch: Mass,
    pub(super) channel_voxels: i64,
    pub(super) sites: &'a [FieldworkCampaignSite],
    pub(super) planned_sites: u64,
}

struct CampaignExecutionPlan<'a> {
    hammer: EquipmentId,
    mining_equipment: EquipmentId,
    batch: Mass,
    channel_voxels: i64,
    sites: &'a [FieldworkCampaignSite],
}

#[derive(Clone, Copy)]
struct CampaignRun {
    search_ticks: u64,
    extraction_ticks: u64,
    extracted: Mass,
    first_search_ticks: u64,
    first_extraction_ticks: u64,
    first_extracted: Mass,
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
    pub(super) extraction_ticks: u64,
    pub(super) extracted: Mass,
    pub(super) realized_attention_delta: i128,
    pub(super) first_search_ticks: u64,
    pub(super) first_extraction_ticks: u64,
    pub(super) first_extracted: Mass,
}

#[derive(Clone, Copy)]
pub(super) struct FieldworkSurveyDecision {
    pub(super) selected_strategy: FieldworkSurveyStrategy,
    pub(super) projected_upgrade_ticks: Option<u64>,
    pub(super) projected_point_search_ticks: u64,
    pub(super) projected_indexed_search_ticks: Option<u64>,
}

const MINIMUM_SURVEY_INVESTMENT_RETURN_PPM: u128 = 100_000;

pub(super) fn planned_future_sites(seed: u64) -> u64 {
    // Keep organic/replay horizons seed-driven while making the maintained fieldwork witnesses
    // span one-, two-, and three-site campaigns. The three-site case is where the authored indexed
    // survey can rationally repay its copper reinforcement, so routine reports must not depend on
    // organic luck to exercise that decision.
    1 + mix64(seed ^ 0x4649_454C_4443_41BE) % 3
}

fn prospecting_duration(
    registries: &Registries,
    method: deep_hearth::labor::ProspectingMethodId,
) -> u64 {
    registries
        .labor()
        .get_prospecting(method)
        .map(|definition| definition.duration().value())
        .unwrap_or_else(|| panic!("fieldwork campaign prospecting method disappeared"))
}

fn expected_point_search_ticks(registries: &Registries, channel_voxels: i64) -> u64 {
    let channel_voxels = u64::try_from(channel_voxels)
        .unwrap_or_else(|_| panic!("fieldwork campaign channel width must be positive"));
    let common_transects = u64::try_from(CHANNEL_COUNT)
        .unwrap_or_else(|_| unreachable!("positive channel count fits u64"))
        .checked_mul(prospecting_duration(registries, PROSPECTING_LOCAL_TRANSECT))
        .unwrap_or_else(|| panic!("fieldwork campaign transect duration overflowed"));
    let inspection = prospecting_duration(registries, PROSPECTING_FIELD_INSPECTION);
    let detailed = prospecting_duration(registries, PROSPECTING_DETAILED_FIELD_SURVEY);
    let doubled_variable = channel_voxels
        .checked_add(1)
        .and_then(|count| count.checked_mul(inspection))
        .and_then(|ticks| ticks.checked_add(detailed.checked_mul(2)?))
        .unwrap_or_else(|| panic!("fieldwork campaign expected point-search duration overflowed"));
    assert!(
        doubled_variable.is_multiple_of(2),
        "fieldwork campaign expected point-search duration must resolve to whole ticks"
    );
    common_transects
        .checked_add(doubled_variable / 2)
        .unwrap_or_else(|| panic!("fieldwork campaign point-search duration overflowed"))
}

fn indexed_search_ticks(registries: &Registries) -> u64 {
    let common_transects = u64::try_from(CHANNEL_COUNT)
        .unwrap_or_else(|_| unreachable!("positive channel count fits u64"))
        .checked_mul(prospecting_duration(registries, PROSPECTING_LOCAL_TRANSECT))
        .unwrap_or_else(|| panic!("fieldwork campaign transect duration overflowed"));
    common_transects
        .checked_add(prospecting_duration(
            registries,
            PROSPECTING_INDEXED_CHANNEL_SURVEY,
        ))
        .unwrap_or_else(|| panic!("fieldwork campaign indexed-search duration overflowed"))
}

fn select_survey_strategy(
    projected_point_search_ticks: u64,
    projected_indexed_search_ticks: Option<u64>,
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
    if return_ppm >= MINIMUM_SURVEY_INVESTMENT_RETURN_PPM {
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
) -> FieldworkSurveyDecision {
    assert!(
        planned_sites > 0,
        "fieldwork survey plan requires at least one future site"
    );
    let point_per_site = expected_point_search_ticks(registries, channel_voxels);
    let indexed_per_site = indexed_search_ticks(registries);
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
    let mut extraction_ticks = 0_u64;
    let mut extracted = Mass::ZERO;
    let mut first_search_ticks = None;
    let mut first_extraction_ticks = None;
    let mut first_extracted = None;
    for (index, site) in plan.sites.iter().enumerate() {
        let search_started_at = state.tick().value();
        let localized = localize_target(
            registries,
            state,
            plan.hammer,
            plan.channel_voxels,
            site.start_x,
            strategy,
        );
        let site_search_ticks = state.tick().value() - search_started_at;
        search_ticks = search_ticks
            .checked_add(site_search_ticks)
            .unwrap_or_else(|| panic!("fieldwork campaign search duration overflowed"));
        let requested = plan.batch.min(localized.resource_mass.upper());
        assert!(
            !requested.is_zero(),
            "fieldwork campaign acquired evidence must support nonzero extraction"
        );
        let extraction = execute_fieldwork_extraction(
            registries,
            state,
            FieldworkExtractionOrder {
                target: localized.target,
                destination: site.destination,
                equipment: plan.mining_equipment,
                requested,
                batch_limit: plan.batch,
            },
        );
        extraction_ticks = extraction_ticks
            .checked_add(extraction.ticks)
            .unwrap_or_else(|| panic!("fieldwork campaign extraction duration overflowed"));
        extracted = extracted
            .checked_add(extraction.extracted)
            .unwrap_or_else(|| panic!("fieldwork campaign extracted mass overflowed"));
        if index == 0 {
            first_search_ticks = Some(site_search_ticks);
            first_extraction_ticks = Some(extraction.ticks);
            first_extracted = Some(extraction.extracted);
        }
    }
    validate_loaded_state(registries, state)
        .unwrap_or_else(|error| panic!("fieldwork campaign state invalid: {error}"));
    CampaignRun {
        search_ticks,
        extraction_ticks,
        extracted,
        first_search_ticks: first_search_ticks
            .unwrap_or_else(|| unreachable!("fieldwork campaign has at least one site")),
        first_extraction_ticks: first_extraction_ticks
            .unwrap_or_else(|| unreachable!("fieldwork campaign has at least one site")),
        first_extracted: first_extracted
            .unwrap_or_else(|| unreachable!("fieldwork campaign has at least one site")),
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
        mining_equipment: plan.mining_equipment,
        batch: plan.batch,
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
    assert_eq!(
        selected.extraction_ticks, baseline.extraction_ticks,
        "fieldwork survey strategy must not change extraction physics"
    );
    assert_eq!(
        selected.extracted, baseline.extracted,
        "fieldwork survey strategy must not change extracted matter"
    );
    let selected_attention = selected
        .search_ticks
        .checked_add(upgrade_ticks)
        .unwrap_or_else(|| panic!("fieldwork selected campaign attention overflowed"));
    FieldworkSurveyCampaignReview {
        planned_sites,
        selected_strategy,
        upgrade_available: decision.projected_upgrade_ticks.is_some(),
        upgrade_ticks,
        projected_point_search_ticks: decision.projected_point_search_ticks,
        projected_indexed_search_ticks: decision.projected_indexed_search_ticks,
        baseline_search_ticks: baseline.search_ticks,
        selected_search_ticks: selected.search_ticks,
        extraction_ticks: selected.extraction_ticks,
        extracted: selected.extracted,
        realized_attention_delta: i128::from(baseline.search_ticks)
            - i128::from(selected_attention),
        first_search_ticks: selected.first_search_ticks,
        first_extraction_ticks: selected.first_extraction_ticks,
        first_extracted: selected.first_extracted,
    }
}

#[cfg(test)]
#[path = "campaign_tests.rs"]
mod tests;

//! Post-action conservation audit, continuation counterfactual, and fieldwork evidence rendering.

use deep_hearth::content::MATERIAL_COPPER;
use deep_hearth::core::quantity::{AggregateMass, Energy, Mass, Volume};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::equipment::{EquipmentDefinitionId, EquipmentId};
use deep_hearth::geology::{ExcavationHardnessEstimate, ResourceMassEstimate};
use deep_hearth::inventory::StockpileId;
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::mining::{
    MiningTargetRequest, MiningTargetResolutionError, resolve_mining_target,
};
use deep_hearth::registry::Registries;
use deep_hearth::spatial::VoxelBounds;

use super::super::focused_runner::focused_probe_role_label;
use super::super::focused_seeds::FocusedProbeCase;
use super::super::physical_time::format_physical_duration;
use super::FieldworkEpisode;
use super::campaign::FieldworkSurveyCampaignReview;
use super::extraction::{
    FieldworkExtraction, FieldworkExtractionOrder, FieldworkStop, execute_fieldwork_extraction,
};
use super::planning::FieldworkToolEstimate;
use super::survey::{
    CHANNEL_COUNT, FieldworkSurveyStrategy, QUATERNARY_CHANNEL_START_X, SECONDARY_CHANNEL_START_X,
    TERTIARY_CHANNEL_START_X, localize_target,
};

pub(super) struct FieldworkEpisodeReview<'a> {
    pub(super) registries: &'a Registries,
    pub(super) state: &'a AppState,
    pub(super) case: FocusedProbeCase,
    pub(super) requested: Mass,
    pub(super) deposit_mass: Mass,
    pub(super) order_horizon: &'static str,
    pub(super) raw: StockpileId,
    pub(super) followup_destination: StockpileId,
    pub(super) reroute_destination: StockpileId,
    pub(super) sampling_hammer: EquipmentId,
    pub(super) channel_voxels: i64,
    pub(super) mining_equipment: EquipmentId,
    pub(super) target_region: VoxelBounds,
    pub(super) estimate: &'a FieldworkToolEstimate,
    pub(super) extraction: &'a FieldworkExtraction,
    pub(super) survey_campaign: &'a FieldworkSurveyCampaignReview,
    pub(super) episode_started_at: u64,
    pub(super) sampling_setup_ticks: u64,
    pub(super) search_ticks: u64,
    pub(super) tool_prep_ticks: u64,
    pub(super) transects: u64,
    pub(super) field_inspections: u64,
    pub(super) detailed_surveys: u64,
    pub(super) observed_hardness: ExcavationHardnessEstimate,
    pub(super) observed_resource_mass: ResourceMassEstimate,
    pub(super) planned_local_mass: Mass,
    pub(super) full_order_tool: Option<EquipmentDefinitionId>,
    pub(super) full_order_tool_label: &'static str,
    pub(super) resource_knowledge_effect: &'static str,
    pub(super) geology_label: &'static str,
    pub(super) copper_rich: bool,
    pub(super) starting_native_copper: Mass,
    pub(super) native_copper: CommodityKey,
    pub(super) matter_before: AggregateMass,
    pub(super) survival_before_energy: Energy,
    pub(super) survival_before_hydration: Volume,
}

struct KnownSiteExploitation {
    first: Option<FieldworkExtraction>,
    completed_orders: u64,
    partial_orders: u64,
    extracted: Mass,
    ticks: u64,
    supply_ended: bool,
    terminal: &'static str,
    final_condition_ppm: u32,
    metabolic_nj: u128,
    hydration_ul: u64,
    final_state: AppState,
}

struct InitialShortfallRecovery {
    sites_visited: u64,
    search_ticks: u64,
    extraction_ticks: u64,
    additional_extracted: Mass,
    fulfilled: Mass,
    remaining: Mass,
    terminal: &'static str,
}

fn execute_known_site_exploitation(
    review: &FieldworkEpisodeReview<'_>,
) -> Option<KnownSiteExploitation> {
    if review.extraction.stop != FieldworkStop::OrderComplete {
        return None;
    }
    let mut repeat_state = review.state.clone();
    let survival_before = *repeat_state
        .survival()
        .player()
        .unwrap_or_else(|| panic!("fieldwork repeat survival record disappeared"));
    let mut first = None;
    let mut completed_orders = 0_u64;
    let mut partial_orders = 0_u64;
    let mut extracted = Mass::ZERO;
    let mut ticks = 0_u64;
    let mut supply_ended = false;
    let mut terminal = "horizon-live-target";

    for _ in 0..super::FIELDWORK_KNOWN_SITE_REPEAT_HORIZON {
        let refreshed = match resolve_mining_target(
            &repeat_state,
            MiningTargetRequest::new(review.target_region, MATERIAL_COPPER),
        ) {
            Ok(refreshed) => refreshed,
            Err(MiningTargetResolutionError::EvidenceInsufficientToResolveTarget { .. }) => {
                supply_ended = true;
                terminal = "target-no-longer-resolved";
                break;
            }
            Err(error) => panic!("fieldwork repeat target resolution failed: {error}"),
        };
        let result = execute_fieldwork_extraction(
            review.registries,
            &mut repeat_state,
            FieldworkExtractionOrder {
                target: refreshed,
                destination: review.followup_destination,
                equipment: review.mining_equipment,
                requested: review.requested,
                batch_limit: review.estimate.batch,
            },
        );
        if first.is_none() {
            first = Some(result);
        }
        extracted = extracted
            .checked_add(result.extracted)
            .unwrap_or_else(|| panic!("fieldwork repeat extracted mass overflowed"));
        ticks = ticks
            .checked_add(result.ticks)
            .unwrap_or_else(|| panic!("fieldwork repeat attention overflowed"));
        if result.stop == FieldworkStop::OrderComplete {
            completed_orders += 1;
        } else {
            partial_orders += 1;
            supply_ended = true;
            terminal = result.stop.label();
            break;
        }
    }

    if completed_orders == super::FIELDWORK_KNOWN_SITE_REPEAT_HORIZON
        && !supply_ended
        && let Err(MiningTargetResolutionError::EvidenceInsufficientToResolveTarget { .. }) =
            resolve_mining_target(
                &repeat_state,
                MiningTargetRequest::new(review.target_region, MATERIAL_COPPER),
            )
    {
        supply_ended = true;
        terminal = "target-depleted-at-horizon";
    }
    validate_loaded_state(review.registries, &repeat_state)
        .unwrap_or_else(|error| panic!("fieldwork repeat state invalid: {error}"));
    let survival_after = repeat_state
        .survival()
        .player()
        .unwrap_or_else(|| panic!("fieldwork repeat survival record disappeared"));
    let metabolic_nj = survival_before
        .metabolic_energy()
        .checked_sub(survival_after.metabolic_energy())
        .unwrap_or_else(|| panic!("fieldwork repeat metabolic reserve increased"))
        .nanojoules();
    let hydration_ul = survival_before
        .hydration()
        .checked_sub(survival_after.hydration())
        .unwrap_or_else(|| panic!("fieldwork repeat hydration reserve increased"))
        .microliters();
    let final_condition_ppm = repeat_state
        .equipment()
        .get_equipment(review.mining_equipment)
        .unwrap_or_else(|| panic!("fieldwork repeat mining tool disappeared"))
        .condition()
        .parts_per_million();
    Some(KnownSiteExploitation {
        first,
        completed_orders,
        partial_orders,
        extracted,
        ticks,
        supply_ended,
        terminal,
        final_condition_ppm,
        metabolic_nj,
        hydration_ul,
        final_state: repeat_state,
    })
}

fn execute_site_reroute(
    review: &FieldworkEpisodeReview<'_>,
    source_state: &AppState,
) -> (u64, FieldworkExtraction) {
    let mut state = source_state.clone();
    let search_started_at = state.tick().value();
    let localization = localize_target(
        review.registries,
        &mut state,
        review.sampling_hammer,
        review.channel_voxels,
        SECONDARY_CHANNEL_START_X,
        FieldworkSurveyStrategy::PointSearch,
    );
    let search_ticks = state.tick().value() - search_started_at;
    let requested = review
        .estimate
        .batch
        .min(localization.resource_mass.upper());
    assert!(
        !requested.is_zero(),
        "fieldwork reroute must resolve a nonzero secondary-site first batch"
    );
    let extraction = execute_fieldwork_extraction(
        review.registries,
        &mut state,
        FieldworkExtractionOrder {
            target: localization.target,
            destination: review.reroute_destination,
            equipment: review.mining_equipment,
            requested,
            batch_limit: review.estimate.batch,
        },
    );
    assert!(
        !extraction.extracted.is_zero(),
        "fieldwork reroute must resume productive extraction"
    );
    validate_loaded_state(review.registries, &state)
        .unwrap_or_else(|error| panic!("fieldwork reroute state invalid: {error}"));
    (search_ticks, extraction)
}

fn execute_initial_shortfall_recovery(
    review: &FieldworkEpisodeReview<'_>,
) -> InitialShortfallRecovery {
    let mut state = review.state.clone();
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
            &mut state,
            review.sampling_hammer,
            review.channel_voxels,
            start_x,
            FieldworkSurveyStrategy::PointSearch,
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
            &mut state,
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

    validate_loaded_state(review.registries, &state)
        .unwrap_or_else(|error| panic!("fieldwork multi-site recovery state invalid: {error}"));
    let fulfilled = review
        .extraction
        .extracted
        .checked_add(additional_extracted)
        .unwrap_or_else(|| panic!("fieldwork multi-site fulfilled mass overflowed"));
    assert_eq!(fulfilled.checked_add(remaining), Some(review.requested));
    InitialShortfallRecovery {
        sites_visited,
        search_ticks,
        extraction_ticks,
        additional_extracted,
        fulfilled,
        remaining,
        terminal: if remaining.is_zero() {
            "order-complete"
        } else {
            "local-search-area-exhausted"
        },
    }
}

fn survival_spend(review: &FieldworkEpisodeReview<'_>) -> (Energy, Volume) {
    let survival_after = review
        .state
        .survival()
        .player()
        .unwrap_or_else(|| panic!("fieldwork final survival record disappeared"));
    let metabolic_energy_spent = review
        .survival_before_energy
        .checked_sub(survival_after.metabolic_energy())
        .unwrap_or_else(|| panic!("fieldwork metabolic reserve increased without intake"));
    let hydration_spent = review
        .survival_before_hydration
        .checked_sub(survival_after.hydration())
        .unwrap_or_else(|| panic!("fieldwork hydration reserve increased without intake"));
    assert!(metabolic_energy_spent > Energy::ZERO);
    assert!(hydration_spent > Volume::ZERO);
    assert!(
        survival_after.metabolic_energy() > Energy::ZERO
            && survival_after.hydration() > Volume::ZERO,
        "fieldwork cost evidence must not clip at exhausted survival reserves"
    );
    assert_eq!(
        survival_after
            .metabolic_energy()
            .checked_add(metabolic_energy_spent),
        Some(review.survival_before_energy),
        "fieldwork reported metabolic cost must reconcile with canonical player reserves"
    );
    assert_eq!(
        survival_after.hydration().checked_add(hydration_spent),
        Some(review.survival_before_hydration),
        "fieldwork reported hydration cost must reconcile with canonical player reserves"
    );
    (metabolic_energy_spent, hydration_spent)
}

fn report_continuation(
    review: &FieldworkEpisodeReview<'_>,
    followup: Option<&FieldworkExtraction>,
) {
    let reusable_kit_ticks = review.sampling_setup_ticks + review.tool_prep_ticks;
    if let Some(followup) = followup {
        reviewln!(
            "FIELDWORK CONTINUATION seed=0x{:016X} available=true reused-knowledge=true reused-tool=true requested={}mg extracted={}mg extraction={}t/{} avoided-search={}t/{} avoided-kit={}t/{} stop={} scope=matched-repeat-order destination-capacity=diagnostic-only",
            review.case.seed(),
            review.requested.milligrams(),
            followup.extracted.milligrams(),
            followup.ticks,
            format_physical_duration(review.registries, followup.ticks),
            review.search_ticks,
            format_physical_duration(review.registries, review.search_ticks),
            reusable_kit_ticks,
            format_physical_duration(review.registries, reusable_kit_ticks),
            followup.stop.label(),
        );
    } else {
        reviewln!(
            "FIELDWORK CONTINUATION seed=0x{:016X} available=false reused-knowledge=true reused-tool=true avoided-search={}t/{} avoided-kit={}t/{} scope=matched-repeat-order reason=known-target-no-longer-resolved",
            review.case.seed(),
            review.search_ticks,
            format_physical_duration(review.registries, review.search_ticks),
            reusable_kit_ticks,
            format_physical_duration(review.registries, reusable_kit_ticks),
        );
    }
}

fn report_known_site_exploitation(
    review: &FieldworkEpisodeReview<'_>,
    exploitation: Option<&KnownSiteExploitation>,
) {
    if let Some(exploitation) = exploitation {
        reviewln!(
            "FIELDWORK DEPLETION seed=0x{:016X} eligible=true repeat-orders=[complete:{} partial:{} horizon:{}] extracted={}mg attention={}t/{} supply-ended={} terminal={} condition-after={}ppm body=[energy:{}nJ hydration:{}uL] scope=matched-orders-on-known-site no-search=true no-new-tool=true diagnostic-only=true",
            review.case.seed(),
            exploitation.completed_orders,
            exploitation.partial_orders,
            super::FIELDWORK_KNOWN_SITE_REPEAT_HORIZON,
            exploitation.extracted.milligrams(),
            exploitation.ticks,
            format_physical_duration(review.registries, exploitation.ticks),
            exploitation.supply_ended,
            exploitation.terminal,
            exploitation.final_condition_ppm,
            exploitation.metabolic_nj,
            exploitation.hydration_ul,
        );
        if exploitation.supply_ended {
            let (search_ticks, reroute) = execute_site_reroute(review, &exploitation.final_state);
            reviewln!(
                "FIELDWORK DEPLETION RECOVERY seed=0x{:016X} depletion-observed=true reroute-proved=true evidence=executed-from-depleted-state post-depletion-execution=true mining-tool-reused=true survey-base-kit-reused=true strategy=point-search survey-upgrade=0t search={}t/{} extraction={}t/{} extracted={}mg stop={}",
                review.case.seed(),
                search_ticks,
                format_physical_duration(review.registries, search_ticks),
                reroute.ticks,
                format_physical_duration(review.registries, reroute.ticks),
                reroute.extracted.milligrams(),
                reroute.stop.label(),
            );
        }
    } else {
        reviewln!(
            "FIELDWORK DEPLETION seed=0x{:016X} eligible=false horizon={} reason=initial-order-supply-ended scope=matched-orders-on-known-site diagnostic-only=true",
            review.case.seed(),
            super::FIELDWORK_KNOWN_SITE_REPEAT_HORIZON,
        );
        let recovery = execute_initial_shortfall_recovery(review);
        let fulfillment_ppm = recovery
            .fulfilled
            .milligrams()
            .checked_mul(1_000_000)
            .unwrap_or_else(|| panic!("fieldwork recovery fulfillment ratio overflowed"))
            / review.requested.milligrams();
        reviewln!(
            "FIELDWORK INITIAL SHORTFALL RECOVERY seed=0x{:016X} initial-supply-ended=true reroute-proved=true evidence=executed-multi-site-from-partial-extraction-state post-shortfall-execution=true mining-tool-reused=true survey-base-kit-reused=true strategy=point-search survey-upgrade=0t sites-visited={} search={}t/{} extraction={}t/{} initial-extracted={}mg additional-extracted={}mg fulfilled={}mg requested={}mg fulfillment={}ppm remaining={}mg terminal={}",
            review.case.seed(),
            recovery.sites_visited,
            recovery.search_ticks,
            format_physical_duration(review.registries, recovery.search_ticks),
            recovery.extraction_ticks,
            format_physical_duration(review.registries, recovery.extraction_ticks),
            review.extraction.extracted.milligrams(),
            recovery.additional_extracted.milligrams(),
            recovery.fulfilled.milligrams(),
            review.requested.milligrams(),
            fulfillment_ppm,
            recovery.remaining.milligrams(),
            recovery.terminal,
        );
    }
}

fn report_survey_campaign(review: &FieldworkEpisodeReview<'_>) {
    let campaign = review.survey_campaign;
    let indexed_projection = campaign
        .projected_indexed_search_ticks
        .map_or_else(|| "unfunded".to_owned(), |ticks| format!("{ticks}t"));
    reviewln!(
        "FIELDWORK SURVEY CAMPAIGN seed=0x{:016X} planned-sites={} upgrade-available={} selected={} policy=min-expected-search-attention-with-minimum-return minimum-return=100000ppm projected=[point:{}t indexed:{}] realized=[baseline-search:{}t selected-search:{}t upgrade:{}t attention-delta:{:+}t] extraction={}t extracted={}mg choice-frozen-before-branch=true",
        review.case.seed(),
        campaign.planned_sites,
        campaign.upgrade_available,
        campaign.selected_strategy.label(),
        campaign.projected_point_search_ticks,
        indexed_projection,
        campaign.baseline_search_ticks,
        campaign.selected_search_ticks,
        campaign.upgrade_ticks,
        campaign.realized_attention_delta,
        campaign.extraction_ticks,
        campaign.extracted.milligrams(),
    );
    reviewln!(
        "FIELDWORK SITE REUSE seed=0x{:016X} available=true kit-reused=true knowledge-reused=false strategy={} search={}t/{} extraction={}t/{} requested={}mg extracted={}mg first-expedition-kit={}t/{} scope=new-site-first-batch upgrade-cost-reported-separately=true",
        review.case.seed(),
        campaign.selected_strategy.label(),
        campaign.first_search_ticks,
        format_physical_duration(review.registries, campaign.first_search_ticks),
        campaign.first_extraction_ticks,
        format_physical_duration(review.registries, campaign.first_extraction_ticks),
        review.estimate.batch.milligrams(),
        campaign.first_extracted.milligrams(),
        review.sampling_setup_ticks + review.tool_prep_ticks,
        format_physical_duration(
            review.registries,
            review.sampling_setup_ticks + review.tool_prep_ticks,
        ),
    );
}

pub(super) fn finalize_fieldwork_episode(review: FieldworkEpisodeReview<'_>) -> FieldworkEpisode {
    let extraction = review.extraction;
    assert_eq!(
        review
            .state
            .equipment()
            .get_equipment(review.mining_equipment)
            .map(|record| record.condition()),
        Some(extraction.condition_after)
    );
    assert_eq!(
        calculate_matter_accounting(review.state)
            .unwrap_or_else(|error| panic!("fieldwork final matter audit failed: {error}"))
            .total(),
        review.matter_before
    );
    validate_loaded_state(review.registries, review.state)
        .unwrap_or_else(|error| panic!("fieldwork final state invalid: {error}"));
    let retained_native_copper = review
        .state
        .inventory()
        .get_stockpile(review.raw)
        .map(|stockpile| stockpile.get_mass(review.native_copper))
        .unwrap_or_else(|| panic!("fieldwork raw stockpile disappeared"));
    let (metabolic_energy_spent, hydration_spent) = survival_spend(&review);
    let exploitation = execute_known_site_exploitation(&review);
    let followup = exploitation.as_ref().and_then(|run| run.first.as_ref());

    let first_ore_ticks = review.sampling_setup_ticks
        + review.search_ticks
        + review.tool_prep_ticks
        + extraction.first_ore_ticks;
    let total_ticks = review.state.tick().value() - review.episode_started_at;
    assert_eq!(
        total_ticks,
        review.sampling_setup_ticks
            + review.search_ticks
            + review.tool_prep_ticks
            + extraction.ticks,
        "fieldwork pacing must account for every elapsed tick, not only extraction"
    );
    let completed = extraction.stop == FieldworkStop::OrderComplete;
    let comparison = if completed {
        "full-order"
    } else {
        "partial-order-not-comparable"
    };
    let estimate_matched = if !completed {
        "not-applicable"
    } else if review.estimate.order_ticks == extraction.ticks {
        "true"
    } else {
        "false"
    };
    let extraction_error = if completed {
        format!(
            "{:+}t",
            i128::from(extraction.ticks) - i128::from(review.estimate.order_ticks)
        )
    } else {
        "not-applicable".to_owned()
    };
    reviewln!(
        "FIELDWORK ESTIMATE FEEDBACK seed=0x{:016X} selected={} order-horizon={} outcome={} requested={}mg output={}mg preparation-estimate={}t preparation-actual={}t wear-adjusted-order-estimate={}t extraction-actual={}t extraction-error={extraction_error} actual-build-plus-order={}t/{} condition={}ppm->{}ppm comparison={comparison} estimate-matched={estimate_matched} choice-frozen-before-action=true service=none",
        review.case.seed(),
        review.estimate.tool.label,
        review.order_horizon,
        extraction.stop.outcome(),
        review.requested.milligrams(),
        extraction.extracted.milligrams(),
        review.estimate.preparation_ticks,
        review.tool_prep_ticks,
        review.estimate.order_ticks,
        extraction.ticks,
        review.tool_prep_ticks + extraction.ticks,
        format_physical_duration(review.registries, review.tool_prep_ticks + extraction.ticks),
        extraction.condition_before.parts_per_million(),
        extraction.condition_after.parts_per_million(),
    );
    report_continuation(&review, followup);
    report_known_site_exploitation(&review, exploitation.as_ref());
    report_survey_campaign(&review);
    reviewln!(
        "FIELDWORK PACING seed=0x{:016X} search={}t/{} sampling-tool={}t/{} extraction-tool={}t/{} extraction={}t/{} batches={} first-ore={}t/{} episode-end={}t/{} output={}mg outcome={} requested={}mg scope=raw-tools-and-preowned-copper-to-first-ore matched-repeat-order-reported-separately=true output-grade={}ppm",
        review.case.seed(),
        review.search_ticks,
        format_physical_duration(review.registries, review.search_ticks),
        review.sampling_setup_ticks,
        format_physical_duration(review.registries, review.sampling_setup_ticks),
        review.tool_prep_ticks,
        format_physical_duration(review.registries, review.tool_prep_ticks),
        extraction.ticks,
        format_physical_duration(review.registries, extraction.ticks),
        extraction.batches,
        first_ore_ticks,
        format_physical_duration(review.registries, first_ore_ticks),
        total_ticks,
        format_physical_duration(review.registries, total_ticks),
        extraction.extracted.milligrams(),
        extraction.stop.outcome(),
        review.requested.milligrams(),
        extraction.output_grade_ppm,
    );
    reviewln!(
        "FIELDWORK EXPERIENCE seed=0x{:016X} sample={} outcome={} order-horizon={} demand=explicit-extraction-order search=compare-local-transects->cheap-inspection->targeted-survey channels={} transects={} selected-channel=observed-strongest field-inspections={} detailed-surveys={} target=acquired-evidence observed-hardness={}..{}Pa observed-resource-mass={}..{}mg planned-local-work={}mg full-order-tool={} resource-knowledge-effect={} geology={} tool={} adaptation={} sampling-setup={}t/{} tool-prep={}t/{} copper-opportunity={} starting-native-copper={}mg retained-native-copper={}mg requested={}mg mining={}mg duration={}t/{} condition={}ppm->{}ppm output-grade={}ppm matter=conserved survival=[energy:{}nJ hydration:{}uL]",
        review.case.seed(),
        focused_probe_role_label(review.case.role()),
        extraction.stop.outcome(),
        review.order_horizon,
        CHANNEL_COUNT,
        review.transects,
        review.field_inspections,
        review.detailed_surveys,
        review.observed_hardness.lower().pascals(),
        review.observed_hardness.upper().pascals(),
        review.observed_resource_mass.lower().milligrams(),
        review.observed_resource_mass.upper().milligrams(),
        review.planned_local_mass.milligrams(),
        review.full_order_tool_label,
        review.resource_knowledge_effect,
        review.geology_label,
        review.estimate.tool.label,
        extraction.adaptation,
        review.sampling_setup_ticks,
        format_physical_duration(review.registries, review.sampling_setup_ticks),
        review.tool_prep_ticks,
        format_physical_duration(review.registries, review.tool_prep_ticks),
        if review.copper_rich {
            "available"
        } else {
            "absent"
        },
        review.starting_native_copper.milligrams(),
        retained_native_copper.milligrams(),
        review.requested.milligrams(),
        extraction.extracted.milligrams(),
        extraction.ticks,
        format_physical_duration(review.registries, extraction.ticks),
        extraction.condition_before.parts_per_million(),
        extraction.condition_after.parts_per_million(),
        extraction.output_grade_ppm,
        metabolic_energy_spent.nanojoules(),
        hydration_spent.microliters(),
    );
    reviewln!(
        "FIELDWORK SUPPLY seed=0x{:016X} outcome={} requested={}mg extracted={}mg shortfall={}mg stop={} effort={}t investment={}t",
        review.case.seed(),
        extraction.stop.outcome(),
        review.requested.milligrams(),
        extraction.extracted.milligrams(),
        review
            .requested
            .checked_sub(extraction.extracted)
            .unwrap_or_else(|| panic!("fieldwork output exceeded order"))
            .milligrams(),
        extraction.stop.label(),
        extraction.ticks,
        review.sampling_setup_ticks + review.tool_prep_ticks,
    );
    reviewln!(
        "FIELDWORK SUPPLY DIAGNOSTIC seed=0x{:016X} initial-reserve={}mg policy-input=false",
        review.case.seed(),
        review.deposit_mass.milligrams(),
    );
    FieldworkEpisode {
        tool: review.estimate.tool.target,
        preparation_ticks: review.tool_prep_ticks,
        projected_ticks: review.estimate.order_ticks,
        observed_hardness: review.observed_hardness,
        observed_resource_mass: review.observed_resource_mass,
        planned_local_mass: review.planned_local_mass,
        full_order_tool: review.full_order_tool,
        resource_knowledge_effect: review.resource_knowledge_effect,
        extraction: *extraction,
    }
}

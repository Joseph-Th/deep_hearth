//! Replayable ordinary prospecting-to-mining episode for the cold-agent report.

use std::collections::BTreeMap;

use deep_hearth::capability::CapabilityValue;
use deep_hearth::content::{
    EQUIPMENT_COPPER_REINFORCED_GEOLOGICAL_HAMMER, EQUIPMENT_COPPER_REINFORCED_PICK,
    EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK, EQUIPMENT_STONE_GEOLOGICAL_HAMMER,
    EQUIPMENT_STONE_PICK, EQUIPMENT_STONE_QUARRY_PICK, FORM_NATIVE_METAL, MATERIAL_COPPER,
    MINING_METHOD_HAND_PICK,
};
use deep_hearth::core::quantity::{Mass, Pressure};
use deep_hearth::core::state::AppState;
use deep_hearth::crafting::resolve_manual_craft;
use deep_hearth::equipment::EquipmentDefinitionId;
use deep_hearth::geology::{ExcavationHardnessEstimate, ResourceMassEstimate};
use deep_hearth::inventory::StockpileId;
use deep_hearth::material::CommodityKey;
use deep_hearth::mining::{MiningOrderRequest, resolve_mining_order};
use deep_hearth::registry::Registries;

use super::equipment_support::pristine_equipment_capability;
use super::focused_seeds::FocusedProbeCase;
use super::manual_craft_planning::{
    manual_craft_topology_plan_for_output, project_manual_assembly_package,
};
use super::manual_craft_selection::{
    first_sufficient_pure_temperature, select_manual_craft_request,
};
use super::primitive_workload::{STOCKPILE_WORK_ORDER_CYCLES, primitive_mining_cycle_mass};
use super::seed::mix64;

const FIELDWORK_KNOWN_SITE_REPEAT_HORIZON: u64 = 12;
const FIELDWORK_BULK_ORDER_BATCHES: u64 = 48;
const FIELDWORK_BULK_ORDER_MIN_BATCHES: u64 = 32;
const FIELDWORK_REINFORCED_BULK_COVERAGE_SEED: u64 = 0;
const FIELDWORK_REINFORCED_BULK_COVERAGE_BATCHES: u64 = 48;
const FIELDWORK_REINFORCED_BULK_COVERAGE_SUPPLY_MG: u64 = 28_000_000;
const FIELDWORK_BULK_INVESTMENT_COVERAGE_SEED: u64 = 2;
const FIELDWORK_BULK_INVESTMENT_COVERAGE_BATCHES: u64 = 40;
const FIELDWORK_BULK_INVESTMENT_COVERAGE_SUPPLY_MG: u64 = 28_000_000;
const FIELDWORK_RESERVE_SCALE_COVERAGE_SEED: u64 = 6;
const FIELDWORK_RESERVE_SCALE_COVERAGE_BATCHES: u64 = 48;

#[path = "fieldwork_probe/campaign.rs"]
mod campaign;
use campaign::{
    FieldworkCampaignSite, FieldworkSurveyCampaignPlan, evaluate_fieldwork_survey_campaign,
    planned_future_sites,
};

#[path = "fieldwork_probe/extraction.rs"]
mod extraction;
use extraction::{FieldworkExtractionOrder, execute_fieldwork_extraction};

#[cfg(test)]
#[path = "fieldwork_probe/supply_tests.rs"]
mod supply_tests;

#[path = "fieldwork_probe/planning.rs"]
mod planning;
use planning::*;

#[path = "fieldwork_probe/review.rs"]
mod review;
use review::{FieldworkEpisodeReview, finalize_fieldwork_episode};

#[path = "fieldwork_probe/recovery.rs"]
mod recovery;

#[cfg(test)]
#[path = "fieldwork_probe/planning_tests.rs"]
mod planning_tests;

#[path = "fieldwork_probe/preparation.rs"]
mod preparation;
use preparation::{assemble_fieldwork_tool, assemble_sampling_hammer};

#[path = "fieldwork_probe/retooling.rs"]
mod retooling;

#[cfg(test)]
#[path = "fieldwork_probe/retooling_tests.rs"]
mod retooling_tests;

#[path = "fieldwork_probe/survey.rs"]
mod survey;
use survey::{
    CHANNEL_START_X, FieldworkLocalization, FieldworkSurveyStrategy, QUATERNARY_CHANNEL_START_X,
    SECONDARY_CHANNEL_START_X, TERTIARY_CHANNEL_START_X, localize_target,
};

#[path = "fieldwork_probe/world.rs"]
mod world;
use world::{FieldworkWorld, build_fieldwork_world, fieldwork_supply};

/// Visible demand spans one immediate local order, the finite twelve-cycle ore workload that
/// ordinary primitive progression prices before mechanizing, and a settlement-scale bulk order.
/// Demand variation is independent of hidden reserve. Maintained coverage includes both sides of
/// heavy-tool investment: large actor-visible opportunity can repay quarry setup, while small
/// localized reserve can cut the same nominal bulk project back before construction.
fn fieldwork_order(registries: &Registries, seed: u64) -> Mass {
    let batch = fieldwork_mining_limits(registries).base_quarry_batch;
    match mix64(seed ^ 0x4649_454C_4444_454D) % 4 {
        0 => short_fieldwork_order(batch, seed),
        1 => multiplied_mass(
            batch,
            FIELDWORK_BULK_ORDER_BATCHES,
            "settlement-scale bulk fieldwork project",
        ),
        _ => multiplied_mass(
            primitive_mining_cycle_mass(registries, seed),
            STOCKPILE_WORK_ORDER_CYCLES,
            "current primitive processing project",
        ),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FieldworkResourceKnowledgeEffect {
    SameTool,
    ChangedTool,
    ChangedFeasibility,
}

impl FieldworkResourceKnowledgeEffect {
    const fn label(self) -> &'static str {
        match self {
            Self::SameTool => "same-tool",
            Self::ChangedTool => "changed-tool",
            Self::ChangedFeasibility => "changed-feasibility",
        }
    }
}

fn fieldwork_order_horizon(registries: &Registries, requested: Mass) -> &'static str {
    let batch = fieldwork_mining_limits(registries).base_quarry_batch;
    if requested <= batch {
        "short"
    } else if requested
        >= multiplied_mass(
            batch,
            FIELDWORK_BULK_ORDER_MIN_BATCHES,
            "bulk fieldwork horizon threshold",
        )
    {
        "bulk"
    } else {
        "project"
    }
}

fn fieldwork_order_for_case(registries: &Registries, case: FocusedProbeCase) -> Mass {
    if case.seed() == FIELDWORK_REINFORCED_BULK_COVERAGE_SEED {
        return multiplied_mass(
            fieldwork_mining_limits(registries).base_quarry_batch,
            FIELDWORK_REINFORCED_BULK_COVERAGE_BATCHES,
            "maintained reinforced bulk-investment fieldwork coverage order",
        );
    }
    if case.seed() == FIELDWORK_BULK_INVESTMENT_COVERAGE_SEED {
        return multiplied_mass(
            fieldwork_mining_limits(registries).base_quarry_batch,
            FIELDWORK_BULK_INVESTMENT_COVERAGE_BATCHES,
            "maintained bulk-investment fieldwork coverage order",
        );
    }
    if case.seed() == FIELDWORK_RESERVE_SCALE_COVERAGE_SEED {
        return multiplied_mass(
            fieldwork_mining_limits(registries).base_quarry_batch,
            FIELDWORK_RESERVE_SCALE_COVERAGE_BATCHES,
            "maintained reserve-scale fieldwork coverage order",
        );
    }
    fieldwork_order(registries, case.seed())
}

fn fieldwork_supply_for_case(case: FocusedProbeCase) -> Mass {
    if case.seed() == FIELDWORK_REINFORCED_BULK_COVERAGE_SEED {
        return Mass::from_milligrams(FIELDWORK_REINFORCED_BULK_COVERAGE_SUPPLY_MG);
    }
    if case.seed() == FIELDWORK_BULK_INVESTMENT_COVERAGE_SEED {
        return Mass::from_milligrams(FIELDWORK_BULK_INVESTMENT_COVERAGE_SUPPLY_MG);
    }
    fieldwork_supply(case.seed())
}

fn short_fieldwork_order(batch: Mass, seed: u64) -> Mass {
    let minimum = (batch.milligrams() / 2).max(1);
    Mass::from_milligrams(
        minimum + mix64(seed ^ 0x4649_454C_444D_4153) % (batch.milligrams() - minimum + 1),
    )
}

pub(super) fn run_fieldwork_probe(registries: &Registries, case: FocusedProbeCase) {
    let episode = run_fieldwork_order(registries, case, fieldwork_order_for_case(registries, case));
    reviewln!(
        "FIELDWORK ENDPOINT seed=0x{:016X} tool={} full-order-tool={} resource-knowledge-effect={} observed-hardness={}..{}Pa observed-resource-mass={}..{}mg planned-local-work={}mg preparation={}t projected-order={}t actual-extraction={}t extracted={}mg outcome={}",
        case.seed(),
        episode.tool.value(),
        episode
            .full_order_tool
            .map_or_else(|| "none".to_owned(), |tool| tool.value().to_string()),
        episode.resource_knowledge_effect.label(),
        episode.observed_hardness.lower().pascals(),
        episode.observed_hardness.upper().pascals(),
        episode.observed_resource_mass.lower().milligrams(),
        episode.observed_resource_mass.upper().milligrams(),
        episode.planned_local_mass.milligrams(),
        episode.preparation_ticks,
        episode.projected_ticks,
        episode.extraction.ticks,
        episode.extraction.extracted.milligrams(),
        episode.extraction.stop.outcome(),
    );
}

struct FieldworkEpisode {
    tool: EquipmentDefinitionId,
    preparation_ticks: u64,
    projected_ticks: u64,
    observed_hardness: ExcavationHardnessEstimate,
    observed_resource_mass: ResourceMassEstimate,
    planned_local_mass: Mass,
    full_order_tool: Option<EquipmentDefinitionId>,
    resource_knowledge_effect: FieldworkResourceKnowledgeEffect,
    extraction: extraction::FieldworkExtraction,
}

fn run_fieldwork_order(
    registries: &Registries,
    case: FocusedProbeCase,
    requested_mine_mass: Mass,
) -> FieldworkEpisode {
    run_fieldwork_with_supply(
        registries,
        case,
        requested_mine_mass,
        fieldwork_supply_for_case(case),
    )
}

// Controlled supply overrides belong to regression setup, never to candidate selection.
fn run_fieldwork_with_supply(
    registries: &Registries,
    case: FocusedProbeCase,
    requested_mine_mass: Mass,
    deposit_mass: Mass,
) -> FieldworkEpisode {
    let seed = case.seed();
    let FieldworkWorld {
        mut state,
        raw,
        parts,
        destination,
        followup_destination,
        recovery_crushed,
        recovery_residue,
        channel_voxels,
        mining_limits,
        geology_label,
        excavation_hardness,
        copper_rich,
        starting_native_copper,
        native_copper,
        matter_before,
    } = build_fieldwork_world(registries, seed, requested_mine_mass, deposit_mass);
    let order_horizon = fieldwork_order_horizon(registries, requested_mine_mass);

    let episode_started_at = state.tick();
    let survival_before = *state
        .survival()
        .player()
        .unwrap_or_else(|| panic!("fieldwork initial survival record disappeared"));
    let (hammer, sampling_setup_ticks) =
        assemble_sampling_hammer(registries, &mut state, raw, parts);
    let search_started_at = state.tick();
    let FieldworkLocalization {
        target,
        hardness: observed_hardness,
        resource_mass: observed_resource_mass,
        transects,
        field_inspections,
        detailed_surveys,
        indexed_surveys,
    } = localize_target(
        registries,
        &mut state,
        hammer,
        channel_voxels,
        CHANNEL_START_X,
        FieldworkSurveyStrategy::PointSearch,
    );
    assert_eq!(indexed_surveys, 0);
    let target_region = target.region();
    let search_ticks = state.tick().value() - search_started_at.value();
    assert!(
        observed_hardness.lower() <= excavation_hardness
            && observed_hardness.upper() >= excavation_hardness,
        "actor-visible hardness band must conservatively contain diagnostic geological truth"
    );
    assert!(
        observed_resource_mass.lower() <= deposit_mass
            && deposit_mass <= observed_resource_mass.upper(),
        "actor-visible reserve band must conservatively contain diagnostic geological truth"
    );
    let planned_local_mass = requested_mine_mass.min(observed_resource_mass.upper());
    assert!(
        !planned_local_mass.is_zero(),
        "fieldwork acquired reserve evidence must leave a nonzero plausible local workload"
    );
    let full_order_estimate = choose_fieldwork_tool_with_market_phase(
        registries,
        &state,
        raw,
        parts,
        observed_hardness.upper(),
        requested_mine_mass,
        "full-order-before-reserve-scale",
    );
    let estimate = choose_fieldwork_tool_with_market_phase(
        registries,
        &state,
        raw,
        parts,
        observed_hardness.upper(),
        planned_local_mass,
        "acquired-evidence",
    )
    .unwrap_or_else(|| {
        panic!("fieldwork bounded raw-tool family has no candidate for the acquired evidence")
    });
    let full_order_tool_label = full_order_estimate
        .as_ref()
        .map_or("none", |candidate| candidate.tool.label);
    let full_order_tool = full_order_estimate
        .as_ref()
        .map(|candidate| candidate.tool.target);
    let resource_knowledge_effect = match full_order_estimate.as_ref() {
        Some(candidate) if candidate.tool.target == estimate.tool.target => {
            FieldworkResourceKnowledgeEffect::SameTool
        }
        Some(_) => FieldworkResourceKnowledgeEffect::ChangedTool,
        None => FieldworkResourceKnowledgeEffect::ChangedFeasibility,
    };
    reviewln!(
        "FIELDWORK DECISION seed=0x{seed:016X} tick={} selected={} policy=min-preparation-plus-wear-adjusted-local-opportunity,then-native-copper,then-raw-mass,ties-light-first requested={}mg observed-resource-mass={}..{}mg planned-local-work={}mg full-order-tool={} resource-knowledge-effect={} preparation={}t projected-order={}t total={}t authorization=not-yet",
        state.tick().value(),
        estimate.tool.label,
        requested_mine_mass.milligrams(),
        observed_resource_mass.lower().milligrams(),
        observed_resource_mass.upper().milligrams(),
        planned_local_mass.milligrams(),
        full_order_tool_label,
        resource_knowledge_effect.label(),
        estimate.preparation_ticks,
        estimate.order_ticks,
        estimate.total_ticks()
    );
    match fieldwork_bulk_crossover(
        registries,
        &state,
        raw,
        parts,
        observed_hardness.upper(),
        mining_limits.base_quarry_batch,
    ) {
        Some(crossover) => reviewln!(
            "FIELDWORK BULK CROSSOVER seed=0x{seed:016X} available=true tool={} order={}mg base-batches={} current-order={}mg scope=diagnostic-visible-state no-hidden-reserve=true",
            crossover.tool_label,
            crossover.order.milligrams(),
            crossover.batches,
            planned_local_mass.milligrams(),
        ),
        None => reviewln!(
            "FIELDWORK BULK CROSSOVER seed=0x{seed:016X} available=false sampled-through=96-base-batches current-order={}mg scope=diagnostic-visible-state no-hidden-reserve=true",
            planned_local_mass.milligrams(),
        ),
    }
    let raw_before: BTreeMap<_, _> = estimate
        .raw
        .keys()
        .map(|&commodity| {
            let mass = state
                .inventory()
                .get_stockpile(raw)
                .unwrap_or_else(|| panic!("fieldwork raw stockpile disappeared"))
                .get_mass(commodity);
            (commodity, mass)
        })
        .collect();
    let (mining_equipment, tool_prep_ticks) =
        assemble_fieldwork_tool(registries, &mut state, raw, parts, estimate.tool);
    for (&commodity, &expected) in &estimate.raw {
        let retained = state
            .inventory()
            .get_stockpile(raw)
            .unwrap_or_else(|| panic!("fieldwork raw stockpile disappeared"))
            .get_mass(commodity);
        assert_eq!(
            raw_before[&commodity].checked_sub(retained),
            Some(expected),
            "fieldwork executed raw bill must match the candidate's material cost"
        );
    }
    assert_eq!(
        tool_prep_ticks, estimate.preparation_ticks,
        "fieldwork executed preparation must agree with its pre-action craft resolutions"
    );
    let campaign_sites = [
        FieldworkCampaignSite {
            start_x: SECONDARY_CHANNEL_START_X,
        },
        FieldworkCampaignSite {
            start_x: TERTIARY_CHANNEL_START_X,
        },
        FieldworkCampaignSite {
            start_x: QUATERNARY_CHANNEL_START_X,
        },
    ];
    let survey_campaign = evaluate_fieldwork_survey_campaign(
        registries,
        &state,
        FieldworkSurveyCampaignPlan {
            raw,
            parts,
            hammer,
            channel_voxels,
            sites: &campaign_sites,
            planned_sites: planned_future_sites(seed),
        },
    );
    let extraction = execute_fieldwork_extraction(
        registries,
        &mut state,
        FieldworkExtractionOrder {
            target,
            destination,
            equipment: mining_equipment,
            requested: requested_mine_mass,
            batch_limit: estimate.batch,
        },
    );
    finalize_fieldwork_episode(FieldworkEpisodeReview {
        registries,
        state: &state,
        case,
        requested: requested_mine_mass,
        deposit_mass,
        order_horizon,
        raw,
        parts,
        ore_source: destination,
        followup_destination,
        recovery_crushed,
        recovery_residue,
        sampling_hammer: hammer,
        channel_voxels,
        mining_equipment,
        target_region,
        estimate: &estimate,
        extraction: &extraction,
        survey_campaign: &survey_campaign,
        episode_started_at: episode_started_at.value(),
        sampling_setup_ticks,
        search_ticks,
        tool_prep_ticks,
        transects,
        field_inspections,
        detailed_surveys,
        observed_hardness,
        observed_resource_mass,
        planned_local_mass,
        full_order_tool,
        full_order_tool_label,
        resource_knowledge_effect,
        geology_label,
        copper_rich,
        starting_native_copper,
        native_copper,
        matter_before,
        survival_before_energy: survival_before.metabolic_energy(),
        survival_before_hydration: survival_before.hydration(),
    })
}

//! Replayable ordinary prospecting-to-mining episode for the cold-agent report.

use std::collections::BTreeMap;

use deep_hearth::capability::CapabilityValue;
use deep_hearth::content::gameplay_fixture::{
    GeologicalDepositSeed, seed_geological_deposit, seed_lot,
};
use deep_hearth::content::{
    EQUIPMENT_COPPER_REINFORCED_PICK, EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK,
    EQUIPMENT_STONE_GEOLOGICAL_HAMMER, EQUIPMENT_STONE_PICK, EQUIPMENT_STONE_QUARRY_PICK,
    FORM_NATIVE_METAL, FORM_ORE, MATERIAL_COPPER, MINING_METHOD_HAND_PICK,
    PROSPECTING_DETAILED_FIELD_SURVEY, PROSPECTING_FIELD_INSPECTION, PROSPECTING_LOCAL_TRANSECT,
};
use deep_hearth::core::quantity::{Energy, Mass, Pressure, Volume};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::core::time::WorldSeed;
use deep_hearth::crafting::resolve_manual_craft;
use deep_hearth::equipment::{
    EquipmentDefinitionId, EquipmentId, validate_assemble_equipment, validate_upgrade_equipment,
};
use deep_hearth::geology::{
    ExcavationHardnessEstimate, FieldProspectingOutcome, FieldProspectingRequest,
    GeologicalEvidenceKind, validate_start_field_prospecting,
};
use deep_hearth::inventory::StockpileId;
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::mining::{
    MiningOrderRequest, MiningStartError, MiningTargetRequest, MiningTargetResolution,
    MiningTargetResolutionError, resolve_mining_order, resolve_mining_target,
    validate_claim_mining_output, validate_start_mining,
};
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;
use deep_hearth::spatial::{VoxelBounds, VoxelCoord};
use deep_hearth::survival::initialize_player_survival;

use super::environment::ROOM_TEMPERATURE;
use super::focused_runner::focused_probe_role_label;
use super::focused_seeds::FocusedProbeCase;
use super::inventory_support::add_solid_stockpile;
use super::manual_craft_execution::execute_manual_craft_batches;
use super::manual_craft_planning::{
    manual_craft_plan_for_available_output, manual_craft_topology_plan_for_output,
};
use super::manual_craft_selection::{
    first_sufficient_pure_temperature, select_manual_craft_request,
};
use super::ore_fixture::copper_ore_composition;
use super::physical_time::format_physical_duration;
use super::prospecting_timing::complete_prospecting_work;
use super::seed::mix64;

#[path = "fieldwork_probe/extraction.rs"]
mod extraction;
use extraction::{FieldworkExtractionOrder, FieldworkStop, execute_fieldwork_extraction};

#[cfg(test)]
#[path = "fieldwork_probe/supply_tests.rs"]
mod supply_tests;

#[path = "fieldwork_probe/planning.rs"]
mod planning;
use planning::*;

#[cfg(test)]
#[path = "fieldwork_probe/planning_tests.rs"]
mod planning_tests;

#[path = "fieldwork_probe/preparation.rs"]
mod preparation;
use preparation::{assemble_fieldwork_tool, assemble_sampling_hammer};

#[path = "fieldwork_probe/survey.rs"]
mod survey;
use survey::{CHANNEL_COUNT, CHANNEL_START_X, horizontal_region, localize_target};

/// Controlled world generation, independent of demand and tool capabilities. The explicit salt
/// retains rich seeds 1–3; seed 6 is maintained shallow coverage. Neither reserve nor tier is
/// exposed to the actor. These are scenario opportunities, not runtime regional generation.
fn fieldwork_supply(seed: u64) -> Mass {
    let variation = mix64(seed ^ 0x4649_454C_4452_5356);
    let milligrams = if mix64(seed ^ 0x4649_454C_4453_5554).is_multiple_of(2) {
        32_000_000 + variation % 32_000_001
    } else {
        25_000 + variation % 175_001
    };
    Mass::from_milligrams(milligrams)
}

/// Visible scenario demand, sampled independently of hidden geology and never inferred from a
/// deposit's reserve. The long horizon is an explicit extraction order, not downstream demand
/// that the ordinary game has yet demonstrated.
fn fieldwork_order(registries: &Registries, seed: u64) -> Mass {
    let batch = fieldwork_mining_limits(registries).base_quarry_batch;
    if mix64(seed ^ 0x4649_454C_4444_454D).is_multiple_of(2) {
        return short_fieldwork_order(batch, seed);
    }
    let minimum = multiplied_mass(batch, 32, "long-order minimum");
    let span = multiplied_mass(batch, 16, "long-order variation");
    minimum
        .checked_add(Mass::from_milligrams(
            mix64(seed ^ 0x4649_454C_444D_4153) % (span.milligrams() + 1),
        ))
        .unwrap_or_else(|| panic!("fieldwork long order overflowed"))
}

fn short_fieldwork_order(batch: Mass, seed: u64) -> Mass {
    let minimum = (batch.milligrams() / 2).max(1);
    Mass::from_milligrams(
        minimum + mix64(seed ^ 0x4649_454C_444D_4153) % (batch.milligrams() - minimum + 1),
    )
}

#[cfg(not(test))]
pub(super) fn run_fieldwork_probe(registries: &Registries, case: FocusedProbeCase) {
    let episode = run_fieldwork_order(registries, case, fieldwork_order(registries, case.seed()));
    reviewln!(
        "FIELDWORK ENDPOINT seed=0x{:016X} tool={} observed-hardness={}..{}Pa preparation={}t projected-order={}t actual-extraction={}t extracted={}mg outcome={}",
        case.seed(),
        episode.tool.value(),
        episode.observed_hardness.lower().pascals(),
        episode.observed_hardness.upper().pascals(),
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
        fieldwork_supply(case.seed()),
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
    let channel_voxels = i64::try_from(
        registries
            .labor()
            .get_prospecting(PROSPECTING_LOCAL_TRANSECT)
            .map(|definition| definition.maximum_region_voxels())
            .unwrap_or_else(|| panic!("fieldwork local-transect definition disappeared")),
    )
    .unwrap_or_else(|_| panic!("fieldwork transect span exceeds coordinate range"));
    assert!(channel_voxels > 0);
    let hidden_channel = i64::try_from(
        mix64(seed ^ 0x4649_454C_4443_484E)
            % u64::try_from(CHANNEL_COUNT)
                .unwrap_or_else(|_| unreachable!("positive channel count fits u64")),
    )
    .unwrap_or_else(|_| unreachable!("fieldwork channel is bounded"));
    let hidden_slot = i64::try_from(
        mix64(seed ^ 0x4649_454C_4453_4C4F)
            % u64::try_from(channel_voxels)
                .unwrap_or_else(|_| unreachable!("positive channel span fits u64")),
    )
    .unwrap_or_else(|_| unreachable!("fieldwork slot is bounded"));
    let mining_limits = fieldwork_mining_limits(registries);
    let hardness_tier = mix64(seed ^ 0x4649_454C_4448_4152) % 3;
    let base_pa = mining_limits.base_quarry_hardness.pascals();
    let reinforced_quarry_pa = mining_limits.reinforced_quarry_hardness.pascals();
    let reinforced_pick_pa = mining_limits.reinforced_pick_hardness.pascals();
    let (geology_label, excavation_hardness) = match hardness_tier {
        0 => {
            let floor = base_pa.saturating_mul(3) / 4;
            let span = base_pa - floor;
            (
                "quarry-soft",
                Pressure::from_pascals(floor + mix64(seed ^ 0x4649_454C_4453_4F46) % (span + 1)),
            )
        }
        1 => {
            let gap = reinforced_quarry_pa
                .checked_sub(base_pa)
                .unwrap_or_else(|| {
                    unreachable!("reinforced quarry hardness exceeds base hardness")
                });
            (
                "quarry-reinforcement",
                Pressure::from_pascals(base_pa + 1 + mix64(seed ^ 0x4649_454C_444D_4544) % gap),
            )
        }
        2 => {
            let gap = reinforced_pick_pa
                .checked_sub(reinforced_quarry_pa)
                .unwrap_or_else(|| {
                    unreachable!("reinforced pick hardness exceeds reinforced quarry hardness")
                });
            (
                "hard-pick-specialist",
                Pressure::from_pascals(
                    reinforced_quarry_pa + 1 + mix64(seed ^ 0x4649_454C_4448_4152) % gap,
                ),
            )
        }
        _ => unreachable!("three fieldwork hardness tiers are exhaustive"),
    };
    let copper_ppm = 350_000 + (mix64(seed ^ 0x4649_454C_4447_5241) % 300_001) as u32;
    let clay_share_ppm = (mix64(seed ^ 0x4649_454C_4443_4C41) % 600_001) as u32;
    assert!(!requested_mine_mass.is_zero());
    assert!(!deposit_mass.is_zero());
    let order_horizon = if requested_mine_mass <= mining_limits.base_quarry_batch {
        "short"
    } else {
        "long"
    };

    let mut state = AppState::new(WorldSeed::new(seed ^ 0x4649_454C_4457_524C));
    let (raw_opportunity, parts_capacity) = fieldwork_raw_opportunity(registries);
    let native_copper = CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL);
    let starting_native_copper = raw_opportunity
        .get(&native_copper)
        .copied()
        .unwrap_or(Mass::ZERO);
    let raw_capacity = raw_opportunity
        .values()
        .copied()
        .try_fold(Mass::ZERO, |total, mass| total.checked_add(mass))
        .unwrap_or_else(|| panic!("fieldwork raw opportunity capacity overflowed"));
    let raw = add_solid_stockpile(&mut state, raw_capacity);
    for (commodity, mass) in raw_opportunity {
        seed_lot(
            registries,
            &mut state,
            raw,
            commodity,
            mass,
            ROOM_TEMPERATURE,
        );
    }
    let parts = add_solid_stockpile(&mut state, parts_capacity);
    // Disclosed landing capacity supports the visible order, never signals hidden reserve.
    let destination = add_solid_stockpile(&mut state, requested_mine_mass);
    let hidden_region = horizontal_region(
        CHANNEL_START_X + hidden_channel * channel_voxels + hidden_slot,
        1,
    );
    seed_geological_deposit(
        registries,
        &mut state,
        GeologicalDepositSeed::new(
            hidden_region,
            CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            deposit_mass,
            ROOM_TEMPERATURE,
            excavation_hardness,
            copper_ore_composition(copper_ppm, clay_share_ppm),
        ),
    );
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("fieldwork initial matter audit failed: {error}"))
        .total();
    initialize_player_survival(registries, &mut state)
        .unwrap_or_else(|error| panic!("fieldwork survival setup failed: {error}"));

    let episode_started_at = state.tick();
    let survival_before = *state
        .survival()
        .player()
        .unwrap_or_else(|| panic!("fieldwork initial survival record disappeared"));
    let (hammer, sampling_setup_ticks) =
        assemble_sampling_hammer(registries, &mut state, raw, parts);
    let search_started_at = state.tick();
    let (target, observed_hardness, transects, field_inspections, detailed_surveys) =
        localize_target(registries, &mut state, hammer, channel_voxels);
    let search_ticks = state.tick().value() - search_started_at.value();
    assert!(
        observed_hardness.lower() <= excavation_hardness
            && observed_hardness.upper() >= excavation_hardness,
        "actor-visible hardness band must conservatively contain diagnostic geological truth"
    );
    let estimate = choose_fieldwork_tool(
        registries,
        &state,
        raw,
        observed_hardness.upper(),
        requested_mine_mass,
    )
    .unwrap_or_else(|| {
        panic!("fieldwork bounded raw-tool family has no candidate for the acquired evidence")
    });
    reviewln!(
        "FIELDWORK DECISION seed=0x{seed:016X} tick={} selected={} policy=min-preparation-plus-wear-adjusted-order,then-native-copper,then-raw-mass,ties-light-first preparation={}t projected-order={}t total={}t authorization=not-yet",
        state.tick().value(),
        estimate.tool.label,
        estimate.preparation_ticks,
        estimate.order_ticks,
        estimate.total_ticks()
    );
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
    let quarry_label = estimate.tool.label;
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
    let extracted_mass = extraction.extracted;
    let mining_ticks = extraction.ticks;
    let batches = extraction.batches;
    let condition_before = extraction.condition_before;
    let condition_after = extraction.condition_after;
    let adaptation = extraction.adaptation;
    let outcome = extraction.stop.outcome();
    let first_ore_ticks =
        sampling_setup_ticks + search_ticks + tool_prep_ticks + extraction.first_ore_ticks;
    assert_eq!(
        state
            .equipment()
            .get_equipment(mining_equipment)
            .map(|record| record.condition()),
        Some(condition_after)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("fieldwork final matter audit failed: {error}"))
            .total(),
        matter_before
    );
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("fieldwork final state invalid: {error}"));
    let retained_native_copper = state
        .inventory()
        .get_stockpile(raw)
        .map(|stockpile| stockpile.get_mass(native_copper))
        .unwrap_or_else(|| panic!("fieldwork raw stockpile disappeared"));
    let survival_after = state
        .survival()
        .player()
        .unwrap_or_else(|| panic!("fieldwork final survival record disappeared"));
    // No intake occurs in this episode: reserve deltas include every canonical tick's
    // basal and work costs, from sampling-tool preparation through the completed extraction order.
    let metabolic_energy_spent = survival_before
        .metabolic_energy()
        .checked_sub(survival_after.metabolic_energy())
        .unwrap_or_else(|| panic!("fieldwork metabolic reserve increased without intake"));
    let hydration_spent = survival_before
        .hydration()
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
        Some(survival_before.metabolic_energy()),
        "fieldwork reported metabolic cost must reconcile with canonical player reserves"
    );
    assert_eq!(
        survival_after.hydration().checked_add(hydration_spent),
        Some(survival_before.hydration()),
        "fieldwork reported hydration cost must reconcile with canonical player reserves"
    );
    let output_grade_ppm = extraction.output_grade_ppm;
    let sampling_setup_time = format_physical_duration(registries, sampling_setup_ticks);
    let tool_prep_time = format_physical_duration(registries, tool_prep_ticks);
    let mining_time = format_physical_duration(registries, mining_ticks);
    let total_ticks = state.tick().value() - episode_started_at.value();
    let first_ore_time = format_physical_duration(registries, first_ore_ticks);
    assert_eq!(
        total_ticks,
        sampling_setup_ticks + search_ticks + tool_prep_ticks + mining_ticks,
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
    } else if estimate.order_ticks == mining_ticks {
        "true"
    } else {
        "false"
    };
    let extraction_error = if completed {
        format!(
            "{:+}t",
            i128::from(mining_ticks) - i128::from(estimate.order_ticks)
        )
    } else {
        "not-applicable".to_owned()
    };
    let search_time = format_physical_duration(registries, search_ticks);
    let total_time = format_physical_duration(registries, total_ticks);
    reviewln!(
        "FIELDWORK ESTIMATE FEEDBACK seed=0x{seed:016X} selected={} order-horizon={order_horizon} outcome={outcome} requested={}mg output={}mg preparation-estimate={}t preparation-actual={}t wear-adjusted-order-estimate={}t extraction-actual={}t extraction-error={extraction_error} actual-build-plus-order={}t/{} condition={}ppm->{}ppm comparison={comparison} estimate-matched={estimate_matched} choice-frozen-before-action=true service=none",
        estimate.tool.label,
        requested_mine_mass.milligrams(),
        extracted_mass.milligrams(),
        estimate.preparation_ticks,
        tool_prep_ticks,
        estimate.order_ticks,
        mining_ticks,
        tool_prep_ticks + mining_ticks,
        format_physical_duration(registries, tool_prep_ticks + mining_ticks),
        condition_before.parts_per_million(),
        condition_after.parts_per_million(),
    );
    reviewln!(
        "FIELDWORK PACING seed=0x{seed:016X} search={search_ticks}t/{search_time} sampling-tool={sampling_setup_ticks}t/{sampling_setup_time} extraction-tool={tool_prep_ticks}t/{tool_prep_time} extraction={mining_ticks}t/{mining_time} batches={batches} first-ore={first_ore_ticks}t/{first_ore_time} episode-end={}t/{total_time} output={}mg outcome={outcome} requested={}mg scope=raw-tools-and-preowned-copper-to-first-ore repeat-extraction-excludes-discovery=true output-grade={output_grade_ppm}ppm",
        total_ticks,
        extracted_mass.milligrams(),
        requested_mine_mass.milligrams(),
    );

    reviewln!(
        "FIELDWORK EXPERIENCE seed=0x{seed:016X} sample={} outcome={outcome} order-horizon={order_horizon} demand=explicit-extraction-order search=compare-local-transects->cheap-inspection->targeted-survey channels={} transects={} selected-channel=observed-strongest field-inspections={} detailed-surveys={} target=acquired-evidence observed-hardness={}..{}Pa geology={geology_label} tool={quarry_label} adaptation={adaptation} sampling-setup={}t/{sampling_setup_time} tool-prep={}t/{tool_prep_time} starting-native-copper={}mg retained-native-copper={}mg requested={}mg mining={}mg duration={}t/{mining_time} condition={}ppm->{}ppm output-grade={output_grade_ppm}ppm matter=conserved survival=[energy:{}nJ hydration:{}uL]",
        focused_probe_role_label(case.role()),
        CHANNEL_COUNT,
        transects,
        field_inspections,
        detailed_surveys,
        observed_hardness.lower().pascals(),
        observed_hardness.upper().pascals(),
        sampling_setup_ticks,
        tool_prep_ticks,
        starting_native_copper.milligrams(),
        retained_native_copper.milligrams(),
        requested_mine_mass.milligrams(),
        extracted_mass.milligrams(),
        mining_ticks,
        condition_before.parts_per_million(),
        condition_after.parts_per_million(),
        metabolic_energy_spent.nanojoules(),
        hydration_spent.microliters(),
    );
    reviewln!(
        "FIELDWORK SUPPLY seed=0x{seed:016X} outcome={outcome} requested={}mg extracted={}mg shortfall={}mg stop={} effort={mining_ticks}t investment={}t",
        requested_mine_mass.milligrams(),
        extracted_mass.milligrams(),
        requested_mine_mass
            .checked_sub(extracted_mass)
            .unwrap_or_else(|| panic!("fieldwork output exceeded order"))
            .milligrams(),
        extraction.stop.label(),
        sampling_setup_ticks + tool_prep_ticks,
    );
    reviewln!(
        "FIELDWORK SUPPLY DIAGNOSTIC seed=0x{seed:016X} initial-reserve={}mg policy-input=false",
        deposit_mass.milligrams(),
    );
    FieldworkEpisode {
        tool: estimate.tool.target,
        preparation_ticks: tool_prep_ticks,
        projected_ticks: estimate.order_ticks,
        observed_hardness,
        extraction,
    }
}

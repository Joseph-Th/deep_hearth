//! Maintained exact-local field expedition through ordinary movement, tool custody, survey, and haulage.

use deep_hearth::content::gameplay_fixture::{
    GeologicalDepositSeed, locate_ground_stockpile, seed_assembled_equipment_at,
    seed_geological_deposit, seed_ground_stockpile, seed_lot, seed_stockpile,
    seed_traversable_edge,
};
use deep_hearth::content::{
    EQUIPMENT_STONE_GEOLOGICAL_HAMMER, EQUIPMENT_STONE_QUARRY_PICK, FORM_ORE, MATERIAL_COPPER,
    PROCESS_HAND_SORT_NATIVE_COPPER, PROSPECTING_LOCAL_TRANSECT,
};
use deep_hearth::core::quantity::{Energy, Mass, Volume};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::equipment::{EquipmentDefinitionId, EquipmentId};
use deep_hearth::geology::{
    ExcavationHardnessEstimate, FieldProspectingOutcome, GeologicalEvidenceKind,
    ResourceMassEstimate,
};
use deep_hearth::inventory::{MaterialLotSelection, StockpileId, StockpileStorageProfile};
use deep_hearth::logistics::{
    assess_player_carrying, validate_drop_equipment, validate_drop_to_ground,
    validate_pickup_equipment, validate_start_player_step,
};
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::mining::{
    MiningTargetRequest, MiningTargetResolutionError, resolve_mining_target,
};
use deep_hearth::player::validate_admit_player;
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;
use deep_hearth::spatial::VoxelCoord;
use deep_hearth::survival::assess_survival;

use super::super::environment::ROOM_TEMPERATURE;
use super::super::focused_case::{FocusedProbeCase, FocusedProbeRole};
use super::super::focused_witnesses::FIELDWORK_ADAPTIVE_RETOOL_COVERAGE_SEED;
use super::super::ore_fixture::copper_ore_composition;
use super::super::tick_observation::{TickEventAllowance, assert_tick_events_within};
use super::extraction::{FieldworkExtractionOrder, FieldworkStop, execute_fieldwork_extraction};
use super::planning::{
    choose_fieldwork_tool_with_market_phase, equipment_component_requirements,
    fieldwork_mining_limits, fieldwork_tool_label,
};
use super::preparation::{assemble_fieldwork_tool, assemble_sampling_hammer};
use super::retooling::{
    FieldworkOwnedOreRecovery, FieldworkSiteToolRequest, prepare_fieldwork_tool_for_site,
};
use super::survey::{CHANNEL_COUNT, horizontal_region, run_survey};
use super::world::{
    generated_fieldwork_raw_opportunity, seed_generated_followup_fieldwork_site,
    seed_generated_primary_fieldwork_site,
};

const CAMP: VoxelCoord = VoxelCoord::new(0, -1, 0);
const BARREN_SITE: VoxelCoord = VoxelCoord::new(1, -1, 0);
const TARGET_SITE: VoxelCoord = VoxelCoord::new(2, -1, 0);
const FOLLOWUP_LOCAL_START_X: i64 = 3;
const EXPEDITION_DEPOSIT_MASS: Mass = Mass::from_milligrams(40_000_000);
const EXPEDITION_ORDER_MASS: Mass = Mass::from_milligrams(30_000_000);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct StepCost {
    load: Mass,
    energy: Energy,
    hydration: Volume,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct GeneratedExactLocalFieldOrderReview {
    pub(super) tool: EquipmentDefinitionId,
    pub(super) observed_hardness_lower_pa: u64,
    pub(super) observed_hardness_upper_pa: u64,
    pub(super) observed_mass_lower_mg: u64,
    pub(super) observed_mass_upper_mg: u64,
    pub(super) returned: Mass,
    pub(super) stop: FieldworkStop,
    pub(super) trips: u64,
    pub(super) depletion_reroute: Option<GeneratedExactLocalDepletionRerouteReview>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct GeneratedExactLocalDepletionRerouteReview {
    pub(super) depleted_repeat_returned: Mass,
    pub(super) depleted_repeat_stop: FieldworkStop,
    pub(super) observed_hardness_lower_pa: u64,
    pub(super) observed_hardness_upper_pa: u64,
    pub(super) observed_mass_lower_mg: u64,
    pub(super) observed_mass_upper_mg: u64,
    pub(super) survey_ticks: u64,
    pub(super) tool: EquipmentDefinitionId,
    pub(super) preparation_ticks: u64,
    pub(super) salvaged: bool,
    pub(super) returned: Mass,
    pub(super) stop: FieldworkStop,
    pub(super) trips: u64,
}

fn assert_exact_local_field_state(registries: &Registries, state: &AppState, context: &str) {
    assert!(
        state.logistics().player().is_some() && state.survival().player().is_some(),
        "exact-local field {context} requires complete player admission"
    );
    let carrying = assess_player_carrying(state)
        .unwrap_or_else(|| panic!("exact-local field {context} lost carried custody"));
    assert_eq!(
        state.logistics().player().map(|player| player.position()),
        Some(carrying.position()),
        "exact-local field {context} player position disagrees with carried custody"
    );
    validate_loaded_state(registries, state)
        .unwrap_or_else(|error| panic!("exact-local field {context} state invalid: {error}"));
}

fn walk_linear_to(
    registries: &Registries,
    state: &mut AppState,
    destination: VoxelCoord,
) -> Vec<StepCost> {
    let mut costs = Vec::new();
    loop {
        let current = state
            .logistics()
            .player()
            .map(|player| player.position())
            .unwrap_or_else(|| panic!("exact-local linear walk lost player position"));
        if current == destination {
            break;
        }
        assert_eq!(
            (current.y(), current.z()),
            (destination.y(), destination.z()),
            "bounded exact-local field route is a horizontal authored corridor"
        );
        let next_x = if current.x() < destination.x() {
            current
                .x()
                .checked_add(1)
                .unwrap_or_else(|| panic!("exact-local corridor x-coordinate overflowed"))
        } else {
            current
                .x()
                .checked_sub(1)
                .unwrap_or_else(|| panic!("exact-local corridor x-coordinate underflowed"))
        };
        costs.push(walk_step(
            registries,
            state,
            VoxelCoord::new(next_x, current.y(), current.z()),
        ));
    }
    costs
}

fn seed_field_kit(registries: &Registries, state: &mut AppState) -> (EquipmentId, EquipmentId) {
    let definitions = [
        EQUIPMENT_STONE_GEOLOGICAL_HAMMER,
        EQUIPMENT_STONE_QUARRY_PICK,
    ];
    let requirements = equipment_component_requirements(registries, &definitions);
    let capacity = requirements
        .values()
        .copied()
        .try_fold(Mass::ZERO, Mass::checked_add)
        .unwrap_or_else(|| panic!("exact-local field-kit component mass overflowed"));
    let source = seed_stockpile(
        state,
        capacity,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    for (commodity, mass) in requirements {
        seed_lot(registries, state, source, commodity, mass, ROOM_TEMPERATURE);
    }
    let hammer = seed_assembled_equipment_at(
        registries,
        state,
        EQUIPMENT_STONE_GEOLOGICAL_HAMMER,
        source,
        CAMP,
    );
    let pick =
        seed_assembled_equipment_at(registries, state, EQUIPMENT_STONE_QUARRY_PICK, source, CAMP);
    locate_ground_stockpile(state, source, CAMP);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(source)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::ZERO),
        "material-backed field kit should consume its disclosed component source exactly"
    );
    (hammer, pick)
}

fn equipment_mass(state: &AppState, equipment: EquipmentId) -> Mass {
    state
        .equipment()
        .get_equipment(equipment)
        .map(|record| record.embodied_mass())
        .unwrap_or_else(|| panic!("exact-local field equipment disappeared"))
}

fn walk_step(registries: &Registries, state: &mut AppState, destination: VoxelCoord) -> StepCost {
    let carrying = assess_player_carrying(state)
        .unwrap_or_else(|| panic!("exact-local field step lost player carrying assessment"));
    let before = assess_survival(registries, state)
        .unwrap_or_else(|| panic!("exact-local field step lost player survival"));
    let work = validate_start_player_step(registries, state, destination)
        .unwrap_or_else(|error| panic!("exact-local field step admission failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("exact-local field step commit failed: {error}"));
    let mut completed = false;
    while state.tick() < work.completes_at() {
        let outcome = advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("exact-local field step tick failed: {error}"));
        assert_tick_events_within(
            &outcome,
            TickEventAllowance {
                player_step: true,
                ..TickEventAllowance::default()
            },
            "exact-local field expedition movement",
        );
        if let Some(step) = outcome.player_step() {
            assert_eq!(step.origin(), work.origin());
            assert_eq!(step.destination(), work.destination());
            completed = true;
        }
    }
    assert!(
        completed,
        "exact-local field step produced no completion outcome"
    );
    let after = assess_survival(registries, state)
        .unwrap_or_else(|| panic!("exact-local field step lost final survival"));
    StepCost {
        load: carrying.stored(),
        energy: Energy::from_nanojoules(
            before
                .metabolic_energy()
                .nanojoules()
                .checked_sub(after.metabolic_energy().nanojoules())
                .unwrap_or_else(|| panic!("field expedition movement created metabolic energy")),
        ),
        hydration: Volume::from_microliters(
            before
                .hydration()
                .microliters()
                .checked_sub(after.hydration().microliters())
                .unwrap_or_else(|| panic!("field expedition movement created hydration")),
        ),
    }
}

fn survey_site(
    registries: &Registries,
    state: &mut AppState,
    position: VoxelCoord,
    hammer: EquipmentId,
) -> Option<FieldProspectingOutcome> {
    let region = horizontal_region(position.x(), 1);
    assert!(region.has_voxel(position));
    let inspection = run_survey(
        registries,
        state,
        GeologicalEvidenceKind::SurfaceExposure,
        region,
        None,
        "exact-local field inspection",
    );
    let finding = state
        .geological_knowledge()
        .get_observation(inspection.observation())
        .and_then(|record| record.finding(MATERIAL_COPPER))
        .unwrap_or_else(|| panic!("exact-local field inspection lost copper finding"));
    if finding.lower_ppm() == 0 {
        return None;
    }
    let detailed = run_survey(
        registries,
        state,
        GeologicalEvidenceKind::ExcavationSample,
        region,
        Some(hammer),
        "exact-local detailed survey",
    );
    let record = state
        .geological_knowledge()
        .get_observation(detailed.observation())
        .unwrap_or_else(|| panic!("exact-local detailed field observation disappeared"));
    assert_eq!(record.evidence(), GeologicalEvidenceKind::ExcavationSample);
    assert!(record.excavation_hardness().is_some());
    assert!(record.resource_mass().is_some());
    Some(detailed)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ExactLocalSearchResult {
    position: VoxelCoord,
    hardness: ExcavationHardnessEstimate,
    resource_mass: ResourceMassEstimate,
    transects: u64,
    field_inspections: u64,
    detailed_surveys: u64,
    survey_ticks: u64,
    travel_steps: u64,
}

fn run_exact_local_point_search(
    registries: &Registries,
    state: &mut AppState,
    hammer: EquipmentId,
    channel_voxels: i64,
    channel_start_x: i64,
) -> ExactLocalSearchResult {
    let mut selected_channel = None::<(i64, u32, u32)>;
    let mut survey_ticks = 0_u64;
    let mut travel_steps = 0_u64;
    for channel_index in 0..CHANNEL_COUNT {
        let channel_start = channel_start_x + channel_index * channel_voxels;
        travel_steps = travel_steps
            .checked_add(
                u64::try_from(
                    walk_linear_to(registries, state, VoxelCoord::new(channel_start, -1, 0)).len(),
                )
                .unwrap_or_else(|_| unreachable!("bounded exact-local travel fits u64")),
            )
            .unwrap_or_else(|| panic!("exact-local search travel count overflowed"));
        let started_at = state.tick().value();
        let outcome = run_survey(
            registries,
            state,
            GeologicalEvidenceKind::SurfaceExposure,
            horizontal_region(channel_start, channel_voxels),
            None,
            "exact-local candidate transect",
        );
        survey_ticks = survey_ticks
            .checked_add(state.tick().value() - started_at)
            .unwrap_or_else(|| panic!("exact-local search survey time overflowed"));
        let finding = state
            .geological_knowledge()
            .get_observation(outcome.observation())
            .and_then(|record| record.finding(MATERIAL_COPPER))
            .unwrap_or_else(|| panic!("exact-local transect copper finding disappeared"));
        let evidence = (finding.lower_ppm(), finding.upper_ppm());
        if selected_channel.is_none_or(|(_start, lower, upper)| evidence > (lower, upper)) {
            selected_channel = Some((channel_start, evidence.0, evidence.1));
        }
    }
    let (selected_channel_start, _, _) = selected_channel
        .unwrap_or_else(|| unreachable!("exact-local point search has candidate channels"));
    travel_steps = travel_steps
        .checked_add(
            u64::try_from(
                walk_linear_to(
                    registries,
                    state,
                    VoxelCoord::new(selected_channel_start, -1, 0),
                )
                .len(),
            )
            .unwrap_or_else(|_| unreachable!("bounded exact-local travel fits u64")),
        )
        .unwrap_or_else(|| panic!("exact-local search travel count overflowed"));

    for (inspection_index, offset) in (0..channel_voxels).enumerate() {
        let position = VoxelCoord::new(selected_channel_start + offset, -1, 0);
        travel_steps = travel_steps
            .checked_add(
                u64::try_from(walk_linear_to(registries, state, position).len())
                    .unwrap_or_else(|_| unreachable!("bounded exact-local travel fits u64")),
            )
            .unwrap_or_else(|| panic!("exact-local search travel count overflowed"));
        let point = horizontal_region(position.x(), 1);
        let started_at = state.tick().value();
        let inspection = run_survey(
            registries,
            state,
            GeologicalEvidenceKind::SurfaceExposure,
            point,
            None,
            "exact-local follow-up inspection",
        );
        survey_ticks = survey_ticks
            .checked_add(state.tick().value() - started_at)
            .unwrap_or_else(|| panic!("exact-local search survey time overflowed"));
        let finding = state
            .geological_knowledge()
            .get_observation(inspection.observation())
            .and_then(|record| record.finding(MATERIAL_COPPER))
            .unwrap_or_else(|| panic!("exact-local follow-up copper finding disappeared"));
        if finding.lower_ppm() == 0 {
            continue;
        }
        let started_at = state.tick().value();
        let detailed = run_survey(
            registries,
            state,
            GeologicalEvidenceKind::ExcavationSample,
            point,
            Some(hammer),
            "exact-local follow-up detailed survey",
        );
        survey_ticks = survey_ticks
            .checked_add(state.tick().value() - started_at)
            .unwrap_or_else(|| panic!("exact-local search survey time overflowed"));
        let record = state
            .geological_knowledge()
            .get_observation(detailed.observation())
            .unwrap_or_else(|| panic!("exact-local follow-up detailed observation disappeared"));
        assert_eq!(record.evidence(), GeologicalEvidenceKind::ExcavationSample);
        return ExactLocalSearchResult {
            position,
            hardness: record
                .excavation_hardness()
                .unwrap_or_else(|| panic!("exact-local follow-up survey lost hardness evidence")),
            resource_mass: record.resource_mass().unwrap_or_else(|| {
                panic!("exact-local follow-up survey lost resource-scale evidence")
            }),
            transects: u64::try_from(CHANNEL_COUNT)
                .unwrap_or_else(|_| unreachable!("positive channel count fits u64")),
            field_inspections: u64::try_from(inspection_index + 1)
                .unwrap_or_else(|_| unreachable!("bounded inspection count fits u64")),
            detailed_surveys: 1,
            survey_ticks,
            travel_steps,
        };
    }
    panic!("maintained exact-local depletion reroute found no localized follow-up target")
}

fn carried_ore_selection(
    state: &AppState,
    stockpile: deep_hearth::inventory::StockpileId,
) -> Vec<MaterialLotSelection> {
    let ore = CommodityKey::new(MATERIAL_COPPER, FORM_ORE);
    state
        .inventory()
        .lots()
        .filter(|lot| lot.stockpile() == stockpile && lot.commodity() == ore)
        .map(|lot| MaterialLotSelection::new(lot.id(), lot.mass()))
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ExactLocalOrderResult {
    trips: u64,
    batches: u64,
    extraction_ticks: u64,
    returned: Mass,
    stop: FieldworkStop,
    outbound_tool_step: StepCost,
    minimum_return_load: Mass,
    maximum_return_load: Mass,
    maximum_return_step: StepCost,
    minimum_utilization_ppm: u128,
    maximum_utilization_ppm: u128,
    movement_steps: u64,
}

fn execute_exact_local_order(
    registries: &Registries,
    state: &mut AppState,
    cache: deep_hearth::inventory::StockpileId,
    tool: EquipmentId,
    tool_mass: Mass,
    body_capacity: Mass,
    requested: Mass,
    batch_limit: Mass,
    target_site: VoxelCoord,
) -> ExactLocalOrderResult {
    assert!(!requested.is_zero());
    let target_region = horizontal_region(target_site.x(), 1);
    let mut remaining_order = requested;
    let mut trip_count = 0_u64;
    let mut total_extraction_batches = 0_u64;
    let mut total_extraction_ticks = 0_u64;
    let mut total_returned = Mass::ZERO;
    let mut outbound_tool_step = None;
    let mut minimum_return_load = None::<Mass>;
    let mut maximum_return_load = Mass::ZERO;
    let mut maximum_return_step = None::<StepCost>;
    let mut minimum_utilization_ppm = u128::MAX;
    let mut maximum_utilization_ppm = 0_u128;
    let mut movement_steps = 0_u64;
    let mut stop = FieldworkStop::OrderComplete;

    while !remaining_order.is_zero() {
        let outbound = walk_linear_to(registries, state, target_site);
        movement_steps = movement_steps
            .checked_add(
                u64::try_from(outbound.len())
                    .unwrap_or_else(|_| unreachable!("bounded exact-local travel fits u64")),
            )
            .unwrap_or_else(|| panic!("exact-local order travel count overflowed"));
        outbound_tool_step.get_or_insert_with(|| {
            *outbound
                .first()
                .unwrap_or_else(|| panic!("exact-local target must be remote from camp"))
        });
        let target = resolve_mining_target(
            state,
            MiningTargetRequest::new(target_region, MATERIAL_COPPER),
        )
        .unwrap_or_else(|error| panic!("exact-local acquired target no longer resolves: {error}"));
        let carried = assess_player_carrying(state)
            .unwrap_or_else(|| panic!("exact-local mining lost carried custody"));
        let trip_request = remaining_order.min(carried.available());
        assert!(
            !trip_request.is_zero(),
            "exact-local field order must leave carrying room for some ore beside the owned tool"
        );
        let extraction = execute_fieldwork_extraction(
            registries,
            state,
            FieldworkExtractionOrder {
                target,
                destination: carried.stockpile(),
                equipment: tool,
                requested: trip_request,
                batch_limit,
            },
        );
        total_extraction_batches = total_extraction_batches
            .checked_add(extraction.batches)
            .unwrap_or_else(|| panic!("exact-local extraction batch accounting overflowed"));
        total_extraction_ticks = total_extraction_ticks
            .checked_add(extraction.ticks)
            .unwrap_or_else(|| panic!("exact-local extraction time accounting overflowed"));
        remaining_order = remaining_order
            .checked_sub(extraction.extracted)
            .unwrap_or_else(|| unreachable!("trip extraction is bounded by remaining order"));

        let supply_ended = if extraction.stop != FieldworkStop::OrderComplete {
            stop = extraction.stop;
            true
        } else if remaining_order.is_zero() {
            false
        } else {
            match resolve_mining_target(
                state,
                MiningTargetRequest::new(target_region, MATERIAL_COPPER),
            ) {
                Ok(_) => false,
                Err(MiningTargetResolutionError::EvidenceInsufficientToResolveTarget {
                    ..
                }) => {
                    stop = FieldworkStop::TargetNoLongerResolved;
                    true
                }
                Err(error) => panic!("exact-local target refresh failed: {error}"),
            }
        };

        let loaded = assess_player_carrying(state)
            .unwrap_or_else(|| panic!("exact-local loaded carrying assessment disappeared"));
        assert_eq!(
            loaded.stored(),
            tool_mass
                .checked_add(extraction.extracted)
                .unwrap_or_else(|| panic!("exact-local loaded custody overflowed"))
        );
        assert_eq!(loaded.capacity(), body_capacity);
        let return_steps = walk_linear_to(registries, state, CAMP);
        movement_steps = movement_steps
            .checked_add(
                u64::try_from(return_steps.len())
                    .unwrap_or_else(|_| unreachable!("bounded exact-local travel fits u64")),
            )
            .unwrap_or_else(|| panic!("exact-local order travel count overflowed"));
        let return_loaded_step = *return_steps
            .first()
            .unwrap_or_else(|| panic!("exact-local target must be remote from camp"));
        let utilization_ppm = u128::from(return_loaded_step.load.milligrams()) * 1_000_000
            / u128::from(body_capacity.milligrams());
        minimum_return_load = Some(
            minimum_return_load.map_or(return_loaded_step.load, |minimum| {
                minimum.min(return_loaded_step.load)
            }),
        );
        if return_loaded_step.load > maximum_return_load {
            maximum_return_load = return_loaded_step.load;
            maximum_return_step = Some(return_loaded_step);
        }
        minimum_utilization_ppm = minimum_utilization_ppm.min(utilization_ppm);
        maximum_utilization_ppm = maximum_utilization_ppm.max(utilization_ppm);
        let carried = assess_player_carrying(state)
            .unwrap_or_else(|| panic!("exact-local return lost carried custody"));
        let ore_selection = carried_ore_selection(state, carried.stockpile());
        assert!(!ore_selection.is_empty());
        validate_drop_to_ground(registries, state, cache, &ore_selection)
            .unwrap_or_else(|error| panic!("exact-local ore cache drop failed: {error}"))
            .commit(state)
            .unwrap_or_else(|error| panic!("exact-local ore cache drop commit failed: {error}"));
        total_returned = total_returned
            .checked_add(extraction.extracted)
            .unwrap_or_else(|| panic!("exact-local returned ore accounting overflowed"));
        trip_count = trip_count
            .checked_add(1)
            .unwrap_or_else(|| panic!("exact-local trip count overflowed"));
        if supply_ended {
            break;
        }
    }

    if remaining_order.is_zero() {
        stop = FieldworkStop::OrderComplete;
    }
    ExactLocalOrderResult {
        trips: trip_count,
        batches: total_extraction_batches,
        extraction_ticks: total_extraction_ticks,
        returned: total_returned,
        stop,
        outbound_tool_step: outbound_tool_step
            .unwrap_or_else(|| unreachable!("nonzero local order executes one outbound trip")),
        minimum_return_load: minimum_return_load
            .unwrap_or_else(|| unreachable!("nonzero local order executes one return trip")),
        maximum_return_load,
        maximum_return_step: maximum_return_step
            .unwrap_or_else(|| unreachable!("nonzero local order executes one return trip")),
        minimum_utilization_ppm,
        maximum_utilization_ppm,
        movement_steps,
    }
}

pub(super) fn run_exact_local_field_expedition(registries: &Registries) {
    let mut state = AppState::new();
    let limits = fieldwork_mining_limits(registries);
    let (hammer, pick) = seed_field_kit(registries, &mut state);
    let hammer_mass = equipment_mass(&state, hammer);
    let pick_mass = equipment_mass(&state, pick);
    let cache = seed_ground_stockpile(&mut state, CAMP, EXPEDITION_DEPOSIT_MASS);
    seed_geological_deposit(
        registries,
        &mut state,
        GeologicalDepositSeed::new(
            horizontal_region(TARGET_SITE.x(), 1),
            CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            EXPEDITION_DEPOSIT_MASS,
            ROOM_TEMPERATURE,
            limits.base_quarry_hardness,
            copper_ore_composition(500_000, 250_000),
        ),
    );
    seed_traversable_edge(&mut state, CAMP, BARREN_SITE);
    seed_traversable_edge(&mut state, BARREN_SITE, TARGET_SITE);
    validate_admit_player(registries, &state, CAMP)
        .unwrap_or_else(|error| panic!("exact-local field player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("exact-local field player admission commit failed: {error}")
        });
    assert_exact_local_field_state(registries, &state, "expedition start");
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("exact-local field initial matter audit failed: {error}"))
        .total();
    let body_capacity = assess_player_carrying(&state)
        .unwrap_or_else(|| panic!("exact-local field player has no carrying envelope"))
        .capacity();

    validate_pickup_equipment(registries, &state, hammer)
        .unwrap_or_else(|error| panic!("exact-local hammer pickup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("exact-local hammer pickup commit failed: {error}"));
    let hammer_carry = assess_player_carrying(&state)
        .unwrap_or_else(|| panic!("exact-local hammer carrying assessment disappeared"));
    assert_eq!(hammer_carry.capacity(), body_capacity);
    assert_eq!(hammer_carry.equipment(), hammer_mass);
    assert_eq!(state.logistics().equipment_position(hammer), Some(CAMP));

    let _ = walk_step(registries, &mut state, BARREN_SITE);
    assert_eq!(
        state.logistics().equipment_position(hammer),
        Some(BARREN_SITE)
    );
    assert!(survey_site(registries, &mut state, BARREN_SITE, hammer).is_none());
    let _ = walk_step(registries, &mut state, TARGET_SITE);
    assert_eq!(
        state.logistics().equipment_position(hammer),
        Some(TARGET_SITE)
    );
    let detailed = survey_site(registries, &mut state, TARGET_SITE, hammer)
        .unwrap_or_else(|| panic!("maintained exact-local target site should be survey-positive"));
    let detailed_record = state
        .geological_knowledge()
        .get_observation(detailed.observation())
        .unwrap_or_else(|| panic!("exact-local detailed survey result disappeared"));
    let observed_mass = detailed_record
        .resource_mass()
        .unwrap_or_else(|| panic!("exact-local detailed survey did not reveal resource scale"));

    let _ = walk_step(registries, &mut state, BARREN_SITE);
    let _ = walk_step(registries, &mut state, CAMP);
    validate_drop_equipment(registries, &state, hammer)
        .unwrap_or_else(|error| panic!("exact-local hammer drop failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("exact-local hammer drop commit failed: {error}"));
    validate_pickup_equipment(registries, &state, pick)
        .unwrap_or_else(|error| panic!("exact-local pick pickup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("exact-local pick pickup commit failed: {error}"));
    let pick_carry = assess_player_carrying(&state)
        .unwrap_or_else(|| panic!("exact-local pick carrying assessment disappeared"));
    assert_eq!(pick_carry.capacity(), body_capacity);
    assert_eq!(pick_carry.equipment(), pick_mass);

    let requested = EXPEDITION_ORDER_MASS.min(observed_mass.upper());
    assert!(
        requested == EXPEDITION_ORDER_MASS,
        "maintained exact-local reserve evidence should support the independently disclosed 30 kg field order"
    );
    let order = execute_exact_local_order(
        registries,
        &mut state,
        cache,
        pick,
        pick_mass,
        body_capacity,
        requested,
        limits.base_quarry_batch,
        TARGET_SITE,
    );
    assert_eq!(order.stop, FieldworkStop::OrderComplete);
    assert_eq!(order.returned, requested);
    assert!(
        order.trips > 1,
        "30 kg field order should require multiple body-scale haul trips"
    );
    assert_eq!(order.maximum_return_load, body_capacity);
    assert!(order.minimum_return_load < order.maximum_return_load);
    assert!(order.maximum_return_step.energy >= order.outbound_tool_step.energy);
    assert!(order.maximum_return_step.hydration >= order.outbound_tool_step.hydration);

    validate_drop_equipment(registries, &state, pick)
        .unwrap_or_else(|error| panic!("exact-local pick drop failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("exact-local pick drop commit failed: {error}"));
    let final_carry = assess_player_carrying(&state)
        .unwrap_or_else(|| panic!("exact-local final carrying assessment disappeared"));
    assert_eq!(final_carry.stored(), Mass::ZERO);
    assert_eq!(final_carry.available(), body_capacity);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(cache)
            .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_ORE))),
        Some(requested)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("exact-local field final matter audit failed: {error}"))
            .total(),
        matter_before,
        "exact-local field expedition must conserve represented matter through extraction and haulage"
    );
    assert_exact_local_field_state(registries, &state, "completed expedition");

    reviewln!(
        "FIELDWORK EXPEDITION evidence=ordinary-exact-local route=[steps:{} barren-sites:1 target-distance:2 haul-trips:{}] survey=[inspections:2 detailed:1 evidence:excavation-sample observed-mass:{}..{}mg] tools=[hammer:{}mg pick:{}mg custody:player-carried] extraction=[order:{}mg batches:{} ticks:{} complete:true] haul=[outbound:{}mg return:{}..{}mg capacity:{}mg utilization:{}..{}ppm] locomotion=[outbound:{}nJ/{}uL loaded-max:{}nJ/{}uL] cache=[returned:{}mg tool-dropped:true] matter=conserved",
        4 + order.movement_steps,
        order.trips,
        observed_mass.lower().milligrams(),
        observed_mass.upper().milligrams(),
        hammer_mass.milligrams(),
        pick_mass.milligrams(),
        requested.milligrams(),
        order.batches,
        order.extraction_ticks,
        order.outbound_tool_step.load.milligrams(),
        order.minimum_return_load.milligrams(),
        order.maximum_return_load.milligrams(),
        body_capacity.milligrams(),
        order.minimum_utilization_ppm,
        order.maximum_utilization_ppm,
        order.outbound_tool_step.energy.nanojoules(),
        order.outbound_tool_step.hydration.microliters(),
        order.maximum_return_step.energy.nanojoules(),
        order.maximum_return_step.hydration.microliters(),
        requested.milligrams(),
    );
}

#[derive(Clone, Copy, Debug)]
struct ExactLocalDepletionSetup {
    repeat_cache: StockpileId,
    reroute_cache: StockpileId,
    recovery_crushed: StockpileId,
    recovery_residue: StockpileId,
    channel_voxels: i64,
}

fn seed_exact_local_depletion_setup(
    registries: &Registries,
    state: &mut AppState,
    case: FocusedProbeCase,
    requested: Mass,
) -> Option<ExactLocalDepletionSetup> {
    if case.role() != FocusedProbeRole::MaintainedCoverage
        || case.seed() != FIELDWORK_ADAPTIVE_RETOOL_COVERAGE_SEED
    {
        return None;
    }
    let channel_voxels = i64::try_from(
        registries
            .labor()
            .get_prospecting(PROSPECTING_LOCAL_TRANSECT)
            .map(|definition| definition.maximum_region_voxels())
            .unwrap_or_else(|| panic!("exact-local follow-up transect definition disappeared")),
    )
    .unwrap_or_else(|_| panic!("exact-local follow-up transect span exceeds coordinate range"));
    assert!(channel_voxels > 0);
    let repeat_cache = seed_ground_stockpile(state, CAMP, requested);
    let reroute_cache = seed_ground_stockpile(state, CAMP, requested);
    let sorting = registries
        .ore_processing()
        .get_manual_constituent_separation(PROCESS_HAND_SORT_NATIVE_COPPER)
        .unwrap_or_else(|| panic!("exact-local depletion recovery lost hand sorting"));
    let recovery_crushed = seed_ground_stockpile(state, CAMP, sorting.max_batch_mass());
    let recovery_residue = seed_ground_stockpile(state, CAMP, sorting.max_batch_mass());
    seed_generated_followup_fieldwork_site(
        registries,
        state,
        case.seed(),
        0,
        channel_voxels,
        FOLLOWUP_LOCAL_START_X,
    );
    let corridor_voxels = CHANNEL_COUNT
        .checked_mul(channel_voxels)
        .unwrap_or_else(|| panic!("exact-local follow-up corridor width overflowed"));
    let last_x = FOLLOWUP_LOCAL_START_X
        .checked_add(corridor_voxels - 1)
        .unwrap_or_else(|| panic!("exact-local follow-up corridor coordinate overflowed"));
    for x in TARGET_SITE.x()..last_x {
        seed_traversable_edge(
            state,
            VoxelCoord::new(x, -1, 0),
            VoxelCoord::new(x + 1, -1, 0),
        );
    }
    Some(ExactLocalDepletionSetup {
        repeat_cache,
        reroute_cache,
        recovery_crushed,
        recovery_residue,
        channel_voxels,
    })
}

#[allow(clippy::too_many_arguments)]
fn run_exact_local_depletion_reroute(
    registries: &Registries,
    state: &mut AppState,
    setup: ExactLocalDepletionSetup,
    raw: StockpileId,
    parts: StockpileId,
    primary_cache: StockpileId,
    hammer: EquipmentId,
    tool: EquipmentId,
    tool_mass: Mass,
    body_capacity: Mass,
    requested: Mass,
    batch_limit: Mass,
) -> GeneratedExactLocalDepletionRerouteReview {
    let repeat = execute_exact_local_order(
        registries,
        state,
        setup.repeat_cache,
        tool,
        tool_mass,
        body_capacity,
        requested,
        batch_limit,
        TARGET_SITE,
    );
    assert!(
        repeat.returned > Mass::ZERO && repeat.returned < requested,
        "adaptive exact-local witness must physically deplete the known primary site during its repeat order"
    );
    assert_ne!(repeat.stop, FieldworkStop::OrderComplete);

    validate_drop_equipment(registries, state, tool)
        .unwrap_or_else(|error| panic!("exact-local depleted tool drop failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("exact-local depleted tool drop commit failed: {error}"));
    validate_pickup_equipment(registries, state, hammer)
        .unwrap_or_else(|error| panic!("exact-local reroute hammer pickup failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("exact-local reroute hammer pickup commit failed: {error}"));
    let search = run_exact_local_point_search(
        registries,
        state,
        hammer,
        setup.channel_voxels,
        FOLLOWUP_LOCAL_START_X,
    );
    let search_return_steps = u64::try_from(walk_linear_to(registries, state, CAMP).len())
        .unwrap_or_else(|_| unreachable!("bounded exact-local search return fits u64"));
    validate_drop_equipment(registries, state, hammer)
        .unwrap_or_else(|error| panic!("exact-local reroute hammer drop failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("exact-local reroute hammer drop commit failed: {error}"));

    let local_order = requested.min(search.resource_mass.upper());
    let choice = prepare_fieldwork_tool_for_site(
        registries,
        state,
        FieldworkSiteToolRequest::new(
            raw,
            parts,
            FieldworkOwnedOreRecovery {
                ore_source: primary_cache,
                crushed_destination: setup.recovery_crushed,
                residue_destination: setup.recovery_residue,
            },
            &[tool],
            search.hardness.upper(),
            local_order,
        ),
    )
    .unwrap_or_else(|| panic!("exact-local reroute found no feasible extraction-tool adaptation"));
    assert!(
        state
            .available_local_equipment()
            .any(|record| record.id() == choice.equipment),
        "exact-local reroute tool must be actor-visible at camp after adaptation"
    );
    let reroute_tool_mass = equipment_mass(state, choice.equipment);
    validate_pickup_equipment(registries, state, choice.equipment)
        .unwrap_or_else(|error| panic!("exact-local reroute tool pickup failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("exact-local reroute tool pickup commit failed: {error}"));
    let reroute = execute_exact_local_order(
        registries,
        state,
        setup.reroute_cache,
        choice.equipment,
        reroute_tool_mass,
        body_capacity,
        local_order,
        choice.batch,
        search.position,
    );
    validate_drop_equipment(registries, state, choice.equipment)
        .unwrap_or_else(|error| panic!("exact-local reroute tool drop failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("exact-local reroute tool drop commit failed: {error}"));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(setup.repeat_cache)
            .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_ORE))),
        Some(repeat.returned)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(setup.reroute_cache)
            .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_ORE))),
        Some(reroute.returned)
    );
    reviewln!(
        "FIELDWORK LOCAL DEPLETION REROUTE seed=0x{:016X} sample=coverage evidence=ordinary-exact-local depletion=[requested:{}mg returned:{}mg trips:{} stop:{}] search=[strategy:point-search travel-steps:{} path:{}+{} transects:{} inspections:{} detailed:{} survey-work:{}t observed-hardness:{}..{}Pa observed-mass:{}..{}mg] retool=[salvage:{} selected:{} preparation:{}t ore-recovery:{}t] extraction=[requested:{}mg returned:{}mg trips:{} movement-steps:{} batches:{} ticks:{} stop:{}] cache=[repeat:{}mg reroute:{}mg tools-dropped:true] matter=conserved",
        FIELDWORK_ADAPTIVE_RETOOL_COVERAGE_SEED,
        requested.milligrams(),
        repeat.returned.milligrams(),
        repeat.trips,
        repeat.stop.label(),
        search.travel_steps + search_return_steps,
        search.travel_steps,
        search_return_steps,
        search.transects,
        search.field_inspections,
        search.detailed_surveys,
        search.survey_ticks,
        search.hardness.lower().pascals(),
        search.hardness.upper().pascals(),
        search.resource_mass.lower().milligrams(),
        search.resource_mass.upper().milligrams(),
        choice.salvaged_equipment.is_some(),
        fieldwork_tool_label(super::planning::FieldworkTool {
            target: choice.definition,
        }),
        choice.preparation_ticks,
        choice.ore_recovery_ticks,
        local_order.milligrams(),
        reroute.returned.milligrams(),
        reroute.trips,
        reroute.movement_steps,
        reroute.batches,
        reroute.extraction_ticks,
        reroute.stop.label(),
        repeat.returned.milligrams(),
        reroute.returned.milligrams(),
    );
    GeneratedExactLocalDepletionRerouteReview {
        depleted_repeat_returned: repeat.returned,
        depleted_repeat_stop: repeat.stop,
        observed_hardness_lower_pa: search.hardness.lower().pascals(),
        observed_hardness_upper_pa: search.hardness.upper().pascals(),
        observed_mass_lower_mg: search.resource_mass.lower().milligrams(),
        observed_mass_upper_mg: search.resource_mass.upper().milligrams(),
        survey_ticks: search.survey_ticks,
        tool: choice.definition,
        preparation_ticks: choice.preparation_ticks,
        salvaged: choice.salvaged_equipment.is_some(),
        returned: reroute.returned,
        stop: reroute.stop,
        trips: reroute.trips,
    }
}

pub(super) fn run_generated_exact_local_field_order(
    registries: &Registries,
    case: FocusedProbeCase,
    requested: Mass,
    deposit_mass: Mass,
    stratify_hardness: bool,
) -> GeneratedExactLocalFieldOrderReview {
    assert!(!requested.is_zero());
    assert!(!deposit_mass.is_zero());
    let mut state = AppState::new();
    let (raw_opportunity, parts_capacity) =
        generated_fieldwork_raw_opportunity(registries, case.seed(), stratify_hardness);
    let raw_capacity = raw_opportunity
        .values()
        .copied()
        .try_fold(Mass::ZERO, Mass::checked_add)
        .unwrap_or_else(|| panic!("generated exact-local field raw capacity overflowed"));
    let raw = seed_ground_stockpile(&mut state, CAMP, raw_capacity);
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
    let parts = seed_ground_stockpile(&mut state, CAMP, parts_capacity);
    let cache = seed_ground_stockpile(&mut state, CAMP, requested);
    let depletion_setup = seed_exact_local_depletion_setup(registries, &mut state, case, requested);
    let fixture = seed_generated_primary_fieldwork_site(
        registries,
        &mut state,
        case.seed(),
        deposit_mass,
        stratify_hardness,
        horizontal_region(TARGET_SITE.x(), 1),
    );
    seed_traversable_edge(&mut state, CAMP, BARREN_SITE);
    seed_traversable_edge(&mut state, BARREN_SITE, TARGET_SITE);
    validate_admit_player(registries, &state, CAMP)
        .unwrap_or_else(|error| panic!("generated exact-local field admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("generated exact-local field admission commit failed: {error}")
        });
    assert_exact_local_field_state(registries, &state, "generated order start");
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("generated exact-local field matter audit failed: {error}"))
        .total();
    let body_capacity = assess_player_carrying(&state)
        .unwrap_or_else(|| panic!("generated exact-local field player has no carrying envelope"))
        .capacity();

    let (hammer, sampling_setup_ticks) =
        assemble_sampling_hammer(registries, &mut state, raw, parts);
    assert!(
        state
            .available_local_equipment()
            .any(|record| record.id() == hammer),
        "generated exact-local sampling hammer should be actor-visible at the camp where it was assembled"
    );
    let hammer_mass = equipment_mass(&state, hammer);
    validate_pickup_equipment(registries, &state, hammer)
        .unwrap_or_else(|error| panic!("generated exact-local hammer pickup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("generated exact-local hammer pickup commit failed: {error}")
        });
    let _ = walk_step(registries, &mut state, BARREN_SITE);
    assert!(survey_site(registries, &mut state, BARREN_SITE, hammer).is_none());
    let _ = walk_step(registries, &mut state, TARGET_SITE);
    let detailed = survey_site(registries, &mut state, TARGET_SITE, hammer)
        .unwrap_or_else(|| panic!("generated exact-local target should be survey-positive"));
    let detailed_record = state
        .geological_knowledge()
        .get_observation(detailed.observation())
        .unwrap_or_else(|| panic!("generated exact-local detailed survey disappeared"));
    let observed_hardness = detailed_record
        .excavation_hardness()
        .unwrap_or_else(|| panic!("generated exact-local survey did not reveal hardness"));
    let observed_mass = detailed_record
        .resource_mass()
        .unwrap_or_else(|| panic!("generated exact-local survey did not reveal resource scale"));
    let _ = walk_step(registries, &mut state, BARREN_SITE);
    let _ = walk_step(registries, &mut state, CAMP);
    validate_drop_equipment(registries, &state, hammer)
        .unwrap_or_else(|error| panic!("generated exact-local hammer drop failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("generated exact-local hammer drop commit failed: {error}"));

    let planned_local_mass = requested.min(observed_mass.upper());
    assert!(
        !planned_local_mass.is_zero(),
        "generated exact-local acquired evidence must leave a nonzero local workload"
    );
    let estimate = choose_fieldwork_tool_with_market_phase(
        registries,
        &state,
        raw,
        parts,
        observed_hardness.upper(),
        planned_local_mass,
        "generated-exact-local-acquired-evidence",
    )
    .unwrap_or_else(|| {
        panic!("generated exact-local fieldwork has no tool candidate for acquired evidence")
    });
    let (tool, preparation_ticks) =
        assemble_fieldwork_tool(registries, &mut state, raw, parts, estimate.tool);
    assert_eq!(
        preparation_ticks, estimate.preparation_ticks,
        "generated exact-local tool preparation diverged from the frozen actor-visible estimate"
    );
    assert!(
        state
            .available_local_equipment()
            .any(|record| record.id() == tool),
        "generated exact-local extraction tool should be actor-visible where it was assembled"
    );
    let tool_mass = equipment_mass(&state, tool);
    validate_pickup_equipment(registries, &state, tool)
        .unwrap_or_else(|error| {
            panic!("generated exact-local extraction-tool pickup failed: {error}")
        })
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("generated exact-local extraction-tool pickup commit failed: {error}")
        });
    let order = execute_exact_local_order(
        registries,
        &mut state,
        cache,
        tool,
        tool_mass,
        body_capacity,
        requested,
        estimate.batch,
        TARGET_SITE,
    );

    let depletion_reroute = depletion_setup.map(|setup| {
        run_exact_local_depletion_reroute(
            registries,
            &mut state,
            setup,
            raw,
            parts,
            cache,
            hammer,
            tool,
            tool_mass,
            body_capacity,
            requested,
            estimate.batch,
        )
    });
    if depletion_reroute.is_none() {
        validate_drop_equipment(registries, &state, tool)
            .unwrap_or_else(|error| {
                panic!("generated exact-local extraction-tool drop failed: {error}")
            })
            .commit(&mut state)
            .unwrap_or_else(|error| {
                panic!("generated exact-local extraction-tool drop commit failed: {error}")
            });
    }
    let final_carry = assess_player_carrying(&state)
        .unwrap_or_else(|| panic!("generated exact-local final carrying assessment disappeared"));
    assert_eq!(final_carry.stored(), Mass::ZERO);
    assert_eq!(final_carry.available(), body_capacity);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(cache)
            .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_ORE))),
        Some(order.returned)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!(
                "generated exact-local final matter audit failed: {error}"
            ))
            .total(),
        matter_before,
        "generated exact-local field order must conserve matter through fabrication, extraction, and haulage"
    );
    assert_exact_local_field_state(registries, &state, "completed generated order");

    reviewln!(
        "FIELDWORK LOCAL ORDER seed=0x{:016X} sample={} evidence=ordinary-exact-local generation=[demand:independent geology:independent supply:{}mg fixture:{}] route=[steps:{} barren-sites:1 target-distance:2 haul-trips:{}] survey=[inspections:2 detailed:1 observed-hardness:{}..{}Pa observed-mass:{}..{}mg] investment=[hammer:{}mg setup:{}t selected:{} tool:{}mg preparation:{}t planned-local:{}mg] extraction=[requested:{}mg returned:{}mg batches:{} ticks:{} stop:{}] haul=[outbound:{}mg return:{}..{}mg capacity:{}mg utilization:{}..{}ppm] locomotion=[outbound:{}nJ/{}uL loaded-max:{}nJ/{}uL] cache=[returned:{}mg tools-dropped:true] matter=conserved",
        case.seed(),
        case.role().label(),
        fixture.deposit_mass.milligrams(),
        fixture.geology_label,
        4 + order.movement_steps,
        order.trips,
        observed_hardness.lower().pascals(),
        observed_hardness.upper().pascals(),
        observed_mass.lower().milligrams(),
        observed_mass.upper().milligrams(),
        hammer_mass.milligrams(),
        sampling_setup_ticks,
        fieldwork_tool_label(estimate.tool),
        tool_mass.milligrams(),
        preparation_ticks,
        planned_local_mass.milligrams(),
        requested.milligrams(),
        order.returned.milligrams(),
        order.batches,
        order.extraction_ticks,
        order.stop.label(),
        order.outbound_tool_step.load.milligrams(),
        order.minimum_return_load.milligrams(),
        order.maximum_return_load.milligrams(),
        body_capacity.milligrams(),
        order.minimum_utilization_ppm,
        order.maximum_utilization_ppm,
        order.outbound_tool_step.energy.nanojoules(),
        order.outbound_tool_step.hydration.microliters(),
        order.maximum_return_step.energy.nanojoules(),
        order.maximum_return_step.hydration.microliters(),
        order.returned.milligrams(),
    );
    GeneratedExactLocalFieldOrderReview {
        tool: estimate.tool.target,
        observed_hardness_lower_pa: observed_hardness.lower().pascals(),
        observed_hardness_upper_pa: observed_hardness.upper().pascals(),
        observed_mass_lower_mg: observed_mass.lower().milligrams(),
        observed_mass_upper_mg: observed_mass.upper().milligrams(),
        returned: order.returned,
        stop: order.stop,
        trips: order.trips,
        depletion_reroute,
    }
}

//! Maintained exact-local field expedition through ordinary movement, tool custody, survey, and haulage.

use deep_hearth::content::gameplay_fixture::{
    GeologicalDepositSeed, locate_ground_stockpile, seed_assembled_equipment_at,
    seed_geological_deposit, seed_ground_stockpile, seed_lot, seed_stockpile,
    seed_traversable_edge,
};
use deep_hearth::content::{
    EQUIPMENT_STONE_GEOLOGICAL_HAMMER, EQUIPMENT_STONE_QUARRY_PICK, FORM_ORE, MATERIAL_COPPER,
    PROSPECTING_DETAILED_FIELD_SURVEY, PROSPECTING_FIELD_INSPECTION,
};
use deep_hearth::core::quantity::{Energy, Mass, Volume};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::equipment::EquipmentId;
use deep_hearth::geology::{FieldProspectingOutcome, GeologicalEvidenceKind};
use deep_hearth::inventory::{MaterialLotSelection, StockpileStorageProfile};
use deep_hearth::logistics::{
    assess_player_carrying, validate_drop_equipment, validate_drop_to_ground,
    validate_pickup_equipment, validate_start_player_step,
};
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::mining::{MiningTargetRequest, resolve_mining_target};
use deep_hearth::player::validate_admit_player;
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;
use deep_hearth::spatial::VoxelCoord;
use deep_hearth::survival::assess_survival;

use super::super::environment::ROOM_TEMPERATURE;
use super::super::ore_fixture::copper_ore_composition;
use super::super::tick_observation::{TickEventAllowance, assert_tick_events_within};
use super::extraction::{FieldworkExtractionOrder, FieldworkStop, execute_fieldwork_extraction};
use super::planning::{equipment_component_requirements, fieldwork_mining_limits};
use super::survey::{horizontal_region, run_survey};

const CAMP: VoxelCoord = VoxelCoord::new(0, -1, 0);
const BARREN_SITE: VoxelCoord = VoxelCoord::new(1, -1, 0);
const TARGET_SITE: VoxelCoord = VoxelCoord::new(2, -1, 0);
const EXPEDITION_DEPOSIT_MASS: Mass = Mass::from_milligrams(40_000_000);
const EXPEDITION_ORDER_MASS: Mass = Mass::from_milligrams(30_000_000);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct StepCost {
    load: Mass,
    energy: Energy,
    hydration: Volume,
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
        PROSPECTING_FIELD_INSPECTION,
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
        PROSPECTING_DETAILED_FIELD_SURVEY,
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
    let target_region = horizontal_region(TARGET_SITE.x(), 1);
    let mut remaining_order = requested;
    let mut trip_count = 0_u64;
    let mut total_extraction_batches = 0_u64;
    let mut total_extraction_ticks = 0_u64;
    let mut total_returned = Mass::ZERO;
    let mut outbound_pick_step = None;
    let mut minimum_return_load = None::<Mass>;
    let mut maximum_return_load = Mass::ZERO;
    let mut maximum_return_step = None::<StepCost>;
    let mut minimum_utilization_ppm = u128::MAX;
    let mut maximum_utilization_ppm = 0_u128;
    while !remaining_order.is_zero() {
        let outbound = walk_step(registries, &mut state, BARREN_SITE);
        outbound_pick_step.get_or_insert(outbound);
        let _ = walk_step(registries, &mut state, TARGET_SITE);
        let target = resolve_mining_target(
            &state,
            MiningTargetRequest::new(target_region, MATERIAL_COPPER),
        )
        .unwrap_or_else(|error| panic!("exact-local acquired target no longer resolves: {error}"));
        let carried = assess_player_carrying(&state)
            .unwrap_or_else(|| panic!("exact-local mining lost carried custody"));
        let trip_request = remaining_order.min(carried.available());
        assert!(
            !trip_request.is_zero(),
            "exact-local field order must leave carrying room for some ore beside the owned pick"
        );
        let extraction = execute_fieldwork_extraction(
            registries,
            &mut state,
            FieldworkExtractionOrder {
                target,
                destination: carried.stockpile(),
                equipment: pick,
                requested: trip_request,
                batch_limit: limits.base_quarry_batch,
            },
        );
        assert_eq!(extraction.stop, FieldworkStop::OrderComplete);
        assert_eq!(extraction.extracted, trip_request);
        total_extraction_batches = total_extraction_batches
            .checked_add(extraction.batches)
            .unwrap_or_else(|| panic!("exact-local extraction batch accounting overflowed"));
        total_extraction_ticks = total_extraction_ticks
            .checked_add(extraction.ticks)
            .unwrap_or_else(|| panic!("exact-local extraction time accounting overflowed"));
        remaining_order = remaining_order
            .checked_sub(extraction.extracted)
            .unwrap_or_else(|| unreachable!("trip extraction is bounded by remaining order"));

        let loaded = assess_player_carrying(&state)
            .unwrap_or_else(|| panic!("exact-local loaded carrying assessment disappeared"));
        assert_eq!(
            loaded.stored(),
            pick_mass
                .checked_add(extraction.extracted)
                .unwrap_or_else(|| panic!("exact-local loaded custody overflowed"))
        );
        assert_eq!(loaded.capacity(), body_capacity);
        let return_loaded_step = walk_step(registries, &mut state, BARREN_SITE);
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
        let _ = walk_step(registries, &mut state, CAMP);

        let carried = assess_player_carrying(&state)
            .unwrap_or_else(|| panic!("exact-local return lost carried custody"));
        let ore_selection = carried_ore_selection(&state, carried.stockpile());
        assert!(!ore_selection.is_empty());
        validate_drop_to_ground(registries, &state, cache, &ore_selection)
            .unwrap_or_else(|error| panic!("exact-local ore cache drop failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("exact-local ore cache drop commit failed: {error}"));
        total_returned = total_returned
            .checked_add(extraction.extracted)
            .unwrap_or_else(|| panic!("exact-local returned ore accounting overflowed"));
        trip_count = trip_count
            .checked_add(1)
            .unwrap_or_else(|| panic!("exact-local trip count overflowed"));
    }
    assert_eq!(total_returned, requested);
    assert!(
        trip_count > 1,
        "30 kg field order should require multiple body-scale haul trips"
    );
    let outbound_pick_step = outbound_pick_step
        .unwrap_or_else(|| unreachable!("nonzero order executes one outbound trip"));
    let minimum_return_load = minimum_return_load
        .unwrap_or_else(|| unreachable!("nonzero order executes one return trip"));
    let maximum_return_step = maximum_return_step
        .unwrap_or_else(|| unreachable!("nonzero order executes one return trip"));
    assert_eq!(maximum_return_load, body_capacity);
    assert!(minimum_return_load < maximum_return_load);
    assert!(maximum_return_step.energy >= outbound_pick_step.energy);
    assert!(maximum_return_step.hydration >= outbound_pick_step.hydration);

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
        4 + trip_count * 4,
        trip_count,
        observed_mass.lower().milligrams(),
        observed_mass.upper().milligrams(),
        hammer_mass.milligrams(),
        pick_mass.milligrams(),
        requested.milligrams(),
        total_extraction_batches,
        total_extraction_ticks,
        outbound_pick_step.load.milligrams(),
        minimum_return_load.milligrams(),
        maximum_return_load.milligrams(),
        body_capacity.milligrams(),
        minimum_utilization_ppm,
        maximum_utilization_ppm,
        outbound_pick_step.energy.nanojoules(),
        outbound_pick_step.hydration.microliters(),
        maximum_return_step.energy.nanojoules(),
        maximum_return_step.hydration.microliters(),
        requested.milligrams(),
    );
}

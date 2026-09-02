//! Contract tests for equipment maintenance execution.

use super::super::maintenance_resolution::EquipmentMaintenanceMaterialResolution;
use super::*;
use crate::capability::{
    CapabilityComparison, CapabilityDefinition, CapabilityId, CapabilityProfile,
    CapabilityRequirement, CapabilityValue, CapabilityValueKind,
};
use crate::content::{
    EQUIPMENT_COPPER_REINFORCED_HAND_CRANK, EQUIPMENT_COPPER_REINFORCED_PICK,
    EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER, EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
    EQUIPMENT_STONE_CRUSHER, EQUIPMENT_STONE_HAND_CRANK, EQUIPMENT_STONE_PICK,
    EQUIPMENT_STONE_SEPARATOR, FORM_CHIP, FORM_HANDLE, FORM_LOG, FORM_REINFORCEMENT, FORM_SCRAP,
    FORM_TOOL, MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD, PROCESS_REKNAP_STONE_SCRAP_TOOL,
    STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries, make_test_registries_with_equipment,
    make_test_registries_with_sensible_heating,
};
use crate::core::quantity::{AggregateMass, Area, Energy, Force, Length, Power, Temperature};
use crate::core::state::validate_loaded_state;
use crate::core::time::{SimulationTick, TickSpan, WorldSeed};
use crate::crafting::{ManualCraftStartRequest, validate_start_manual_craft};
use crate::energy::{
    EnergyCarrier, EnergyStoreDefinition, EnergyStoreDefinitionId, PreciseEnergy,
    add_energy_store_with_initial_for_fixture, calculate_explicit_energy_accounting,
};
use crate::equipment::{
    EquipmentDefinition, EquipmentDefinitionId, EquipmentMaintenanceProfile, add_equipment,
    apply_equipment_condition_plan, decide_equipment_wear, validate_assemble_equipment,
    validate_upgrade_equipment,
};

use crate::inventory::{
    MaterialLotSelection, StockpileId, add_solid_stockpile_for_test, deposit_composed_lot_for_test,
    deposit_lot_for_test, validate_explicit_consumption_selection, validate_mount_stockpile,
};
use crate::maintenance::MaintenanceThresholds;
use crate::material::{CommodityKey, CompositionComponent, MaterialComposition};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::production::{ProcessDefinition, ProcessId, validate_start_process};
use crate::simulation::advance_tick;
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::structural::{
    StructuralLoadKind, add_structural_element, calculate_aggregate_weight_force_ceiling,
    materialize_structural_element_for_test, validate_activate_structural_element,
};
use crate::survival::{SurvivalExertion, initialize_player_survival};
use crate::thermal::{
    SensibleHeatingProcessDefinition, SensibleHeatingRequest, resolve_sensible_heating_process,
};

const TEST_CAPABILITY: CapabilityId = CapabilityId::new(812_001);
const TEST_DEFINITION: EquipmentDefinitionId = EquipmentDefinitionId::new(812_001);
const HEATING_POWER: CapabilityId = CapabilityId::new(812_002);
const MAX_TEMPERATURE: CapabilityId = CapabilityId::new(812_003);
const MAX_BATCH_MASS: CapabilityId = CapabilityId::new(812_004);
const ENERGY_DEFINITION: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(812_001);
const HEATING_PROCESS: ProcessId = ProcessId::new(812_001);

fn condition(parts_per_million: u32) -> Condition {
    match Condition::new(parts_per_million) {
        Ok(condition) => condition,
        Err(error) => panic!("maintenance condition fixture failed: {error}"),
    }
}

fn initialize_service_player(registries: &Registries, state: &mut AppState) {
    initialize_player_survival(registries, state)
        .unwrap_or_else(|error| panic!("maintenance player-survival fixture failed: {error}"));
}

fn finish_service(
    registries: &Registries,
    state: &mut AppState,
    completes_at: SimulationTick,
) -> EquipmentMaintenanceOutcome {
    let mut completion = None;
    while state.tick() < completes_at {
        let tick = advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("maintenance completion tick failed: {error}"));
        if let Some(outcome) = tick.equipment_maintenance() {
            assert!(completion.replace(outcome).is_none());
        }
    }
    assert_eq!(state.player_work().active(), None);
    completion.unwrap_or_else(|| panic!("maintenance reached completion tick without an outcome"))
}

#[test]
fn every_builtin_primitive_component_service_executes_from_its_real_assembly_traces() {
    let registries = build_registries();
    for (case, definition_id) in [
        EQUIPMENT_STONE_PICK,
        EQUIPMENT_STONE_HAND_CRANK,
        EQUIPMENT_COPPER_REINFORCED_PICK,
        EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
        EQUIPMENT_STONE_CRUSHER,
        EQUIPMENT_STONE_SEPARATOR,
        EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
        EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
    ]
    .into_iter()
    .enumerate()
    {
        let definition = registries
            .equipment()
            .get_equipment(definition_id)
            .unwrap_or_else(|| {
                panic!(
                    "primitive service definition {} disappeared",
                    definition_id.value()
                )
            });
        let assembly_profile = definition.assembly_profile().unwrap_or_else(|| {
            panic!(
                "primitive service definition {} lost assembly profile",
                definition_id.value()
            )
        });
        let maintenance = definition.maintenance_profile().unwrap_or_else(|| {
            panic!(
                "primitive service definition {} lost maintenance profile",
                definition_id.value()
            )
        });
        assert!(maintenance.is_component_replacement());

        let mut state = AppState::new(WorldSeed::new(
            0x8120_C100_u64
                .checked_add(
                    u64::try_from(case).unwrap_or_else(|_| unreachable!("bounded case fits u64")),
                )
                .unwrap_or_else(|| unreachable!("bounded primitive service seed cannot overflow")),
        ));
        initialize_service_player(&registries, &mut state);
        let assembly = add_solid_stockpile_for_test(&mut state, definition.mass())
            .unwrap_or_else(|error| panic!("primitive service assembly stockpile failed: {error}"));
        for input in assembly_profile.inputs() {
            deposit_lot_for_test(
                &registries,
                &mut state,
                assembly,
                input.commodity(),
                input.mass(),
                Temperature::from_millikelvin(300_000),
            )
            .unwrap_or_else(|error| panic!("primitive service assembly input failed: {error}"));
        }
        let equipment = validate_assemble_equipment(&registries, &state, definition_id, assembly)
            .unwrap_or_else(|error| panic!("primitive service assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("primitive service assembly commit failed: {error}"));
        let wear = decide_equipment_wear(&state, equipment, 100_000)
            .unwrap_or_else(|error| panic!("primitive service wear plan failed: {error}"));
        apply_equipment_condition_plan(&mut state, wear)
            .unwrap_or_else(|error| panic!("primitive service wear commit failed: {error}"));

        let replacement_mass = maintenance.full_service_replacement_mass();
        let replacement = add_solid_stockpile_for_test(&mut state, replacement_mass)
            .unwrap_or_else(|error| {
                panic!("primitive service replacement stockpile failed: {error}")
            });
        deposit_lot_for_test(
            &registries,
            &mut state,
            replacement,
            maintenance.replacement(),
            replacement_mass,
            Temperature::from_millikelvin(310_000),
        )
        .unwrap_or_else(|error| panic!("primitive service replacement input failed: {error}"));
        let spent = add_solid_stockpile_for_test(&mut state, replacement_mass)
            .unwrap_or_else(|error| panic!("primitive service spent stockpile failed: {error}"));
        let matter_before = calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("primitive service matter audit failed: {error}"))
            .total();
        let energy_before = explicit_energy(&registries, &state);

        let resolution = resolve_equipment_maintenance(
            &registries,
            &state,
            EquipmentMaintenanceRequest::new(equipment, replacement, spent),
        )
        .unwrap_or_else(|error| panic!("primitive service resolution failed: {error}"));
        assert!(resolution.replaces_embodied_component());
        assert_eq!(resolution.material_mass(), replacement_mass);
        let outcome = validate_equipment_maintenance(&registries, &state, resolution)
            .unwrap_or_else(|error| panic!("primitive service validation failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("primitive service commit failed: {error}"));
        assert_eq!(outcome.equipment(), equipment);
        assert_eq!(outcome.target_condition(), maintenance.restored_condition());

        let record = state
            .equipment()
            .get_equipment(equipment)
            .unwrap_or_else(|| panic!("primitive serviced equipment disappeared"));
        assert_eq!(record.definition(), definition_id);
        assert_eq!(record.embodied_mass(), definition.mass());
        assert_eq!(record.condition(), outcome.condition_before());
        if definition.upgrade_profile().is_some() {
            assert!(record.embodied_material().iter().any(|trace| {
                trace.profile().commodity()
                    == CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT)
                    && trace.mass() == Mass::from_milligrams(20_000)
            }));
        }
        assert_eq!(
            state
                .inventory()
                .get_stockpile(spent)
                .map(|stockpile| stockpile.get_mass(maintenance.spent())),
            Some(replacement_mass),
            "primitive service must emit exactly the replaced component mass as spent material"
        );
        assert_eq!(
            calculate_matter_accounting(&state)
                .unwrap_or_else(|error| panic!(
                    "primitive service final matter audit failed: {error}"
                ))
                .total(),
            matter_before
        );
        assert_eq!(explicit_energy(&registries, &state), energy_before);
        let completion = finish_service(&registries, &mut state, outcome.completes_at());
        assert_eq!(completion.equipment(), equipment);
        assert_eq!(completion.condition_before(), outcome.condition_before());
        assert_eq!(
            completion.condition_after(),
            maintenance.restored_condition()
        );
        assert_eq!(
            state
                .equipment()
                .get_equipment(equipment)
                .map(|record| record.condition()),
            Some(maintenance.restored_condition())
        );
        validate_loaded_state(&registries, &state)
            .unwrap_or_else(|error| panic!("primitive service state audit failed: {error}"));
    }
}

#[test]
fn accumulated_maintenance_stone_scrap_can_reknap_the_next_pick_component() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x8120_C102));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("stone scrap maintenance survival setup failed: {error}"));

    let assembly = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| {
            panic!("stone scrap maintenance assembly stockpile failed: {error}")
        });
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(800_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(200_000),
        ),
    ] {
        deposit_lot_for_test(
            &registries,
            &mut state,
            assembly,
            commodity,
            mass,
            Temperature::from_millikelvin(300_000),
        )
        .unwrap_or_else(|error| {
            panic!("stone scrap maintenance assembly material failed: {error}")
        });
    }
    let pick = validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_PICK, assembly)
        .unwrap_or_else(|error| panic!("stone scrap maintenance pick assembly failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("stone scrap maintenance pick assembly commit failed: {error}")
        });

    let fresh_replacements =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_600_000))
            .unwrap_or_else(|error| panic!("fresh stone replacement stockpile failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        fresh_replacements,
        CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
        Mass::from_milligrams(1_600_000),
        Temperature::from_millikelvin(300_000),
    )
    .unwrap_or_else(|error| panic!("fresh stone replacement fixture failed: {error}"));
    let spent = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(3_000_000))
        .unwrap_or_else(|error| panic!("stone scrap maintenance spent stockpile failed: {error}"));
    let recovered = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("stone scrap reknap output stockpile failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("stone scrap maintenance matter-before failed: {error}"))
        .total();

    for service in 0..2 {
        let wear = decide_equipment_wear(&state, pick, 100_000).unwrap_or_else(|error| {
            panic!("stone scrap maintenance wear {service} failed: {error}")
        });
        apply_equipment_condition_plan(&mut state, wear).unwrap_or_else(|error| {
            panic!("stone scrap maintenance wear commit {service} failed: {error}")
        });
        let resolution = resolve_equipment_maintenance(
            &registries,
            &state,
            EquipmentMaintenanceRequest::new(pick, fresh_replacements, spent),
        )
        .unwrap_or_else(|error| panic!("fresh stone service {service} failed: {error}"));
        assert_eq!(resolution.material_mass(), Mass::from_milligrams(800_000));
        let outcome = validate_equipment_maintenance(&registries, &state, resolution)
            .unwrap_or_else(|error| {
                panic!("fresh stone service validation {service} failed: {error}")
            })
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("fresh stone service commit {service} failed: {error}"));
        assert_eq!(outcome.equipment(), pick);
        assert_eq!(outcome.material_mass(), Mass::from_milligrams(800_000));
        assert_eq!(outcome.target_condition(), condition(1_000_000));
        let completion = finish_service(&registries, &mut state, outcome.completes_at());
        assert_eq!(completion.condition_after(), condition(1_000_000));
    }
    assert_eq!(
        state
            .inventory()
            .get_stockpile(spent)
            .map(|stockpile| { stockpile.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_SCRAP)) }),
        Some(Mass::from_milligrams(1_600_000))
    );
    let scrap = state
        .inventory()
        .lot_ids(spent)
        .find(|lot| {
            state.inventory().get_lot(*lot).is_some_and(|record| {
                record.commodity() == CommodityKey::new(MATERIAL_STONE, FORM_SCRAP)
                    && record.mass() >= Mass::from_milligrams(1_000_000)
            })
        })
        .unwrap_or_else(|| panic!("maintenance stone scrap lot disappeared"));

    let reknap_job = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_REKNAP_STONE_SCRAP_TOOL,
            spent,
            MaterialLotSelection::new(scrap, Mass::from_milligrams(1_000_000)),
            recovered,
        ),
    )
    .unwrap_or_else(|error| panic!("maintenance scrap reknap start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("maintenance scrap reknap commit failed: {error}"));
    while state.production().get_job(reknap_job).is_some() {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("maintenance scrap reknap tick failed: {error}"));
    }
    assert_eq!(
        state
            .inventory()
            .get_stockpile(spent)
            .map(|stockpile| { stockpile.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_SCRAP)) }),
        Some(Mass::from_milligrams(600_000))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(recovered)
            .map(|stockpile| { stockpile.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_TOOL)) }),
        Some(Mass::from_milligrams(800_000))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(recovered)
            .map(|stockpile| { stockpile.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_CHIP)) }),
        Some(Mass::from_milligrams(200_000))
    );

    let wear = decide_equipment_wear(&state, pick, 100_000)
        .unwrap_or_else(|error| panic!("reknapped component wear failed: {error}"));
    apply_equipment_condition_plan(&mut state, wear)
        .unwrap_or_else(|error| panic!("reknapped component wear commit failed: {error}"));
    let recycled_resolution = resolve_equipment_maintenance(
        &registries,
        &state,
        EquipmentMaintenanceRequest::new(pick, recovered, spent),
    )
    .unwrap_or_else(|error| panic!("reknapped component service failed: {error}"));
    assert_eq!(
        recycled_resolution.material_mass(),
        Mass::from_milligrams(800_000)
    );
    let recycled_outcome = validate_equipment_maintenance(&registries, &state, recycled_resolution)
        .unwrap_or_else(|error| panic!("reknapped component service validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("reknapped component service commit failed: {error}"));
    assert_eq!(recycled_outcome.equipment(), pick);
    assert_eq!(
        recycled_outcome.material_mass(),
        Mass::from_milligrams(800_000)
    );
    assert_eq!(recycled_outcome.target_condition(), condition(1_000_000));
    let recycled_completion =
        finish_service(&registries, &mut state, recycled_outcome.completes_at());
    assert_eq!(recycled_completion.condition_after(), condition(1_000_000));

    assert_eq!(
        state
            .equipment()
            .get_equipment(pick)
            .map(|record| record.condition()),
        Some(condition(1_000_000))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(recovered)
            .map(|stockpile| { stockpile.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_TOOL)) }),
        Some(Mass::ZERO)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(recovered)
            .map(|stockpile| { stockpile.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_CHIP)) }),
        Some(Mass::from_milligrams(200_000))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(spent)
            .map(|stockpile| { stockpile.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_SCRAP)) }),
        Some(Mass::from_milligrams(1_400_000))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(fresh_replacements)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::ZERO),
        "the third service must use recovered stone rather than hidden fresh replacement stock"
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("stone scrap maintenance matter-after failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn component_maintenance_preserves_upgrade_and_exchanges_exact_embodied_trace() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x8120_C001));
    initialize_service_player(&registries, &mut state);
    let assembly = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("component service assembly stockpile failed: {error}"));
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(800_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(200_000),
        ),
    ] {
        deposit_lot_for_test(
            &registries,
            &mut state,
            assembly,
            commodity,
            mass,
            Temperature::from_millikelvin(300_000),
        )
        .unwrap_or_else(|error| panic!("component service assembly material failed: {error}"));
    }
    let pick = validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_PICK, assembly)
        .unwrap_or_else(|error| panic!("component service pick assembly failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("component service pick assembly commit failed: {error}"));

    let reinforcement = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20_000))
        .unwrap_or_else(|error| {
            panic!("component service reinforcement stockpile failed: {error}")
        });
    deposit_lot_for_test(
        &registries,
        &mut state,
        reinforcement,
        CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
        Mass::from_milligrams(20_000),
        Temperature::from_millikelvin(300_000),
    )
    .unwrap_or_else(|error| panic!("component service reinforcement material failed: {error}"));
    assert_eq!(
        validate_upgrade_equipment(
            &registries,
            &state,
            pick,
            EQUIPMENT_COPPER_REINFORCED_PICK,
            reinforcement,
        )
        .unwrap_or_else(|error| panic!("component service upgrade failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("component service upgrade commit failed: {error}")),
        pick
    );
    let wear = decide_equipment_wear(&state, pick, 400_000)
        .unwrap_or_else(|error| panic!("component service wear planning failed: {error}"));
    apply_equipment_condition_plan(&mut state, wear)
        .unwrap_or_else(|error| panic!("component service wear commit failed: {error}"));
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("component service provenance tick failed: {error}"));

    let old_stone_trace = state
        .equipment()
        .get_equipment(pick)
        .unwrap_or_else(|| panic!("component service upgraded pick disappeared"))
        .embodied_material()
        .iter()
        .find(|trace| trace.profile().commodity() == CommodityKey::new(MATERIAL_STONE, FORM_TOOL))
        .cloned()
        .unwrap_or_else(|| panic!("component service old stone component disappeared"));
    let replacement = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(800_000))
        .unwrap_or_else(|error| panic!("component service replacement stockpile failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        replacement,
        CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
        Mass::from_milligrams(800_000),
        Temperature::from_millikelvin(310_000),
    )
    .unwrap_or_else(|error| panic!("component service fresh component failed: {error}"));
    let spent = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(800_000))
        .unwrap_or_else(|error| panic!("component service spent stockpile failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("component service matter before failed: {error}"))
        .total();
    let energy_before = explicit_energy(&registries, &state);

    let resolution = resolve_equipment_maintenance(
        &registries,
        &state,
        EquipmentMaintenanceRequest::new(pick, replacement, spent),
    )
    .unwrap_or_else(|error| panic!("component service resolution failed: {error}"));
    assert!(resolution.replaces_embodied_component());
    assert_eq!(resolution.material_mass(), Mass::from_milligrams(800_000));
    let outcome = validate_equipment_maintenance(&registries, &state, resolution)
        .unwrap_or_else(|error| panic!("component service validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("component service commit failed: {error}"));
    assert_eq!(outcome.equipment(), pick);
    assert_eq!(outcome.target_condition(), Condition::PRISTINE);

    let record = state
        .equipment()
        .get_equipment(pick)
        .unwrap_or_else(|| panic!("component service pick disappeared after repair"));
    assert_eq!(record.definition(), EQUIPMENT_COPPER_REINFORCED_PICK);
    assert_eq!(record.condition(), outcome.condition_before());
    assert_eq!(record.embodied_mass(), Mass::from_milligrams(1_020_000));
    assert!(record.embodied_material().iter().any(|trace| {
        trace.profile().commodity() == CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT)
            && trace.mass() == Mass::from_milligrams(20_000)
    }));
    let fresh_stone = record
        .embodied_material()
        .iter()
        .find(|trace| trace.profile().commodity() == CommodityKey::new(MATERIAL_STONE, FORM_TOOL))
        .unwrap_or_else(|| panic!("component service fresh stone trace disappeared"));
    assert_eq!(fresh_stone.mass(), Mass::from_milligrams(800_000));
    assert_eq!(
        fresh_stone.provenance().latest_created_at(),
        SimulationTick::new(1),
        "replacement component must remain distinguishably newer than the original equipment assembly"
    );
    assert_eq!(
        fresh_stone.profile().temperature(),
        Temperature::from_millikelvin(310_000)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(spent)
            .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_SCRAP))),
        Some(Mass::from_milligrams(800_000))
    );
    let spent_trace = state
        .inventory()
        .lots()
        .find(|lot| lot.stockpile() == spent)
        .unwrap_or_else(|| panic!("component service spent lot disappeared"));
    assert_eq!(
        spent_trace.temperature(),
        old_stone_trace.profile().temperature()
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("component service matter after failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(explicit_energy(&registries, &state), energy_before);
    let completion = finish_service(&registries, &mut state, outcome.completes_at());
    assert_eq!(completion.condition_after(), Condition::PRISTINE);
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("component service final state audit failed: {error}"));
    let encoded =
        serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("component service persistence serialization failed: {error}")
        });
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("component service persistence decode failed: {error}"));
    let loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("component service trusted reload failed: {error}"));
    assert_eq!(
        loaded, state,
        "component service must persist a newer replacement trace inside the older equipment identity"
    );
}

fn registries() -> Registries {
    let profile = match CapabilityProfile::new([(
        TEST_CAPABILITY,
        CapabilityValue::Mass(Mass::from_milligrams(50_000)),
    )]) {
        Ok(profile) => profile,
        Err(error) => panic!("maintenance capability fixture failed: {error}"),
    };
    let thresholds = match MaintenanceThresholds::new(condition(600_000), condition(250_000)) {
        Ok(thresholds) => thresholds,
        Err(error) => panic!("equipment maintenance fixture failed: {error}"),
    };
    make_test_registries_with_equipment(
        CapabilityDefinition::new(
            TEST_CAPABILITY,
            "maintenance fixture supported mass",
            CapabilityValueKind::Mass,
        ),
        EquipmentDefinition::new(
            TEST_DEFINITION,
            "maintenance fixture press",
            Mass::from_milligrams(40_000),
            profile,
            thresholds,
        )
        .with_maintenance_profile(EquipmentMaintenanceProfile::new(
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(7),
            CommodityKey::new(MATERIAL_WOOD, FORM_CHIP),
            condition(700_000),
            TickSpan::new(1),
            SurvivalExertion::REST,
        )),
    )
}

fn occupied_registries() -> Registries {
    let profile = match CapabilityProfile::new([
        (
            HEATING_POWER,
            CapabilityValue::Power(Power::from_microwatts(1_000_000)),
        ),
        (
            MAX_TEMPERATURE,
            CapabilityValue::Temperature(Temperature::from_millikelvin(400_000)),
        ),
        (
            MAX_BATCH_MASS,
            CapabilityValue::Mass(Mass::from_milligrams(20)),
        ),
    ]) {
        Ok(profile) => profile,
        Err(error) => panic!("maintenance occupancy capability fixture failed: {error}"),
    };
    let thresholds = match MaintenanceThresholds::new(condition(600_000), condition(250_000)) {
        Ok(thresholds) => thresholds,
        Err(error) => panic!("maintenance occupancy maintenance fixture failed: {error}"),
    };
    make_test_registries_with_sensible_heating(
        vec![
            CapabilityDefinition::new(
                HEATING_POWER,
                "maintenance occupancy heating power",
                CapabilityValueKind::Power,
            ),
            CapabilityDefinition::new(
                MAX_TEMPERATURE,
                "maintenance occupancy maximum temperature",
                CapabilityValueKind::Temperature,
            ),
            CapabilityDefinition::new(
                MAX_BATCH_MASS,
                "maintenance occupancy maximum batch mass",
                CapabilityValueKind::Mass,
            ),
        ],
        EquipmentDefinition::new(
            TEST_DEFINITION,
            "maintenance occupancy heater",
            Mass::from_milligrams(40_000),
            profile,
            thresholds,
        )
        .with_maintenance_profile(EquipmentMaintenanceProfile::new(
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(7),
            CommodityKey::new(MATERIAL_WOOD, FORM_CHIP),
            condition(700_000),
            TickSpan::new(1),
            SurvivalExertion::REST,
        )),
        EnergyStoreDefinition::new_with_transfer_limits(
            ENERGY_DEFINITION,
            "maintenance occupancy battery",
            EnergyCarrier::Electrical,
            Energy::from_nanojoules(1_000_000_000),
            Power::ZERO,
            Power::from_microwatts(500_000),
        ),
        ProcessDefinition::new_selected_batch(
            HEATING_PROCESS,
            "maintenance occupancy sensible heating",
            vec![
                CapabilityRequirement::new(
                    HEATING_POWER,
                    CapabilityComparison::AtLeast,
                    CapabilityValue::Power(Power::from_picowatts(1)),
                ),
                CapabilityRequirement::new(
                    MAX_TEMPERATURE,
                    CapabilityComparison::AtLeast,
                    CapabilityValue::Temperature(Temperature::from_millikelvin(1)),
                ),
                CapabilityRequirement::new(
                    MAX_BATCH_MASS,
                    CapabilityComparison::AtLeast,
                    CapabilityValue::Mass(Mass::from_milligrams(1)),
                ),
            ],
        ),
        SensibleHeatingProcessDefinition::new(
            HEATING_PROCESS,
            HEATING_POWER,
            MAX_TEMPERATURE,
            MAX_BATCH_MASS,
            EnergyCarrier::Electrical,
            1,
        ),
    )
}

fn explicit_energy(registries: &Registries, state: &AppState) -> PreciseEnergy {
    calculate_explicit_energy_accounting(registries, state)
        .unwrap_or_else(|error| panic!("maintenance energy accounting failed: {error}"))
        .total()
        .unwrap_or_else(|| panic!("maintenance exact energy total overflowed"))
}

fn add_material(
    registries: &Registries,
    state: &mut AppState,
    stockpile: StockpileId,
    mass: Mass,
) -> crate::inventory::MaterialLotId {
    match deposit_lot_for_test(
        registries,
        state,
        stockpile,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        mass,
        Temperature::from_millikelvin(300_000),
    ) {
        Ok(lot) => lot,
        Err(error) => panic!("maintenance material fixture failed: {error}"),
    }
}

fn bind(
    state: &AppState,
    equipment: EquipmentId,
    source: StockpileId,
    lot: crate::inventory::MaterialLotId,
    mass: Mass,
    spent: StockpileId,
    after: Condition,
) -> EquipmentMaintenanceResolution {
    bind_selections(
        state,
        equipment,
        source,
        &[MaterialLotSelection::new(lot, mass)],
        spent,
        after,
    )
}

fn bind_selections(
    state: &AppState,
    equipment: EquipmentId,
    source: StockpileId,
    selections: &[MaterialLotSelection],
    spent_destination: StockpileId,
    condition_after: Condition,
) -> EquipmentMaintenanceResolution {
    let record = state
        .equipment()
        .get_equipment(equipment)
        .unwrap_or_else(|| panic!("maintenance binding fixture references unknown equipment"));
    let material = validate_explicit_consumption_selection(state.inventory(), source, selections)
        .unwrap_or_else(|error| panic!("maintenance binding fixture selection failed: {error:?}"));
    EquipmentMaintenanceResolution {
        equipment,
        expected_equipment_revision: state.equipment().revision(),
        condition_before: record.condition(),
        condition_after,
        material,
        spent: CommodityKey::new(MATERIAL_WOOD, FORM_CHIP),
        spent_destination,
        material_mode: EquipmentMaintenanceMaterialResolution::AggregateWearStock,
        duration: TickSpan::new(1),
        exertion: SurvivalExertion::REST,
    }
}

#[test]
fn authored_maintenance_resolution_binds_exact_replacement_stock_and_service_target() {
    let registries = registries();
    let mut state = AppState::new(WorldSeed::new(0x8120_0000));
    initialize_service_player(&registries, &mut state);
    let equipment = add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000))
        .unwrap_or_else(|error| panic!("maintenance resolver equipment fixture failed: {error}"));
    let second_equipment =
        add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000)).unwrap_or_else(
            |error| panic!("second maintenance resolver equipment fixture failed: {error}"),
        );
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("maintenance resolver source fixture failed: {error}"));
    let spent = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("maintenance resolver spent fixture failed: {error}"));
    add_material(&registries, &mut state, source, Mass::from_milligrams(20));

    let resolution = resolve_equipment_maintenance(
        &registries,
        &state,
        EquipmentMaintenanceRequest::new(equipment, source, spent),
    )
    .unwrap_or_else(|error| panic!("maintenance resolution failed: {error}"));
    assert_eq!(resolution.equipment(), equipment);
    assert_eq!(resolution.material_source(), source);
    assert_eq!(resolution.spent_destination(), spent);
    assert_eq!(
        resolution.spent_commodity(),
        CommodityKey::new(MATERIAL_WOOD, FORM_CHIP)
    );
    assert_eq!(
        resolution.material_mass(),
        Mass::from_milligrams(2),
        "partial maintenance must scale replacement stock with restored condition"
    );
    assert_eq!(resolution.condition_before(), condition(500_000));
    assert_eq!(resolution.condition_after(), condition(700_000));

    let outcome = validate_equipment_maintenance(&registries, &state, resolution)
        .unwrap_or_else(|error| panic!("maintenance transaction validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("maintenance transaction commit failed: {error}"));
    assert_eq!(outcome.condition_before(), condition(500_000));
    assert_eq!(outcome.target_condition(), condition(700_000));
    assert_eq!(outcome.material_mass(), Mass::from_milligrams(2));
    assert_eq!(
        state
            .equipment()
            .get_equipment(equipment)
            .map(|record| record.condition()),
        Some(condition(500_000)),
        "service admission must not grant condition recovery before labor completes"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(source)
            .map(|record| record.stored_mass()),
        Some(Mass::from_milligrams(18))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(spent)
            .map(|record| record.stored_mass()),
        Some(Mass::from_milligrams(2))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(spent)
            .map(|record| record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_LOG))),
        Some(Mass::ZERO),
        "spent maintenance output must not remain reusable replacement stock"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(spent)
            .map(|record| record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_CHIP))),
        Some(Mass::from_milligrams(2)),
        "maintenance must conserve the selected matter in the authored spent form"
    );
    assert_eq!(
        resolve_equipment_maintenance(
            &registries,
            &state,
            EquipmentMaintenanceRequest::new(second_equipment, spent, source),
        ),
        Err(
            EquipmentMaintenanceResolutionError::InsufficientReplacementMaterial {
                stockpile: spent,
                commodity: CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
                available: Mass::ZERO,
                required: Mass::from_milligrams(2),
            }
        ),
        "spent maintenance output must not service another worn machine"
    );
    let completion = finish_service(&registries, &mut state, outcome.completes_at());
    assert_eq!(completion.condition_after(), condition(700_000));
}

#[test]
fn authored_maintenance_resolution_rejects_unneeded_or_understocked_service() {
    let registries = registries();
    let mut state = AppState::new(WorldSeed::new(0x8120_000A));
    let healthy = add_equipment(&registries, &mut state, TEST_DEFINITION, condition(700_000))
        .unwrap_or_else(|error| panic!("healthy maintenance equipment fixture failed: {error}"));
    let worn = add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000))
        .unwrap_or_else(|error| panic!("worn maintenance equipment fixture failed: {error}"));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("maintenance stock fixture failed: {error}"));
    let spent = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("maintenance spent fixture failed: {error}"));
    add_material(&registries, &mut state, source, Mass::from_milligrams(1));

    assert_eq!(
        resolve_equipment_maintenance(
            &registries,
            &state,
            EquipmentMaintenanceRequest::new(healthy, source, spent),
        ),
        Err(
            EquipmentMaintenanceResolutionError::ConditionAtOrAboveServiceTarget {
                equipment: healthy,
                current: condition(700_000),
                target: condition(700_000),
            }
        )
    );
    assert_eq!(
        resolve_equipment_maintenance(
            &registries,
            &state,
            EquipmentMaintenanceRequest::new(worn, source, spent),
        ),
        Err(
            EquipmentMaintenanceResolutionError::InsufficientReplacementMaterial {
                stockpile: source,
                commodity: CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
                available: Mass::from_milligrams(1),
                required: Mass::from_milligrams(2),
            }
        )
    );
}

#[test]
fn maintenance_filters_contaminated_stock_and_rejects_forged_impure_selection() {
    let registries = registries();
    let mut state = AppState::new(WorldSeed::new(0x8120_000B));
    let equipment = add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000))
        .unwrap_or_else(|error| panic!("impure maintenance equipment fixture failed: {error}"));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(27))
        .unwrap_or_else(|error| panic!("impure maintenance source fixture failed: {error}"));
    let spent = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("impure maintenance spent fixture failed: {error}"));
    let composition = MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_WOOD, 900_000),
        CompositionComponent::new(MATERIAL_STONE, 100_000),
    ])
    .unwrap_or_else(|error| panic!("impure maintenance composition fixture failed: {error}"));
    let lot = deposit_composed_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(20),
        Temperature::from_millikelvin(300_000),
        composition,
    )
    .unwrap_or_else(|error| panic!("impure maintenance lot fixture failed: {error}"));
    let before = state.clone();
    let replacement = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);

    assert_eq!(
        resolve_equipment_maintenance(
            &registries,
            &state,
            EquipmentMaintenanceRequest::new(equipment, source, spent),
        ),
        Err(
            EquipmentMaintenanceResolutionError::InsufficientReplacementMaterial {
                stockpile: source,
                commodity: replacement,
                available: Mass::ZERO,
                required: Mass::from_milligrams(2),
            }
        )
    );
    assert_eq!(state, before);

    let resolution = bind(
        &state,
        equipment,
        source,
        lot,
        Mass::from_milligrams(7),
        spent,
        condition(700_000),
    );
    assert_eq!(
        validate_equipment_maintenance(&registries, &state, resolution),
        Err(EquipmentMaintenanceError::ImpureReplacementMaterial {
            commodity: replacement,
        })
    );
    assert_eq!(state, before);

    add_material(&registries, &mut state, source, Mass::from_milligrams(2));
    let resolved = resolve_equipment_maintenance(
        &registries,
        &state,
        EquipmentMaintenanceRequest::new(equipment, source, spent),
    )
    .unwrap_or_else(|error| {
        panic!("maintenance should skip contaminated replacement stock: {error}")
    });
    assert_eq!(resolved.material_mass(), Mass::from_milligrams(2));
}

#[test]
fn maintenance_moves_exact_material_to_spent_storage_and_preserves_conservation() {
    let registries = registries();
    let mut state = AppState::new(WorldSeed::new(0x8120_0001));
    initialize_service_player(&registries, &mut state);
    let equipment =
        match add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000)) {
            Ok(equipment) => equipment,
            Err(error) => panic!("maintenance equipment fixture failed: {error}"),
        };
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("maintenance source fixture failed: {error}"),
    };
    let spent = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("maintenance spent fixture failed: {error}"),
    };
    let lot = add_material(&registries, &mut state, source, Mass::from_milligrams(20));
    let source_lot = match state.inventory().get_lot(lot) {
        Some(record) => record,
        None => panic!("maintenance source lot disappeared"),
    };
    let temperature_before = source_lot.temperature();
    let composition_before = source_lot.composition().clone();
    let particle_size_before = source_lot.particle_size();
    let created_before = source_lot.created_at();
    let latest_before = source_lot.latest_created_at();
    let matter_before = match calculate_matter_accounting(&state) {
        Ok(accounting) => accounting.total(),
        Err(error) => panic!("maintenance initial matter accounting failed: {error}"),
    };
    let energy_before = explicit_energy(&registries, &state);
    let resolution = bind(
        &state,
        equipment,
        source,
        lot,
        Mass::from_milligrams(7),
        spent,
        condition(700_000),
    );
    let token = match validate_equipment_maintenance(&registries, &state, resolution) {
        Ok(token) => token,
        Err(error) => panic!("maintenance validation failed: {error}"),
    };
    assert_eq!(token.material_mass(), Mass::from_milligrams(7));

    let outcome = match token.commit(&mut state) {
        Ok(outcome) => outcome,
        Err(error) => panic!("maintenance commit failed: {error}"),
    };

    assert_eq!(outcome.condition_before(), condition(500_000));
    assert_eq!(outcome.target_condition(), condition(700_000));
    assert_eq!(outcome.material_mass(), Mass::from_milligrams(7));
    assert_eq!(
        state
            .equipment()
            .get_equipment(equipment)
            .map(|record| record.condition()),
        Some(condition(500_000))
    );
    assert_eq!(
        state.inventory().get_lot(lot).map(|record| record.mass()),
        Some(Mass::from_milligrams(13))
    );
    let spent_lot = match state
        .inventory()
        .lots()
        .find(|record| record.stockpile() == spent)
    {
        Some(record) => record,
        None => panic!("maintenance spent material missing"),
    };
    assert_eq!(spent_lot.mass(), Mass::from_milligrams(7));
    assert_eq!(
        spent_lot.commodity(),
        CommodityKey::new(MATERIAL_WOOD, FORM_CHIP)
    );
    assert_eq!(spent_lot.temperature(), temperature_before);
    assert_eq!(spent_lot.composition(), &composition_before);
    assert_eq!(spent_lot.particle_size(), particle_size_before);
    assert_eq!(spent_lot.created_at(), created_before);
    assert_eq!(spent_lot.latest_created_at(), latest_before);
    assert_eq!(
        calculate_matter_accounting(&state).map(|accounting| accounting.total()),
        Ok(matter_before)
    );
    assert_eq!(explicit_energy(&registries, &state), energy_before);
    let completion = finish_service(&registries, &mut state, outcome.completes_at());
    assert_eq!(completion.condition_after(), condition(700_000));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn maintenance_rejects_non_improvement_and_allows_spent_material_to_return_to_source() {
    let registries = registries();
    let mut state = AppState::new(WorldSeed::new(0x8120_0002));
    initialize_service_player(&registries, &mut state);
    let equipment =
        match add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000)) {
            Ok(equipment) => equipment,
            Err(error) => panic!("maintenance rejection equipment fixture failed: {error}"),
        };
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("maintenance rejection source fixture failed: {error}"),
    };
    let spent = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("maintenance rejection spent fixture failed: {error}"),
    };
    let lot = add_material(&registries, &mut state, source, Mass::from_milligrams(10));
    let before = state.clone();

    let no_improvement = bind(
        &state,
        equipment,
        source,
        lot,
        Mass::from_milligrams(1),
        spent,
        condition(500_000),
    );
    assert_eq!(
        validate_equipment_maintenance(&registries, &state, no_improvement),
        Err(EquipmentMaintenanceError::ConditionNotImproved {
            equipment,
            before: condition(500_000),
            after: condition(500_000),
        })
    );
    assert_eq!(state, before);

    let same_destination = resolve_equipment_maintenance(
        &registries,
        &state,
        EquipmentMaintenanceRequest::new(equipment, source, source),
    )
    .unwrap_or_else(|error| panic!("same-stockpile maintenance resolution failed: {error}"));
    let outcome = validate_equipment_maintenance(&registries, &state, same_destination)
        .unwrap_or_else(|error| panic!("same-stockpile maintenance validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("same-stockpile maintenance commit failed: {error}"));
    assert_eq!(outcome.material_mass(), Mass::from_milligrams(2));
    assert_eq!(
        state
            .equipment()
            .get_equipment(equipment)
            .map(|record| record.condition()),
        Some(condition(500_000))
    );
    let source_record = state
        .inventory()
        .get_stockpile(source)
        .unwrap_or_else(|| panic!("same-stockpile maintenance source disappeared"));
    assert_eq!(source_record.stored_mass(), Mass::from_milligrams(10));
    assert_eq!(
        source_record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_LOG)),
        Mass::from_milligrams(8)
    );
    assert_eq!(
        source_record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_CHIP)),
        Mass::from_milligrams(2)
    );
    let completion = finish_service(&registries, &mut state, outcome.completes_at());
    assert_eq!(completion.condition_after(), condition(700_000));
}

#[test]
fn maintenance_rechecks_inventory_and_equipment_before_any_partial_commit() {
    let registries = registries();
    let mut state = AppState::new(WorldSeed::new(0x8120_0003));
    initialize_service_player(&registries, &mut state);
    let equipment =
        match add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000)) {
            Ok(equipment) => equipment,
            Err(error) => panic!("maintenance stale equipment fixture failed: {error}"),
        };
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("maintenance stale source fixture failed: {error}"),
    };
    let spent = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("maintenance stale spent fixture failed: {error}"),
    };
    let lot = add_material(&registries, &mut state, source, Mass::from_milligrams(10));

    let inventory_stale = match validate_equipment_maintenance(
        &registries,
        &state,
        bind(
            &state,
            equipment,
            source,
            lot,
            Mass::from_milligrams(2),
            spent,
            condition(600_000),
        ),
    ) {
        Ok(token) => token,
        Err(error) => panic!("maintenance stale inventory validation failed: {error}"),
    };
    if let Err(error) = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1)) {
        panic!("maintenance stale inventory mutation failed: {error}");
    }
    let condition_before = state
        .equipment()
        .get_equipment(equipment)
        .map(|record| record.condition());
    assert!(matches!(
        inventory_stale.commit(&mut state),
        Err(EquipmentMaintenanceCommitError::StaleInventoryRevision {
            expected: _expected,
            actual: _actual,
        })
    ));
    assert_eq!(
        state
            .equipment()
            .get_equipment(equipment)
            .map(|record| record.condition()),
        condition_before
    );
    assert_eq!(
        state.inventory().get_lot(lot).map(|record| record.mass()),
        Some(Mass::from_milligrams(10))
    );

    let equipment_stale = match validate_equipment_maintenance(
        &registries,
        &state,
        bind(
            &state,
            equipment,
            source,
            lot,
            Mass::from_milligrams(2),
            spent,
            condition(600_000),
        ),
    ) {
        Ok(token) => token,
        Err(error) => panic!("maintenance stale equipment validation failed: {error}"),
    };
    let wear = match decide_equipment_wear(&state, equipment, 1_000) {
        Ok(plan) => plan,
        Err(error) => panic!("maintenance stale equipment wear failed: {error}"),
    };
    if let Err(error) = apply_equipment_condition_plan(&mut state, wear) {
        panic!("maintenance stale equipment wear commit failed: {error}");
    }
    let lot_mass_before = state.inventory().get_lot(lot).map(|record| record.mass());
    assert!(matches!(
        equipment_stale.commit(&mut state),
        Err(EquipmentMaintenanceCommitError::StaleEquipmentRevision {
            expected: _expected,
            actual: _actual,
        })
    ));
    assert_eq!(
        state.inventory().get_lot(lot).map(|record| record.mass()),
        lot_mass_before
    );
}

#[test]
fn maintenance_resolution_is_invalidated_by_equipment_change_before_validation() {
    let registries = registries();
    let mut state = AppState::new(WorldSeed::new(0x8120_0009));
    let equipment =
        match add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000)) {
            Ok(equipment) => equipment,
            Err(error) => panic!("maintenance stale-resolution equipment fixture failed: {error}"),
        };
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("maintenance stale-resolution source fixture failed: {error}"),
    };
    let spent = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("maintenance stale-resolution spent fixture failed: {error}"),
    };
    let lot = add_material(&registries, &mut state, source, Mass::from_milligrams(1));
    let expected_revision = state.equipment().revision();
    let resolution = bind(
        &state,
        equipment,
        source,
        lot,
        Mass::from_milligrams(1),
        spent,
        condition(600_000),
    );
    assert_eq!(resolution.condition_before(), condition(500_000));
    let wear = match decide_equipment_wear(&state, equipment, 1_000) {
        Ok(plan) => plan,
        Err(error) => panic!("maintenance stale-resolution wear planning failed: {error}"),
    };
    if let Err(error) = apply_equipment_condition_plan(&mut state, wear) {
        panic!("maintenance stale-resolution wear commit failed: {error}");
    }
    let actual_revision = state.equipment().revision();
    let inventory_before = state.inventory().clone();

    assert_eq!(
        validate_equipment_maintenance(&registries, &state, resolution),
        Err(EquipmentMaintenanceError::StaleEquipmentResolution {
            equipment,
            expected_revision,
            actual_revision,
        })
    );
    assert_eq!(state.inventory(), &inventory_before);
}

fn active_support(
    registries: &Registries,
    state: &mut AppState,
    x: i64,
) -> crate::structural::StructuralElementId {
    let bounds = match VoxelBounds::new(VoxelCoord::new(x, 0, 0), VoxelCoord::new(x + 1, 1, 1)) {
        Ok(bounds) => bounds,
        Err(error) => panic!("maintenance support bounds fixture failed: {error}"),
    };
    let element = match add_structural_element(
        registries,
        state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        crate::structural::make_test_structural_geometry(
            bounds,
            Length::from_micrometers(1),
            Area::from_square_millimeters(1_000),
        ),
        true,
    ) {
        Ok(element) => element,
        Err(error) => panic!("maintenance support fixture failed: {error}"),
    };
    materialize_structural_element_for_test(registries, state, element, FORM_LOG);
    let activation = match validate_activate_structural_element(registries, state, element) {
        Ok(token) => token,
        Err(error) => panic!("maintenance support activation failed: {error}"),
    };
    if let Err(error) = activation.commit(state) {
        panic!("maintenance support activation commit failed: {error}");
    }
    element
}

#[test]
fn maintenance_material_relocation_updates_supported_stockpile_loads_atomically() {
    let registries = registries();
    let mut state = AppState::new(WorldSeed::new(0x8120_0004));
    initialize_service_player(&registries, &mut state);
    let equipment =
        match add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000)) {
            Ok(equipment) => equipment,
            Err(error) => panic!("maintenance support equipment fixture failed: {error}"),
        };
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("maintenance supported source fixture failed: {error}"),
    };
    let spent = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("maintenance supported spent fixture failed: {error}"),
    };
    let lot = add_material(&registries, &mut state, source, Mass::from_milligrams(10));
    let source_support = active_support(&registries, &mut state, 0);
    let spent_support = active_support(&registries, &mut state, 2);
    for (stockpile, support) in [(source, source_support), (spent, spent_support)] {
        let token = match validate_mount_stockpile(&registries, &state, stockpile, support) {
            Ok(token) => token,
            Err(error) => panic!("maintenance stockpile mount failed: {error}"),
        };
        if let Err(error) = token.commit(&mut state) {
            panic!("maintenance stockpile mount commit failed: {error}");
        }
    }
    let source_load_before = state
        .structures()
        .get_element(source_support)
        .map(|record| record.load(StructuralLoadKind::StoredMatter))
        .unwrap_or(Force::ZERO);
    assert!(source_load_before > Force::ZERO);
    assert_eq!(
        state
            .structures()
            .get_element(spent_support)
            .map(|record| record.load(StructuralLoadKind::StoredMatter)),
        Some(Force::ZERO)
    );

    let token = match validate_equipment_maintenance(
        &registries,
        &state,
        bind(
            &state,
            equipment,
            source,
            lot,
            Mass::from_milligrams(10),
            spent,
            condition(700_000),
        ),
    ) {
        Ok(token) => token,
        Err(error) => panic!("maintenance supported validation failed: {error}"),
    };
    if let Err(error) = token.commit(&mut state) {
        panic!("maintenance supported commit failed: {error}");
    }

    assert_eq!(
        state
            .structures()
            .get_element(source_support)
            .map(|record| record.load(StructuralLoadKind::StoredMatter)),
        Some(Force::ZERO)
    );
    assert_eq!(
        state
            .structures()
            .get_element(spent_support)
            .map(|record| record.load(StructuralLoadKind::StoredMatter)),
        Some(source_load_before)
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn maintenance_preserves_multiple_partial_lot_profiles_without_id_collision() {
    let registries = registries();
    let mut state = AppState::new(WorldSeed::new(0x8120_0005));
    initialize_service_player(&registries, &mut state);
    let equipment =
        match add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000)) {
            Ok(equipment) => equipment,
            Err(error) => panic!("multi-lot maintenance equipment fixture failed: {error}"),
        };
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("multi-lot maintenance source fixture failed: {error}"),
    };
    let spent = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("multi-lot maintenance spent fixture failed: {error}"),
    };
    let first = match deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(5),
        Temperature::from_millikelvin(300_000),
    ) {
        Ok(lot) => lot,
        Err(error) => panic!("multi-lot first fixture failed: {error}"),
    };
    let second = match deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(5),
        Temperature::from_millikelvin(310_000),
    ) {
        Ok(lot) => lot,
        Err(error) => panic!("multi-lot second fixture failed: {error}"),
    };
    let resolution = bind_selections(
        &state,
        equipment,
        source,
        &[
            MaterialLotSelection::new(first, Mass::from_milligrams(2)),
            MaterialLotSelection::new(second, Mass::from_milligrams(2)),
        ],
        spent,
        condition(700_000),
    );
    let token = match validate_equipment_maintenance(&registries, &state, resolution) {
        Ok(token) => token,
        Err(error) => panic!("multi-lot maintenance validation failed: {error}"),
    };
    if let Err(error) = token.commit(&mut state) {
        panic!("multi-lot maintenance commit failed: {error}");
    }

    assert_eq!(
        state.inventory().get_lot(first).map(|lot| lot.mass()),
        Some(Mass::from_milligrams(3))
    );
    assert_eq!(
        state.inventory().get_lot(second).map(|lot| lot.mass()),
        Some(Mass::from_milligrams(3))
    );
    let mut spent_lots: Vec<_> = state
        .inventory()
        .lots()
        .filter(|lot| lot.stockpile() == spent)
        .map(|lot| (lot.id(), lot.mass(), lot.temperature()))
        .collect();
    spent_lots.sort_by_key(|entry| entry.0);
    assert_eq!(spent_lots.len(), 2);
    assert_ne!(spent_lots[0].0, spent_lots[1].0);
    assert_eq!(spent_lots[0].1, Mass::from_milligrams(2));
    assert_eq!(spent_lots[1].1, Mass::from_milligrams(2));
    assert_eq!(
        spent_lots
            .iter()
            .map(|entry| entry.2)
            .collect::<std::collections::BTreeSet<_>>(),
        std::collections::BTreeSet::from([
            Temperature::from_millikelvin(300_000),
            Temperature::from_millikelvin(310_000),
        ])
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn maintenance_spent_capacity_failure_is_atomic() {
    let registries = registries();
    let mut state = AppState::new(WorldSeed::new(0x8120_0006));
    initialize_service_player(&registries, &mut state);
    let equipment =
        match add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000)) {
            Ok(equipment) => equipment,
            Err(error) => panic!("maintenance capacity equipment fixture failed: {error}"),
        };
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("maintenance capacity source fixture failed: {error}"),
    };
    let spent = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("maintenance capacity spent fixture failed: {error}"),
    };
    let lot = add_material(&registries, &mut state, source, Mass::from_milligrams(10));
    let resolution = bind(
        &state,
        equipment,
        source,
        lot,
        Mass::from_milligrams(2),
        spent,
        condition(700_000),
    );
    let before = state.clone();

    assert_eq!(
        validate_equipment_maintenance(&registries, &state, resolution),
        Err(EquipmentMaintenanceError::Material(
            EquipmentMaintenanceMaterialError::SpentCapacityExceeded {
                stockpile: spent,
                capacity: Mass::from_milligrams(1),
                committed: Mass::ZERO,
                requested: Mass::from_milligrams(2),
            }
        ))
    );
    assert_eq!(state, before);
}

#[cfg(feature = "test-soak")]
#[test]
#[ignore = "long-horizon soak"]
fn equipment_maintenance_soak_preserves_timed_service_resources_and_replay() {
    let registries = registries();
    let mut first = AppState::new(WorldSeed::new(0x8120_0007));
    initialize_service_player(&registries, &mut first);
    let equipment =
        match add_equipment(&registries, &mut first, TEST_DEFINITION, condition(700_000)) {
            Ok(equipment) => equipment,
            Err(error) => panic!("maintenance soak equipment fixture failed: {error}"),
        };
    let source = match add_solid_stockpile_for_test(&mut first, Mass::from_milligrams(500)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("maintenance soak source fixture failed: {error}"),
    };
    let spent = match add_solid_stockpile_for_test(&mut first, Mass::from_milligrams(500)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("maintenance soak spent fixture failed: {error}"),
    };
    add_material(&registries, &mut first, source, Mass::from_milligrams(500));
    let initial_matter = match calculate_matter_accounting(&first) {
        Ok(accounting) => accounting.total(),
        Err(error) => panic!("maintenance soak initial matter accounting failed: {error}"),
    };
    let initial_energy = explicit_energy(&registries, &first);
    let mut second = first.clone();

    for cycle in 0..500_u64 {
        for state in [&mut first, &mut second] {
            let wear = match decide_equipment_wear(state, equipment, 1_000) {
                Ok(plan) => plan,
                Err(error) => panic!("maintenance soak wear planning failed at {cycle}: {error}"),
            };
            if let Err(error) = apply_equipment_condition_plan(state, wear) {
                panic!("maintenance soak wear commit failed at {cycle}: {error}");
            }
            assert_eq!(
                state
                    .equipment()
                    .get_equipment(equipment)
                    .map(|record| record.condition()),
                Some(condition(699_000))
            );
            let resolution = resolve_equipment_maintenance(
                &registries,
                state,
                EquipmentMaintenanceRequest::new(equipment, source, spent),
            )
            .unwrap_or_else(|error| {
                panic!("maintenance soak resolution failed at {cycle}: {error}")
            });
            assert_eq!(resolution.material_mass(), Mass::from_milligrams(1));
            assert_eq!(resolution.condition_before(), condition(699_000));
            assert_eq!(resolution.condition_after(), condition(700_000));
            assert_eq!(resolution.duration(), TickSpan::new(1));
            let maintenance = match validate_equipment_maintenance(&registries, state, resolution) {
                Ok(token) => token,
                Err(error) => panic!("maintenance soak validation failed at {cycle}: {error}"),
            };
            let outcome = maintenance.commit(state).unwrap_or_else(|error| {
                panic!("maintenance soak commit failed at {cycle}: {error}")
            });
            assert_eq!(outcome.target_condition(), condition(700_000));
            let completion = finish_service(&registries, state, outcome.completes_at());
            assert_eq!(completion.condition_before(), condition(699_000));
            assert_eq!(completion.condition_after(), condition(700_000));
        }
        if cycle % 53 == 0 {
            assert_eq!(validate_loaded_state(&registries, &first), Ok(()));
            assert_eq!(
                calculate_matter_accounting(&first).map(|accounting| accounting.total()),
                Ok(initial_matter)
            );
            assert_eq!(explicit_energy(&registries, &first), initial_energy);
        }
    }

    assert_eq!(first, second);
    assert_eq!(validate_loaded_state(&registries, &first), Ok(()));
    assert_eq!(
        calculate_matter_accounting(&first).map(|accounting| accounting.total()),
        Ok(initial_matter)
    );
    assert_eq!(explicit_energy(&registries, &first), initial_energy);
    assert_eq!(
        first
            .equipment()
            .get_equipment(equipment)
            .map(|record| record.condition()),
        Some(condition(700_000))
    );
    assert_eq!(
        first
            .inventory()
            .get_stockpile(source)
            .map(|record| record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_LOG))),
        Some(Mass::ZERO)
    );
    assert_eq!(
        first
            .inventory()
            .get_stockpile(spent)
            .map(|record| record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_CHIP))),
        Some(Mass::from_milligrams(500))
    );
}

#[test]
fn maintenance_commit_rechecks_late_production_occupancy_before_moving_material() {
    let registries = occupied_registries();
    let mut state = AppState::new(WorldSeed::new(0x8120_0008));
    initialize_service_player(&registries, &mut state);
    let equipment =
        match add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000)) {
            Ok(equipment) => equipment,
            Err(error) => panic!("maintenance occupancy equipment fixture failed: {error}"),
        };
    let process_source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("maintenance occupancy process source failed: {error}"),
    };
    let process_destination =
        match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20)) {
            Ok(stockpile) => stockpile,
            Err(error) => panic!("maintenance occupancy process destination failed: {error}"),
        };
    let maintenance_source =
        match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1)) {
            Ok(stockpile) => stockpile,
            Err(error) => panic!("maintenance occupancy maintenance source failed: {error}"),
        };
    let spent = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("maintenance occupancy spent destination failed: {error}"),
    };
    let process_lot = add_material(
        &registries,
        &mut state,
        process_source,
        Mass::from_milligrams(10),
    );
    let maintenance_lot = add_material(
        &registries,
        &mut state,
        maintenance_source,
        Mass::from_milligrams(1),
    );
    let energy_store = match add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        ENERGY_DEFINITION,
        Energy::from_nanojoules(1_000_000_000),
    ) {
        Ok(store) => store,
        Err(error) => panic!("maintenance occupancy energy fixture failed: {error}"),
    };
    let maintenance = match validate_equipment_maintenance(
        &registries,
        &state,
        bind(
            &state,
            equipment,
            maintenance_source,
            maintenance_lot,
            Mass::from_milligrams(1),
            spent,
            condition(600_000),
        ),
    ) {
        Ok(token) => token,
        Err(error) => panic!("maintenance occupancy validation failed: {error}"),
    };

    let selection = [MaterialLotSelection::new(
        process_lot,
        Mass::from_milligrams(10),
    )];
    let heating = match resolve_sensible_heating_process(
        &registries,
        &state,
        SensibleHeatingRequest::new(
            HEATING_PROCESS,
            process_source,
            &selection,
            equipment,
            energy_store,
            Temperature::from_millikelvin(301_000),
        ),
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("maintenance occupancy heating resolution failed: {error}"),
    };
    let start = match validate_start_process(
        &registries,
        &state,
        heating.process_resolution(),
        process_source,
        process_destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("maintenance occupancy process validation failed: {error}"),
    };
    let job = match start.commit(&mut state) {
        Ok(job) => job,
        Err(error) => panic!("maintenance occupancy process commit failed: {error}"),
    };
    let job_record = match state.production().get_job(job) {
        Some(record) => record,
        None => panic!("maintenance occupancy process job missing after start"),
    };
    let expected_error = EquipmentMaintenanceCommitError::EquipmentBusy {
        equipment,
        job,
        release: job_record.occupancy_release(),
    };
    let maintenance_mass_before = state
        .inventory()
        .get_lot(maintenance_lot)
        .map(|lot| lot.mass());
    let condition_before = state
        .equipment()
        .get_equipment(equipment)
        .map(|record| record.condition());

    assert_eq!(maintenance.commit(&mut state), Err(expected_error));
    assert_eq!(
        state
            .inventory()
            .get_lot(maintenance_lot)
            .map(|lot| lot.mass()),
        maintenance_mass_before
    );
    assert_eq!(
        state
            .equipment()
            .get_equipment(equipment)
            .map(|record| record.condition()),
        condition_before
    );
}

#[test]
fn maintenance_counts_reserved_inbound_as_capacity_but_not_structural_weight() {
    let registries = occupied_registries();
    let mut state = AppState::new(WorldSeed::new(0x8120_0009));
    initialize_service_player(&registries, &mut state);
    let process_equipment = match add_equipment(
        &registries,
        &mut state,
        TEST_DEFINITION,
        Condition::PRISTINE,
    ) {
        Ok(equipment) => equipment,
        Err(error) => panic!("reserved-weight process equipment fixture failed: {error}"),
    };
    let maintenance_equipment =
        match add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000)) {
            Ok(equipment) => equipment,
            Err(error) => panic!("reserved-weight maintenance equipment fixture failed: {error}"),
        };
    let process_source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(5)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("reserved-weight process source fixture failed: {error}"),
    };
    let maintenance_source =
        match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2)) {
            Ok(stockpile) => stockpile,
            Err(error) => panic!("reserved-weight maintenance source fixture failed: {error}"),
        };
    let spent = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("reserved-weight spent fixture failed: {error}"),
    };
    let process_lot = add_material(
        &registries,
        &mut state,
        process_source,
        Mass::from_milligrams(5),
    );
    let maintenance_lot = add_material(
        &registries,
        &mut state,
        maintenance_source,
        Mass::from_milligrams(2),
    );
    let support = active_support(&registries, &mut state, 0);
    let mount = match validate_mount_stockpile(&registries, &state, spent, support) {
        Ok(token) => token,
        Err(error) => panic!("reserved-weight spent mount validation failed: {error}"),
    };
    if let Err(error) = mount.commit(&mut state) {
        panic!("reserved-weight spent mount commit failed: {error}");
    }
    let energy_store = match add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        ENERGY_DEFINITION,
        Energy::from_nanojoules(1_000_000_000),
    ) {
        Ok(store) => store,
        Err(error) => panic!("reserved-weight energy fixture failed: {error}"),
    };
    let process_selection = [MaterialLotSelection::new(
        process_lot,
        Mass::from_milligrams(5),
    )];
    let heating = match resolve_sensible_heating_process(
        &registries,
        &state,
        SensibleHeatingRequest::new(
            HEATING_PROCESS,
            process_source,
            &process_selection,
            process_equipment,
            energy_store,
            Temperature::from_millikelvin(301_000),
        ),
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("reserved-weight heating resolution failed: {error}"),
    };
    let start = match validate_start_process(
        &registries,
        &state,
        heating.process_resolution(),
        process_source,
        spent,
    ) {
        Ok(token) => token,
        Err(error) => panic!("reserved-weight process validation failed: {error}"),
    };
    if let Err(error) = start.commit(&mut state) {
        panic!("reserved-weight process commit failed: {error}");
    }

    let spent_before = match state.inventory().get_stockpile(spent) {
        Some(record) => record,
        None => panic!("reserved-weight spent stockpile disappeared"),
    };
    assert_eq!(spent_before.reserved_inbound(), Mass::from_milligrams(5));
    assert_eq!(spent_before.stored_mass(), Mass::ZERO);
    assert_eq!(
        state
            .structures()
            .get_element(support)
            .map(|record| record.load(StructuralLoadKind::StoredMatter)),
        Some(Force::ZERO)
    );

    let maintenance = match validate_equipment_maintenance(
        &registries,
        &state,
        bind(
            &state,
            maintenance_equipment,
            maintenance_source,
            maintenance_lot,
            Mass::from_milligrams(2),
            spent,
            condition(700_000),
        ),
    ) {
        Ok(token) => token,
        Err(error) => panic!("reserved-weight maintenance validation failed: {error}"),
    };
    if let Err(error) = maintenance.commit(&mut state) {
        panic!("reserved-weight maintenance commit failed: {error}");
    }

    let spent_after = match state.inventory().get_stockpile(spent) {
        Some(record) => record,
        None => panic!("reserved-weight spent stockpile disappeared after maintenance"),
    };
    assert_eq!(spent_after.reserved_inbound(), Mass::from_milligrams(5));
    assert_eq!(spent_after.stored_mass(), Mass::from_milligrams(2));
    let expected_weight = match calculate_aggregate_weight_force_ceiling(
        AggregateMass::from_mass(Mass::from_milligrams(2)),
        registries.core().gravity(),
    ) {
        Some(force) => force,
        None => panic!("reserved-weight expected load overflowed"),
    };
    assert_eq!(
        state
            .structures()
            .get_element(support)
            .map(|record| record.load(StructuralLoadKind::StoredMatter)),
        Some(expected_weight)
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

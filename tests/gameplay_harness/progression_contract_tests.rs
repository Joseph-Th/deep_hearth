//! Cheap primitive-progression topology and generator contracts.

use std::collections::BTreeSet;

use deep_hearth::content::gameplay_fixture::seed_lot;
use deep_hearth::content::{
    ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE, ENERGY_STONE_FLYWHEEL_DRIVE,
    EQUIPMENT_COPPER_REINFORCED_GEOLOGICAL_HAMMER, EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
    EQUIPMENT_COPPER_REINFORCED_PICK, EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
    EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK, EQUIPMENT_COPPER_REINFORCED_STONE_ROTARY_QUERN,
    EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR, EQUIPMENT_COPPER_REINFORCED_WOODWORKING_ADZE,
    FORM_BOARD, FORM_CHIP, FORM_INGOT, FORM_LOG, FORM_NATIVE_METAL, FORM_REINFORCEMENT, FORM_SCRAP,
    FORM_TOOL, MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD,
    PROCESS_COLD_WORK_COPPER_INGOT_REINFORCEMENT, PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
    PROCESS_COLD_WORK_COPPER_SCRAP_REINFORCEMENT, PROCESS_CONCENTRATE_COPPER,
    PROCESS_HAND_BREAK_ORE, PROCESS_HAND_GRIND_CRUSHED_ORE, PROCESS_HAND_SORT_NATIVE_COPPER,
    PROCESS_KNAP_STONE_TOOL, PROCESS_REKNAP_STONE_SCRAP_TOOL, PROCESS_SEPARATE_NATIVE_COPPER,
    PROCESS_SHAPE_WOOD_BOARDS, build_registries,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::AppState;
use deep_hearth::material::CommodityKey;
use deep_hearth::registry::{ProcessEnergyRole, ProcessEquipmentRole, ProcessExecutionFamily};

use super::environment::ROOM_TEMPERATURE;
use super::inventory_support::add_solid_stockpile;
use super::manual_craft_planning::manual_craft_plan_for_available_output;
use super::manual_craft_topology_planning::manual_craft_topology_plan_for_output_from_inputs;

#[test]
fn bootstrap_planning_excludes_faster_required_equipment_producers() {
    let registries = build_registries();
    let boards = CommodityKey::new(MATERIAL_WOOD, FORM_BOARD);
    assert!(
        registries
            .crafting()
            .manual_producers(boards)
            .any(|definition| registries
                .process_topology(definition.process())
                .is_some_and(
                    |topology| topology.equipment_role() == ProcessEquipmentRole::Required
                )),
        "bootstrap-planning regression requires a competing required-equipment board route"
    );

    let (selected, batches) = manual_craft_topology_plan_for_output_from_inputs(
        &registries,
        boards,
        Mass::from_milligrams(800_000),
        &[CommodityKey::new(MATERIAL_WOOD, FORM_LOG)],
        "bootstrap-planning regression",
    );

    assert_eq!(selected.process(), PROCESS_SHAPE_WOOD_BOARDS);
    assert_eq!(batches, 1);
    assert_ne!(
        registries
            .process_topology(selected.process())
            .unwrap_or_else(|| panic!("selected bootstrap route lost process topology"))
            .equipment_role(),
        ProcessEquipmentRole::Required
    );
}

#[test]
fn coarse_manual_ore_work_branches_between_visible_sorting_and_mortar_liberation() {
    let registries = build_registries();
    let breaking = registries
        .ore_processing()
        .get_manual_comminution(PROCESS_HAND_BREAK_ORE)
        .unwrap_or_else(|| panic!("manual ore breaking disappeared"));
    let sorting = registries
        .ore_processing()
        .get_manual_constituent_separation(PROCESS_HAND_SORT_NATIVE_COPPER)
        .unwrap_or_else(|| panic!("manual native-copper sorting disappeared"));
    let grinding = registries
        .ore_processing()
        .get_manual_comminution(PROCESS_HAND_GRIND_CRUSHED_ORE)
        .unwrap_or_else(|| panic!("manual mortar grinding disappeared"));
    let concentration = registries
        .ore_processing()
        .get_constituent_separation(PROCESS_CONCENTRATE_COPPER)
        .unwrap_or_else(|| panic!("copper concentration disappeared"));

    assert_eq!(grinding.input_form(), breaking.output_form());
    assert_eq!(
        grinding.input_particle_size_range(),
        Some(breaking.output_particle_size())
    );
    assert_eq!(
        sorting.input_particle_size_range(),
        breaking.output_particle_size(),
        "coarse broken ore must remain directly hand-sortable"
    );
    assert_eq!(grinding.output_form(), concentration.input_form());
    assert_eq!(
        concentration.input_particle_size_range(),
        Some(grinding.output_particle_size()),
        "mortar work should replace the liberation step, not the downstream separator"
    );
    assert!(
        grinding.processing_rate() < breaking.processing_rate(),
        "fine hand grinding should remain more attention-intensive than initial cobbing"
    );
}

#[test]
fn current_manual_craft_planning_ignores_unowned_salvage_inputs() {
    let registries = build_registries();
    let mut state = AppState::new();
    let raw = add_solid_stockpile(&mut state, Mass::from_milligrams(10_000_000));
    seed_lot(
        &registries,
        &mut state,
        raw,
        CommodityKey::new(MATERIAL_WOOD, deep_hearth::content::FORM_LOG),
        Mass::from_milligrams(10_000_000),
        ROOM_TEMPERATURE,
    );
    deep_hearth::survival::initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("manual-craft planning player setup failed: {error}"));
    let boards = CommodityKey::new(MATERIAL_WOOD, FORM_BOARD);

    let (selected, batches, selected_source) = manual_craft_plan_for_available_output(
        &registries,
        &state,
        &[raw],
        boards,
        Mass::from_milligrams(1_600_000),
        "available board-route regression",
    );

    assert_eq!(selected.process(), PROCESS_SHAPE_WOOD_BOARDS);
    assert_eq!(batches, 2);
    assert_eq!(selected_source, raw);
}

#[test]
fn primitive_topology_planning_does_not_select_unacquired_cast_stock() {
    let registries = build_registries();
    let reinforcement = CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT);
    let native = CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL);
    let ingot = CommodityKey::new(MATERIAL_COPPER, FORM_INGOT);
    let cast_route = registries
        .crafting()
        .get_manual(PROCESS_COLD_WORK_COPPER_INGOT_REINFORCEMENT)
        .unwrap_or_else(|| panic!("cast-copper reinforcement route disappeared"));
    let native_route = registries
        .crafting()
        .get_manual(PROCESS_COLD_WORK_COPPER_REINFORCEMENT)
        .unwrap_or_else(|| panic!("native-copper reinforcement route disappeared"));
    assert_eq!(cast_route.input(), ingot);
    assert_eq!(native_route.input(), native);
    assert!(
        cast_route.duration() < native_route.duration(),
        "regression requires the later cast-stock route to be the globally faster route"
    );

    let (selected, batches) = manual_craft_topology_plan_for_output_from_inputs(
        &registries,
        reinforcement,
        Mass::from_milligrams(20_000),
        &[native],
        "primitive native-copper topology regression",
    );

    assert_eq!(selected.process(), PROCESS_COLD_WORK_COPPER_REINFORCEMENT);
    assert_eq!(batches, 1);
}

#[test]
fn primitive_recovery_and_reinforcement_routes_remain_connected() {
    let registries = build_registries();

    let hand_break = registries
        .ore_processing()
        .get_manual_comminution(PROCESS_HAND_BREAK_ORE)
        .unwrap_or_else(|| panic!("manual ore breaking disappeared"));
    let manual_sort = registries
        .ore_processing()
        .get_manual_constituent_separation(PROCESS_HAND_SORT_NATIVE_COPPER)
        .unwrap_or_else(|| panic!("manual native-copper sorting disappeared"));
    let powered_sort = registries
        .ore_processing()
        .get_constituent_separation(PROCESS_SEPARATE_NATIVE_COPPER)
        .unwrap_or_else(|| panic!("powered native-copper separation disappeared"));
    assert_eq!(manual_sort.input_form(), hand_break.output_form());
    assert_eq!(
        manual_sort.input_particle_size_range(),
        hand_break.output_particle_size()
    );
    assert!(manual_sort.target_recovery_ppm() > 0);
    assert!(manual_sort.target_recovery_ppm() < powered_sort.target_recovery_ppm());
    assert!(!manual_sort.max_batch_mass().is_zero());
    assert!(!manual_sort.processing_rate().is_zero());

    for (process, expected_family) in [
        (
            PROCESS_HAND_BREAK_ORE,
            ProcessExecutionFamily::ManualComminution,
        ),
        (
            PROCESS_HAND_SORT_NATIVE_COPPER,
            ProcessExecutionFamily::ManualSeparation,
        ),
    ] {
        let topology = registries
            .process_topology(process)
            .unwrap_or_else(|| panic!("manual progression process lost canonical topology"));
        assert_eq!(topology.execution_family(), expected_family);
        assert_eq!(
            topology.equipment_role(),
            ProcessEquipmentRole::Optional,
            "primitive manual recovery should remain equipment-assisted rather than tool-gated"
        );
        assert!(
            !topology.nominal_providers().is_empty(),
            "primitive manual recovery should retain at least one durable-tool investment path"
        );
        assert_eq!(topology.energy_role(), ProcessEnergyRole::None);
        assert!(topology.compatible_energy_stores().is_empty());
    }

    let fresh_stone = registries
        .crafting()
        .get_manual(PROCESS_KNAP_STONE_TOOL)
        .unwrap_or_else(|| panic!("fresh stone knapping disappeared"));
    let recycled_stone = registries
        .crafting()
        .get_manual(PROCESS_REKNAP_STONE_SCRAP_TOOL)
        .unwrap_or_else(|| panic!("stone scrap reknapping disappeared"));
    assert_eq!(
        recycled_stone.input(),
        CommodityKey::new(MATERIAL_STONE, FORM_SCRAP)
    );
    assert!(recycled_stone.duration() > fresh_stone.duration());
    let recovered = recycled_stone
        .outputs()
        .iter()
        .filter(|output| {
            matches!(
                output.commodity(),
                commodity
                    if commodity == CommodityKey::new(MATERIAL_STONE, FORM_TOOL)
                        || commodity == CommodityKey::new(MATERIAL_STONE, FORM_CHIP)
            )
        })
        .map(|output| output.mass())
        .try_fold(deep_hearth::core::quantity::Mass::ZERO, |total, mass| {
            total.checked_add(mass)
        })
        .unwrap_or_else(|| panic!("stone recovery output mass overflowed"));
    assert_eq!(recovered, recycled_stone.input_mass());

    let native_work = registries
        .crafting()
        .get_manual(PROCESS_COLD_WORK_COPPER_REINFORCEMENT)
        .unwrap_or_else(|| panic!("native-copper reinforcement work disappeared"));
    let scrap_rework = registries
        .crafting()
        .get_manual(PROCESS_COLD_WORK_COPPER_SCRAP_REINFORCEMENT)
        .unwrap_or_else(|| panic!("copper scrap rework disappeared"));
    assert!(scrap_rework.duration() > native_work.duration());

    let reinforcement = CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT);
    let canonical_inputs = registries
        .equipment()
        .get_equipment(EQUIPMENT_COPPER_REINFORCED_PICK)
        .and_then(|definition| definition.upgrade_profile())
        .unwrap_or_else(|| panic!("reinforced pick lost its additive upgrade route"))
        .additions()
        .inputs();
    let canonical = canonical_inputs
        .iter()
        .find(|input| input.commodity() == reinforcement)
        .unwrap_or_else(|| panic!("reinforced pick lost its canonical copper reinforcement input"));
    assert_eq!(
        canonical_inputs
            .iter()
            .filter(|input| input.commodity() == reinforcement)
            .count(),
        1,
        "reinforced pick must identify one unambiguous canonical copper reinforcement input"
    );

    let compatible_targets = registries
        .equipment()
        .definitions()
        .filter_map(|definition| {
            let inputs = definition.upgrade_profile()?.additions().inputs();
            inputs
                .iter()
                .any(|input| {
                    input.commodity() == canonical.commodity() && input.mass() == canonical.mass()
                })
                .then_some(definition.id())
        })
        .collect::<BTreeSet<_>>();
    let required_targets = BTreeSet::from([
        EQUIPMENT_COPPER_REINFORCED_PICK,
        EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
        EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
        EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK,
        EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
        EQUIPMENT_COPPER_REINFORCED_STONE_ROTARY_QUERN,
        EQUIPMENT_COPPER_REINFORCED_GEOLOGICAL_HAMMER,
        EQUIPMENT_COPPER_REINFORCED_WOODWORKING_ADZE,
    ]);
    assert!(required_targets.is_subset(&compatible_targets));
    for target in required_targets {
        let definition = registries
            .equipment()
            .get_equipment(target)
            .unwrap_or_else(|| panic!("required primitive reinforcement target disappeared"));
        let maintenance = definition
            .maintenance_profile()
            .unwrap_or_else(|| panic!("required primitive reinforcement target lost maintenance"));
        assert!(
            maintenance.is_component_replacement(),
            "primitive reinforcement target must localize wear to an embodied replaceable component"
        );
    }

    let base_flywheel = registries
        .energy()
        .get_store(ENERGY_STONE_FLYWHEEL_DRIVE)
        .unwrap_or_else(|| panic!("stone flywheel disappeared"));
    let reinforced_flywheel = registries
        .energy()
        .get_store(ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE)
        .unwrap_or_else(|| panic!("reinforced flywheel disappeared"));
    let upgrade = reinforced_flywheel
        .upgrade_profile()
        .unwrap_or_else(|| panic!("reinforced flywheel lost its additive upgrade route"));
    assert_eq!(upgrade.from(), ENERGY_STONE_FLYWHEEL_DRIVE);
    assert_eq!(
        upgrade.additions().inputs(),
        std::slice::from_ref(canonical)
    );
    assert_eq!(base_flywheel.carrier(), reinforced_flywheel.carrier());
    assert!(reinforced_flywheel.capacity() > base_flywheel.capacity());
    assert!(reinforced_flywheel.max_input_power() >= base_flywheel.max_input_power());
    assert!(reinforced_flywheel.max_output_power() >= base_flywheel.max_output_power());
}

//! Exact built-in process-topology coverage for resolver families, providers, and energy roles.

use crate::content::{
    ENERGY_ELECTRICAL_BUFFER, ENERGY_THERMAL_SINK, EQUIPMENT_CASTING_MOLD,
    EQUIPMENT_COPPER_REINFORCED_WOODWORKING_ADZE, EQUIPMENT_ELECTRIC_FURNACE,
    EQUIPMENT_STONE_WOODWORKING_ADZE, EQUIPMENT_TIMBER_FRAME_COMMINUTION_MILL,
    EQUIPMENT_TIMBER_FRAME_SAW_BENCH, EQUIPMENT_TIMBER_ORE_DRESSING_TABLE,
    EQUIPMENT_TIMBER_TREADLE_HAMMER, PROCESS_CAST_PURE_COPPER,
    PROCESS_COLD_WORK_COPPER_REINFORCEMENT, PROCESS_COLD_WORK_COPPER_SAW_BLADE,
    PROCESS_COLD_WORK_COPPER_SCRAP_REINFORCEMENT, PROCESS_CRUSH_ORE, PROCESS_GRIND_CRUSHED_ORE,
    PROCESS_HAND_SORT_NATIVE_COPPER, PROCESS_KNAP_STONE_TOOL, PROCESS_MELT_PURE_COPPER,
    PROCESS_PIERCE_COPPER_SCREEN_PLATE, PROCESS_SAW_WOOD_BOARDS, PROCESS_SCREEN_CRUSHED_ORE,
    PROCESS_SEPARATE_NATIVE_COPPER, PROCESS_SHAPE_WOOD_BOARDS, build_registries,
};
use crate::energy::EnergyCarrier;

use super::{ProcessEnergyRole, ProcessEquipmentRole, ProcessExecutionFamily};

#[test]
fn every_builtin_process_has_one_derived_execution_topology() {
    let registries = build_registries();
    for process in registries.production().definitions() {
        let topology = registries
            .process_topology(process.id())
            .unwrap_or_else(|| {
                panic!(
                    "built-in process {} has no physical execution topology",
                    process.id().value()
                )
            });
        match topology.equipment_role() {
            ProcessEquipmentRole::None => assert!(topology.nominal_providers().is_empty()),
            ProcessEquipmentRole::Optional | ProcessEquipmentRole::Required => {
                assert!(!topology.nominal_providers().is_empty());
            }
        }
        match topology.energy_role() {
            ProcessEnergyRole::None => assert!(topology.compatible_energy_stores().is_empty()),
            ProcessEnergyRole::Supply(_) | ProcessEnergyRole::Sink(_) => {
                assert!(!topology.compatible_energy_stores().is_empty());
            }
        }
    }
}

#[test]
fn manual_craft_topology_exposes_optional_and_required_tool_providers() {
    let registries = build_registries();
    let knapping = registries
        .process_topology(PROCESS_KNAP_STONE_TOOL)
        .unwrap_or_else(|| panic!("stone-knapping process topology disappeared"));
    assert_eq!(
        knapping.execution_family(),
        ProcessExecutionFamily::ManualCraft
    );
    assert_eq!(knapping.equipment_role(), ProcessEquipmentRole::None);
    assert!(knapping.nominal_providers().is_empty());

    let hewing = registries
        .process_topology(PROCESS_SHAPE_WOOD_BOARDS)
        .unwrap_or_else(|| panic!("board-hewing process topology disappeared"));
    assert_eq!(
        hewing.execution_family(),
        ProcessExecutionFamily::ManualCraft
    );
    assert_eq!(hewing.equipment_role(), ProcessEquipmentRole::Optional);
    assert_eq!(
        hewing.nominal_providers(),
        &[
            EQUIPMENT_STONE_WOODWORKING_ADZE,
            EQUIPMENT_COPPER_REINFORCED_WOODWORKING_ADZE,
        ]
    );

    let sawing = registries
        .process_topology(PROCESS_SAW_WOOD_BOARDS)
        .unwrap_or_else(|| panic!("board-sawing process topology disappeared"));
    assert_eq!(
        sawing.execution_family(),
        ProcessExecutionFamily::ManualCraft
    );
    assert_eq!(sawing.equipment_role(), ProcessEquipmentRole::Required);
    assert_eq!(
        sawing.nominal_providers(),
        &[EQUIPMENT_TIMBER_FRAME_SAW_BENCH]
    );

    for process in [
        PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
        PROCESS_COLD_WORK_COPPER_SCRAP_REINFORCEMENT,
        PROCESS_COLD_WORK_COPPER_SAW_BLADE,
    ] {
        let hammering = registries
            .process_topology(process)
            .unwrap_or_else(|| panic!("copper hammering process {} disappeared", process.value()));
        assert_eq!(
            hammering.execution_family(),
            ProcessExecutionFamily::ManualCraft
        );
        assert_eq!(hammering.equipment_role(), ProcessEquipmentRole::Optional);
        assert_eq!(
            hammering.nominal_providers(),
            &[EQUIPMENT_TIMBER_TREADLE_HAMMER]
        );
        assert_eq!(hammering.energy_role(), ProcessEnergyRole::None);
    }

    let piercing = registries
        .process_topology(PROCESS_PIERCE_COPPER_SCREEN_PLATE)
        .unwrap_or_else(|| panic!("copper screen piercing topology disappeared"));
    assert_eq!(piercing.equipment_role(), ProcessEquipmentRole::None);
    assert!(piercing.nominal_providers().is_empty());
}

#[test]
fn settlement_ore_dressing_equipment_is_discovered_for_each_authored_role() {
    let registries = build_registries();

    for (process, equipment) in [
        (PROCESS_CRUSH_ORE, EQUIPMENT_TIMBER_FRAME_COMMINUTION_MILL),
        (
            PROCESS_GRIND_CRUSHED_ORE,
            EQUIPMENT_TIMBER_FRAME_COMMINUTION_MILL,
        ),
        (
            PROCESS_SCREEN_CRUSHED_ORE,
            EQUIPMENT_TIMBER_ORE_DRESSING_TABLE,
        ),
        (
            PROCESS_SEPARATE_NATIVE_COPPER,
            EQUIPMENT_TIMBER_ORE_DRESSING_TABLE,
        ),
    ] {
        let topology = registries
            .process_topology(process)
            .unwrap_or_else(|| panic!("settlement ore process {} disappeared", process.value()));
        assert_eq!(topology.equipment_role(), ProcessEquipmentRole::Required);
        assert!(
            topology.nominal_providers().contains(&equipment),
            "process {} must discover settlement equipment {} through its authored capabilities",
            process.value(),
            equipment.value()
        );
    }

    let screening = registries
        .process_topology(PROCESS_SCREEN_CRUSHED_ORE)
        .unwrap_or_else(|| panic!("screening topology disappeared"));
    assert!(
        !screening
            .nominal_providers()
            .contains(&EQUIPMENT_TIMBER_FRAME_COMMINUTION_MILL),
        "the comminution mill must not silently acquire an ore-dressing role"
    );
    let crushing = registries
        .process_topology(PROCESS_CRUSH_ORE)
        .unwrap_or_else(|| panic!("crushing topology disappeared"));
    assert!(
        !crushing
            .nominal_providers()
            .contains(&EQUIPMENT_TIMBER_ORE_DRESSING_TABLE),
        "the ore-dressing table must not silently acquire a comminution role"
    );
}

#[test]
fn process_topology_preserves_manual_machine_and_energy_direction_semantics() {
    let registries = build_registries();

    let manual = registries
        .process_topology(PROCESS_HAND_SORT_NATIVE_COPPER)
        .unwrap_or_else(|| panic!("hand-sort process topology disappeared"));
    assert_eq!(
        manual.execution_family(),
        ProcessExecutionFamily::ManualSeparation
    );
    assert_eq!(manual.equipment_role(), ProcessEquipmentRole::None);
    assert_eq!(manual.energy_role(), ProcessEnergyRole::None);
    assert!(manual.nominal_providers().is_empty());
    assert!(manual.compatible_energy_stores().is_empty());

    let melting = registries
        .process_topology(PROCESS_MELT_PURE_COPPER)
        .unwrap_or_else(|| panic!("copper-melting process topology disappeared"));
    assert_eq!(melting.execution_family(), ProcessExecutionFamily::Melting);
    assert_eq!(
        melting.energy_role(),
        ProcessEnergyRole::Supply(EnergyCarrier::Electrical)
    );
    assert_eq!(melting.equipment_role(), ProcessEquipmentRole::Required);
    assert_eq!(melting.nominal_providers(), &[EQUIPMENT_ELECTRIC_FURNACE]);
    assert_eq!(
        melting.compatible_energy_stores(),
        &[ENERGY_ELECTRICAL_BUFFER]
    );

    let casting = registries
        .process_topology(PROCESS_CAST_PURE_COPPER)
        .unwrap_or_else(|| panic!("copper-casting process topology disappeared"));
    assert_eq!(casting.execution_family(), ProcessExecutionFamily::Casting);
    assert_eq!(
        casting.energy_role(),
        ProcessEnergyRole::Sink(EnergyCarrier::Thermal)
    );
    assert_eq!(casting.nominal_providers(), &[EQUIPMENT_CASTING_MOLD]);
    assert_eq!(casting.compatible_energy_stores(), &[ENERGY_THERMAL_SINK]);
}

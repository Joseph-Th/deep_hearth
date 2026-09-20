//! Contract tests for manual crafting and shaping.

use super::*;
use crate::content::{
    EQUIPMENT_STONE_WOODWORKING_ADZE, EQUIPMENT_TIMBER_TREADLE_HAMMER, FORM_BOARD, FORM_CHEST_BODY,
    FORM_CHIP, FORM_CRUSHED, FORM_DOUBLE_WALL_CHEST_BODY, FORM_HANDLE, FORM_INGOT, FORM_LOG,
    FORM_LUMP, FORM_NATIVE_METAL, FORM_ORE, FORM_REINFORCEMENT, FORM_SCRAP, FORM_TOOL,
    MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD, PROCESS_ASSEMBLE_DOUBLE_WALL_TIMBER_CHEST,
    PROCESS_ASSEMBLE_TIMBER_CHEST, PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
    PROCESS_COLD_WORK_COPPER_SCRAP_REINFORCEMENT, PROCESS_KNAP_STONE_TOOL,
    PROCESS_REKNAP_STONE_SCRAP_TOOL, PROCESS_SHAPE_WOOD_BOARDS, PROSPECTING_FIELD_INSPECTION,
    STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
};
use crate::core::quantity::{Area, Energy, Force, Length, Mass, Temperature, Volume};
use crate::core::state::{StateValidationError, validate_loaded_state};
use crate::core::time::{TickSpan, WorldSeed};
use crate::equipment::validate_assemble_equipment;
use crate::geology::{FieldProspectingRequest, validate_start_field_prospecting};
use crate::inventory::{
    MaterialLotId, MaterialLotSelection, add_solid_stockpile_for_test,
    deposit_composed_lot_for_test, deposit_lot_for_test, validate_mount_stockpile,
    validate_unmount_stockpile,
};
use crate::labor::{
    PlayerWorkCommitError, PlayerWorkStartError, PlayerWorkValidationError,
    calculate_player_work_resource_budget,
};
use crate::maintenance::Condition;
use crate::material::{CommodityKey, CompositionComponent, MaterialComposition};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::production::{
    ProcessDefinition, ProcessId, ProductionAvailabilityChange, ProductionRegistry,
    ProductionSuspensionReason, StartProcessError, validate_start_process,
};
use crate::simulation::advance_tick;
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::structural::{
    StructuralElementId, StructuralLifecycle, StructuralLoadKind, add_structural_element,
    materialize_structural_element_for_test, validate_activate_structural_element,
    validate_set_structural_load,
};
use crate::survival::{
    SurvivalExertion, Vitality, assess_survival, initialize_player_survival, player_record,
};

fn stone_lump() -> CommodityKey {
    CommodityKey::new(MATERIAL_STONE, FORM_LUMP)
}

#[path = "mod_tests/tooling.rs"]
mod tooling;

#[path = "mod_tests/material_routes.rs"]
mod material_routes;

fn make_fixture() -> (
    Registries,
    AppState,
    StockpileId,
    MaterialLotId,
    StockpileId,
) {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xC4AF_7001));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("manual craft survival initialization failed: {error}"));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2_000_000))
        .unwrap_or_else(|error| panic!("manual craft source fixture failed: {error}"));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2_000_000))
        .unwrap_or_else(|error| panic!("manual craft destination fixture failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        stone_lump(),
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("manual craft stone fixture failed: {error}"));
    (registries, state, source, lot, destination)
}

fn make_next_tick_fatal(registries: &Registries, state: &mut AppState) {
    let physiology = registries.survival().physiology();
    let player = state
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("fatal manual-craft fixture player disappeared"));
    let expected_revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        expected_revision,
        expected_revision + 1,
        player_record(
            Energy::ZERO,
            player.hydration(),
            Vitality::from_parts_per_million_unchecked(
                physiology.starvation_vitality_loss_ppm_per_tick(),
            ),
            player.nutrition(),
            player.vitality_recovery_remainder(),
        ),
    );
}

#[path = "mod_tests/death.rs"]
mod death;

fn active_stockpile_support(registries: &Registries, state: &mut AppState) -> StructuralElementId {
    active_stockpile_support_at(registries, state, 0)
}

fn active_stockpile_support_at(
    registries: &Registries,
    state: &mut AppState,
    x: i64,
) -> StructuralElementId {
    let max_x = x
        .checked_add(1)
        .unwrap_or_else(|| panic!("manual craft support x-coordinate overflowed"));
    let bounds = VoxelBounds::new(VoxelCoord::new(x, 0, 0), VoxelCoord::new(max_x, 1, 1))
        .unwrap_or_else(|error| panic!("manual craft support bounds failed: {error}"));
    let support = add_structural_element(
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
    )
    .unwrap_or_else(|error| panic!("manual craft support allocation failed: {error}"));
    materialize_structural_element_for_test(registries, state, support, FORM_LOG);
    let _ = validate_activate_structural_element(registries, state, support)
        .unwrap_or_else(|error| panic!("manual craft support activation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("manual craft support activation commit failed: {error}"));
    support
}

#[path = "mod_tests/suspension.rs"]
mod suspension;

#[path = "mod_tests/lifecycle.rs"]
mod lifecycle;

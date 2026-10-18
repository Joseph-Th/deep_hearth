//! Contracts for persisted quick-access selection over canonical carried custody.

use super::*;

use crate::content::{
    EQUIPMENT_STONE_PICK, FORM_HANDLE, FORM_LUMP, FORM_TOOL, MATERIAL_STONE, MATERIAL_WOOD,
    build_registries,
};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::{AppState, StateValidationError};
use crate::equipment::validate_assemble_equipment;
use crate::inventory::{add_solid_stockpile_for_test, deposit_lot_for_test};
use crate::logistics::{
    LogisticsValidationError, PLAYER_QUICK_ACCESS_SLOT_COUNT, place_equipment_for_fixture,
    validate_drop_equipment, validate_pickup_equipment,
};
use crate::material::CommodityKey;
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::spatial::VoxelCoord;
use crate::survival::{Vitality, player_record_with_thermal_comfort};

const ORIGIN: VoxelCoord = VoxelCoord::new(0, 0, 0);

fn selected_pick_fixture() -> (
    crate::registry::Registries,
    AppState,
    crate::equipment::EquipmentId,
) {
    let registries = build_registries();
    let mut state = AppState::new();
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("selection assembly stockpile failed: {error}"));
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
            source,
            commodity,
            mass,
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("selection assembly material failed: {error}"));
    }
    let pick = validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_PICK, source)
        .unwrap_or_else(|error| panic!("selection pick assembly failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("selection pick assembly commit failed: {error}"));
    place_equipment_for_fixture(&mut state, pick, ORIGIN);
    validate_admit_player(&registries, &state, ORIGIN)
        .unwrap_or_else(|error| panic!("selection player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("selection player admission commit failed: {error}"));
    validate_pickup_equipment(&registries, &state, pick)
        .unwrap_or_else(|error| panic!("selection pickup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("selection pickup commit failed: {error}"));
    (registries, state, pick)
}

fn kill_selected_player(state: &mut AppState) {
    let player = state
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("quick-access death fixture missing survival player"));
    let revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        revision,
        revision
            .checked_add(1)
            .unwrap_or_else(|| panic!("death fixture revision overflowed")),
        player_record_with_thermal_comfort(
            player.metabolic_energy(),
            player.hydration(),
            Vitality::ZERO,
            player.nutrition(),
            player.vitality_recovery_remainder(),
            player.thermal_comfort_ppm(),
        ),
    );
}

#[test]
fn quick_access_rejects_dead_player_even_for_idempotent_selection() {
    let (registries, mut state, pick) = selected_pick_fixture();
    kill_selected_player(&mut state);
    let before = state.clone();
    assert_eq!(
        validate_select_quick_access_slot(&state, PlayerQuickAccessSlot::FIRST).err(),
        Some(PlayerQuickAccessSelectionError::PlayerDead)
    );
    assert_eq!(
        validate_set_active_selection(&registries, &state, None).err(),
        Some(PlayerActiveSelectionError::PlayerDead)
    );
    assert_eq!(
        validate_set_quick_access_slot(
            &registries,
            &state,
            PlayerQuickAccessSlot::FIRST,
            Some(PlayerActiveSelection::equipment(pick)),
        )
        .err(),
        Some(PlayerActiveSelectionError::PlayerDead)
    );
    assert_eq!(state, before);
}

#[test]
fn quick_access_validations_reject_survival_change_before_commit() {
    let (registries, mut state, pick) = selected_pick_fixture();
    let second = PlayerQuickAccessSlot::new(1)
        .unwrap_or_else(|| panic!("second quick-access slot disappeared"));
    let slot_change = validate_select_quick_access_slot(&state, second)
        .unwrap_or_else(|error| panic!("slot change validation failed: {error}"));
    let equipment_binding = validate_set_quick_access_slot(
        &registries,
        &state,
        second,
        Some(PlayerActiveSelection::equipment(pick)),
    )
    .unwrap_or_else(|error| panic!("equipment binding validation failed: {error}"));
    let unchanged_selection =
        validate_select_quick_access_slot(&state, PlayerQuickAccessSlot::FIRST)
            .unwrap_or_else(|error| panic!("idempotent selection validation failed: {error}"));
    let expected = state.survival().revision();
    kill_selected_player(&mut state);
    let actual = state.survival().revision();
    let before = state.clone();
    assert_eq!(
        slot_change.commit(&mut state),
        Err(PlayerQuickAccessSelectionCommitError::StaleSurvivalRevision { expected, actual })
    );
    assert_eq!(
        unchanged_selection.commit(&mut state),
        Err(PlayerQuickAccessSelectionCommitError::StaleSurvivalRevision { expected, actual })
    );
    assert_eq!(
        equipment_binding.commit(&mut state),
        Err(PlayerActiveSelectionCommitError::StaleSurvivalRevision { expected, actual })
    );
    assert_eq!(state, before);
}

#[test]
fn equipment_selection_persists_and_clears_when_equipment_leaves_custody() {
    let (registries, mut state, pick) = selected_pick_fixture();
    let selection = PlayerActiveSelection::equipment(pick);
    assert_eq!(state.logistics().active_selection(), None);

    validate_set_active_selection(&registries, &state, Some(selection))
        .unwrap_or_else(|error| panic!("active selection validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("active selection commit failed: {error}"));
    assert_eq!(state.logistics().active_selection(), Some(selection));

    let revision = state.logistics().revision();
    validate_set_active_selection(&registries, &state, Some(selection))
        .unwrap_or_else(|error| panic!("idempotent selection validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("idempotent selection commit failed: {error}"));
    assert_eq!(state.logistics().revision(), revision);

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("active selection serialization failed: {error}"));
    let loaded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("active selection decode failed: {error}"));
    let loaded = loaded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("active selection trusted load failed: {error}"));
    assert_eq!(loaded, state);
    assert_eq!(loaded.logistics().active_selection(), Some(selection));

    validate_drop_equipment(&registries, &state, pick)
        .unwrap_or_else(|error| panic!("selected-equipment drop failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("selected-equipment drop commit failed: {error}"));
    assert_eq!(state.logistics().active_selection(), None);
}

#[test]
fn material_selection_is_semantic_and_survives_persistence_without_lot_identity() {
    let (registries, mut state, _pick) = selected_pick_fixture();
    let carried = state
        .logistics()
        .player()
        .map(|player| player.carried_stockpile())
        .unwrap_or_else(|| panic!("material selection fixture lost player"));
    let commodity = CommodityKey::new(MATERIAL_STONE, FORM_LUMP);
    let _ = deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        commodity,
        Mass::from_milligrams(300_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("material selection deposit failed: {error}"));
    let selection = PlayerActiveSelection::material(commodity);

    validate_set_active_selection(&registries, &state, Some(selection))
        .unwrap_or_else(|error| panic!("material selection validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("material selection commit failed: {error}"));
    assert_eq!(state.logistics().active_selection(), Some(selection));

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("material selection serialization failed: {error}"));
    let loaded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("material selection decode failed: {error}"));
    let loaded = loaded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("material selection trusted load failed: {error}"));
    assert_eq!(loaded.logistics().active_selection(), Some(selection));
}

#[test]
fn nine_slot_quick_access_bar_persists_semantic_bindings_and_selected_slot() {
    let (registries, mut state, pick) = selected_pick_fixture();
    let carried = state
        .logistics()
        .player()
        .map(|player| player.carried_stockpile())
        .unwrap_or_else(|| panic!("quick-access persistence fixture lost player"));
    let commodity = CommodityKey::new(MATERIAL_STONE, FORM_LUMP);
    let _ = deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        commodity,
        Mass::from_milligrams(300_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("quick-access persistence material failed: {error}"));
    let equipment_slot = PlayerQuickAccessSlot::new(1)
        .unwrap_or_else(|| panic!("quick-access equipment slot disappeared"));
    let material_slot = PlayerQuickAccessSlot::new(4)
        .unwrap_or_else(|| panic!("quick-access material slot disappeared"));

    validate_set_quick_access_slot(
        &registries,
        &state,
        equipment_slot,
        Some(PlayerActiveSelection::equipment(pick)),
    )
    .unwrap_or_else(|error| panic!("quick-access equipment binding failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("quick-access equipment binding commit failed: {error}"));
    validate_set_quick_access_slot(
        &registries,
        &state,
        material_slot,
        Some(PlayerActiveSelection::material(commodity)),
    )
    .unwrap_or_else(|error| panic!("quick-access material binding failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("quick-access material binding commit failed: {error}"));
    validate_select_quick_access_slot(&state, material_slot)
        .unwrap_or_else(|error| panic!("quick-access slot selection failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("quick-access slot selection commit failed: {error}"));

    assert_eq!(
        state.logistics().quick_access_slots().count(),
        PLAYER_QUICK_ACCESS_SLOT_COUNT
    );
    assert_eq!(
        state.logistics().quick_access_slot(equipment_slot),
        Some(PlayerActiveSelection::equipment(pick))
    );
    assert_eq!(
        state.logistics().quick_access_slot(material_slot),
        Some(PlayerActiveSelection::material(commodity))
    );
    assert_eq!(state.logistics().active_quick_access_slot(), material_slot);
    assert_eq!(
        state.logistics().active_selection(),
        Some(PlayerActiveSelection::material(commodity))
    );

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("quick-access persistence serialization failed: {error}"));
    let loaded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("quick-access persistence decode failed: {error}"));
    let loaded = loaded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("quick-access persistence trusted load failed: {error}"));
    assert_eq!(loaded, state);
}

#[test]
fn quick_access_binding_moves_instead_of_duplicates_and_drop_clears_inactive_equipment_slot() {
    let (registries, mut state, pick) = selected_pick_fixture();
    let first = PlayerQuickAccessSlot::new(1)
        .unwrap_or_else(|| panic!("quick-access first slot disappeared"));
    let second = PlayerQuickAccessSlot::new(6)
        .unwrap_or_else(|| panic!("quick-access second slot disappeared"));
    let selection = PlayerActiveSelection::equipment(pick);

    validate_set_quick_access_slot(&registries, &state, first, Some(selection))
        .unwrap_or_else(|error| panic!("quick-access first binding failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("quick-access first binding commit failed: {error}"));
    validate_set_quick_access_slot(&registries, &state, second, Some(selection))
        .unwrap_or_else(|error| panic!("quick-access move binding failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("quick-access move binding commit failed: {error}"));

    assert_eq!(state.logistics().quick_access_slot(first), None);
    assert_eq!(state.logistics().quick_access_slot(second), Some(selection));
    assert_eq!(
        state.logistics().active_quick_access_slot(),
        PlayerQuickAccessSlot::FIRST
    );
    assert_eq!(state.logistics().active_selection(), None);

    validate_drop_equipment(&registries, &state, pick)
        .unwrap_or_else(|error| panic!("inactive-slot equipment drop failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("inactive-slot equipment drop commit failed: {error}"));
    assert_eq!(state.logistics().quick_access_slot(second), None);
    assert_eq!(
        state.logistics().active_quick_access_slot(),
        PlayerQuickAccessSlot::FIRST
    );
}

#[test]
fn selection_rejects_noncarried_equipment_and_forged_equipment_persistence() {
    let (registries, mut state, pick) = selected_pick_fixture();
    validate_drop_equipment(&registries, &state, pick)
        .unwrap_or_else(|error| panic!("noncarried selection drop failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("noncarried selection drop commit failed: {error}"));
    assert_eq!(
        validate_set_active_selection(
            &registries,
            &state,
            Some(PlayerActiveSelection::equipment(pick)),
        )
        .err(),
        Some(PlayerActiveSelectionError::EquipmentNotCarried { equipment: pick })
    );

    validate_pickup_equipment(&registries, &state, pick)
        .unwrap_or_else(|error| panic!("forged-selection pickup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("forged-selection pickup commit failed: {error}"));
    validate_set_active_selection(
        &registries,
        &state,
        Some(PlayerActiveSelection::equipment(pick)),
    )
    .unwrap_or_else(|error| panic!("forged-selection baseline validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("forged-selection baseline commit failed: {error}"));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("forged-selection serialization failed: {error}"));
    encoded["state"]["systems"]["logistics"]["carried_equipment"] = serde_json::json!([]);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("forged-selection decode failed: {error}"));
    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Logistics(
            LogisticsValidationError::QuickAccessEquipmentNotCarried {
                slot: PlayerQuickAccessSlot::FIRST,
                equipment: pick,
            }
        )))
    );
}

#[test]
fn trusted_load_rejects_out_of_range_and_duplicate_quick_access_state() {
    let (registries, mut state, pick) = selected_pick_fixture();
    validate_set_active_selection(
        &registries,
        &state,
        Some(PlayerActiveSelection::equipment(pick)),
    )
    .unwrap_or_else(|error| panic!("quick-access forged-state baseline failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("quick-access forged-state baseline commit failed: {error}"));
    let base = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("quick-access forged-state serialization failed: {error}"));

    let mut invalid_active = base.clone();
    invalid_active["state"]["systems"]["logistics"]["active_quick_access_slot"] =
        serde_json::json!(9);
    let invalid_active: LoadedSaveEnvelope = serde_json::from_value(invalid_active)
        .unwrap_or_else(|error| panic!("out-of-range quick-access slot should decode: {error}"));
    assert_eq!(
        invalid_active.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Logistics(
            LogisticsValidationError::QuickAccessSlotOutOfRange { slot: 9 }
        )))
    );

    let mut duplicate = base;
    duplicate["state"]["systems"]["logistics"]["quick_access_slots"][1] =
        duplicate["state"]["systems"]["logistics"]["quick_access_slots"][0].clone();
    let duplicate: LoadedSaveEnvelope = serde_json::from_value(duplicate)
        .unwrap_or_else(|error| panic!("duplicate quick-access state should decode: {error}"));
    assert_eq!(
        duplicate.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Logistics(
            LogisticsValidationError::DuplicateQuickAccessSelection {
                first: PlayerQuickAccessSlot::FIRST,
                second: PlayerQuickAccessSlot::new(1)
                    .unwrap_or_else(|| panic!("second quick-access slot disappeared")),
            }
        )))
    );
}

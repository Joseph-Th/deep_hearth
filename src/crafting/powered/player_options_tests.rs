//! Player-powered-work starts must deny dead actors before inspecting input stock.

use super::*;
use crate::content::{PROCESS_SHAPE_WOOD_BOARDS, build_registries};
use crate::core::state::AppState;
use crate::player::validate_admit_player;
use crate::spatial::VoxelCoord;
use crate::survival::{Vitality, player_record_with_thermal_comfort};

#[test]
fn dead_player_cannot_reuse_a_powered_work_option_to_inspect_inventory() {
    let registries = build_registries();
    let mut state = AppState::new();
    let source = validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    // A row is never an authorization token. Its feasibility must be rechecked
    // on start, but a dead actor must fail before that inventory/machine inspection.
    let option = PlayerPoweredCraftOption {
        transform: PROCESS_SHAPE_WOOD_BOARDS,
        process: PROCESS_SHAPE_WOOD_BOARDS,
        source,
        equipment: EquipmentId::new(1),
        energy_store: EnergyStoreId::new(1),
        batch_mass: Mass::from_milligrams(1_000_000),
        available_batches: 1,
        required_energy: Energy::ZERO,
        duration: TickSpan::new(1),
        charge_ready: false,
    };
    assert!(matches!(
        validate_start_player_powered_craft_option(&registries, &state, option, source),
        Err(PlayerPoweredCraftOptionError::Input(_))
    ));
    let before = *state.survival().player().unwrap();
    let revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record_with_thermal_comfort(
            before.metabolic_energy(),
            before.hydration(),
            Vitality::ZERO,
            before.nutrition(),
            before.vitality_recovery_remainder(),
            before.thermal_comfort_ppm(),
        ),
    );
    let snapshot = state.clone();
    assert_eq!(
        player_powered_craft_options(&registries, &state, source, PROCESS_SHAPE_WOOD_BOARDS),
        Err(PlayerPoweredCraftOptionError::PlayerDead)
    );
    assert_eq!(
        validate_start_player_powered_craft_option(&registries, &state, option, source).err(),
        Some(PlayerPoweredCraftOptionError::PlayerDead)
    );
    assert_eq!(state, snapshot);
}

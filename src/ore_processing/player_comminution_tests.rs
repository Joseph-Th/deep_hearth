//! Player-owned powered ore options cannot outlive the actor that observed them.

use super::*;

use crate::content::{FORM_ORE, MATERIAL_GALENA, PROCESS_CRUSH_ORE, build_registries};
use crate::core::quantity::Temperature;
use crate::core::state::validate_loaded_state;
use crate::inventory::{deposit_lot_for_test, project_material_stacks};
use crate::material::CommodityKey;
use crate::player::validate_admit_player;
use crate::spatial::VoxelCoord;
use crate::survival::{Vitality, player_record_with_thermal_comfort};

#[test]
fn dead_actor_cannot_inspect_a_retained_powered_ore_choice_before_authorization() {
    let registries = build_registries();
    let mut state = AppState::new();
    let carried = validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    let ore = CommodityKey::new(MATERIAL_GALENA, FORM_ORE);
    let mass = Mass::from_milligrams(100_000);
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        ore,
        mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    let stack = project_material_stacks(&registries, &state, carried)
        .unwrap()
        .into_iter()
        .find(|stack| stack.commodity() == ore)
        .unwrap();
    // The original stack and observed quantity are real custody. Unused machine
    // identities are not inspected when the player is no longer alive.
    let option = PlayerComminutionOption {
        stack,
        process: PROCESS_CRUSH_ORE,
        equipment: EquipmentId::new(98_101),
        store: EnergyStoreId::new(98_102),
        destination: carried,
        maximum_mass: mass,
        duration: TickSpan::new(1),
        required_energy: Energy::ZERO,
        deferred_until_boundary: false,
    };
    let player = state.survival().player().copied().unwrap();
    let revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record_with_thermal_comfort(
            player.metabolic_energy(),
            player.hydration(),
            Vitality::ZERO,
            player.nutrition(),
            player.vitality_recovery_remainder(),
            player.thermal_comfort_ppm(),
        ),
    );
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        ore,
        mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let before = state.clone();
    for requested in [Mass::ZERO, mass, Mass::from_milligrams(100_001)] {
        assert!(matches!(
            validate_start_player_comminution_option(&registries, &state, &option, requested),
            Err(PlayerComminutionOptionError::Start(
                StartPlayerProcessError::PlayerDead
            ))
        ));
    }
    assert_eq!(state, before);
}

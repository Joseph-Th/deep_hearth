//! Trusted-load reserve obligations for control-rate excavation.

use super::*;
use crate::core::quantity::Volume;

#[test]
fn off_boundary_excavation_load_requires_all_unsettled_exertion() {
    let (registries, mut state, pick, _) = generated_excavation_fixture();
    for _ in 0..37 {
        let _ = advance_control_tick(&registries, &mut state).unwrap();
    }
    let started_at = state.control_tick();
    let start = validate_start_player_block_excavation(
        &registries,
        &state,
        BlockExcavationRequest::new(
            positive_x_view(),
            MINING_METHOD_HAND_PICK,
            pick,
            Mass::from_milligrams(200_000),
        ),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let duration = TickSpan::new(
        start
            .completes_at()
            .checked_steps_since(started_at)
            .unwrap()
            / CONTROL_TICKS_PER_SIMULATION_TICK,
    );
    let method = registries
        .mining()
        .get_method(MINING_METHOD_HAND_PICK)
        .unwrap();
    let budget = project_survival_resource_budget(
        registries.survival().physiology(),
        method.exertion(),
        duration,
    )
    .unwrap();
    // Twenty control steps have passed, but no slow tick has paid for them.
    for _ in 0..20 {
        let _ = advance_control_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(state.tick(), started_at.simulation_tick());
    let mut save = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    let energy = budget.metabolic_energy();
    let hydration = budget.hydration();
    save["state"]["systems"]["survival"]["player"]["metabolic_energy"] =
        serde_json::to_value(energy).unwrap();
    save["state"]["systems"]["survival"]["player"]["hydration"] =
        serde_json::to_value(hydration).unwrap();
    let load = |value| {
        serde_json::from_value::<LoadedSaveEnvelope>(value)
            .unwrap()
            .into_state(&registries)
    };
    assert!(load(save.clone()).is_ok());

    let short_energy = energy.checked_sub(Energy::from_nanojoules(1)).unwrap();
    let mut underfunded = save.clone();
    underfunded["state"]["systems"]["survival"]["player"]["metabolic_energy"] =
        serde_json::to_value(short_energy).unwrap();
    assert_eq!(
        load(underfunded),
        Err(LoadError::InvalidState(StateValidationError::PlayerWork(
            PlayerWorkValidationError::InsufficientMetabolicEnergy {
                available: short_energy,
                required: energy,
            }
        )))
    );
    let short_hydration = hydration.checked_sub(Volume::from_microliters(1)).unwrap();
    save["state"]["systems"]["survival"]["player"]["hydration"] =
        serde_json::to_value(short_hydration).unwrap();
    assert_eq!(
        load(save),
        Err(LoadError::InvalidState(StateValidationError::PlayerWork(
            PlayerWorkValidationError::InsufficientHydration {
                available: short_hydration,
                required: hydration,
            }
        )))
    );
}

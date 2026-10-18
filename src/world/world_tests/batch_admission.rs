//! Held-tool excavation adapts physical batch size to live admission limits.

use super::*;
use crate::capability::CapabilityValue;
use crate::equipment::{
    degrade_equipment_condition_for_test, resolve_available_equipment_provider,
};
use crate::mining::resolve_mining_physics;

#[test]
fn held_excavation_uses_largest_affordable_batch_and_conserves_mass_after_replay() {
    let (registries, mut state, pick, target) = generated_excavation_fixture();
    let method = registries
        .mining()
        .get_method(MINING_METHOD_HAND_PICK)
        .unwrap_or_else(|| panic!("hand-pick method missing"));
    let provider = resolve_available_equipment_provider(&registries, &state, pick)
        .unwrap_or_else(|error| panic!("held pick unavailable: {error}"));
    let maximum_mass = match provider.get_capability(method.max_batch_mass_capability()) {
        Some(CapabilityValue::Mass(mass)) => mass,
        other => panic!("held pick has no batch mass: {other:?}"),
    }
    .min(
        state
            .world()
            .represented_mass_at(registries.world(), target)
            .unwrap_or_else(|| panic!("target mass missing")),
    );
    let smaller_mass = Mass::from_milligrams(maximum_mass.milligrams() / 2);
    let hardness = registries
        .world()
        .block_for_state(BLOCK_STATE_STONE)
        .and_then(|block| block.excavation())
        .unwrap_or_else(|| panic!("stone excavation definition missing"))
        .hardness();
    let energy_for = |mass| {
        let duration = resolve_mining_physics(
            registries.core().physical_tick_duration(),
            method,
            provider.definition(),
            provider.condition(),
            hardness,
            mass,
        )
        .unwrap_or_else(|error| panic!("pick physics failed: {error}"))
        .duration();
        crate::labor::calculate_player_work_resource_budget(
            registries.survival().physiology(),
            method.exertion(),
            duration,
        )
        .unwrap_or_else(|error| panic!("mining work budget failed: {error:?}"))
        .metabolic_energy()
    };
    let available = energy_for(smaller_mass);
    assert!(available < energy_for(maximum_mass));
    let before = state
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("mining player missing"));
    let revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record(
            available,
            before.hydration(),
            before.vitality(),
            before.nutrition(),
            before.vitality_recovery_remainder(),
        ),
    );
    assert!(matches!(
        validate_start_player_block_excavation(
            &registries,
            &state,
            BlockExcavationRequest::new(
                positive_x_view(),
                MINING_METHOD_HAND_PICK,
                pick,
                maximum_mass
            ),
        ),
        Err(BlockExcavationError::PlayerWork(
            PlayerWorkStartError::InsufficientMetabolicEnergy { .. }
        ))
    ));

    let total_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("mining baseline matter audit failed: {error}"))
        .total();
    let authorized =
        validate_start_player_block_excavation_active(&registries, &state, positive_x_view())
            .unwrap_or_else(|error| panic!("affordable mining admission failed: {error}"));
    let mut committed = state.clone();
    let started = authorized
        .commit(&mut committed)
        .unwrap_or_else(|error| panic!("partial mining commit failed: {error}"));
    assert!(started.excavated_mass() >= smaller_mass);
    assert!(started.excavated_mass() < maximum_mass);
    let next_mass = Mass::from_milligrams(started.excavated_mass().milligrams() + 1);
    assert!(matches!(
        validate_start_player_block_excavation(
            &registries,
            &state,
            BlockExcavationRequest::new(
                positive_x_view(),
                MINING_METHOD_HAND_PICK,
                pick,
                next_mass
            ),
        ),
        Err(BlockExcavationError::PlayerWork(
            PlayerWorkStartError::InsufficientMetabolicEnergy { .. }
        ))
    ));

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &committed))
        .unwrap_or_else(|error| panic!("partial mining save failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("partial mining decode failed: {error}"));
    let mut loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("partial mining load failed: {error}"));
    assert_eq!(loaded, committed);
    while loaded.control_tick() < started.completes_at() {
        let _ = advance_control_tick(&registries, &mut loaded)
            .unwrap_or_else(|error| panic!("partial mining continuation failed: {error}"));
    }
    assert_eq!(loaded.player_work().active(), None);
    assert_eq!(
        calculate_matter_accounting(&loaded)
            .unwrap_or_else(|error| panic!("partial mining matter audit failed: {error}"))
            .total(),
        total_before
    );
}

#[test]
fn held_excavation_uses_remaining_tool_life_instead_of_rejecting_all_work() {
    let (registries, mut state, pick, target) = generated_excavation_fixture();
    degrade_equipment_condition_for_test(&mut state, pick, 999_500);
    let method = registries
        .mining()
        .get_method(MINING_METHOD_HAND_PICK)
        .unwrap_or_else(|| panic!("hand-pick method missing"));
    let provider = resolve_available_equipment_provider(&registries, &state, pick)
        .unwrap_or_else(|error| panic!("worn held pick unavailable: {error}"));
    let maximum_mass = match provider.get_capability(method.max_batch_mass_capability()) {
        Some(CapabilityValue::Mass(mass)) => mass,
        other => panic!("worn pick has no batch mass: {other:?}"),
    }
    .min(
        state
            .world()
            .represented_mass_at(registries.world(), target)
            .unwrap_or_else(|| panic!("worn pick target mass missing")),
    );
    assert!(matches!(
        validate_start_player_block_excavation(
            &registries,
            &state,
            BlockExcavationRequest::new(
                positive_x_view(),
                MINING_METHOD_HAND_PICK,
                pick,
                maximum_mass
            ),
        ),
        Err(BlockExcavationError::Physics(
            crate::mining::MiningPhysicsError::ConditionDuration(_)
        ))
    ));

    let selected =
        validate_start_player_block_excavation_active(&registries, &state, positive_x_view())
            .unwrap_or_else(|error| panic!("worn pick partial admission failed: {error}"));
    let started = selected
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("worn pick partial work commit failed: {error}"));
    assert!(!started.excavated_mass().is_zero());
    assert!(started.excavated_mass() < maximum_mass);
    while state.control_tick() < started.completes_at() {
        let _ = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("worn pick continuation failed: {error}"));
    }
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        state
            .world()
            .represented_mass_at(registries.world(), target),
        Some(
            registries
                .world()
                .represented_mass_for_state(BLOCK_STATE_STONE)
                .unwrap_or_else(|| panic!("stone mass disappeared"))
                .checked_sub(started.excavated_mass())
                .unwrap_or_else(|| panic!("worn pick extracted beyond cell mass"))
        )
    );
}

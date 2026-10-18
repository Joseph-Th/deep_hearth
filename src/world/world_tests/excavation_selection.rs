//! Held-tool excavation method selection across physics and current work admission.

use super::*;
use crate::capability::CapabilityValue;
use crate::equipment::resolve_available_equipment_provider;
use crate::mining::{
    MiningMethodDefinition, MiningMethodId, MiningRegistry, resolve_mining_physics,
};
use crate::registry::{Registries, RegistryDomains, RegistryPresentation};
use crate::survival::SurvivalExertion;

fn with_lower_exertion_pick_method(base: &Registries, id: MiningMethodId) -> Registries {
    let preferred = base
        .mining()
        .get_method(MINING_METHOD_HAND_PICK)
        .unwrap_or_else(|| panic!("preferred mining method missing"));
    let fallback = MiningMethodDefinition::new(
        id,
        "lower-exertion hand pick mining",
        preferred.mass_flow_capability(),
        preferred.max_batch_mass_capability(),
        preferred.max_hardness_capability(),
        preferred.condition_wear_ppm_per_active_tick(),
        SurvivalExertion::new(
            Energy::from_nanojoules(1),
            preferred.exertion().hydration_loss_per_tick(),
        ),
    );
    Registries::new(
        base.schema_version(),
        base.core().clone(),
        RegistryDomains {
            energy: base.energy().clone(),
            fluid: base.fluid().clone(),
            capabilities: base.capabilities().clone(),
            crafting: base.crafting().clone(),
            labor: base.labor().clone(),
            equipment: base.equipment().clone(),
            storage: base.storage().clone(),
            structural: base.structural().clone(),
            world: base.world().clone(),
            materials: base.materials().clone(),
            mining: MiningRegistry::new(base.mining().definitions().cloned().chain([fallback])),
            ore_processing: base.ore_processing().clone(),
            thermal: base.thermal().clone(),
            production: base.production().clone(),
            survival: base.survival().clone(),
            presentation: RegistryPresentation {
                textures: base.textures().clone(),
                shaders: base.shaders().clone(),
            },
        },
    )
}

#[test]
fn direct_excavation_uses_admissible_method_when_preferred_method_is_too_costly() {
    let (base, mut state, pick, target) = generated_excavation_fixture();
    let fallback_id = MiningMethodId::new(10_000);
    let registries = with_lower_exertion_pick_method(&base, fallback_id);
    let preferred = registries
        .mining()
        .get_method(MINING_METHOD_HAND_PICK)
        .unwrap_or_else(|| panic!("preferred mining method missing"));
    let fallback = registries
        .mining()
        .get_method(fallback_id)
        .unwrap_or_else(|| panic!("fallback mining method missing"));

    let provider = resolve_available_equipment_provider(&registries, &state, pick)
        .unwrap_or_else(|error| panic!("held pick provider unavailable: {error}"));
    let batch = match provider.get_capability(preferred.max_batch_mass_capability()) {
        Some(CapabilityValue::Mass(mass)) => mass,
        other => panic!("held pick batch capacity missing: {other:?}"),
    }
    .min(
        state
            .world()
            .represented_mass_at(registries.world(), target)
            .unwrap_or_else(|| panic!("target cell mass missing")),
    );
    let hardness = registries
        .world()
        .block_for_state(BLOCK_STATE_STONE)
        .and_then(|block| block.excavation())
        .unwrap_or_else(|| panic!("targeted stone excavation definition missing"))
        .hardness();
    let duration = resolve_mining_physics(
        registries.core().physical_tick_duration(),
        preferred,
        provider.definition(),
        provider.condition(),
        hardness,
        batch,
    )
    .unwrap_or_else(|error| panic!("preferred mining physics failed: {error}"))
    .duration();
    let budget_for = |method: &MiningMethodDefinition| {
        crate::labor::calculate_player_work_resource_budget(
            registries.survival().physiology(),
            method.exertion(),
            duration,
        )
        .unwrap_or_else(|error| panic!("mining resource budget failed: {error:?}"))
        .metabolic_energy()
    };
    let affordable = budget_for(fallback)
        .checked_add(Energy::from_nanojoules(1))
        .unwrap_or_else(|| panic!("fallback budget exceeds energy representation"));
    assert!(affordable < budget_for(preferred));
    let before = state
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("excavation player missing"));
    let revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record(
            affordable,
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
            BlockExcavationRequest::new(positive_x_view(), MINING_METHOD_HAND_PICK, pick, batch,),
        ),
        Err(BlockExcavationError::PlayerWork(
            PlayerWorkStartError::InsufficientMetabolicEnergy { .. }
        ))
    ));
    let started =
        validate_start_player_block_excavation_active(&registries, &state, positive_x_view())
            .unwrap_or_else(|error| panic!("admissible fallback was rejected: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("admissible fallback commit failed: {error}"));
    assert_eq!(started.target(), target);
    assert!(matches!(
        state.player_work().active(),
        Some(PlayerWork::BlockExcavation { work }) if work.method() == fallback_id
    ));

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("fallback excavation serialization failed: {error}"));
    let loaded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("fallback excavation decode failed: {error}"));
    assert_eq!(
        loaded
            .into_state(&registries)
            .unwrap_or_else(|error| panic!("fallback excavation trusted load failed: {error}")),
        state
    );
}

#[test]
fn direct_excavation_keeps_deterministic_preference_when_both_methods_are_admissible() {
    let (base, mut state, _pick, target) = generated_excavation_fixture();
    let registries = with_lower_exertion_pick_method(&base, MiningMethodId::new(10_000));
    let started =
        validate_start_player_block_excavation_active(&registries, &state, positive_x_view())
            .unwrap_or_else(|error| panic!("ordinary mining selection was rejected: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("ordinary mining start failed: {error}"));
    assert_eq!(started.target(), target);
    assert!(matches!(
        state.player_work().active(),
        Some(PlayerWork::BlockExcavation { work })
            if work.method() == MINING_METHOD_HAND_PICK
    ));
}

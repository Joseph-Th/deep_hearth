//! Contract tests for cross-domain player-work operability.

use super::*;

use crate::content::{MANUAL_POWER_HAND_CRANK, MINING_METHOD_HAND_PICK, build_registries};
use crate::core::quantity::{Energy, Mass, MassFlow, Volume};
use crate::core::time::TickSpan;
use crate::labor::{ManualPowerDefinition, ManualPowerMethodId};
use crate::mining::{MiningMethodDefinition, MiningMethodId};

#[test]
fn built_player_work_routes_are_operable() {
    let _ = build_registries();
}

#[test]
fn manual_power_operability_rejects_zero_effective_metabolic_output() {
    let registries = build_registries();
    let baseline = registries
        .labor()
        .get_manual_power(MANUAL_POWER_HAND_CRANK)
        .copied()
        .unwrap_or_else(|| panic!("built manual-power fixture disappeared"));
    let impossible = ManualPowerDefinition::new(
        ManualPowerMethodId::new(990_001),
        baseline.power_capability(),
        baseline.carrier(),
        1,
        baseline.condition_wear_ppm_per_active_tick(),
        SurvivalExertion::new(Energy::from_nanojoules(1), Volume::from_microliters(1)),
    );

    assert_eq!(
        best_operable_manual_power_full_charge_duration(
            registries.core(),
            registries.equipment(),
            registries.energy(),
            registries.survival().physiology(),
            &impossible,
        ),
        None
    );
}

#[test]
fn manual_power_operability_rejects_token_only_route_that_cannot_fill_a_store() {
    let registries = build_registries();
    let baseline = registries
        .labor()
        .get_manual_power(MANUAL_POWER_HAND_CRANK)
        .copied()
        .unwrap_or_else(|| panic!("built manual-power fixture disappeared"));
    let token_only = ManualPowerDefinition::new(
        ManualPowerMethodId::new(990_003),
        baseline.power_capability(),
        baseline.carrier(),
        baseline.metabolic_efficiency_ppm(),
        baseline.condition_wear_ppm_per_active_tick(),
        SurvivalExertion::new(
            Energy::from_nanojoules(10_000_000),
            Volume::from_microliters(1),
        ),
    );
    let provider_power = registries
        .equipment()
        .definitions()
        .filter(|equipment| !equipment.requires_structural_support())
        .find_map(|equipment| {
            match resolve_equipment_capability(
                equipment,
                Condition::PRISTINE,
                token_only.power_capability(),
            ) {
                Some(CapabilityValue::Power(power)) if !power.is_zero() => Some(power),
                _ => None,
            }
        })
        .unwrap_or_else(|| panic!("built manual-power fixture has no portable provider"));
    let store = registries
        .energy()
        .definitions()
        .find(|store| store.carrier() == token_only.carrier() && !store.max_input_power().is_zero())
        .unwrap_or_else(|| panic!("built manual-power fixture has no compatible store"));
    assert!(
        resolve_manual_power_schedule(
            Energy::from_nanojoules(1),
            provider_power.min(store.max_input_power()),
            registries.core().physical_tick_duration(),
            token_only.maximum_exertion(),
            token_only.metabolic_efficiency_ppm(),
        )
        .is_ok(),
        "regression fixture must preserve the former token-success route"
    );

    assert_eq!(
        best_operable_manual_power_full_charge_duration(
            registries.core(),
            registries.equipment(),
            registries.energy(),
            registries.survival().physiology(),
            &token_only,
        ),
        None
    );
}

#[test]
fn mining_operability_rejects_provider_batch_beyond_full_reserves() {
    let registries = build_registries();
    let baseline = registries
        .mining()
        .get_method(MINING_METHOD_HAND_PICK)
        .unwrap_or_else(|| panic!("built mining fixture disappeared"));
    let physiology = registries.survival().physiology();
    let impossible_energy = Energy::from_nanojoules(
        physiology
            .maximum_metabolic_energy()
            .nanojoules()
            .checked_add(1)
            .unwrap_or_else(|| panic!("test metabolic reserve cannot be incremented")),
    );
    let impossible = MiningMethodDefinition::new(
        MiningMethodId::new(990_002),
        "impossible mining fixture",
        baseline.mass_flow_capability(),
        baseline.max_batch_mass_capability(),
        baseline.max_hardness_capability(),
        baseline.condition_wear_ppm_per_active_tick(),
        SurvivalExertion::new(impossible_energy, Volume::ZERO),
    );

    assert_eq!(
        best_operable_mining_duration(
            registries.core(),
            registries.equipment(),
            physiology,
            &impossible,
        ),
        None
    );
}

#[test]
fn fixed_player_work_rejects_duration_beyond_full_reserves() {
    let registries = build_registries();
    let physiology = registries.survival().physiology();
    let impossible_duration = TickSpan::new(
        physiology
            .maximum_hydration()
            .microliters()
            .saturating_add(1),
    );
    let result = std::panic::catch_unwind(|| {
        assert_player_work_fits_reserves(
            physiology,
            SurvivalExertion::new(Energy::from_nanojoules(1), Volume::from_microliters(1)),
            impossible_duration,
            "test work",
            1,
        );
    });

    assert!(result.is_err());
}

#[test]
fn manual_ore_operability_rejects_maximum_batch_beyond_full_reserves() {
    let registries = build_registries();
    let physiology = registries.survival().physiology();
    let impossible_energy = Energy::from_nanojoules(
        physiology
            .maximum_metabolic_energy()
            .nanojoules()
            .checked_add(1)
            .unwrap_or_else(|| panic!("test metabolic reserve cannot be incremented")),
    );
    let result = std::panic::catch_unwind(|| {
        assert_manual_ore_batch_fits_reserves(
            registries.core(),
            physiology,
            MassFlow::from_milligrams_per_second(1),
            Mass::from_milligrams(1),
            SurvivalExertion::new(impossible_energy, Volume::ZERO),
            "test manual ore process",
            1,
        );
    });

    assert!(result.is_err());
}

//! Contract tests for cross-domain player-work operability.

use super::*;

use crate::content::{MANUAL_POWER_HAND_CRANK, MINING_METHOD_HAND_PICK, build_registries};
use crate::core::quantity::{Energy, Volume};
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
        best_operable_manual_power_duration(
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

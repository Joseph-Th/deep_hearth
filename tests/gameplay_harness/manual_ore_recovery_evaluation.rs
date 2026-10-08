//! Counterfactual review layer for the executed manual ore-recovery fallback.

use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::registry::Registries;
use deep_hearth::survival::assess_survival;

use super::manual_ore_recovery::{ManualOreRecoveryPlan, execute_manual_ore_recovery};
use super::manual_ore_recovery_planning::project_manual_ore_recovery_attention;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ManualOreRecoveryReview {
    pub(super) feed_mass: Mass,
    pub(super) attention_ticks: u64,
    pub(super) recovered_native: Mass,
    pub(super) residue_mass: Mass,
    pub(super) manual_recovery_ppm: u32,
    pub(super) powered_recovery_ppm: u32,
    pub(super) metabolic_cost_nj: u128,
    pub(super) hydration_cost_ul: u64,
}

pub(super) fn evaluate_manual_ore_recovery(
    registries: &Registries,
    decision_state: &AppState,
    plan: ManualOreRecoveryPlan,
) -> ManualOreRecoveryReview {
    let mut state = decision_state.clone();
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("manual ore recovery matter setup failed: {error}"))
        .total();
    let survival_before = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("manual ore recovery player disappeared at decision point"));
    let execution = execute_manual_ore_recovery(registries, &mut state, plan);
    assert_eq!(
        execution.attention_ticks,
        project_manual_ore_recovery_attention(registries, plan.feed_mass),
        "executed manual ore attention diverged from its pre-action projection"
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("manual ore recovery matter audit failed: {error}"))
            .total(),
        matter_before
    );
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("manual ore recovery state audit failed: {error}"));
    let survival_after = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("manual ore recovery player disappeared after work"));
    ManualOreRecoveryReview {
        feed_mass: plan.feed_mass,
        attention_ticks: execution.attention_ticks,
        recovered_native: execution.recovered_native,
        residue_mass: plan
            .feed_mass
            .checked_sub(execution.recovered_native)
            .unwrap_or_else(|| unreachable!("manual ore recovery output is bounded by feed")),
        manual_recovery_ppm: execution.manual_recovery_ppm,
        powered_recovery_ppm: execution.powered_recovery_ppm,
        metabolic_cost_nj: survival_before
            .metabolic_energy()
            .checked_sub(survival_after.metabolic_energy())
            .unwrap_or_else(|| unreachable!("manual ore recovery cannot create metabolic reserve"))
            .nanojoules(),
        hydration_cost_ul: survival_before
            .hydration()
            .checked_sub(survival_after.hydration())
            .unwrap_or_else(|| unreachable!("manual ore recovery cannot create hydration reserve"))
            .microliters(),
    }
}

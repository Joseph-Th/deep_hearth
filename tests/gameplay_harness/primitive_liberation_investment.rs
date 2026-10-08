//! Pre-action investment decision for extending primitive ore liberation capacity.

use deep_hearth::content::{
    ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE, EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
    MANUAL_POWER_HAND_CRANK, PROCESS_CLEAN_NATIVE_COPPER_CONCENTRATE, PROCESS_CONCENTRATE_COPPER,
    PROCESS_CRUSH_ORE, PROCESS_FINE_GRIND_SCREEN_OVERSIZE, PROCESS_GRIND_CRUSHED_ORE,
    PROCESS_REGRIND_COPPER_TAILINGS, PROCESS_SCAVENGE_COPPER_TAILINGS, PROCESS_SCREEN_CRUSHED_ORE,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::registry::Registries;

use super::capital_investment_policy::{CapitalInvestmentPolicy, clears_attention_return};
use super::manual_ore_recovery_planning::project_manual_ore_recovery_attention;
use super::primitive_liberation_generation::{
    LiberationGenerationRole, inherited_progression_condition,
};
use super::primitive_liberation_kit_planning::project_incremental_kit_acquisition_attention;
use super::settlement_power_planning::{ManualPowerSequenceRequest, project_manual_power_sequence};

const LIBERATION_POWERED_ROUTE_STAGES: [deep_hearth::production::ProcessId; 8] = [
    PROCESS_CRUSH_ORE,
    PROCESS_GRIND_CRUSHED_ORE,
    PROCESS_SCREEN_CRUSHED_ORE,
    PROCESS_FINE_GRIND_SCREEN_OVERSIZE,
    PROCESS_CONCENTRATE_COPPER,
    PROCESS_REGRIND_COPPER_TAILINGS,
    PROCESS_SCAVENGE_COPPER_TAILINGS,
    PROCESS_CLEAN_NATIVE_COPPER_CONCENTRATE,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum LiberationExtensionChoice {
    ManualFallback,
    BuildKit,
}

impl LiberationExtensionChoice {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::ManualFallback => "manual-fallback",
            Self::BuildKit => "build-kit",
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct LiberationExtensionPlan {
    pub(super) choice: LiberationExtensionChoice,
    pub(super) manual_campaign_attention: u64,
    pub(super) acquisition_attention: u64,
    pub(super) conservative_charge_attention: u64,
    pub(super) powered_attention_upper: u64,
    pub(super) minimum_return_ppm: u64,
    pub(super) minimum_attention_return: u64,
}

pub(super) fn plan_liberation_extension(
    registries: &Registries,
    seed: u64,
    role: LiberationGenerationRole,
    batch_mass: Mass,
    planned_batches: u64,
) -> LiberationExtensionPlan {
    assert!(planned_batches > 0);
    let manual_per_batch = project_manual_ore_recovery_attention(registries, batch_mass);
    let manual_campaign_attention = manual_per_batch
        .checked_mul(planned_batches)
        .unwrap_or_else(|| panic!("liberation manual campaign projection overflowed"));
    let acquisition = project_incremental_kit_acquisition_attention(registries);
    let provider_condition = inherited_progression_condition(
        registries,
        EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
        seed,
        role,
        0x4352_414E_4B00_0001,
    );
    let store = registries
        .energy()
        .get_store(ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE)
        .unwrap_or_else(|| panic!("liberation inherited flywheel definition disappeared"));
    let stage_count = u64::try_from(LIBERATION_POWERED_ROUTE_STAGES.len())
        .unwrap_or_else(|_| unreachable!("bounded liberation stage count fits u64"));
    let charge_count = stage_count
        .checked_mul(planned_batches)
        .unwrap_or_else(|| panic!("liberation conservative charge count overflowed"));
    let charge_projection = project_manual_power_sequence(
        registries,
        ManualPowerSequenceRequest {
            method: MANUAL_POWER_HAND_CRANK,
            equipment: EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
            starting_condition: provider_condition,
            store: ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE,
            energy_per_charge: store.capacity(),
            charges: charge_count,
        },
        "liberation conservative powered-route charging",
    );
    let powered_attention_upper = acquisition
        .total_ticks
        .checked_add(charge_projection.attention_ticks)
        .unwrap_or_else(|| panic!("liberation powered attention upper bound overflowed"));
    let investment_policy = CapitalInvestmentPolicy::baseline();
    let minimum_return_ppm = investment_policy.minimum_return_ppm();
    let minimum_attention_return =
        investment_policy.minimum_attention_return(0, acquisition.total_ticks);
    let choice = if clears_attention_return(
        manual_campaign_attention,
        powered_attention_upper,
        minimum_attention_return,
    ) {
        LiberationExtensionChoice::BuildKit
    } else {
        LiberationExtensionChoice::ManualFallback
    };
    LiberationExtensionPlan {
        choice,
        manual_campaign_attention,
        acquisition_attention: acquisition.total_ticks,
        conservative_charge_attention: charge_projection.attention_ticks,
        powered_attention_upper,
        minimum_return_ppm,
        minimum_attention_return,
    }
}

#[cfg(test)]
include_primitive_liberation_investment_contract_tests!();

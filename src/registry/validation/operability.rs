//! Registry-wide proof that authored player work has at least one physically executable route.

use crate::core::time::TickSpan;
use crate::labor::calculate_player_work_resource_budget;
use crate::survival::{PhysiologyDefinition, SurvivalExertion};

use super::super::{CoreDefinitions, RegistryDomains};

mod craft;
mod fixed;
mod manual_ore;
mod manual_power;
mod mining;

use craft::validate_manual_craft_operability;
use fixed::validate_fixed_player_work_operability;
use manual_ore::validate_manual_ore_operability;
use manual_power::validate_manual_power_operability;
use mining::validate_mining_operability;

fn assert_player_work_fits_reserves(
    physiology: PhysiologyDefinition,
    exertion: SurvivalExertion,
    duration: TickSpan,
    owner: &str,
    id: u64,
) {
    let budget = calculate_player_work_resource_budget(physiology, exertion, duration)
        .unwrap_or_else(|error| panic!("{owner} {id} work budget overflows: {error:?}"));
    assert!(
        budget.metabolic_energy() <= physiology.maximum_metabolic_energy(),
        "{owner} {id} requires {} nJ but full metabolic reserves hold only {} nJ",
        budget.metabolic_energy().nanojoules(),
        physiology.maximum_metabolic_energy().nanojoules(),
    );
    assert!(
        budget.hydration() <= physiology.maximum_hydration(),
        "{owner} {id} requires {} uL hydration but full hydration reserves hold only {} uL",
        budget.hydration().microliters(),
        physiology.maximum_hydration().microliters(),
    );
}

pub(super) fn validate_player_work_operability(core: &CoreDefinitions, domains: &RegistryDomains) {
    let physiology = domains.survival.physiology();
    validate_manual_power_operability(core, domains, physiology);
    validate_mining_operability(core, domains, physiology);
    validate_manual_craft_operability(core, domains, physiology);
    validate_manual_ore_operability(core, domains, physiology);
    validate_fixed_player_work_operability(domains, physiology);
}

#[cfg(test)]
use manual_ore::assert_manual_ore_batch_fits_reserves;
#[cfg(test)]
use manual_power::best_operable_manual_power_full_charge_duration;
#[cfg(test)]
use mining::best_operable_mining_duration;

#[cfg(test)]
#[path = "operability_tests.rs"]
mod tests;

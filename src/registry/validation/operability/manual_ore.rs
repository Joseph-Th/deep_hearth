//! Proves authored manual ore-processing batches are reserve-feasible and tool-useful.

use crate::core::time::TickSpan;
use crate::equipment::resolve_equipment_mass_flow_schedule;
use crate::labor::calculate_player_work_resource_budget;
use crate::maintenance::Condition;
use crate::ore_processing::{ManualOreProcessProfile, project_manual_ore_duration};
use crate::survival::PhysiologyDefinition;

use super::super::super::{CoreDefinitions, RegistryDomains};
use super::assert_player_work_fits_reserves;

pub(super) fn assert_manual_ore_batch_fits_reserves(
    core: &CoreDefinitions,
    physiology: PhysiologyDefinition,
    profile: ManualOreProcessProfile,
    owner: &str,
    id: u64,
) {
    let duration = project_manual_ore_duration(
        core.physical_tick_duration(),
        profile,
        profile.max_batch_mass(),
    )
    .unwrap_or_else(|error| panic!("{owner} {id} maximum-batch duration failed: {error}"));
    assert_player_work_fits_reserves(physiology, profile.exertion(), duration, owner, id);
}

fn best_operable_manual_ore_equipment_duration(
    core: &CoreDefinitions,
    domains: &RegistryDomains,
    profile: ManualOreProcessProfile,
) -> Option<TickSpan> {
    let equipment_profile = profile.equipment_profile()?;
    domains
        .equipment
        .definitions()
        .filter_map(|equipment| {
            let schedule = resolve_equipment_mass_flow_schedule(
                equipment,
                Condition::PRISTINE,
                equipment_profile.mass_flow_capability(),
                profile.max_batch_mass(),
                core.physical_tick_duration(),
                equipment_profile.condition_wear_ppm_per_active_tick(),
            )
            .ok()?;
            let budget = calculate_player_work_resource_budget(
                domains.survival.physiology(),
                profile.exertion(),
                schedule.duration(),
            )
            .ok()?;
            (budget.metabolic_energy() <= domains.survival.physiology().maximum_metabolic_energy()
                && budget.hydration() <= domains.survival.physiology().maximum_hydration())
            .then_some(schedule.duration())
        })
        .min()
}

fn validate_manual_ore_profile_operability(
    core: &CoreDefinitions,
    domains: &RegistryDomains,
    physiology: PhysiologyDefinition,
    profile: ManualOreProcessProfile,
    owner: &str,
    process: u64,
) {
    assert_manual_ore_batch_fits_reserves(core, physiology, profile, owner, process);
    if profile.equipment_profile().is_none() {
        return;
    }

    let hand = project_manual_ore_duration(
        core.physical_tick_duration(),
        profile,
        profile.max_batch_mass(),
    )
    .unwrap_or_else(|error| panic!("{owner} {process} hand projection failed: {error}"));
    assert!(
        best_operable_manual_ore_equipment_duration(core, domains, profile)
            .is_some_and(|assisted| assisted < hand),
        "{owner} {process} has no pristine optional equipment route faster than hand work"
    );
}

pub(super) fn validate_manual_ore_operability(
    core: &CoreDefinitions,
    domains: &RegistryDomains,
    physiology: PhysiologyDefinition,
) {
    for process in domains.production.definitions() {
        if let Some(definition) = domains.ore_processing.get_manual_comminution(process.id()) {
            validate_manual_ore_profile_operability(
                core,
                domains,
                physiology,
                definition.operating_profile(),
                "manual comminution process",
                u64::from(process.id().value()),
            );
        }
        if let Some(definition) = domains
            .ore_processing
            .get_manual_constituent_separation(process.id())
        {
            validate_manual_ore_profile_operability(
                core,
                domains,
                physiology,
                definition.operating_profile(),
                "manual constituent-separation process",
                u64::from(process.id().value()),
            );
        }
    }
}

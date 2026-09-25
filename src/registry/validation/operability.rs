//! Registry-wide proof that authored player work has at least one physically executable route.

use crate::capability::CapabilityValue;
use crate::core::time::TickSpan;
use crate::crafting::{
    ManualCraftDefinition, ManualCraftEquipmentProfile, resolve_manual_craft_equipment_physics,
};
use crate::energy::EnergyRegistry;
use crate::equipment::{
    EquipmentRegistry, resolve_equipment_capability, resolve_equipment_mass_flow_schedule,
};
use crate::labor::{
    ManualPowerDefinition, calculate_player_work_resource_budget,
    project_manual_power_configuration,
};
use crate::maintenance::Condition;
use crate::mining::{MiningMethodDefinition, resolve_mining_physics};
use crate::ore_processing::{ManualOreProcessProfile, project_manual_ore_duration};
use crate::survival::{PhysiologyDefinition, SurvivalExertion};

use super::super::{CoreDefinitions, RegistryDomains};

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

fn assert_manual_ore_batch_fits_reserves(
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

fn best_operable_manual_craft_equipment_duration(
    core: &CoreDefinitions,
    domains: &RegistryDomains,
    definition: &ManualCraftDefinition,
    profile: ManualCraftEquipmentProfile,
) -> Option<TickSpan> {
    domains
        .equipment
        .definitions()
        .filter_map(|equipment| {
            let Ok(schedule) = resolve_manual_craft_equipment_physics(
                equipment,
                Condition::PRISTINE,
                profile.mass_flow_capability(),
                definition.input_mass(),
                core.physical_tick_duration(),
                profile.condition_wear_ppm_per_active_tick(),
            ) else {
                return None;
            };
            let duration = schedule.duration();
            let physiology = domains.survival.physiology();
            let Ok(budget) =
                calculate_player_work_resource_budget(physiology, definition.exertion(), duration)
            else {
                return None;
            };
            (budget.metabolic_energy() <= physiology.maximum_metabolic_energy()
                && budget.hydration() <= physiology.maximum_hydration())
            .then_some(duration)
        })
        .min()
}

fn best_operable_manual_power_full_charge_duration(
    core: &CoreDefinitions,
    equipment_registry: &EquipmentRegistry,
    energy_registry: &EnergyRegistry,
    physiology: PhysiologyDefinition,
    definition: &ManualPowerDefinition,
) -> Option<TickSpan> {
    equipment_registry
        .definitions()
        .filter_map(|equipment| {
            energy_registry
                .definitions()
                .filter_map(|store| {
                    let projection = project_manual_power_configuration(
                        core,
                        physiology,
                        *definition,
                        equipment,
                        Condition::PRISTINE,
                        store,
                        store.capacity(),
                    )
                    .ok()?;
                    let budget = projection.resource_budget();
                    (budget.metabolic_energy() <= physiology.maximum_metabolic_energy()
                        && budget.hydration() <= physiology.maximum_hydration())
                    .then_some(projection.duration())
                })
                .min()
        })
        .min()
}

fn best_operable_mining_duration(
    core: &CoreDefinitions,
    equipment_registry: &EquipmentRegistry,
    physiology: PhysiologyDefinition,
    definition: &MiningMethodDefinition,
) -> Option<TickSpan> {
    equipment_registry
        .definitions()
        .filter(|equipment| !equipment.requires_structural_support())
        .filter_map(|equipment| {
            let Some(CapabilityValue::Mass(maximum_batch)) = resolve_equipment_capability(
                equipment,
                Condition::PRISTINE,
                definition.max_batch_mass_capability(),
            ) else {
                return None;
            };
            if maximum_batch.is_zero() {
                return None;
            }
            let Some(CapabilityValue::Pressure(maximum_hardness)) = resolve_equipment_capability(
                equipment,
                Condition::PRISTINE,
                definition.max_hardness_capability(),
            ) else {
                return None;
            };
            if maximum_hardness.is_zero() {
                return None;
            }
            let physics = resolve_mining_physics(
                core.physical_tick_duration(),
                definition,
                equipment,
                Condition::PRISTINE,
                maximum_hardness,
                maximum_batch,
            )
            .ok()?;
            let budget = calculate_player_work_resource_budget(
                physiology,
                definition.exertion(),
                physics.duration(),
            )
            .ok()?;
            (budget.metabolic_energy() <= physiology.maximum_metabolic_energy()
                && budget.hydration() <= physiology.maximum_hydration())
            .then_some(physics.duration())
        })
        .min()
}

fn validate_manual_power_operability(
    core: &CoreDefinitions,
    domains: &RegistryDomains,
    physiology: PhysiologyDefinition,
) {
    for definition in domains.labor.manual_power_definitions() {
        assert!(
            best_operable_manual_power_full_charge_duration(
                core,
                &domains.equipment,
                &domains.energy,
                physiology,
                definition,
            )
            .is_some(),
            "manual power method {} has no pristine portable provider and compatible finite store that can be charged from empty to full within condition and survival limits",
            definition.id().value()
        );
    }
}

fn validate_mining_operability(
    core: &CoreDefinitions,
    domains: &RegistryDomains,
    physiology: PhysiologyDefinition,
) {
    for definition in domains.mining.definitions() {
        assert!(
            best_operable_mining_duration(core, &domains.equipment, physiology, definition)
                .is_some(),
            "mining method {} has no pristine portable provider route that can complete one provider-sized batch at that provider's maximum excavation resistance within condition and survival limits",
            definition.id().value()
        );
    }
}

fn validate_manual_craft_operability(
    core: &CoreDefinitions,
    domains: &RegistryDomains,
    physiology: PhysiologyDefinition,
) {
    for definition in domains.crafting.definitions() {
        match definition.equipment_profile() {
            None => assert_player_work_fits_reserves(
                physiology,
                definition.exertion(),
                definition.duration(),
                "manual craft process",
                u64::from(definition.process().value()),
            ),
            Some(profile) => {
                let best_equipment_duration = best_operable_manual_craft_equipment_duration(
                    core, domains, definition, profile,
                );
                if !profile.requires_equipment() {
                    assert_player_work_fits_reserves(
                        physiology,
                        definition.exertion(),
                        definition.duration(),
                        "manual craft process",
                        u64::from(definition.process().value()),
                    );
                    assert!(
                        best_equipment_duration
                            .is_some_and(|duration| duration < definition.duration()),
                        "optional equipment for manual craft process {} has no pristine route faster than the equipment-free {}-tick batch",
                        definition.process().value(),
                        definition.duration().value()
                    );
                }
                assert!(
                    best_equipment_duration.is_some(),
                    "manual craft process {} has no pristine equipment route that can finish one authored batch within condition and survival limits",
                    definition.process().value()
                );
            }
        }
    }
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

fn validate_manual_ore_operability(
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

fn validate_fixed_player_work_operability(
    domains: &RegistryDomains,
    physiology: PhysiologyDefinition,
) {
    for definition in domains.labor.prospecting_definitions() {
        assert_player_work_fits_reserves(
            physiology,
            definition.exertion(),
            definition.duration(),
            "prospecting method",
            u64::from(definition.id().value()),
        );
    }

    for definition in domains.equipment.definitions() {
        let Some(maintenance) = definition.maintenance_profile() else {
            continue;
        };
        assert_player_work_fits_reserves(
            physiology,
            maintenance.exertion(),
            maintenance.required_service_duration(Condition::FAILED),
            "equipment full service",
            u64::from(definition.id().value()),
        );
    }

    for definition in domains.storage.definitions() {
        assert_player_work_fits_reserves(
            physiology,
            definition.dismantle_exertion(),
            definition.dismantle_duration(),
            "storage dismantling",
            u64::from(definition.id().value()),
        );
    }
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
#[path = "operability_tests.rs"]
mod tests;

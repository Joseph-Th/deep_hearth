//! Registry-wide proof that authored player work has at least one physically executable route.

use crate::capability::CapabilityValue;
use crate::core::quantity::{Energy, Pressure};
use crate::core::time::TickSpan;
use crate::crafting::{
    ManualCraftDefinition, ManualCraftEquipmentProfile, resolve_manual_craft_equipment_schedule,
};
use crate::energy::EnergyRegistry;
use crate::equipment::{EquipmentRegistry, resolve_equipment_capability};
use crate::labor::{
    ManualPowerDefinition, calculate_player_work_resource_budget, resolve_manual_power_schedule,
};
use crate::maintenance::{Condition, calculate_usable_condition_after_active_ticks};
use crate::mining::{MiningMethodDefinition, resolve_mining_physics};
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
            let Some(CapabilityValue::MassFlow(flow)) = resolve_equipment_capability(
                equipment,
                Condition::PRISTINE,
                profile.mass_flow_capability(),
            ) else {
                return None;
            };
            let Ok(schedule) = resolve_manual_craft_equipment_schedule(
                flow,
                definition.input_mass(),
                core.physical_tick_duration(),
                profile.condition_wear_ppm_per_active_tick(),
                Condition::PRISTINE,
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

fn best_operable_manual_power_duration(
    core: &CoreDefinitions,
    equipment_registry: &EquipmentRegistry,
    energy_registry: &EnergyRegistry,
    physiology: PhysiologyDefinition,
    definition: &ManualPowerDefinition,
) -> Option<TickSpan> {
    let minimum_output = Energy::from_nanojoules(1);
    equipment_registry
        .definitions()
        .filter(|equipment| !equipment.requires_structural_support())
        .filter_map(|equipment| {
            let Some(CapabilityValue::Power(equipment_power)) = resolve_equipment_capability(
                equipment,
                Condition::PRISTINE,
                definition.power_capability(),
            ) else {
                return None;
            };
            if equipment_power.is_zero() {
                return None;
            }
            energy_registry
                .definitions()
                .filter(|store| {
                    store.carrier() == definition.carrier() && !store.max_input_power().is_zero()
                })
                .filter_map(|store| {
                    let transfer_power = std::cmp::min(equipment_power, store.max_input_power());
                    let schedule = resolve_manual_power_schedule(
                        minimum_output,
                        transfer_power,
                        core.physical_tick_duration(),
                        definition.maximum_exertion(),
                        definition.metabolic_efficiency_ppm(),
                    )
                    .ok()?;
                    let duration = schedule.duration();
                    calculate_usable_condition_after_active_ticks(
                        definition.condition_wear_ppm_per_active_tick(),
                        Condition::PRISTINE,
                        duration,
                    )
                    .ok()?;
                    let budget = calculate_player_work_resource_budget(
                        physiology,
                        schedule.exertion(),
                        duration,
                    )
                    .ok()?;
                    (budget.metabolic_energy() <= physiology.maximum_metabolic_energy()
                        && budget.hydration() <= physiology.maximum_hydration())
                    .then_some(duration)
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
            let physics = resolve_mining_physics(
                core.physical_tick_duration(),
                definition,
                equipment,
                Condition::PRISTINE,
                Pressure::from_pascals(1),
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

pub(super) fn validate_player_work_operability(core: &CoreDefinitions, domains: &RegistryDomains) {
    let physiology = domains.survival.physiology();

    for definition in domains.labor.manual_power_definitions() {
        assert!(
            best_operable_manual_power_duration(
                core,
                &domains.equipment,
                &domains.energy,
                physiology,
                definition,
            )
            .is_some(),
            "manual power method {} has no pristine portable provider and compatible sink route that can produce nonzero energy within condition and survival limits",
            definition.id().value()
        );
    }

    for definition in domains.mining.definitions() {
        assert!(
            best_operable_mining_duration(core, &domains.equipment, physiology, definition)
                .is_some(),
            "mining method {} has no pristine portable provider route that can complete one provider-sized batch within condition and survival limits",
            definition.id().value()
        );
    }

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

#[cfg(test)]
#[path = "operability_tests.rs"]
mod tests;

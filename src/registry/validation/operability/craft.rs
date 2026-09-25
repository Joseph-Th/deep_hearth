//! Proves authored manual crafting has at least one reserve-feasible execution route.

use crate::core::time::TickSpan;
use crate::crafting::{
    ManualCraftDefinition, ManualCraftEquipmentProfile, resolve_manual_craft_equipment_physics,
};
use crate::labor::calculate_player_work_resource_budget;
use crate::maintenance::Condition;
use crate::survival::PhysiologyDefinition;

use super::super::super::{CoreDefinitions, RegistryDomains};
use super::assert_player_work_fits_reserves;

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

pub(super) fn validate_manual_craft_operability(
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

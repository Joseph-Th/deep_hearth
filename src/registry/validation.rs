//! Cross-domain authored-reference and practical-operability validation.

use crate::capability::CapabilityValue;
use crate::core::time::TickSpan;
use crate::crafting::{
    ManualCraftDefinition, ManualCraftEquipmentProfile, resolve_manual_craft_equipment_schedule,
};
use crate::equipment::resolve_equipment_capability;
use crate::labor::calculate_player_work_resource_budget;
use crate::maintenance::Condition;
use crate::material::MaterialAssemblyProfile;
use crate::survival::{PhysiologyDefinition, SurvivalExertion, SurvivalRegistry};

use super::{CoreDefinitions, RegistryDomains};

fn assert_nonperishable_infrastructure_assembly(
    owner: &str,
    assembly: &MaterialAssemblyProfile,
    survival: &SurvivalRegistry,
) {
    for input in assembly.inputs() {
        assert!(
            !survival.has_food_material(input.commodity().material()),
            "{owner} cannot embody material {} because that material has an authored edible form and embodied infrastructure does not track storage age",
            input.commodity().material().value()
        );
    }
}

fn validate_infrastructure_perishability(domains: &RegistryDomains) {
    for definition in domains.energy.definitions() {
        if let Some(assembly) = definition.assembly_profile() {
            assert_nonperishable_infrastructure_assembly(
                "energy-store assembly",
                assembly,
                &domains.survival,
            );
        }
        if let Some(upgrade) = definition.upgrade_profile() {
            assert_nonperishable_infrastructure_assembly(
                "energy-store upgrade",
                upgrade.additions(),
                &domains.survival,
            );
        }
    }
    for definition in domains.equipment.definitions() {
        if let Some(assembly) = definition.assembly_profile() {
            assert_nonperishable_infrastructure_assembly(
                "equipment assembly",
                assembly,
                &domains.survival,
            );
        }
        if let Some(upgrade) = definition.upgrade_profile() {
            assert_nonperishable_infrastructure_assembly(
                "equipment upgrade",
                upgrade.additions(),
                &domains.survival,
            );
        }
    }
    for definition in domains.storage.definitions() {
        assert_nonperishable_infrastructure_assembly(
            "storage-enclosure assembly",
            definition.assembly_profile(),
            &domains.survival,
        );
    }
}

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

fn validate_fixed_player_work_operability(core: &CoreDefinitions, domains: &RegistryDomains) {
    let physiology = domains.survival.physiology();

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

pub(super) fn validate_registry_domains(core: &CoreDefinitions, domains: &RegistryDomains) {
    domains
        .energy
        .validate_references(&domains.materials, core.physical_tick_duration());
    domains.fluid.validate_references(&domains.materials);
    domains.crafting.validate_references(
        &domains.production,
        &domains.materials,
        &domains.capabilities,
    );
    domains
        .labor
        .validate_references(&domains.capabilities, &domains.equipment, &domains.energy);
    domains
        .equipment
        .validate_references(&domains.capabilities, &domains.materials);
    domains.storage.validate_references(&domains.materials);
    domains
        .production
        .validate_references(&domains.materials, &domains.capabilities);
    domains
        .mining
        .validate_references(&domains.capabilities, &domains.equipment);
    domains.ore_processing.validate_references(
        &domains.production,
        &domains.capabilities,
        &domains.materials,
    );
    domains
        .survival
        .validate_references(&domains.materials, &domains.fluid);
    validate_infrastructure_perishability(domains);
    domains.thermal.validate_references(
        &domains.production,
        &domains.capabilities,
        &domains.materials,
    );
    domains
        .presentation
        .textures
        .validate_references(&domains.materials, &domains.equipment);
    validate_fixed_player_work_operability(core, domains);
}

#[cfg(test)]
#[path = "validation_tests.rs"]
mod tests;

//! Cross-domain authored-reference validation and registry-wide invariant composition.

use crate::material::MaterialAssemblyProfile;
use crate::survival::SurvivalRegistry;

use super::{CoreDefinitions, RegistryDomains};

mod operability;

use operability::validate_player_work_operability;

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

fn assert_optional_nonperishable_infrastructure_assembly(
    owner: &str,
    assembly: Option<&MaterialAssemblyProfile>,
    survival: &SurvivalRegistry,
) {
    if let Some(assembly) = assembly {
        assert_nonperishable_infrastructure_assembly(owner, assembly, survival);
    }
}

fn validate_infrastructure_perishability(domains: &RegistryDomains) {
    for definition in domains.energy.definitions() {
        assert_optional_nonperishable_infrastructure_assembly(
            "energy-store assembly",
            definition.assembly_profile(),
            &domains.survival,
        );
        assert_optional_nonperishable_infrastructure_assembly(
            "energy-store upgrade",
            definition
                .upgrade_profile()
                .map(|upgrade| upgrade.additions()),
            &domains.survival,
        );
    }
    for definition in domains.equipment.definitions() {
        assert_optional_nonperishable_infrastructure_assembly(
            "equipment assembly",
            definition.assembly_profile(),
            &domains.survival,
        );
        assert_optional_nonperishable_infrastructure_assembly(
            "equipment upgrade",
            definition
                .upgrade_profile()
                .map(|upgrade| upgrade.additions()),
            &domains.survival,
        );
    }
    for definition in domains.storage.definitions() {
        assert_nonperishable_infrastructure_assembly(
            "storage-enclosure assembly",
            definition.assembly_profile(),
            &domains.survival,
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
        .validate_references(&domains.capabilities);
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
    validate_player_work_operability(core, domains);
}

//! Built-in immutable game definitions; sibling files own authored definitions for each registry domain.

mod capabilities;
mod materials;
mod processes;

use crate::registry::{CoreDefinitions, Registries, RegistrySchemaVersion};

#[cfg(test)]
use crate::production::{ProcessDefinition, ProductionRegistry};

pub use materials::{
    FORM_CONCENTRATE, FORM_INGOT, FORM_LOG, FORM_LUMP, FORM_ORE, MATERIAL_CHARCOAL,
    MATERIAL_COPPER, MATERIAL_SLAG, MATERIAL_WOOD,
};

const DEFAULT_TICKS_PER_SECOND: u16 = 20;
const REGISTRY_SCHEMA_VERSION: RegistrySchemaVersion = RegistrySchemaVersion::new(1);

/// Builds the immutable built-in registry set used by a new application instance.
#[must_use]
pub fn build_registries() -> Registries {
    Registries::new(
        REGISTRY_SCHEMA_VERSION,
        CoreDefinitions::new(DEFAULT_TICKS_PER_SECOND),
        capabilities::build_capability_registry(),
        materials::build_material_registry(),
        processes::build_production_registry(),
    )
}

#[cfg(test)]
pub(crate) fn make_test_registries_with_process(process: ProcessDefinition) -> Registries {
    let mut production = ProductionRegistry::new();
    production.register_process_for_test(process);
    Registries::new(
        REGISTRY_SCHEMA_VERSION,
        CoreDefinitions::new(DEFAULT_TICKS_PER_SECOND),
        capabilities::build_capability_registry(),
        materials::build_material_registry(),
        production,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{
        CapabilityComparison, CapabilityDefinition, CapabilityId, CapabilityRegistry,
        CapabilityRequirement, CapabilityValue, CapabilityValueKind,
    };
    use crate::core::quantity::{Mass, Temperature};
    use crate::material::{CommodityKey, MaterialInputSpec};
    use crate::production::{ProcessDefinition, ProcessId, ProductionRegistry};

    const TEST_CAPABILITY: CapabilityId = CapabilityId::new(700_001);
    const TEST_PROCESS: ProcessId = ProcessId::new(700_001);

    #[test]
    fn built_in_tick_rate_is_nonzero_and_stable() {
        let registries = build_registries();

        assert_eq!(registries.core().ticks_per_second().get(), 20);
    }

    #[test]
    fn process_capability_references_are_validated_during_registry_assembly() {
        let mut capabilities = CapabilityRegistry::new();
        capabilities.register_capability(CapabilityDefinition::new(
            TEST_CAPABILITY,
            "test chamber temperature",
            CapabilityValueKind::Temperature,
        ));
        let process = ProcessDefinition::new(
            TEST_PROCESS,
            "test capability process",
            vec![MaterialInputSpec::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
                Mass::from_milligrams(1),
            )],
            vec![CapabilityRequirement::new(
                TEST_CAPABILITY,
                CapabilityComparison::AtLeast,
                CapabilityValue::Temperature(Temperature::from_millikelvin(500_000)),
            )],
        );
        let mut production = ProductionRegistry::new();
        production.register_process_for_test(process);

        let registries = Registries::new(
            REGISTRY_SCHEMA_VERSION,
            CoreDefinitions::new(DEFAULT_TICKS_PER_SECOND),
            capabilities,
            materials::build_material_registry(),
            production,
        );

        assert!(
            registries
                .capabilities()
                .get_capability(TEST_CAPABILITY)
                .is_some()
        );
        assert!(registries.production().get_process(TEST_PROCESS).is_some());
    }
}

//! Proves each manual-power method can execute a nonzero charge into compatible finite storage.

use crate::capability::CapabilityValue;
use crate::core::quantity::Energy;
use crate::core::time::TickSpan;
use crate::energy::EnergyRegistry;
use crate::equipment::{EquipmentRegistry, resolve_equipment_capability};
use crate::labor::{ManualPowerDefinition, project_manual_power_configuration};
use crate::maintenance::Condition;
use crate::survival::PhysiologyDefinition;

use super::super::super::{CoreDefinitions, RegistryDomains};

// Manual generation has no authored batch size. The smallest positive stored-energy unit proves
// that the method itself can perform useful finite work without inventing a downstream consumer or
// requiring a lossy destination buffer to be filled in one session.
const MINIMUM_OPERABLE_OUTPUT: Energy = Energy::from_nanojoules(1);

pub(super) fn best_operable_manual_power_duration(
    core: &CoreDefinitions,
    equipment_registry: &EquipmentRegistry,
    energy_registry: &EnergyRegistry,
    physiology: PhysiologyDefinition,
    definition: &ManualPowerDefinition,
) -> Option<TickSpan> {
    equipment_registry
        .definitions()
        .filter(|equipment| !equipment.requires_structural_support())
        .filter(|equipment| {
            matches!(
                resolve_equipment_capability(
                    equipment,
                    Condition::PRISTINE,
                    definition.power_capability(),
                ),
                Some(CapabilityValue::Power(power)) if !power.is_zero()
            )
        })
        .filter_map(|equipment| {
            energy_registry
                .definitions()
                .filter(|store| {
                    store.carrier() == definition.carrier() && !store.max_input_power().is_zero()
                })
                .filter_map(|store| {
                    let projection = project_manual_power_configuration(
                        core,
                        physiology,
                        *definition,
                        equipment,
                        Condition::PRISTINE,
                        store,
                        MINIMUM_OPERABLE_OUTPUT,
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

pub(super) fn validate_manual_power_operability(
    core: &CoreDefinitions,
    domains: &RegistryDomains,
    physiology: PhysiologyDefinition,
) {
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
            "manual power method {} has no pristine portable provider and compatible finite store that can accept a nonzero authoritative charge within condition and survival limits",
            definition.id().value()
        );
    }
}

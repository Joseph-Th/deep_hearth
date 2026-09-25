//! Proves each manual-power method can fully charge a compatible finite store.

use crate::capability::CapabilityValue;
use crate::core::time::TickSpan;
use crate::energy::EnergyRegistry;
use crate::equipment::{EquipmentRegistry, resolve_equipment_capability};
use crate::labor::{ManualPowerDefinition, project_manual_power_configuration};
use crate::maintenance::Condition;
use crate::survival::PhysiologyDefinition;

use super::super::super::{CoreDefinitions, RegistryDomains};

pub(super) fn best_operable_manual_power_full_charge_duration(
    core: &CoreDefinitions,
    equipment_registry: &EquipmentRegistry,
    energy_registry: &EnergyRegistry,
    physiology: PhysiologyDefinition,
    definition: &ManualPowerDefinition,
) -> Option<TickSpan> {
    let compatible_stores = energy_registry
        .definitions()
        .filter(|store| {
            store.carrier() == definition.carrier() && !store.max_input_power().is_zero()
        })
        .collect::<Vec<_>>();

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
            compatible_stores
                .iter()
                .copied()
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

pub(super) fn validate_manual_power_operability(
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

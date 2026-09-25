//! Proves each mining method has a portable reserve-feasible provider route.

use crate::capability::CapabilityValue;
use crate::core::time::TickSpan;
use crate::equipment::{EquipmentRegistry, resolve_equipment_capability};
use crate::labor::calculate_player_work_resource_budget;
use crate::maintenance::Condition;
use crate::mining::{MiningMethodDefinition, resolve_mining_physics};
use crate::survival::PhysiologyDefinition;

use super::super::super::{CoreDefinitions, RegistryDomains};

pub(super) fn best_operable_mining_duration(
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

pub(super) fn validate_mining_operability(
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

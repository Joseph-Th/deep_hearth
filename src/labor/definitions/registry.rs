//! Deterministic lookup and cross-registry validation for authored labor methods.

use std::collections::BTreeMap;

use crate::capability::{CapabilityRegistry, CapabilityValue, CapabilityValueKind};
use crate::energy::EnergyRegistry;
use crate::equipment::EquipmentRegistry;
use crate::maintenance::{Condition, calculate_usable_condition_after_active_ticks};

use super::{
    ManualPowerDefinition, ManualPowerMethodId, ProspectingDefinition, ProspectingMethodId,
};

/// Immutable deterministic lookup for authored player-labor method semantics.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LaborRegistry {
    manual_power: BTreeMap<ManualPowerMethodId, ManualPowerDefinition>,
    prospecting: BTreeMap<ProspectingMethodId, ProspectingDefinition>,
}

impl LaborRegistry {
    pub(crate) fn new(
        manual_power_definitions: impl IntoIterator<Item = ManualPowerDefinition>,
        prospecting_definitions: impl IntoIterator<Item = ProspectingDefinition>,
    ) -> Self {
        let mut manual_power = BTreeMap::new();
        for definition in manual_power_definitions {
            let id = definition.id();
            assert!(
                manual_power.insert(id, definition).is_none(),
                "duplicate manual power method {}",
                id.value()
            );
        }
        let mut prospecting = BTreeMap::new();
        for definition in prospecting_definitions {
            let id = definition.id();
            assert!(
                prospecting.insert(id, definition).is_none(),
                "duplicate prospecting method {}",
                id.value()
            );
        }
        Self {
            manual_power,
            prospecting,
        }
    }

    #[must_use]
    pub fn get_manual_power(&self, id: ManualPowerMethodId) -> Option<&ManualPowerDefinition> {
        self.manual_power.get(&id)
    }

    #[must_use]
    pub fn get_prospecting(&self, id: ProspectingMethodId) -> Option<&ProspectingDefinition> {
        self.prospecting.get(&id)
    }

    pub fn prospecting_definitions(&self) -> impl Iterator<Item = &ProspectingDefinition> {
        self.prospecting.values()
    }

    pub(crate) fn validate_references(
        &self,
        capabilities: &CapabilityRegistry,
        equipment: &EquipmentRegistry,
        energy: &EnergyRegistry,
    ) {
        self.validate_manual_power_references(capabilities, equipment, energy);
        self.validate_prospecting_references(equipment);
    }

    fn validate_manual_power_references(
        &self,
        capabilities: &CapabilityRegistry,
        equipment: &EquipmentRegistry,
        energy: &EnergyRegistry,
    ) {
        for definition in self.manual_power.values() {
            let capability = capabilities
                .get_capability(definition.power_capability())
                .unwrap_or_else(|| {
                    panic!(
                        "manual power method {} references missing capability {}",
                        definition.id().value(),
                        definition.power_capability().value()
                    )
                });
            assert_eq!(
                capability.kind(),
                CapabilityValueKind::Power,
                "manual power method {} capability {} must have Power value kind",
                definition.id().value(),
                definition.power_capability().value()
            );
            assert!(
                equipment.definitions().any(|provider| {
                    !provider.requires_structural_support()
                        && matches!(
                            provider
                                .capabilities()
                                .get_capability(definition.power_capability()),
                            Some(CapabilityValue::Power(power)) if !power.is_zero()
                        )
                }),
                "manual power method {} capability {} has no portable equipment provider with nonzero power",
                definition.id().value(),
                definition.power_capability().value()
            );
            assert!(
                energy.definitions().any(|store| {
                    store.carrier() == definition.carrier() && !store.max_input_power().is_zero()
                }),
                "manual power method {} has no finite {:?} energy store that can accept generated work",
                definition.id().value(),
                definition.carrier()
            );
        }
    }

    fn validate_prospecting_references(&self, equipment: &EquipmentRegistry) {
        for definition in self.prospecting.values() {
            let Some(profile) = definition.equipment() else {
                continue;
            };
            assert!(
                calculate_usable_condition_after_active_ticks(
                    profile.condition_wear_ppm_per_active_tick(),
                    Condition::PRISTINE,
                    definition.duration(),
                )
                .is_ok(),
                "prospecting method {} cannot complete its authored {}-tick duration with a pristine instrument at {} ppm wear per active tick",
                definition.id().value(),
                definition.duration().value(),
                profile.condition_wear_ppm_per_active_tick()
            );
            for equipment_id in [Some(profile.primary()), profile.alternative()]
                .into_iter()
                .flatten()
            {
                let equipment_definition =
                    equipment.get_equipment(equipment_id).unwrap_or_else(|| {
                        panic!(
                            "prospecting method {} references missing equipment definition {}",
                            definition.id().value(),
                            equipment_id.value()
                        )
                    });
                assert!(
                    !equipment_definition.requires_structural_support(),
                    "prospecting method {} equipment definition {} requires structural installation but prospecting instruments must be portable",
                    definition.id().value(),
                    equipment_id.value()
                );
            }
        }
    }
}

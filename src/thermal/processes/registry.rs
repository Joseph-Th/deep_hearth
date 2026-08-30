//! Immutable thermal process definitions and exclusive resolver registry ownership.

use std::collections::BTreeMap;

use crate::capability::{
    CapabilityComparison, CapabilityId, CapabilityRegistry, CapabilityValueKind,
};
use crate::energy::EnergyCarrier;
use crate::maintenance::assert_valid_condition_wear_ppm_per_tick;
use crate::material::{CommodityKey, MaterialPhase, MaterialRegistry, ParticleSizeStatePolicy};
use crate::production::{ProcessId, ProcessInputPolicy, ProductionRegistry};

use super::super::casting_execution::CastingProcessDefinition;
use super::super::melting_execution::MeltingProcessDefinition;

/// Immutable declaration that one process is resolved as ideal sensible heating.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SensibleHeatingProcessDefinition {
    process: ProcessId,
    heating_power_capability: CapabilityId,
    max_temperature_capability: CapabilityId,
    max_batch_mass_capability: CapabilityId,
    energy_carrier: EnergyCarrier,
    condition_wear_ppm_per_active_tick: u32,
}

impl SensibleHeatingProcessDefinition {
    #[must_use]
    pub const fn new(
        process: ProcessId,
        heating_power_capability: CapabilityId,
        max_temperature_capability: CapabilityId,
        max_batch_mass_capability: CapabilityId,
        energy_carrier: EnergyCarrier,
        condition_wear_ppm_per_active_tick: u32,
    ) -> Self {
        assert_valid_condition_wear_ppm_per_tick(condition_wear_ppm_per_active_tick);
        Self {
            process,
            heating_power_capability,
            max_temperature_capability,
            max_batch_mass_capability,
            energy_carrier,
            condition_wear_ppm_per_active_tick,
        }
    }

    #[must_use]
    pub const fn process(self) -> ProcessId {
        self.process
    }

    #[must_use]
    pub const fn heating_power_capability(self) -> CapabilityId {
        self.heating_power_capability
    }

    #[must_use]
    pub const fn max_temperature_capability(self) -> CapabilityId {
        self.max_temperature_capability
    }

    #[must_use]
    pub const fn max_batch_mass_capability(self) -> CapabilityId {
        self.max_batch_mass_capability
    }

    #[must_use]
    pub const fn energy_carrier(self) -> EnergyCarrier {
        self.energy_carrier
    }

    /// Returns baseline condition loss for each authoritative tick spent actively running.
    #[must_use]
    pub const fn condition_wear_ppm_per_active_tick(self) -> u32 {
        self.condition_wear_ppm_per_active_tick
    }
}

/// Immutable lookup table for process-specific thermal resolution semantics.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThermalRegistry {
    sensible_heating: BTreeMap<ProcessId, SensibleHeatingProcessDefinition>,
    melting: BTreeMap<ProcessId, MeltingProcessDefinition>,
    casting: BTreeMap<ProcessId, CastingProcessDefinition>,
}

impl ThermalRegistry {
    pub(crate) fn new(
        sensible_heating_definitions: impl IntoIterator<Item = SensibleHeatingProcessDefinition>,
        melting_definitions: impl IntoIterator<Item = MeltingProcessDefinition>,
        casting_definitions: impl IntoIterator<Item = CastingProcessDefinition>,
    ) -> Self {
        let mut sensible_heating = BTreeMap::new();
        for definition in sensible_heating_definitions {
            let process = definition.process();
            assert!(
                sensible_heating.insert(process, definition).is_none(),
                "duplicate sensible-heating definition for process {}",
                process.value()
            );
        }
        let mut melting = BTreeMap::new();
        for definition in melting_definitions {
            let process = definition.process();
            assert!(
                !sensible_heating.contains_key(&process),
                "thermal process {} cannot be registered as both sensible heating and melting",
                process.value()
            );
            assert!(
                melting.insert(process, definition).is_none(),
                "duplicate melting definition for process {}",
                process.value()
            );
        }
        let mut casting = BTreeMap::new();
        for definition in casting_definitions {
            let process = definition.process();
            assert!(
                !sensible_heating.contains_key(&process) && !melting.contains_key(&process),
                "thermal process {} cannot be registered under multiple thermal resolvers",
                process.value()
            );
            assert!(
                casting.insert(process, definition).is_none(),
                "duplicate casting definition for process {}",
                process.value()
            );
        }
        Self {
            sensible_heating,
            melting,
            casting,
        }
    }

    #[must_use]
    pub fn get_sensible_heating(
        &self,
        process: ProcessId,
    ) -> Option<SensibleHeatingProcessDefinition> {
        self.sensible_heating.get(&process).copied()
    }

    #[must_use]
    pub fn get_melting(&self, process: ProcessId) -> Option<&MeltingProcessDefinition> {
        self.melting.get(&process)
    }

    #[must_use]
    pub fn get_casting(&self, process: ProcessId) -> Option<CastingProcessDefinition> {
        self.casting.get(&process).copied()
    }

    pub(crate) fn has_process(&self, process: ProcessId) -> bool {
        self.sensible_heating.contains_key(&process)
            || self.melting.contains_key(&process)
            || self.casting.contains_key(&process)
    }

    pub(crate) fn validate_references(
        &self,
        production: &ProductionRegistry,
        capabilities: &CapabilityRegistry,
        materials: &MaterialRegistry,
    ) {
        for definition in self.sensible_heating.values().copied() {
            validate_common_thermal_references(
                definition.process(),
                definition.heating_power_capability(),
                definition.max_temperature_capability(),
                definition.max_batch_mass_capability(),
                production,
                capabilities,
            );
        }
        for definition in self.casting.values().copied() {
            validate_common_thermal_references(
                definition.process(),
                definition.cooling_power_capability(),
                definition.max_temperature_capability(),
                definition.max_batch_mass_capability(),
                production,
                capabilities,
            );
            validate_casting_material_references(definition, materials);
        }
        for definition in self.melting.values() {
            validate_common_thermal_references(
                definition.process(),
                definition.heating_power_capability(),
                definition.max_temperature_capability(),
                definition.max_batch_mass_capability(),
                production,
                capabilities,
            );
            validate_melting_form_references(definition, materials);
        }
    }
}

fn validate_casting_material_references(
    definition: CastingProcessDefinition,
    materials: &MaterialRegistry,
) {
    let liquid_form = materials
        .get_form(definition.liquid_form())
        .unwrap_or_else(|| {
            panic!(
                "casting process {} references missing input form {}",
                definition.process().value(),
                definition.liquid_form().value()
            )
        });
    assert_eq!(
        liquid_form.phase(),
        MaterialPhase::Liquid,
        "casting process {} input form {} must be liquid",
        definition.process().value(),
        definition.liquid_form().value()
    );
    let solid_form = materials
        .get_form(definition.solid_form())
        .unwrap_or_else(|| {
            panic!(
                "casting process {} references missing output form {}",
                definition.process().value(),
                definition.solid_form().value()
            )
        });
    assert_eq!(
        solid_form.phase(),
        MaterialPhase::Solid,
        "casting process {} output form {} must be solid",
        definition.process().value(),
        definition.solid_form().value()
    );
    assert_eq!(
        solid_form.particle_size_policy(),
        ParticleSizeStatePolicy::Untracked,
        "casting process {} output form {} cannot require particle-size state because casting has no authored particulate output distribution",
        definition.process().value(),
        definition.solid_form().value()
    );
    validate_casting_applicable_materials(definition, materials);
}

fn validate_casting_applicable_materials(
    definition: CastingProcessDefinition,
    materials: &MaterialRegistry,
) {
    let material = materials
        .get_material(definition.material())
        .unwrap_or_else(|| {
            panic!(
                "casting process {} references unknown material {}",
                definition.process().value(),
                definition.material().value()
            )
        });
    assert!(
        materials.has_commodity(CommodityKey::new(
            definition.material(),
            definition.liquid_form()
        )) && materials.has_commodity(CommodityKey::new(
            definition.material(),
            definition.solid_form()
        )),
        "casting process {} material {} must be authored in both input form {} and output form {}",
        definition.process().value(),
        definition.material().value(),
        definition.liquid_form().value(),
        definition.solid_form().value()
    );
    let melting_point = material
        .properties()
        .thermal()
        .melting_point()
        .unwrap_or_else(|| {
            panic!(
                "casting process {} material {} has no fusion properties",
                definition.process().value(),
                definition.material().value()
            )
        });
    assert!(
        definition.output_temperature() <= melting_point,
        "casting process {} output temperature {} mK exceeds material {} melting point {} mK",
        definition.process().value(),
        definition.output_temperature().millikelvin(),
        definition.material().value(),
        melting_point.millikelvin()
    );
}

fn validate_melting_form_references(
    definition: &MeltingProcessDefinition,
    materials: &MaterialRegistry,
) {
    let material = materials
        .get_material(definition.material())
        .unwrap_or_else(|| {
            panic!(
                "melting process {} references unknown material {}",
                definition.process().value(),
                definition.material().value()
            )
        });
    assert!(
        material.properties().thermal().melting_point().is_some(),
        "melting process {} material {} has no fusion properties",
        definition.process().value(),
        definition.material().value()
    );
    for &solid_form_id in definition.solid_forms() {
        let solid_form = materials.get_form(solid_form_id).unwrap_or_else(|| {
            panic!(
                "melting process {} references missing input form {}",
                definition.process().value(),
                solid_form_id.value()
            )
        });
        assert_eq!(
            solid_form.phase(),
            MaterialPhase::Solid,
            "melting process {} input form {} must be solid",
            definition.process().value(),
            solid_form_id.value()
        );
        assert!(
            materials.has_commodity(CommodityKey::new(definition.material(), solid_form_id)),
            "melting process {} material {} is not authored in accepted input form {}",
            definition.process().value(),
            definition.material().value(),
            solid_form_id.value()
        );
    }
    let liquid_form = materials
        .get_form(definition.liquid_form())
        .unwrap_or_else(|| {
            panic!(
                "melting process {} references missing output form {}",
                definition.process().value(),
                definition.liquid_form().value()
            )
        });
    assert_eq!(
        liquid_form.phase(),
        MaterialPhase::Liquid,
        "melting process {} output form {} must be liquid",
        definition.process().value(),
        definition.liquid_form().value()
    );
    assert!(
        materials.has_commodity(CommodityKey::new(
            definition.material(),
            definition.liquid_form()
        )),
        "melting process {} material {} is not authored in output form {}",
        definition.process().value(),
        definition.material().value(),
        definition.liquid_form().value()
    );
}

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;

fn validate_common_thermal_references(
    process: ProcessId,
    thermal_power_capability: CapabilityId,
    max_temperature_capability: CapabilityId,
    max_batch_mass_capability: CapabilityId,
    production: &ProductionRegistry,
    capabilities: &CapabilityRegistry,
) {
    let process_definition = match production.get_process(process) {
        Some(definition) => definition,
        None => panic!(
            "thermal definition references missing process {}",
            process.value()
        ),
    };
    assert!(
        matches!(
            process_definition.input_policy(),
            ProcessInputPolicy::SelectedBatch
        ),
        "thermal process {} must use selected-batch input policy",
        process.value()
    );
    let power = match capabilities.get_capability(thermal_power_capability) {
        Some(capability) => capability,
        None => panic!(
            "thermal process {} references missing thermal-transfer-power capability {}",
            process.value(),
            thermal_power_capability.value()
        ),
    };
    assert_eq!(
        power.kind(),
        CapabilityValueKind::Power,
        "thermal process {} thermal-transfer-power capability must be Power",
        process.value()
    );
    let maximum = match capabilities.get_capability(max_temperature_capability) {
        Some(capability) => capability,
        None => panic!(
            "thermal process {} references missing maximum-temperature capability {}",
            process.value(),
            max_temperature_capability.value()
        ),
    };
    assert_eq!(
        maximum.kind(),
        CapabilityValueKind::Temperature,
        "thermal process {} maximum-temperature capability must be Temperature",
        process.value()
    );
    let maximum_batch = match capabilities.get_capability(max_batch_mass_capability) {
        Some(capability) => capability,
        None => panic!(
            "thermal process {} references missing maximum-batch-mass capability {}",
            process.value(),
            max_batch_mass_capability.value()
        ),
    };
    assert_eq!(
        maximum_batch.kind(),
        CapabilityValueKind::Mass,
        "thermal process {} maximum-batch-mass capability must be Mass",
        process.value()
    );
    for (capability, role) in [
        (thermal_power_capability, "thermal-transfer-power"),
        (max_temperature_capability, "maximum-temperature"),
        (max_batch_mass_capability, "maximum-batch-mass"),
    ] {
        let requirement = process_definition
            .get_capability_requirement(capability)
            .unwrap_or_else(|| {
                panic!(
                    "thermal process {} must require its resolver-owned {role} capability {}",
                    process.value(),
                    capability.value()
                )
            });
        assert_eq!(
            requirement.comparison(),
            CapabilityComparison::AtLeast,
            "thermal process {} resolver-owned {role} capability {} must use AtLeast comparison",
            process.value(),
            capability.value()
        );
    }
}

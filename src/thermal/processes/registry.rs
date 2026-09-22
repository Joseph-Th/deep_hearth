//! Immutable thermal process definitions and exclusive resolver registry ownership.

use std::collections::BTreeMap;

use crate::capability::{CapabilityId, CapabilityRegistry};
use crate::energy::EnergyCarrier;
use crate::maintenance::assert_valid_condition_wear_ppm_per_tick;
use crate::material::MaterialRegistry;
use crate::production::{ProcessId, ProductionRegistry};

mod phase_change;
mod validation;

pub use phase_change::{
    CastingPhaseChange, CastingProcessDefinition, MeltingProcessDefinition, PhaseChangeForms,
    PhaseChangeProcessProfile,
};

use validation::{
    validate_casting_material_references, validate_common_thermal_references,
    validate_melting_form_references,
};

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

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;

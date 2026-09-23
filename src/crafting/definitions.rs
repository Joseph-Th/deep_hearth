//! Immutable physical definitions for manual and finite-work shaping operations.

use crate::capability::CapabilityId;
use crate::core::quantity::{Mass, MassSpecificEnergy};
use crate::core::time::TickSpan;
use crate::energy::EnergyCarrier;
use crate::maintenance::assert_valid_condition_wear_ppm_per_tick;
use crate::material::CommodityKey;
use crate::production::ProcessId;
use crate::survival::SurvivalExertion;

/// One conserved output of a manual shaping operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ManualCraftOutput {
    commodity: CommodityKey,
    mass: Mass,
}

/// Finite-work machine route for a material transform already authored by manual crafting.
///
/// Powered routes deliberately reference an existing manual transform instead of restating its
/// material inputs, yields, or output forms. The powered definition owns only the distinct process
/// identity and machine-resource physics needed to execute that same transformation unattended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PoweredCraftDefinition {
    process: ProcessId,
    transform: ProcessId,
    mass_flow_capability: CapabilityId,
    energy_carrier: EnergyCarrier,
    specific_energy: MassSpecificEnergy,
    condition_wear_ppm_per_active_tick: u32,
}

impl PoweredCraftDefinition {
    #[must_use]
    pub fn new(
        process: ProcessId,
        transform: ProcessId,
        mass_flow_capability: CapabilityId,
        energy_carrier: EnergyCarrier,
        specific_energy: MassSpecificEnergy,
        condition_wear_ppm_per_active_tick: u32,
    ) -> Self {
        assert_ne!(
            process, transform,
            "powered craft process must have a distinct execution identity"
        );
        assert!(
            specific_energy.nanojoules_per_milligram() != 0,
            "powered craft specific energy must be nonzero"
        );
        assert_valid_condition_wear_ppm_per_tick(condition_wear_ppm_per_active_tick);
        Self {
            process,
            transform,
            mass_flow_capability,
            energy_carrier,
            specific_energy,
            condition_wear_ppm_per_active_tick,
        }
    }

    #[must_use]
    pub const fn process(self) -> ProcessId {
        self.process
    }

    #[must_use]
    pub const fn transform(self) -> ProcessId {
        self.transform
    }

    #[must_use]
    pub const fn mass_flow_capability(self) -> CapabilityId {
        self.mass_flow_capability
    }

    #[must_use]
    pub const fn energy_carrier(self) -> EnergyCarrier {
        self.energy_carrier
    }

    #[must_use]
    pub const fn specific_energy(self) -> MassSpecificEnergy {
        self.specific_energy
    }

    #[must_use]
    pub const fn condition_wear_ppm_per_active_tick(self) -> u32 {
        self.condition_wear_ppm_per_active_tick
    }
}

/// Durable equipment semantics for a manual shaping process.
///
/// The capability is physical material throughput supplied by runtime equipment. Optional profiles
/// retain the fixed hand-work duration authored on [`ManualCraftDefinition`] as a fallback; required
/// profiles reject equipment-less requests. When equipment is used, condition-adjusted throughput
/// replaces fixed timing and the provider wears for the exact active duration. Transformation yield
/// remains authored by the process definition rather than by the equipment profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManualCraftEquipmentProfile {
    mass_flow_capability: CapabilityId,
    condition_wear_ppm_per_active_tick: u32,
    required: bool,
}

impl ManualCraftEquipmentProfile {
    #[must_use]
    pub fn new(
        mass_flow_capability: CapabilityId,
        condition_wear_ppm_per_active_tick: u32,
    ) -> Self {
        assert_valid_condition_wear_ppm_per_tick(condition_wear_ppm_per_active_tick);
        Self {
            mass_flow_capability,
            condition_wear_ppm_per_active_tick,
            required: false,
        }
    }

    /// Authors a shaping route that cannot be performed without a compatible durable tool.
    #[must_use]
    pub fn new_required(
        mass_flow_capability: CapabilityId,
        condition_wear_ppm_per_active_tick: u32,
    ) -> Self {
        assert_valid_condition_wear_ppm_per_tick(condition_wear_ppm_per_active_tick);
        Self {
            mass_flow_capability,
            condition_wear_ppm_per_active_tick,
            required: true,
        }
    }

    #[must_use]
    pub const fn mass_flow_capability(self) -> CapabilityId {
        self.mass_flow_capability
    }

    #[must_use]
    pub const fn condition_wear_ppm_per_active_tick(self) -> u32 {
        self.condition_wear_ppm_per_active_tick
    }

    #[must_use]
    pub const fn requires_equipment(self) -> bool {
        self.required
    }
}

impl ManualCraftOutput {
    #[must_use]
    pub fn new(commodity: CommodityKey, mass: Mass) -> Self {
        assert!(!mass.is_zero(), "manual craft output mass must be nonzero");
        Self { commodity, mass }
    }

    #[must_use]
    pub const fn commodity(self) -> CommodityKey {
        self.commodity
    }

    #[must_use]
    pub const fn mass(self) -> Mass {
        self.mass
    }
}

/// Immutable physical shaping rule for a no-machine process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManualCraftDefinition {
    process: ProcessId,
    input: CommodityKey,
    input_mass: Mass,
    duration: TickSpan,
    exertion: SurvivalExertion,
    outputs: Vec<ManualCraftOutput>,
    equipment: Option<ManualCraftEquipmentProfile>,
}

impl ManualCraftDefinition {
    #[must_use]
    pub fn new(
        process: ProcessId,
        input: CommodityKey,
        input_mass: Mass,
        duration: TickSpan,
        exertion: SurvivalExertion,
        mut outputs: Vec<ManualCraftOutput>,
    ) -> Self {
        assert!(
            !input_mass.is_zero(),
            "manual craft input mass must be nonzero"
        );
        assert!(!duration.is_zero(), "manual craft duration must be nonzero");
        exertion.assert_active_player_work();
        assert!(
            !outputs.is_empty(),
            "manual craft must produce conserved output matter"
        );
        outputs.sort();
        for pair in outputs.windows(2) {
            assert!(
                pair[0].commodity() != pair[1].commodity(),
                "manual craft {} contains duplicate output commodity {}",
                process.value(),
                pair[0].commodity().value()
            );
        }
        let mut output_mass = Mass::ZERO;
        for output in &outputs {
            assert_eq!(
                output.commodity().material(),
                input.material(),
                "manual craft {} may change form but not material identity",
                process.value()
            );
            output_mass = output_mass.checked_add(output.mass()).unwrap_or_else(|| {
                panic!("manual craft {} output mass overflows", process.value())
            });
        }
        assert_eq!(
            output_mass,
            input_mass,
            "manual craft {} must conserve exact input mass",
            process.value()
        );
        Self {
            process,
            input,
            input_mass,
            duration,
            exertion,
            outputs,
            equipment: None,
        }
    }

    #[must_use]
    pub fn with_equipment_profile(mut self, equipment: ManualCraftEquipmentProfile) -> Self {
        assert!(
            self.equipment.is_none(),
            "manual craft {} cannot define more than one equipment profile",
            self.process.value()
        );
        self.equipment = Some(equipment);
        self
    }

    #[must_use]
    pub const fn process(&self) -> ProcessId {
        self.process
    }

    #[must_use]
    pub const fn input(&self) -> CommodityKey {
        self.input
    }

    #[must_use]
    pub const fn input_mass(&self) -> Mass {
        self.input_mass
    }

    #[must_use]
    pub const fn duration(&self) -> TickSpan {
        self.duration
    }

    #[must_use]
    pub const fn exertion(&self) -> SurvivalExertion {
        self.exertion
    }

    #[must_use]
    pub fn outputs(&self) -> &[ManualCraftOutput] {
        &self.outputs
    }

    #[must_use]
    pub const fn equipment_profile(&self) -> Option<ManualCraftEquipmentProfile> {
        self.equipment
    }
}

#[cfg(test)]
#[path = "definitions_tests.rs"]
mod tests;

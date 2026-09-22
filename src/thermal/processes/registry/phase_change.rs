//! Immutable pure phase-change process definitions owned by the thermal registry.

use crate::capability::CapabilityId;
use crate::core::quantity::Temperature;
use crate::energy::EnergyCarrier;
use crate::maintenance::assert_valid_condition_wear_ppm_per_tick;
use crate::material::{FormId, MaterialId};
use crate::production::ProcessId;

/// Shared equipment, carrier, and wear contract for one authored pure phase-change process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhaseChangeProcessProfile {
    transfer_power_capability: CapabilityId,
    max_temperature_capability: CapabilityId,
    max_batch_mass_capability: CapabilityId,
    energy_carrier: EnergyCarrier,
    condition_wear_ppm_per_active_tick: u32,
}

impl PhaseChangeProcessProfile {
    #[must_use]
    pub const fn new(
        transfer_power_capability: CapabilityId,
        max_temperature_capability: CapabilityId,
        max_batch_mass_capability: CapabilityId,
        energy_carrier: EnergyCarrier,
        condition_wear_ppm_per_active_tick: u32,
    ) -> Self {
        assert_valid_condition_wear_ppm_per_tick(condition_wear_ppm_per_active_tick);
        Self {
            transfer_power_capability,
            max_temperature_capability,
            max_batch_mass_capability,
            energy_carrier,
            condition_wear_ppm_per_active_tick,
        }
    }

    #[must_use]
    pub const fn transfer_power_capability(self) -> CapabilityId {
        self.transfer_power_capability
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

    #[must_use]
    pub const fn condition_wear_ppm_per_active_tick(self) -> u32 {
        self.condition_wear_ppm_per_active_tick
    }
}

/// Authored material-form transition owned by one phase-change process definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhaseChangeForms {
    input: FormId,
    output: FormId,
}

impl PhaseChangeForms {
    #[must_use]
    pub const fn new(input: FormId, output: FormId) -> Self {
        Self { input, output }
    }

    #[must_use]
    pub const fn input(self) -> FormId {
        self.input
    }

    #[must_use]
    pub const fn output(self) -> FormId {
        self.output
    }
}

/// Immutable declaration that one selected-batch process solidifies pure liquid matter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CastingPhaseChange {
    forms: PhaseChangeForms,
    output_temperature: Temperature,
}

impl CastingPhaseChange {
    #[must_use]
    pub const fn new(forms: PhaseChangeForms, output_temperature: Temperature) -> Self {
        assert!(
            output_temperature.millikelvin() > 0,
            "casting output temperature must be above absolute zero"
        );
        Self {
            forms,
            output_temperature,
        }
    }

    #[must_use]
    pub const fn liquid_form(self) -> FormId {
        self.forms.input()
    }

    #[must_use]
    pub const fn solid_form(self) -> FormId {
        self.forms.output()
    }

    #[must_use]
    pub const fn output_temperature(self) -> Temperature {
        self.output_temperature
    }
}

/// Immutable declaration that one selected-batch process solidifies pure liquid matter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CastingProcessDefinition {
    process: ProcessId,
    profile: PhaseChangeProcessProfile,
    material: MaterialId,
    phase_change: CastingPhaseChange,
}

impl CastingProcessDefinition {
    #[must_use]
    pub const fn new(
        process: ProcessId,
        profile: PhaseChangeProcessProfile,
        material: MaterialId,
        phase_change: CastingPhaseChange,
    ) -> Self {
        Self {
            process,
            profile,
            material,
            phase_change,
        }
    }

    #[must_use]
    pub const fn process(self) -> ProcessId {
        self.process
    }

    #[must_use]
    pub const fn cooling_power_capability(self) -> CapabilityId {
        self.profile.transfer_power_capability()
    }

    #[must_use]
    pub const fn max_temperature_capability(self) -> CapabilityId {
        self.profile.max_temperature_capability()
    }

    #[must_use]
    pub const fn max_batch_mass_capability(self) -> CapabilityId {
        self.profile.max_batch_mass_capability()
    }

    #[must_use]
    pub const fn energy_carrier(self) -> EnergyCarrier {
        self.profile.energy_carrier()
    }

    #[must_use]
    pub const fn material(self) -> MaterialId {
        self.material
    }

    #[must_use]
    pub const fn liquid_form(self) -> FormId {
        self.phase_change.liquid_form()
    }

    #[must_use]
    pub const fn solid_form(self) -> FormId {
        self.phase_change.solid_form()
    }

    /// Temperature of the solid lot after the casting cycle removes latent and sensible heat.
    #[must_use]
    pub const fn output_temperature(self) -> Temperature {
        self.phase_change.output_temperature()
    }

    #[must_use]
    pub const fn condition_wear_ppm_per_active_tick(self) -> u32 {
        self.profile.condition_wear_ppm_per_active_tick()
    }
}

/// Immutable declaration that one selected-batch process performs pure-material melting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MeltingProcessDefinition {
    process: ProcessId,
    profile: PhaseChangeProcessProfile,
    material: MaterialId,
    solid_forms: Vec<FormId>,
    liquid_form: FormId,
}

impl MeltingProcessDefinition {
    #[must_use]
    pub fn new(
        process: ProcessId,
        profile: PhaseChangeProcessProfile,
        material: MaterialId,
        solid_forms: Vec<FormId>,
        liquid_form: FormId,
    ) -> Self {
        assert!(
            !solid_forms.is_empty(),
            "melting process must accept at least one solid input form"
        );
        assert!(
            solid_forms.windows(2).all(|pair| pair[0] < pair[1]),
            "melting input forms must be strictly ordered and unique"
        );
        Self {
            process,
            profile,
            material,
            solid_forms,
            liquid_form,
        }
    }

    #[must_use]
    pub const fn process(&self) -> ProcessId {
        self.process
    }

    #[must_use]
    pub const fn heating_power_capability(&self) -> CapabilityId {
        self.profile.transfer_power_capability()
    }

    #[must_use]
    pub const fn max_temperature_capability(&self) -> CapabilityId {
        self.profile.max_temperature_capability()
    }

    #[must_use]
    pub const fn max_batch_mass_capability(&self) -> CapabilityId {
        self.profile.max_batch_mass_capability()
    }

    #[must_use]
    pub const fn energy_carrier(&self) -> EnergyCarrier {
        self.profile.energy_carrier()
    }

    #[must_use]
    pub const fn material(&self) -> MaterialId {
        self.material
    }

    #[must_use]
    pub fn solid_forms(&self) -> &[FormId] {
        &self.solid_forms
    }

    #[must_use]
    pub const fn liquid_form(&self) -> FormId {
        self.liquid_form
    }

    #[must_use]
    pub const fn condition_wear_ppm_per_active_tick(&self) -> u32 {
        self.profile.condition_wear_ppm_per_active_tick()
    }
}

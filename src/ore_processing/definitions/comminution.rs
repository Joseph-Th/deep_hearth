//! Comminution definitions for powered and direct-labor particle reduction.

use crate::capability::CapabilityId;
use crate::core::quantity::{Mass, MassFlow, MassSpecificEnergy};
use crate::energy::EnergyCarrier;
use crate::material::{FormId, ParticleSizeDistribution, ParticleSizeRange};
use crate::production::ProcessId;
use crate::survival::SurvivalExertion;

use super::{ManualOreProcessProfile, PoweredOreProcessProfile};

/// Material-side particle-reduction physics shared by powered and direct-labor comminution.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ComminutionPhysics {
    input_form: FormId,
    output_form: FormId,
    input_particle_size_range: Option<ParticleSizeRange>,
    output_particle_size: ParticleSizeDistribution,
}

impl ComminutionPhysics {
    fn new<P>(input_form: FormId, output_form: FormId, output_particle_size: P) -> Self
    where
        P: Into<ParticleSizeDistribution>,
    {
        Self {
            input_form,
            output_form,
            input_particle_size_range: None,
            output_particle_size: output_particle_size.into(),
        }
    }

    fn new_with_input_particle_size_range<P>(
        input_form: FormId,
        output_form: FormId,
        input_particle_size_range: ParticleSizeRange,
        output_particle_size: P,
    ) -> Self
    where
        P: Into<ParticleSizeDistribution>,
    {
        Self {
            input_form,
            output_form,
            input_particle_size_range: Some(input_particle_size_range),
            output_particle_size: output_particle_size.into(),
        }
    }

    const fn input_form(&self) -> FormId {
        self.input_form
    }

    const fn output_form(&self) -> FormId {
        self.output_form
    }

    const fn input_particle_size_range(&self) -> Option<ParticleSizeRange> {
        self.input_particle_size_range
    }

    fn output_particle_size(&self) -> ParticleSizeRange {
        self.output_particle_size.envelope()
    }

    const fn output_particle_size_distribution(&self) -> &ParticleSizeDistribution {
        &self.output_particle_size
    }
}

/// Immutable declaration that one selected-batch process reduces solid material to a finer form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComminutionProcessDefinition {
    process: ProcessId,
    physics: ComminutionPhysics,
    operating: PoweredOreProcessProfile,
}

/// Immutable selected-batch comminution performed directly by player labor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManualComminutionProcessDefinition {
    process: ProcessId,
    physics: ComminutionPhysics,
    operating: ManualOreProcessProfile,
}

impl ManualComminutionProcessDefinition {
    /// Authors direct-labor reduction of coarse untracked feed into an explicit particulate state.
    #[must_use]
    pub fn new<P>(
        process: ProcessId,
        input_form: FormId,
        output_form: FormId,
        output_particle_size: P,
        operating: ManualOreProcessProfile,
    ) -> Self
    where
        P: Into<ParticleSizeDistribution>,
    {
        Self {
            process,
            physics: ComminutionPhysics::new(input_form, output_form, output_particle_size),
            operating,
        }
    }

    #[must_use]
    pub const fn process(&self) -> ProcessId {
        self.process
    }

    #[must_use]
    pub const fn input_form(&self) -> FormId {
        self.physics.input_form()
    }

    #[must_use]
    pub const fn output_form(&self) -> FormId {
        self.physics.output_form()
    }

    #[must_use]
    pub const fn input_particle_size_range(&self) -> Option<ParticleSizeRange> {
        self.physics.input_particle_size_range()
    }

    #[must_use]
    pub fn output_particle_size(&self) -> ParticleSizeRange {
        self.physics.output_particle_size()
    }

    #[must_use]
    pub const fn output_particle_size_distribution(&self) -> &ParticleSizeDistribution {
        self.physics.output_particle_size_distribution()
    }

    #[must_use]
    pub const fn processing_rate(&self) -> MassFlow {
        self.operating.processing_rate()
    }

    #[must_use]
    pub const fn max_batch_mass(&self) -> Mass {
        self.operating.max_batch_mass()
    }

    #[must_use]
    pub const fn exertion(&self) -> SurvivalExertion {
        self.operating.exertion()
    }
}

impl ComminutionProcessDefinition {
    #[must_use]
    pub fn new<P>(
        process: ProcessId,
        input_form: FormId,
        output_form: FormId,
        output_particle_size: P,
        operating: PoweredOreProcessProfile,
    ) -> Self
    where
        P: Into<ParticleSizeDistribution>,
    {
        Self {
            process,
            physics: ComminutionPhysics::new(input_form, output_form, output_particle_size),
            operating,
        }
    }

    /// Authors a comminution operation that accepts only particulate feed whose complete envelope
    /// lies inside `input_particle_size_range`.
    ///
    /// This is an equipment/process operating constraint, not a recipe unlock. It lets physically
    /// distinct mill passes reject feed that is too coarse or too fine for the authored operation.
    #[must_use]
    pub fn new_with_input_particle_size_range<P>(
        process: ProcessId,
        input_form: FormId,
        output_form: FormId,
        input_particle_size_range: ParticleSizeRange,
        output_particle_size: P,
        operating: PoweredOreProcessProfile,
    ) -> Self
    where
        P: Into<ParticleSizeDistribution>,
    {
        Self {
            process,
            physics: ComminutionPhysics::new_with_input_particle_size_range(
                input_form,
                output_form,
                input_particle_size_range,
                output_particle_size,
            ),
            operating,
        }
    }

    #[must_use]
    pub const fn process(&self) -> ProcessId {
        self.process
    }

    #[must_use]
    pub const fn input_form(&self) -> FormId {
        self.physics.input_form()
    }

    #[must_use]
    pub const fn output_form(&self) -> FormId {
        self.physics.output_form()
    }

    /// Returns the authored admissible particulate feed envelope, when the operation has one.
    #[must_use]
    pub const fn input_particle_size_range(&self) -> Option<ParticleSizeRange> {
        self.physics.input_particle_size_range()
    }

    #[must_use]
    pub fn output_particle_size(&self) -> ParticleSizeRange {
        self.physics.output_particle_size()
    }

    /// Returns the authored weighted size classes produced by this comminution operation.
    #[must_use]
    pub const fn output_particle_size_distribution(&self) -> &ParticleSizeDistribution {
        self.physics.output_particle_size_distribution()
    }

    #[must_use]
    pub const fn mass_flow_capability(&self) -> CapabilityId {
        self.operating.mass_flow_capability()
    }

    #[must_use]
    pub const fn max_batch_mass_capability(&self) -> CapabilityId {
        self.operating.max_batch_mass_capability()
    }

    #[must_use]
    pub const fn energy_carrier(&self) -> EnergyCarrier {
        self.operating.energy_carrier()
    }

    #[must_use]
    pub const fn specific_energy(&self) -> MassSpecificEnergy {
        self.operating.specific_energy()
    }

    #[must_use]
    pub const fn condition_wear_ppm_per_active_tick(&self) -> u32 {
        self.operating.condition_wear_ppm_per_active_tick()
    }

    pub(in crate::ore_processing) const fn operating_profile(&self) -> PoweredOreProcessProfile {
        self.operating
    }
}

//! Immutable ore/material-preparation process definitions shared by the runtime resolvers.

use crate::capability::CapabilityId;
use crate::core::quantity::{Length, Mass, MassFlow, MassSpecificEnergy};
use crate::energy::EnergyCarrier;
use crate::maintenance::assert_valid_condition_wear_ppm_per_tick;
use crate::material::{
    COMPOSITION_PARTS_PER_MILLION, FormId, ParticleSizeDistribution, ParticleSizeRange,
};
use crate::production::ProcessId;
use crate::survival::SurvivalExertion;

mod separation;

pub(in crate::ore_processing) use separation::ConstituentSeparationPhysics;
pub use separation::{
    ConstituentSeparationProcessDefinition, ManualConstituentSeparationProcessDefinition,
};

/// Shared powered-throughput envelope for ore-preparation operations.
///
/// Comminution, screening, and constituent separation all consume finite work energy while an
/// equipment provider moves a bounded mass through the operation. Keeping that physical envelope in
/// one type prevents the three resolvers from drifting into subtly different rate, energy, or wear
/// contracts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PoweredOreProcessProfile {
    mass_flow_capability: CapabilityId,
    max_batch_mass_capability: CapabilityId,
    energy_carrier: EnergyCarrier,
    specific_energy: MassSpecificEnergy,
    condition_wear_ppm_per_active_tick: u32,
}

/// Direct player-labor throughput envelope for low-tech ore preparation.
///
/// Manual processing deliberately carries no equipment or abstract energy input. Its cost is
/// exclusive player attention plus exact survival expenditure, while the bounded rate and batch
/// size keep machinery materially superior once infrastructure exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManualOreProcessProfile {
    processing_rate: MassFlow,
    max_batch_mass: Mass,
    exertion: SurvivalExertion,
}

impl ManualOreProcessProfile {
    #[must_use]
    pub const fn new(
        processing_rate: MassFlow,
        max_batch_mass: Mass,
        exertion: SurvivalExertion,
    ) -> Self {
        assert!(
            !processing_rate.is_zero(),
            "manual ore-processing rate must be nonzero"
        );
        assert!(
            !max_batch_mass.is_zero(),
            "manual ore-processing maximum batch mass must be nonzero"
        );
        assert!(
            !exertion.energy_cost_per_tick().is_zero(),
            "manual ore-processing exertion must consume metabolic energy"
        );
        Self {
            processing_rate,
            max_batch_mass,
            exertion,
        }
    }

    #[must_use]
    pub const fn processing_rate(self) -> MassFlow {
        self.processing_rate
    }

    #[must_use]
    pub const fn max_batch_mass(self) -> Mass {
        self.max_batch_mass
    }

    #[must_use]
    pub const fn exertion(self) -> SurvivalExertion {
        self.exertion
    }
}

/// Authored selectivity of one constituent-separation pass.
///
/// Target recovery must be nonzero and strictly exceed non-target recovery so the operation always
/// enriches target content rather than merely relabeling an arbitrary split of the feed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConstituentRecoveryProfile {
    target_ppm: u32,
    non_target_ppm: u32,
}

impl ConstituentRecoveryProfile {
    #[must_use]
    pub const fn new(target_ppm: u32, non_target_ppm: u32) -> Self {
        assert!(
            target_ppm != 0 && target_ppm <= COMPOSITION_PARTS_PER_MILLION,
            "constituent separation target recovery must be within 1..=1,000,000 ppm"
        );
        assert!(
            non_target_ppm < target_ppm,
            "constituent separation non-target recovery must be below target recovery"
        );
        Self {
            target_ppm,
            non_target_ppm,
        }
    }

    #[must_use]
    pub const fn target_ppm(self) -> u32 {
        self.target_ppm
    }

    #[must_use]
    pub const fn non_target_ppm(self) -> u32 {
        self.non_target_ppm
    }
}

impl PoweredOreProcessProfile {
    #[must_use]
    pub const fn new(
        mass_flow_capability: CapabilityId,
        max_batch_mass_capability: CapabilityId,
        energy_carrier: EnergyCarrier,
        specific_energy: MassSpecificEnergy,
        condition_wear_ppm_per_active_tick: u32,
    ) -> Self {
        assert!(
            !specific_energy.is_zero(),
            "powered ore-processing mass-specific energy must be nonzero"
        );
        assert_valid_condition_wear_ppm_per_tick(condition_wear_ppm_per_active_tick);
        Self {
            mass_flow_capability,
            max_batch_mass_capability,
            energy_carrier,
            specific_energy,
            condition_wear_ppm_per_active_tick,
        }
    }

    #[must_use]
    pub(super) const fn mass_flow_capability(self) -> CapabilityId {
        self.mass_flow_capability
    }

    #[must_use]
    pub(super) const fn max_batch_mass_capability(self) -> CapabilityId {
        self.max_batch_mass_capability
    }

    #[must_use]
    pub(super) const fn energy_carrier(self) -> EnergyCarrier {
        self.energy_carrier
    }

    #[must_use]
    pub(super) const fn specific_energy(self) -> MassSpecificEnergy {
        self.specific_energy
    }

    #[must_use]
    pub(super) const fn condition_wear_ppm_per_active_tick(self) -> u32 {
        self.condition_wear_ppm_per_active_tick
    }
}

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

/// Immutable declaration that one selected-batch process classifies particulate material by size.
///
/// The aperture is an exact classification boundary. Runtime resolution succeeds only when every
/// selected particle-size class lies wholly on one side of that boundary, so screening never
/// invents a mass fraction for an unresolved class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreeningProcessDefinition {
    process: ProcessId,
    input_form: FormId,
    output_form: FormId,
    aperture: Length,
    operating: PoweredOreProcessProfile,
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

impl ScreeningProcessDefinition {
    /// Stable output stream identity for material at or below the authored aperture.
    pub const UNDERSIZE_STREAM: crate::production::ProcessOutputStreamId =
        crate::production::ProcessOutputStreamId::new(1);
    /// Stable output stream identity for material strictly above the authored aperture.
    pub const OVERSIZE_STREAM: crate::production::ProcessOutputStreamId =
        crate::production::ProcessOutputStreamId::new(2);

    #[must_use]
    pub const fn new(
        process: ProcessId,
        input_form: FormId,
        output_form: FormId,
        aperture: Length,
        operating: PoweredOreProcessProfile,
    ) -> Self {
        assert!(!aperture.is_zero(), "screening aperture must be nonzero");
        Self {
            process,
            input_form,
            output_form,
            aperture,
            operating,
        }
    }

    #[must_use]
    pub const fn process(self) -> ProcessId {
        self.process
    }

    #[must_use]
    pub const fn input_form(self) -> FormId {
        self.input_form
    }

    #[must_use]
    pub const fn output_form(self) -> FormId {
        self.output_form
    }

    #[must_use]
    pub const fn aperture(self) -> Length {
        self.aperture
    }

    #[must_use]
    pub const fn mass_flow_capability(self) -> CapabilityId {
        self.operating.mass_flow_capability()
    }

    #[must_use]
    pub const fn max_batch_mass_capability(self) -> CapabilityId {
        self.operating.max_batch_mass_capability()
    }

    #[must_use]
    pub const fn energy_carrier(self) -> EnergyCarrier {
        self.operating.energy_carrier()
    }

    #[must_use]
    pub const fn specific_energy(self) -> MassSpecificEnergy {
        self.operating.specific_energy()
    }

    #[must_use]
    pub const fn condition_wear_ppm_per_active_tick(self) -> u32 {
        self.operating.condition_wear_ppm_per_active_tick()
    }

    pub(super) const fn operating_profile(self) -> PoweredOreProcessProfile {
        self.operating
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

    pub(super) const fn operating_profile(&self) -> PoweredOreProcessProfile {
        self.operating
    }
}

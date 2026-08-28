//! Immutable ore/material-preparation process definitions shared by the runtime resolvers.

use crate::capability::CapabilityId;
use crate::core::quantity::{Length, MassSpecificEnergy};
use crate::energy::EnergyCarrier;
use crate::maintenance::assert_valid_condition_wear_ppm_per_tick;
use crate::material::{
    COMPOSITION_PARTS_PER_MILLION, FormId, MaterialId, ParticleSizeDistribution, ParticleSizeRange,
};
use crate::production::ProcessId;

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
    const PERFECT_BINARY: Self = Self {
        target_ppm: COMPOSITION_PARTS_PER_MILLION,
        non_target_ppm: 0,
    };

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

/// Immutable declaration that one selected-batch process reduces solid material to a finer form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComminutionProcessDefinition {
    process: ProcessId,
    input_form: FormId,
    output_form: FormId,
    input_particle_size_range: Option<ParticleSizeRange>,
    output_particle_size: ParticleSizeDistribution,
    operating: PoweredOreProcessProfile,
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

/// Immutable declaration that one selected-batch process separates an authored target constituent
/// from physically liberated particulate feed.
///
/// A binary definition names the only admissible residue material. A concentration definition
/// accepts any non-target constituents, allowing one authored physical separation method to handle
/// variable gangue without proliferating composition-specific recipes. Concentration authors both
/// target recovery and lower non-target recovery, so concentrate grade emerges from feed assay and
/// separator selectivity instead of assuming perfect gangue rejection. Binary separation represents
/// deterministic sorting of already liberated target particles and therefore uses complete target
/// recovery with zero non-target recovery. The resolver derives output masses from exact selected
/// composition. Unrecovered constituents remain represented in particulate residue, and both
/// concentration streams retain the selected feed's particle-size state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConstituentSeparationProcessDefinition {
    process: ProcessId,
    input_form: FormId,
    target_material: MaterialId,
    target_output_form: FormId,
    residue_material: Option<MaterialId>,
    residue_output_form: FormId,
    recovery: ConstituentRecoveryProfile,
    operating: PoweredOreProcessProfile,
}

impl ConstituentSeparationProcessDefinition {
    pub const TARGET_STREAM: crate::production::ProcessOutputStreamId =
        crate::production::ProcessOutputStreamId::new(1);
    pub const RESIDUE_STREAM: crate::production::ProcessOutputStreamId =
        crate::production::ProcessOutputStreamId::new(2);

    #[must_use]
    pub const fn new_binary(
        process: ProcessId,
        input_form: FormId,
        target_material: MaterialId,
        target_output_form: FormId,
        residue_material: MaterialId,
        residue_output_form: FormId,
        operating: PoweredOreProcessProfile,
    ) -> Self {
        assert!(
            target_material.value() != residue_material.value(),
            "constituent separation target and residue materials must differ"
        );
        Self {
            process,
            input_form,
            target_material,
            target_output_form,
            residue_material: Some(residue_material),
            residue_output_form,
            recovery: ConstituentRecoveryProfile::PERFECT_BINARY,
            operating,
        }
    }

    /// Authors selective finite-recovery concentration of one liberated target constituent from
    /// arbitrary non-target gangue.
    #[must_use]
    pub const fn new_concentration(
        process: ProcessId,
        input_form: FormId,
        target_material: MaterialId,
        target_output_form: FormId,
        residue_output_form: FormId,
        recovery: ConstituentRecoveryProfile,
        operating: PoweredOreProcessProfile,
    ) -> Self {
        Self {
            process,
            input_form,
            target_material,
            target_output_form,
            residue_material: None,
            residue_output_form,
            recovery,
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
    pub const fn target_material(self) -> MaterialId {
        self.target_material
    }

    #[must_use]
    pub const fn target_output_form(self) -> FormId {
        self.target_output_form
    }

    #[must_use]
    pub const fn residue_material(self) -> Option<MaterialId> {
        self.residue_material
    }

    #[must_use]
    pub const fn residue_output_form(self) -> FormId {
        self.residue_output_form
    }

    /// Returns the authored fraction of exact target content recovered to the target stream.
    /// Recovery is conservatively floored once at the whole-milligram output boundary; the
    /// unresolved fractional target and all intentionally unrecovered target remain explicit
    /// residue matter.
    #[must_use]
    pub const fn target_recovery_ppm(self) -> u32 {
        self.recovery.target_ppm()
    }

    /// Returns the fraction of each non-target constituent carried into a concentration target
    /// stream. Binary sorting always returns zero here.
    #[must_use]
    pub const fn non_target_recovery_ppm(self) -> u32 {
        self.recovery.non_target_ppm()
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
            input_form,
            output_form,
            input_particle_size_range: None,
            output_particle_size: output_particle_size.into(),
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
            input_form,
            output_form,
            input_particle_size_range: Some(input_particle_size_range),
            output_particle_size: output_particle_size.into(),
            operating,
        }
    }

    #[must_use]
    pub const fn process(&self) -> ProcessId {
        self.process
    }

    #[must_use]
    pub const fn input_form(&self) -> FormId {
        self.input_form
    }

    #[must_use]
    pub const fn output_form(&self) -> FormId {
        self.output_form
    }

    /// Returns the authored admissible particulate feed envelope, when the operation has one.
    #[must_use]
    pub const fn input_particle_size_range(&self) -> Option<ParticleSizeRange> {
        self.input_particle_size_range
    }

    #[must_use]
    pub fn output_particle_size(&self) -> ParticleSizeRange {
        self.output_particle_size.envelope()
    }

    /// Returns the authored weighted size classes produced by this comminution operation.
    #[must_use]
    pub const fn output_particle_size_distribution(&self) -> &ParticleSizeDistribution {
        &self.output_particle_size
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

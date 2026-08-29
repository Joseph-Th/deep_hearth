//! Immutable ore/material-preparation process definitions shared by the runtime resolvers.

use crate::capability::CapabilityId;
use crate::core::quantity::{Length, Mass, MassFlow, MassSpecificEnergy};
use crate::energy::EnergyCarrier;
use crate::maintenance::assert_valid_condition_wear_ppm_per_tick;
use crate::material::{
    COMPOSITION_PARTS_PER_MILLION, CommodityKey, FormId, MaterialId, ParticleSizeDistribution,
    ParticleSizeRange,
};
use crate::production::ProcessId;
use crate::survival::SurvivalExertion;

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

/// Immutable declaration that one selected-batch process separates an authored target constituent
/// from physically liberated particulate feed.
///
/// Sorting represents deterministic recovery of already liberated target particles with authored
/// finite target recovery and zero non-target recovery. Every non-target constituent remains in a blended
/// particulate residue when that material authors the required residue form, allowing one physical
/// sorting operation to handle variable gangue without composition-specific recipes. Sorting requires
/// the selected commodity host to be the target material because it represents recognizable,
/// independently sortable target pieces. Concentration instead operates on prepared composition-bearing
/// particulate feed and may accept a gangue-hosted commodity when the target constituent is actually
/// present. It additionally authors lower non-target recovery, so concentrate grade emerges from feed
/// assay and separator selectivity instead of assuming perfect gangue rejection. The resolver derives
/// output masses from exact selected composition. Unrecovered constituents remain represented in
/// particulate residue, and concentration streams retain the selected feed's particle-size state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConstituentSeparationMode {
    Sorting,
    Concentration,
}

/// Material-side physics shared by powered and direct-labor separation routes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ConstituentSeparationPhysics {
    input_form: FormId,
    input_particle_size_range: Option<ParticleSizeRange>,
    target_material: MaterialId,
    target_output_form: FormId,
    mode: ConstituentSeparationMode,
    residue_output_form: FormId,
    recovery: ConstituentRecoveryProfile,
}

impl ConstituentSeparationPhysics {
    const fn new_sorting(
        input_form: FormId,
        target_material: MaterialId,
        target_output_form: FormId,
        residue_output_form: FormId,
        target_recovery_ppm: u32,
    ) -> Self {
        Self {
            input_form,
            input_particle_size_range: None,
            target_material,
            target_output_form,
            mode: ConstituentSeparationMode::Sorting,
            residue_output_form,
            recovery: ConstituentRecoveryProfile::new(target_recovery_ppm, 0),
        }
    }

    const fn new_sorting_with_input_particle_size_range(
        input_form: FormId,
        input_particle_size_range: ParticleSizeRange,
        target_material: MaterialId,
        target_output_form: FormId,
        residue_output_form: FormId,
        target_recovery_ppm: u32,
    ) -> Self {
        Self {
            input_form,
            input_particle_size_range: Some(input_particle_size_range),
            target_material,
            target_output_form,
            mode: ConstituentSeparationMode::Sorting,
            residue_output_form,
            recovery: ConstituentRecoveryProfile::new(target_recovery_ppm, 0),
        }
    }

    const fn new_concentration(
        input_form: FormId,
        input_particle_size_range: ParticleSizeRange,
        target_output: CommodityKey,
        residue_output_form: FormId,
        recovery: ConstituentRecoveryProfile,
    ) -> Self {
        Self {
            input_form,
            input_particle_size_range: Some(input_particle_size_range),
            target_material: target_output.material(),
            target_output_form: target_output.form(),
            mode: ConstituentSeparationMode::Concentration,
            residue_output_form,
            recovery,
        }
    }

    pub(super) const fn input_form(self) -> FormId {
        self.input_form
    }

    pub(super) const fn input_particle_size_range(self) -> Option<ParticleSizeRange> {
        self.input_particle_size_range
    }

    pub(super) const fn target_material(self) -> MaterialId {
        self.target_material
    }

    pub(super) const fn target_output_form(self) -> FormId {
        self.target_output_form
    }

    pub(super) const fn is_sorting(self) -> bool {
        matches!(self.mode, ConstituentSeparationMode::Sorting)
    }

    pub(super) const fn is_concentration(self) -> bool {
        matches!(self.mode, ConstituentSeparationMode::Concentration)
    }

    pub(super) const fn residue_output_form(self) -> FormId {
        self.residue_output_form
    }

    pub(super) const fn target_recovery_ppm(self) -> u32 {
        self.recovery.target_ppm()
    }

    pub(super) const fn non_target_recovery_ppm(self) -> u32 {
        self.recovery.non_target_ppm()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConstituentSeparationProcessDefinition {
    process: ProcessId,
    physics: ConstituentSeparationPhysics,
    operating: PoweredOreProcessProfile,
}

/// Immutable selected-batch constituent separation performed directly by player labor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManualConstituentSeparationProcessDefinition {
    process: ProcessId,
    physics: ConstituentSeparationPhysics,
    operating: ManualOreProcessProfile,
}

impl ConstituentSeparationProcessDefinition {
    pub const TARGET_STREAM: crate::production::ProcessOutputStreamId =
        crate::production::ProcessOutputStreamId::new(1);
    pub const RESIDUE_STREAM: crate::production::ProcessOutputStreamId =
        crate::production::ProcessOutputStreamId::new(2);

    /// Authors finite-recovery sorting of an already liberated target constituent from arbitrary gangue.
    ///
    /// Every non-target constituent remains physically represented in the particulate residue
    /// stream. The residue commodity host is derived from its dominant non-target material rather
    /// than baking one gangue identity into the process definition.
    #[must_use]
    pub const fn new_sorting(
        process: ProcessId,
        input_form: FormId,
        target_material: MaterialId,
        target_output_form: FormId,
        residue_output_form: FormId,
        target_recovery_ppm: u32,
        operating: PoweredOreProcessProfile,
    ) -> Self {
        Self {
            process,
            physics: ConstituentSeparationPhysics::new_sorting(
                input_form,
                target_material,
                target_output_form,
                residue_output_form,
                target_recovery_ppm,
            ),
            operating,
        }
    }

    /// Authors selective finite-recovery concentration of one liberated target constituent from
    /// arbitrary non-target gangue.
    #[must_use]
    pub const fn new_concentration(
        process: ProcessId,
        input_form: FormId,
        input_particle_size_range: ParticleSizeRange,
        target_output: CommodityKey,
        residue_output_form: FormId,
        recovery: ConstituentRecoveryProfile,
        operating: PoweredOreProcessProfile,
    ) -> Self {
        Self {
            process,
            physics: ConstituentSeparationPhysics::new_concentration(
                input_form,
                input_particle_size_range,
                target_output,
                residue_output_form,
                recovery,
            ),
            operating,
        }
    }

    #[must_use]
    pub const fn process(self) -> ProcessId {
        self.process
    }

    #[must_use]
    pub const fn input_form(self) -> FormId {
        self.physics.input_form()
    }

    /// Complete particulate feed envelope that is physically liberated enough for this separation.
    /// Sorting may omit this when the target occurs as independently sortable coarse pieces.
    #[must_use]
    pub const fn input_particle_size_range(self) -> Option<ParticleSizeRange> {
        self.physics.input_particle_size_range()
    }

    #[must_use]
    pub const fn target_material(self) -> MaterialId {
        self.physics.target_material()
    }

    #[must_use]
    pub const fn target_output_form(self) -> FormId {
        self.physics.target_output_form()
    }

    #[must_use]
    pub const fn residue_output_form(self) -> FormId {
        self.physics.residue_output_form()
    }

    /// Returns the authored fraction of exact target content recovered to the target stream.
    /// Recovery is conservatively floored once at the whole-milligram output boundary; the
    /// unresolved fractional target and all intentionally unrecovered target remain explicit
    /// residue matter.
    #[must_use]
    pub const fn target_recovery_ppm(self) -> u32 {
        self.physics.target_recovery_ppm()
    }

    /// Returns the fraction of each non-target constituent carried into a concentration target
    /// stream. Sorting always returns zero here.
    #[must_use]
    pub const fn non_target_recovery_ppm(self) -> u32 {
        self.physics.non_target_recovery_ppm()
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

    pub(super) const fn physics(self) -> ConstituentSeparationPhysics {
        self.physics
    }
}

impl ManualConstituentSeparationProcessDefinition {
    pub const TARGET_STREAM: crate::production::ProcessOutputStreamId =
        crate::production::ProcessOutputStreamId::new(1);
    pub const RESIDUE_STREAM: crate::production::ProcessOutputStreamId =
        crate::production::ProcessOutputStreamId::new(2);

    /// Authors deterministic hand sorting of liberated target pieces from gangue.
    #[must_use]
    pub const fn new_sorting(
        process: ProcessId,
        input_form: FormId,
        input_particle_size_range: ParticleSizeRange,
        target_output: CommodityKey,
        residue_output_form: FormId,
        target_recovery_ppm: u32,
        operating: ManualOreProcessProfile,
    ) -> Self {
        Self {
            process,
            physics: ConstituentSeparationPhysics::new_sorting_with_input_particle_size_range(
                input_form,
                input_particle_size_range,
                target_output.material(),
                target_output.form(),
                residue_output_form,
                target_recovery_ppm,
            ),
            operating,
        }
    }

    #[must_use]
    pub const fn process(self) -> ProcessId {
        self.process
    }

    #[must_use]
    pub const fn input_form(self) -> FormId {
        self.physics.input_form()
    }

    /// Complete particulate feed envelope that remains individually hand-sortable.
    #[must_use]
    pub fn input_particle_size_range(self) -> ParticleSizeRange {
        self.physics
            .input_particle_size_range()
            .unwrap_or_else(|| unreachable!("manual sorting always authors a visible-piece range"))
    }

    #[must_use]
    pub const fn target_material(self) -> MaterialId {
        self.physics.target_material()
    }

    #[must_use]
    pub const fn target_output_form(self) -> FormId {
        self.physics.target_output_form()
    }

    #[must_use]
    pub const fn residue_output_form(self) -> FormId {
        self.physics.residue_output_form()
    }

    #[must_use]
    pub const fn target_recovery_ppm(self) -> u32 {
        self.physics.target_recovery_ppm()
    }

    #[must_use]
    pub const fn processing_rate(self) -> MassFlow {
        self.operating.processing_rate()
    }

    #[must_use]
    pub const fn max_batch_mass(self) -> Mass {
        self.operating.max_batch_mass()
    }

    #[must_use]
    pub const fn exertion(self) -> SurvivalExertion {
        self.operating.exertion()
    }

    pub(super) const fn physics(self) -> ConstituentSeparationPhysics {
        self.physics
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

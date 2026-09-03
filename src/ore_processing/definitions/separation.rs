//! Constituent-separation definitions shared by powered and direct-labor resolvers.

use crate::capability::CapabilityId;
use crate::core::quantity::{Mass, MassFlow, MassSpecificEnergy};
use crate::energy::EnergyCarrier;
use crate::material::{
    COMPOSITION_PARTS_PER_MILLION, CommodityKey, FormId, MaterialId, ParticleSizeRange,
};
use crate::production::ProcessId;
use crate::survival::SurvivalExertion;

use super::{ManualOreProcessProfile, PoweredOreProcessProfile};

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

fn minimum_feed_mass_for_target_recovery(
    target: Mass,
    constituent_ppm: u32,
    recovery_ppm: u32,
) -> Option<Mass> {
    if target.is_zero() {
        return Some(Mass::ZERO);
    }
    if constituent_ppm == 0
        || constituent_ppm > 1_000_000
        || recovery_ppm == 0
        || recovery_ppm > 1_000_000
    {
        return None;
    }
    let denominator = u128::from(constituent_ppm) * u128::from(recovery_ppm);
    let numerator = u128::from(target.milligrams()) * 1_000_000_000_000_u128;
    let feed_milligrams = numerator.div_ceil(denominator);
    u64::try_from(feed_milligrams)
        .ok()
        .map(Mass::from_milligrams)
}

/// Material-side physics shared by powered and direct-labor separation routes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ore_processing) struct ConstituentSeparationPhysics {
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

    pub(in crate::ore_processing) const fn input_form(self) -> FormId {
        self.input_form
    }

    pub(in crate::ore_processing) const fn input_particle_size_range(
        self,
    ) -> Option<ParticleSizeRange> {
        self.input_particle_size_range
    }

    pub(in crate::ore_processing) const fn target_material(self) -> MaterialId {
        self.target_material
    }

    pub(in crate::ore_processing) const fn target_output_form(self) -> FormId {
        self.target_output_form
    }

    pub(in crate::ore_processing) const fn is_sorting(self) -> bool {
        matches!(self.mode, ConstituentSeparationMode::Sorting)
    }

    pub(in crate::ore_processing) const fn is_concentration(self) -> bool {
        matches!(self.mode, ConstituentSeparationMode::Concentration)
    }

    pub(in crate::ore_processing) const fn residue_output_form(self) -> FormId {
        self.residue_output_form
    }

    pub(in crate::ore_processing) const fn target_recovery_ppm(self) -> u32 {
        self.recovery.target_ppm()
    }

    pub(in crate::ore_processing) const fn non_target_recovery_ppm(self) -> u32 {
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

    /// Minimum selected feed mass whose exact target-constituent share can recover `target` whole
    /// milligrams under this process's authored target recovery.
    ///
    /// `constituent_ppm` is the observed target-material share of the candidate feed. The result
    /// owns the same conservative whole-milligram recovery boundary as runtime separation and is
    /// suitable for actor planning before exact selection resolution.
    #[must_use]
    pub fn minimum_feed_mass_for_target_recovery(
        self,
        target: Mass,
        constituent_ppm: u32,
    ) -> Option<Mass> {
        minimum_feed_mass_for_target_recovery(target, constituent_ppm, self.target_recovery_ppm())
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

    pub(in crate::ore_processing) const fn operating_profile(self) -> PoweredOreProcessProfile {
        self.operating
    }

    pub(in crate::ore_processing) const fn physics(self) -> ConstituentSeparationPhysics {
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

    /// Minimum selected feed mass whose exact target-constituent share can recover `target` whole
    /// milligrams under this manual process's authored recovery.
    #[must_use]
    pub fn minimum_feed_mass_for_target_recovery(
        self,
        target: Mass,
        constituent_ppm: u32,
    ) -> Option<Mass> {
        minimum_feed_mass_for_target_recovery(target, constituent_ppm, self.target_recovery_ppm())
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

    pub(in crate::ore_processing) const fn physics(self) -> ConstituentSeparationPhysics {
        self.physics
    }
}

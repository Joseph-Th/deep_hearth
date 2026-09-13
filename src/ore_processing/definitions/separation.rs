//! Constituent-separation definitions shared by powered and direct-labor resolvers.

use crate::core::quantity::Mass;
use crate::material::{
    COMPOSITION_PARTS_PER_MILLION, CommodityKey, FormId, MaterialId, ParticleSizeRange,
};

mod manual;
mod powered;

pub use manual::ManualConstituentSeparationProcessDefinition;
pub use powered::ConstituentSeparationProcessDefinition;

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
        || constituent_ppm > COMPOSITION_PARTS_PER_MILLION
        || recovery_ppm == 0
        || recovery_ppm > COMPOSITION_PARTS_PER_MILLION
    {
        return None;
    }
    let denominator = u128::from(constituent_ppm) * u128::from(recovery_ppm);
    let composition_scale = u128::from(COMPOSITION_PARTS_PER_MILLION);
    let numerator = u128::from(target.milligrams()) * composition_scale * composition_scale;
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

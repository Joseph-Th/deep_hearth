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
/// Target recovery must be nonzero, and non-target recovery may be at most half of target recovery.
/// That selectivity margin guarantees target enrichment even after each recovered constituent is
/// conservatively floored to the authoritative whole-milligram mass boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConstituentRecoveryProfile {
    target_ppm: u32,
    non_target_ppm: u32,
}

fn minimum_homogeneous_constituent_ppm_for_target_recovery(
    target: Mass,
    feed: Mass,
    recovery_ppm: u32,
) -> Option<u32> {
    if target.is_zero() {
        return Some(0);
    }
    if feed.is_zero() || recovery_ppm == 0 || recovery_ppm > COMPOSITION_PARTS_PER_MILLION {
        return None;
    }
    let composition_scale = u128::from(COMPOSITION_PARTS_PER_MILLION);
    let numerator = u128::from(target.milligrams()) * composition_scale * composition_scale;
    let denominator = u128::from(feed.milligrams()) * u128::from(recovery_ppm);
    let constituent_ppm = numerator.div_ceil(denominator);
    if constituent_ppm > composition_scale {
        return None;
    }
    u32::try_from(constituent_ppm).ok()
}

impl ConstituentRecoveryProfile {
    #[must_use]
    pub const fn new(target_ppm: u32, non_target_ppm: u32) -> Self {
        assert!(
            target_ppm != 0 && target_ppm <= COMPOSITION_PARTS_PER_MILLION,
            "constituent separation target recovery must be within 1..=1,000,000 ppm"
        );
        assert!(
            (non_target_ppm as u64) * 2 <= target_ppm as u64,
            "constituent separation non-target recovery must be at most half of target recovery so whole-milligram rounding cannot erase target enrichment"
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

fn minimum_homogeneous_feed_mass_for_target_recovery(
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

fn maximum_homogeneous_target_recovery_from_feed(
    feed: Mass,
    constituent_ppm: u32,
    recovery_ppm: u32,
) -> Option<Mass> {
    if feed.is_zero() {
        return Some(Mass::ZERO);
    }
    if constituent_ppm == 0
        || constituent_ppm > COMPOSITION_PARTS_PER_MILLION
        || recovery_ppm == 0
        || recovery_ppm > COMPOSITION_PARTS_PER_MILLION
    {
        return None;
    }
    let composition_scale = u128::from(COMPOSITION_PARTS_PER_MILLION);
    let denominator = composition_scale * composition_scale;
    let recovered_milligrams = u128::from(feed.milligrams())
        .checked_mul(u128::from(constituent_ppm))?
        .checked_mul(u128::from(recovery_ppm))?
        / denominator;
    u64::try_from(recovered_milligrams)
        .ok()
        .map(Mass::from_milligrams)
}

fn project_batched_homogeneous_target_recovery(
    feed: Mass,
    constituent_ppm: u32,
    recovery_ppm: u32,
    max_batch_mass: Mass,
) -> Option<Mass> {
    if feed.is_zero() {
        return Some(Mass::ZERO);
    }
    if max_batch_mass.is_zero() {
        return None;
    }
    let full_batches = feed.milligrams() / max_batch_mass.milligrams();
    let remainder = Mass::from_milligrams(feed.milligrams() % max_batch_mass.milligrams());
    let full_batch_recovery = maximum_homogeneous_target_recovery_from_feed(
        max_batch_mass,
        constituent_ppm,
        recovery_ppm,
    )?;
    let full_recovery_milligrams = full_batch_recovery.milligrams().checked_mul(full_batches)?;
    let remainder_recovery =
        maximum_homogeneous_target_recovery_from_feed(remainder, constituent_ppm, recovery_ppm)?;
    Mass::from_milligrams(full_recovery_milligrams).checked_add(remainder_recovery)
}

fn minimum_batched_homogeneous_feed_mass_for_target_recovery(
    target: Mass,
    constituent_ppm: u32,
    recovery_ppm: u32,
    max_batch_mass: Mass,
) -> Option<Mass> {
    if target.is_zero() {
        return Some(Mass::ZERO);
    }
    if max_batch_mass.is_zero()
        || maximum_homogeneous_target_recovery_from_feed(
            max_batch_mass,
            constituent_ppm,
            recovery_ppm,
        )?
        .is_zero()
    {
        return None;
    }
    let lower =
        minimum_homogeneous_feed_mass_for_target_recovery(target, constituent_ppm, recovery_ppm)?;
    let sufficient = |feed| {
        project_batched_homogeneous_target_recovery(
            feed,
            constituent_ppm,
            recovery_ppm,
            max_batch_mass,
        )
        .is_some_and(|recovered| recovered >= target)
    };
    if sufficient(lower) {
        return Some(lower);
    }

    let mut upper = lower;
    loop {
        upper = Mass::from_milligrams(upper.milligrams().checked_mul(2)?);
        if sufficient(upper) {
            break;
        }
    }

    let mut lower_milligrams = lower.milligrams().checked_add(1)?;
    let mut upper_milligrams = upper.milligrams();
    while lower_milligrams < upper_milligrams {
        let candidate = lower_milligrams + (upper_milligrams - lower_milligrams) / 2;
        if sufficient(Mass::from_milligrams(candidate)) {
            upper_milligrams = candidate;
        } else {
            lower_milligrams = candidate + 1;
        }
    }
    Some(Mass::from_milligrams(lower_milligrams))
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

//! Exact constituent recovery arithmetic for one validated separation input profile.

use std::collections::BTreeMap;

use crate::core::arithmetic::scale_u128_fraction_floor;
use crate::core::quantity::{Mass, Temperature};
use crate::material::{COMPOSITION_PARTS_PER_MILLION, MaterialId, ParticleSizeDistribution};
use crate::ore_processing::definitions::ConstituentSeparationPhysics;

use super::super::ConstituentSeparationBatchError;
use super::input::{ExactInputProfile, SeparationInputKey};

pub(super) struct ExactStreamGroup {
    pub(super) temperature: Temperature,
    pub(super) particle_size: ParticleSizeDistribution,
    pub(super) mass: Mass,
    pub(super) constituent_numerators: BTreeMap<MaterialId, u128>,
}

pub(super) struct RecoveredGroup {
    pub(super) target: ExactStreamGroup,
    pub(super) residue: ExactStreamGroup,
    pub(super) recovered_target_mass: Mass,
}

fn recovered_whole_milligrams(
    exact_constituent_numerator: u128,
    recovery_ppm: u32,
) -> Result<u64, ConstituentSeparationBatchError> {
    let recovered_numerator = scale_u128_fraction_floor(
        exact_constituent_numerator,
        recovery_ppm,
        COMPOSITION_PARTS_PER_MILLION,
    );
    u64::try_from(recovered_numerator / u128::from(COMPOSITION_PARTS_PER_MILLION))
        .map_err(|_| ConstituentSeparationBatchError::MassOverflow)
}

pub(super) fn recover_group(
    definition: ConstituentSeparationPhysics,
    key: SeparationInputKey,
    mut input: ExactInputProfile,
) -> Result<RecoveredGroup, ConstituentSeparationBatchError> {
    let (temperature, particle_size) = key;
    let denominator = u128::from(COMPOSITION_PARTS_PER_MILLION);
    let exact_target_numerator = input
        .constituent_numerators
        .get(&definition.target_material())
        .copied()
        .unwrap_or_else(|| unreachable!("validated separation input contains target matter"));
    let recovered_target_milligrams =
        recovered_whole_milligrams(exact_target_numerator, definition.target_recovery_ppm())?;
    let recovered_target_numerator = u128::from(recovered_target_milligrams) * denominator;
    let remaining_target_numerator = exact_target_numerator
        .checked_sub(recovered_target_numerator)
        .unwrap_or_else(|| unreachable!("floored recovery cannot exceed exact target matter"));
    input
        .constituent_numerators
        .insert(definition.target_material(), remaining_target_numerator);

    let mut recovered_constituent_numerators = BTreeMap::new();
    if recovered_target_numerator != 0 {
        recovered_constituent_numerators
            .insert(definition.target_material(), recovered_target_numerator);
    }
    let mut target_milligrams = recovered_target_milligrams;
    if definition.is_concentration() && recovered_target_milligrams != 0 {
        for (material, numerator) in &mut input.constituent_numerators {
            if *material == definition.target_material() {
                continue;
            }
            let recovered_milligrams =
                recovered_whole_milligrams(*numerator, definition.non_target_recovery_ppm())?;
            if recovered_milligrams == 0 {
                continue;
            }
            let recovered_numerator = u128::from(recovered_milligrams) * denominator;
            *numerator = numerator
                .checked_sub(recovered_numerator)
                .unwrap_or_else(|| {
                    unreachable!(
                        "floored non-target recovery cannot exceed exact constituent matter"
                    )
                });
            recovered_constituent_numerators.insert(*material, recovered_numerator);
            target_milligrams = target_milligrams
                .checked_add(recovered_milligrams)
                .ok_or(ConstituentSeparationBatchError::MassOverflow)?;
        }
    }

    let target_mass = Mass::from_milligrams(target_milligrams);
    let residue_mass = input
        .mass
        .checked_sub(target_mass)
        .unwrap_or_else(|| unreachable!("constituent projection cannot exceed selected mass"));
    debug_assert!(!residue_mass.is_zero());
    Ok(RecoveredGroup {
        target: ExactStreamGroup {
            temperature,
            particle_size: particle_size.clone(),
            mass: target_mass,
            constituent_numerators: recovered_constituent_numerators,
        },
        residue: ExactStreamGroup {
            temperature,
            particle_size,
            mass: residue_mass,
            constituent_numerators: input.constituent_numerators,
        },
        recovered_target_mass: Mass::from_milligrams(recovered_target_milligrams),
    })
}

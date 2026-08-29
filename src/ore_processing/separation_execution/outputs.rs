//! Pure physical projection of constituent-separation inputs into target and residue streams.

use std::collections::BTreeMap;

use crate::core::arithmetic::scale_u128_fraction_floor;
use crate::core::quantity::{Mass, Temperature};
use crate::inventory::ConsumedMaterialTrace;
use crate::material::{
    COMPOSITION_PARTS_PER_MILLION, CommodityKey, MaterialComposition, MaterialId, MaterialLotSpec,
    MaterialRegistry, ParticleSizeDistribution, ParticleSizeStatePolicy,
};
use crate::ore_processing::definitions::ConstituentSeparationPhysics;

use super::ConstituentSeparationBatchError;

mod blending;

use blending::{
    ParticulateOutputKey, add_blended_particulate_stream, add_blended_residue,
    build_particulate_outputs,
};

#[derive(Debug)]
pub(super) struct SeparationOutputs {
    pub(super) target: Vec<MaterialLotSpec>,
    pub(super) residue: Vec<MaterialLotSpec>,
    pub(super) target_mass: Mass,
    pub(super) residue_mass: Mass,
}

type TargetOutputKey = (Temperature, Option<ParticleSizeDistribution>);
type SeparationInputKey = (Temperature, ParticleSizeDistribution);

#[derive(Debug)]
struct ExactInputProfile {
    mass: Mass,
    constituent_numerators: BTreeMap<MaterialId, u128>,
}

impl ExactInputProfile {
    fn new() -> Self {
        Self {
            mass: Mass::ZERO,
            constituent_numerators: BTreeMap::new(),
        }
    }

    fn add_trace(
        &mut self,
        trace: &ConsumedMaterialTrace,
    ) -> Result<(), ConstituentSeparationBatchError> {
        self.mass = self
            .mass
            .checked_add(trace.mass())
            .ok_or(ConstituentSeparationBatchError::MassOverflow)?;
        for component in trace.profile().composition().components() {
            let numerator =
                u128::from(trace.mass().milligrams()) * u128::from(component.parts_per_million());
            let current = self
                .constituent_numerators
                .get(&component.material())
                .copied()
                .unwrap_or(0);
            self.constituent_numerators.insert(
                component.material(),
                current
                    .checked_add(numerator)
                    .ok_or(ConstituentSeparationBatchError::MassOverflow)?,
            );
        }
        Ok(())
    }
}

fn add_target_mass(
    grouped: &mut BTreeMap<TargetOutputKey, Mass>,
    temperature: Temperature,
    particle_size: Option<ParticleSizeDistribution>,
    mass: Mass,
) -> Result<(), ConstituentSeparationBatchError> {
    let key = (temperature, particle_size);
    let current = grouped.get(&key).copied().unwrap_or(Mass::ZERO);
    grouped.insert(
        key,
        current
            .checked_add(mass)
            .ok_or(ConstituentSeparationBatchError::MassOverflow)?,
    );
    Ok(())
}

fn build_target_outputs(
    grouped: BTreeMap<TargetOutputKey, Mass>,
    commodity: CommodityKey,
) -> Result<Vec<MaterialLotSpec>, ConstituentSeparationBatchError> {
    let mut outputs = grouped
        .into_iter()
        .map(|((temperature, particle_size), mass)| {
            let composition = MaterialComposition::pure(commodity.material());
            match particle_size {
                Some(particle_size) => MaterialLotSpec::with_composition_and_particle_size(
                    commodity,
                    mass,
                    temperature,
                    composition,
                    particle_size,
                ),
                None => {
                    MaterialLotSpec::with_composition(commodity, mass, temperature, composition)
                }
            }
            .map_err(ConstituentSeparationBatchError::Output)
        })
        .collect::<Result<Vec<_>, _>>()?;
    outputs.sort();
    Ok(outputs)
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

struct CollectedInputs {
    selected_mass: Mass,
    grouped: BTreeMap<SeparationInputKey, ExactInputProfile>,
}

struct ExactStreamGroup {
    temperature: Temperature,
    particle_size: ParticleSizeDistribution,
    mass: Mass,
    constituent_numerators: BTreeMap<MaterialId, u128>,
}

struct RecoveredGroup {
    target: ExactStreamGroup,
    residue: ExactStreamGroup,
    recovered_target_mass: Mass,
}

#[derive(Default)]
struct OutputAccumulator {
    target_by_profile: BTreeMap<TargetOutputKey, Mass>,
    target_particulate_by_profile: BTreeMap<ParticulateOutputKey, Mass>,
    residue_by_profile: BTreeMap<ParticulateOutputKey, Mass>,
    target_mass: Mass,
    recovered_target_mass: Mass,
    residue_mass: Mass,
}

fn validate_input_trace(
    materials: &MaterialRegistry,
    definition: ConstituentSeparationPhysics,
    trace: &ConsumedMaterialTrace,
) -> Result<SeparationInputKey, ConstituentSeparationBatchError> {
    let profile = trace.profile();
    if profile.commodity().form() != definition.input_form() {
        return Err(ConstituentSeparationBatchError::InputFormMismatch {
            expected: definition.input_form(),
            found: profile.commodity().form(),
        });
    }
    if definition.is_sorting() && profile.commodity().material() != definition.target_material() {
        return Err(
            ConstituentSeparationBatchError::SortingInputHostMaterialMismatch {
                expected: definition.target_material(),
                found: profile.commodity().material(),
            },
        );
    }

    let mut has_non_target = false;
    for component in profile.composition().components() {
        if component.material() == definition.target_material() {
            continue;
        }
        has_non_target = true;
        if !materials.has_commodity(CommodityKey::new(
            component.material(),
            definition.residue_output_form(),
        )) {
            return Err(ConstituentSeparationBatchError::UnsupportedResidueForm {
                material: component.material(),
                form: definition.residue_output_form(),
            });
        }
    }
    if profile
        .composition()
        .parts_per_million(definition.target_material())
        == 0
    {
        return Err(ConstituentSeparationBatchError::MissingTargetConstituent {
            material: definition.target_material(),
        });
    }
    if !has_non_target {
        return Err(ConstituentSeparationBatchError::MissingNonTargetConstituent);
    }

    let particle_size = profile
        .particle_size_distribution()
        .cloned()
        .unwrap_or_else(|| {
            unreachable!("authored constituent-separation input form requires particulate state")
        });
    if let Some(required) = definition.input_particle_size_range() {
        let found = particle_size.envelope();
        if found.minimum_diameter() < required.minimum_diameter()
            || found.maximum_diameter() > required.maximum_diameter()
        {
            return Err(
                ConstituentSeparationBatchError::InputParticleSizeOutsideOperatingRange {
                    required,
                    found,
                },
            );
        }
    }
    Ok((profile.temperature(), particle_size))
}

fn collect_inputs(
    materials: &MaterialRegistry,
    definition: ConstituentSeparationPhysics,
    traces: &[ConsumedMaterialTrace],
) -> Result<CollectedInputs, ConstituentSeparationBatchError> {
    if traces.is_empty() {
        return Err(ConstituentSeparationBatchError::EmptyInput);
    }

    let mut selected_mass = Mass::ZERO;
    let mut grouped = BTreeMap::<SeparationInputKey, ExactInputProfile>::new();
    for trace in traces {
        let key = validate_input_trace(materials, definition, trace)?;
        selected_mass = selected_mass
            .checked_add(trace.mass())
            .ok_or(ConstituentSeparationBatchError::MassOverflow)?;
        grouped
            .entry(key)
            .or_insert_with(ExactInputProfile::new)
            .add_trace(trace)?;
    }
    Ok(CollectedInputs {
        selected_mass,
        grouped,
    })
}

fn recover_group(
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

impl OutputAccumulator {
    fn add_group(
        &mut self,
        definition: ConstituentSeparationPhysics,
        target_particle_size_policy: ParticleSizeStatePolicy,
        group: RecoveredGroup,
    ) -> Result<(), ConstituentSeparationBatchError> {
        let RecoveredGroup {
            target,
            residue,
            recovered_target_mass,
        } = group;

        if !target.mass.is_zero() {
            if definition.is_concentration() {
                match target_particle_size_policy {
                    ParticleSizeStatePolicy::Required => add_blended_particulate_stream(
                        &mut self.target_particulate_by_profile,
                        target.temperature,
                        target.particle_size.clone(),
                        target.constituent_numerators,
                        target.mass,
                        |_| {
                            CommodityKey::new(
                                definition.target_material(),
                                definition.target_output_form(),
                            )
                        },
                    )?,
                    ParticleSizeStatePolicy::Untracked => unreachable!(
                        "validated concentration target output must retain particulate state"
                    ),
                }
            } else {
                let particle_size = match target_particle_size_policy {
                    ParticleSizeStatePolicy::Required => Some(target.particle_size),
                    ParticleSizeStatePolicy::Untracked => None,
                };
                add_target_mass(
                    &mut self.target_by_profile,
                    target.temperature,
                    particle_size,
                    target.mass,
                )?;
            }
        }

        let target_mass = target.mass;
        let residue_mass = residue.mass;
        add_blended_residue(
            &mut self.residue_by_profile,
            definition,
            residue.temperature,
            residue.particle_size,
            residue.constituent_numerators,
            residue.mass,
        )?;
        self.target_mass = self
            .target_mass
            .checked_add(target_mass)
            .ok_or(ConstituentSeparationBatchError::MassOverflow)?;
        self.recovered_target_mass = self
            .recovered_target_mass
            .checked_add(recovered_target_mass)
            .ok_or(ConstituentSeparationBatchError::MassOverflow)?;
        self.residue_mass = self
            .residue_mass
            .checked_add(residue_mass)
            .ok_or(ConstituentSeparationBatchError::MassOverflow)?;
        Ok(())
    }

    fn finish(
        self,
        definition: ConstituentSeparationPhysics,
        selected_mass: Mass,
    ) -> Result<SeparationOutputs, ConstituentSeparationBatchError> {
        if self.recovered_target_mass.is_zero() {
            return Err(ConstituentSeparationBatchError::TargetBelowMassResolution {
                material: definition.target_material(),
                selected: selected_mass,
            });
        }
        let target = if definition.is_sorting() {
            build_target_outputs(
                self.target_by_profile,
                CommodityKey::new(
                    definition.target_material(),
                    definition.target_output_form(),
                ),
            )?
        } else {
            build_particulate_outputs(self.target_particulate_by_profile)?
        };
        Ok(SeparationOutputs {
            target,
            residue: build_particulate_outputs(self.residue_by_profile)?,
            target_mass: self.target_mass,
            residue_mass: self.residue_mass,
        })
    }
}

pub(super) fn resolve_separation_outputs(
    materials: &MaterialRegistry,
    definition: ConstituentSeparationPhysics,
    target_particle_size_policy: ParticleSizeStatePolicy,
    traces: &[ConsumedMaterialTrace],
) -> Result<SeparationOutputs, ConstituentSeparationBatchError> {
    let CollectedInputs {
        selected_mass,
        grouped,
    } = collect_inputs(materials, definition, traces)?;
    let mut outputs = OutputAccumulator::default();
    for (key, input) in grouped {
        outputs.add_group(
            definition,
            target_particle_size_policy,
            recover_group(definition, key, input)?,
        )?;
    }
    outputs.finish(definition, selected_mass)
}

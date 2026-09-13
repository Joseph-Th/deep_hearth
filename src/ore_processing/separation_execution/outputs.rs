//! Pure physical projection of constituent-separation inputs into target and residue streams.

use std::collections::BTreeMap;

use crate::core::quantity::{Mass, Temperature};
use crate::inventory::ConsumedMaterialTrace;
use crate::material::{
    CommodityKey, MaterialComposition, MaterialLotSpec, MaterialRegistry, ParticleSizeDistribution,
    ParticleSizeStatePolicy,
};
use crate::ore_processing::definitions::ConstituentSeparationPhysics;

use super::ConstituentSeparationBatchError;

mod blending;
mod input;
mod recovery;

use blending::{
    ParticulateOutputKey, add_blended_particulate_stream, add_blended_residue,
    build_particulate_outputs,
};
use input::{CollectedInputs, collect_inputs};
use recovery::{RecoveredGroup, recover_group};

#[derive(Debug)]
pub(super) struct SeparationOutputs {
    pub(super) target: Vec<MaterialLotSpec>,
    pub(super) residue: Vec<MaterialLotSpec>,
    pub(super) target_mass: Mass,
    pub(super) residue_mass: Mass,
}

type TargetOutputKey = (Temperature, Option<ParticleSizeDistribution>);

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

#[derive(Default)]
struct OutputAccumulator {
    target_by_profile: BTreeMap<TargetOutputKey, Mass>,
    target_particulate_by_profile: BTreeMap<ParticulateOutputKey, Mass>,
    residue_by_profile: BTreeMap<ParticulateOutputKey, Mass>,
    target_mass: Mass,
    recovered_target_mass: Mass,
    residue_mass: Mass,
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

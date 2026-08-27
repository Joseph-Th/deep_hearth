//! Pure physical projection of constituent-separation inputs into target and residue streams.

use std::collections::{BTreeMap, BTreeSet};

use crate::core::quantity::{Mass, Temperature};
use crate::inventory::ConsumedMaterialTrace;
use crate::material::{
    COMPOSITION_PARTS_PER_MILLION, CommodityKey, CompositionComponent, MaterialComposition,
    MaterialId, MaterialLotSpec, ParticleSizeDistribution, ParticleSizeStatePolicy,
};
use crate::ore_processing::ConstituentSeparationProcessDefinition;

use super::ConstituentSeparationBatchError;

#[derive(Debug)]
pub(super) struct SeparationOutputs {
    pub(super) target: Vec<MaterialLotSpec>,
    pub(super) residue: Vec<MaterialLotSpec>,
    pub(super) target_mass: Mass,
    pub(super) residue_mass: Mass,
}

type TargetOutputKey = (Temperature, Option<ParticleSizeDistribution>);
type ParticulateOutputKey = (
    CommodityKey,
    Temperature,
    MaterialComposition,
    ParticleSizeDistribution,
);
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

fn add_particulate_mass(
    grouped: &mut BTreeMap<ParticulateOutputKey, Mass>,
    commodity: CommodityKey,
    temperature: Temperature,
    composition: MaterialComposition,
    particle_size: ParticleSizeDistribution,
    mass: Mass,
) -> Result<(), ConstituentSeparationBatchError> {
    let key = (commodity, temperature, composition, particle_size);
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

/// Reconstructs one concentration tailings stream without fabricating purified gangue lots.
///
/// Constituent numerators use ppm-mg units. Dividing each exact numerator by the residue mass gives
/// a common integer-ppm base composition plus a bounded set of per-ppm remainders. The remainder
/// schedule below distributes those +1 ppm corrections over deterministic mass intervals. This
/// preserves every constituent numerator exactly while keeping every output lot at, or within one
/// ppm of, the aggregate tailings assay instead of turning whole-milligram constituent floors into
/// freely selectable pure materials.
fn add_blended_concentration_residue(
    grouped: &mut BTreeMap<ParticulateOutputKey, Mass>,
    definition: ConstituentSeparationProcessDefinition,
    temperature: Temperature,
    particle_size: ParticleSizeDistribution,
    constituent_numerators: BTreeMap<MaterialId, u128>,
    residue_mass: Mass,
) -> Result<(), ConstituentSeparationBatchError> {
    let residue_milligrams = residue_mass.milligrams();
    let residue_milligrams_u128 = u128::from(residue_milligrams);
    let mut base_ppm = BTreeMap::<MaterialId, u32>::new();
    let mut remainders = Vec::<(MaterialId, u64)>::new();
    let mut base_total = 0_u64;
    let mut remainder_total = 0_u128;

    for (material, numerator) in constituent_numerators {
        let base = u32::try_from(numerator / residue_milligrams_u128)
            .map_err(|_| ConstituentSeparationBatchError::MassOverflow)?;
        let remainder = u64::try_from(numerator % residue_milligrams_u128)
            .map_err(|_| ConstituentSeparationBatchError::MassOverflow)?;
        if base != 0 {
            base_ppm.insert(material, base);
        }
        if remainder != 0 {
            remainders.push((material, remainder));
        }
        base_total = base_total
            .checked_add(u64::from(base))
            .ok_or(ConstituentSeparationBatchError::MassOverflow)?;
        remainder_total = remainder_total
            .checked_add(u128::from(remainder))
            .ok_or(ConstituentSeparationBatchError::MassOverflow)?;
    }

    let missing_ppm = u64::from(COMPOSITION_PARTS_PER_MILLION)
        .checked_sub(base_total)
        .unwrap_or_else(|| unreachable!("exact residue averages cannot exceed normalized ppm"));
    assert_eq!(
        remainder_total,
        residue_milligrams_u128 * u128::from(missing_ppm),
        "exact residue remainders must equal the normalized ppm deficit"
    );

    let mut emit_interval = |active: &BTreeSet<MaterialId>, interval_mass: u64| {
        if interval_mass == 0 {
            return Ok(());
        }
        let mut components = Vec::with_capacity(base_ppm.len() + active.len());
        let materials = base_ppm
            .keys()
            .copied()
            .chain(active.iter().copied())
            .collect::<BTreeSet<_>>();
        for material in materials {
            let ppm = base_ppm.get(&material).copied().unwrap_or(0)
                + u32::from(active.contains(&material));
            if ppm != 0 {
                components.push(CompositionComponent::new(material, ppm));
            }
        }
        let composition = MaterialComposition::new(components).unwrap_or_else(|error| {
            unreachable!("exact blended concentration residue must be normalized: {error}")
        });
        let host = composition
            .components()
            .iter()
            .find(|component| component.material() != definition.target_material())
            .map(|component| component.material())
            .unwrap_or_else(|| unreachable!("concentration residue must retain non-target gangue"));
        add_particulate_mass(
            grouped,
            CommodityKey::new(host, definition.residue_output_form()),
            temperature,
            composition,
            particle_size.clone(),
            Mass::from_milligrams(interval_mass),
        )
    };

    if missing_ppm == 0 {
        return emit_interval(&BTreeSet::new(), residue_milligrams);
    }

    // Concatenating each constituent's remainder over `missing_ppm` complete residue-mass laps and
    // projecting those positions modulo the residue mass yields a 0/1 correction matrix with exact
    // column sums and exactly `missing_ppm` corrections on every represented milligram. Sweeping
    // interval boundaries compresses that matrix to O(constituent-count) lots rather than one lot
    // per milligram.
    let mut events = BTreeMap::<u64, Vec<(MaterialId, bool)>>::new();
    events.entry(0).or_default();
    events.entry(residue_milligrams).or_default();
    let mut cursor = 0_u128;
    for (material, remainder) in remainders {
        let start = u64::try_from(cursor % residue_milligrams_u128)
            .map_err(|_| ConstituentSeparationBatchError::MassOverflow)?;
        let end_absolute = u128::from(start) + u128::from(remainder);
        if end_absolute <= residue_milligrams_u128 {
            let end = u64::try_from(end_absolute)
                .map_err(|_| ConstituentSeparationBatchError::MassOverflow)?;
            events.entry(start).or_default().push((material, true));
            events.entry(end).or_default().push((material, false));
        } else {
            let end = u64::try_from(end_absolute - residue_milligrams_u128)
                .map_err(|_| ConstituentSeparationBatchError::MassOverflow)?;
            events.entry(0).or_default().push((material, true));
            events.entry(end).or_default().push((material, false));
            events.entry(start).or_default().push((material, true));
            events
                .entry(residue_milligrams)
                .or_default()
                .push((material, false));
        }
        cursor = cursor
            .checked_add(u128::from(remainder))
            .ok_or(ConstituentSeparationBatchError::MassOverflow)?;
    }
    assert_eq!(
        cursor,
        residue_milligrams_u128 * u128::from(missing_ppm),
        "residue correction schedule must cover an exact number of mass laps"
    );

    let positions = events.keys().copied().collect::<Vec<_>>();
    let mut active = BTreeSet::<MaterialId>::new();
    for window in positions.windows(2) {
        let position = window[0];
        for (material, enabled) in events
            .get(&position)
            .unwrap_or_else(|| unreachable!("event position came from the event map"))
        {
            if *enabled {
                active.insert(*material);
            } else {
                active.remove(material);
            }
        }
        debug_assert_eq!(
            active.len(),
            usize::try_from(missing_ppm).unwrap_or(usize::MAX)
        );
        emit_interval(&active, window[1] - position)?;
    }
    Ok(())
}

fn build_particulate_outputs(
    grouped: BTreeMap<ParticulateOutputKey, Mass>,
) -> Result<Vec<MaterialLotSpec>, ConstituentSeparationBatchError> {
    let mut outputs = grouped
        .into_iter()
        .map(
            |((commodity, temperature, composition, particle_size), mass)| {
                MaterialLotSpec::with_composition_and_particle_size(
                    commodity,
                    mass,
                    temperature,
                    composition,
                    particle_size,
                )
                .map_err(ConstituentSeparationBatchError::Output)
            },
        )
        .collect::<Result<Vec<_>, _>>()?;
    outputs.sort();
    Ok(outputs)
}

pub(super) fn resolve_separation_outputs(
    definition: ConstituentSeparationProcessDefinition,
    target_particle_size_policy: ParticleSizeStatePolicy,
    traces: &[ConsumedMaterialTrace],
) -> Result<SeparationOutputs, ConstituentSeparationBatchError> {
    if traces.is_empty() {
        return Err(ConstituentSeparationBatchError::EmptyInput);
    }

    let mut selected_mass = Mass::ZERO;
    let mut grouped = BTreeMap::<SeparationInputKey, ExactInputProfile>::new();
    for trace in traces {
        let profile = trace.profile();
        if profile.commodity().form() != definition.input_form() {
            return Err(ConstituentSeparationBatchError::InputFormMismatch {
                expected: definition.input_form(),
                found: profile.commodity().form(),
            });
        }
        if profile.commodity().material() != definition.target_material() {
            return Err(ConstituentSeparationBatchError::InputHostMaterialMismatch {
                expected: definition.target_material(),
                found: profile.commodity().material(),
            });
        }
        let mut has_non_target = false;
        for component in profile.composition().components() {
            if component.material() != definition.target_material() {
                has_non_target = true;
                if definition
                    .residue_material()
                    .is_some_and(|residue| component.material() != residue)
                {
                    return Err(ConstituentSeparationBatchError::UnsupportedConstituent {
                        material: component.material(),
                    });
                }
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
        match definition.residue_material() {
            Some(residue) if profile.composition().parts_per_million(residue) == 0 => {
                return Err(ConstituentSeparationBatchError::MissingResidueConstituent {
                    material: residue,
                });
            }
            None if !has_non_target => {
                return Err(ConstituentSeparationBatchError::MissingNonTargetConstituent);
            }
            Some(_) | None => {}
        }
        let particle_size = profile
            .particle_size_distribution()
            .cloned()
            .unwrap_or_else(|| {
                unreachable!(
                    "authored constituent-separation input form requires particulate state"
                )
            });
        let key = (profile.temperature(), particle_size);
        selected_mass = selected_mass
            .checked_add(trace.mass())
            .ok_or(ConstituentSeparationBatchError::MassOverflow)?;
        grouped
            .entry(key)
            .or_insert_with(ExactInputProfile::new)
            .add_trace(trace)?;
    }

    let mut target_by_profile = BTreeMap::new();
    let mut residue_by_profile = BTreeMap::<ParticulateOutputKey, Mass>::new();
    let mut target_mass = Mass::ZERO;
    let mut residue_mass = Mass::ZERO;
    for ((temperature, particle_size), mut input) in grouped {
        let denominator = u128::from(COMPOSITION_PARTS_PER_MILLION);
        let exact_target_numerator = input
            .constituent_numerators
            .get(&definition.target_material())
            .copied()
            .unwrap_or_else(|| unreachable!("validated separation input contains target matter"));
        let recovered_target_milligrams = u64::try_from(
            exact_target_numerator * u128::from(definition.target_recovery_ppm())
                / (denominator * denominator),
        )
        .map_err(|_| ConstituentSeparationBatchError::MassOverflow)?;
        let recovered_target_numerator = u128::from(recovered_target_milligrams) * denominator;
        let remaining_target_numerator = exact_target_numerator
            .checked_sub(recovered_target_numerator)
            .unwrap_or_else(|| unreachable!("floored recovery cannot exceed exact target matter"));
        input
            .constituent_numerators
            .insert(definition.target_material(), remaining_target_numerator);
        let group_target = Mass::from_milligrams(recovered_target_milligrams);
        let group_residue = input
            .mass
            .checked_sub(group_target)
            .unwrap_or_else(|| unreachable!("constituent projection cannot exceed selected mass"));
        debug_assert!(!group_residue.is_zero());
        if !group_target.is_zero() {
            let target_particle_size = match target_particle_size_policy {
                ParticleSizeStatePolicy::Required => Some(particle_size.clone()),
                ParticleSizeStatePolicy::Untracked => None,
            };
            add_target_mass(
                &mut target_by_profile,
                temperature,
                target_particle_size,
                group_target,
            )?;
        }

        if let Some(residue_material) = definition.residue_material() {
            let mut whole_component_milligrams = 0_u64;
            let mut remainders = Vec::with_capacity(input.constituent_numerators.len());
            for (material, numerator) in input.constituent_numerators {
                let whole = u64::try_from(numerator / denominator)
                    .map_err(|_| ConstituentSeparationBatchError::MassOverflow)?;
                let remainder = numerator % denominator;
                whole_component_milligrams = whole_component_milligrams
                    .checked_add(whole)
                    .ok_or(ConstituentSeparationBatchError::MassOverflow)?;
                if whole != 0 {
                    add_particulate_mass(
                        &mut residue_by_profile,
                        CommodityKey::new(material, definition.residue_output_form()),
                        temperature,
                        MaterialComposition::pure(material),
                        particle_size.clone(),
                        Mass::from_milligrams(whole),
                    )?;
                }
                remainders.push((material, remainder));
            }

            let boundary_milligrams = group_residue
                .milligrams()
                .checked_sub(whole_component_milligrams)
                .unwrap_or_else(|| unreachable!("component floors cannot exceed selected mass"));
            for _ in 0..boundary_milligrams {
                let mut remaining_ppm = denominator;
                let mut boundary_components = Vec::new();
                for (material, remainder) in &mut remainders {
                    if *remainder == 0 || remaining_ppm == 0 {
                        continue;
                    }
                    let taken = (*remainder).min(remaining_ppm);
                    let ppm = u32::try_from(taken)
                        .unwrap_or_else(|_| unreachable!("boundary component is normalized ppm"));
                    boundary_components.push(CompositionComponent::new(*material, ppm));
                    *remainder -= taken;
                    remaining_ppm -= taken;
                }
                assert_eq!(
                    remaining_ppm, 0,
                    "composition remainders must exactly fill each boundary milligram"
                );
                let boundary_composition = MaterialComposition::new(boundary_components)
                    .unwrap_or_else(|error| {
                        unreachable!(
                            "bounded separation boundary composition must be valid: {error}"
                        )
                    });
                add_particulate_mass(
                    &mut residue_by_profile,
                    CommodityKey::new(residue_material, definition.residue_output_form()),
                    temperature,
                    boundary_composition,
                    particle_size.clone(),
                    Mass::from_milligrams(1),
                )?;
            }
            debug_assert!(remainders.iter().all(|(_, remainder)| *remainder == 0));
        } else {
            add_blended_concentration_residue(
                &mut residue_by_profile,
                definition,
                temperature,
                particle_size.clone(),
                input.constituent_numerators,
                group_residue,
            )?;
        }
        target_mass = target_mass
            .checked_add(group_target)
            .ok_or(ConstituentSeparationBatchError::MassOverflow)?;
        residue_mass = residue_mass
            .checked_add(group_residue)
            .ok_or(ConstituentSeparationBatchError::MassOverflow)?;
    }

    if target_mass.is_zero() {
        return Err(ConstituentSeparationBatchError::TargetBelowMassResolution {
            material: definition.target_material(),
            selected: selected_mass,
        });
    }

    Ok(SeparationOutputs {
        target: build_target_outputs(
            target_by_profile,
            CommodityKey::new(
                definition.target_material(),
                definition.target_output_form(),
            ),
        )?,
        residue: build_particulate_outputs(residue_by_profile)?,
        target_mass,
        residue_mass,
    })
}

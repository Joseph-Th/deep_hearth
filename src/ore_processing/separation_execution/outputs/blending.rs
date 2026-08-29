//! Exact particulate blending and deterministic ppm-remainder reconstruction.

use std::collections::{BTreeMap, BTreeSet};

use crate::core::quantity::{Mass, Temperature};
use crate::material::{
    COMPOSITION_PARTS_PER_MILLION, CommodityKey, CompositionComponent, MaterialComposition,
    MaterialId, MaterialLotSpec, ParticleSizeDistribution,
};
use crate::ore_processing::definitions::ConstituentSeparationPhysics;

use super::ConstituentSeparationBatchError;

pub(super) type ParticulateOutputKey = (
    CommodityKey,
    Temperature,
    MaterialComposition,
    ParticleSizeDistribution,
);

pub(super) fn add_particulate_mass(
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

struct BlendedPpmProjection {
    base_ppm: BTreeMap<MaterialId, u32>,
    remainders: Vec<(MaterialId, u64)>,
    missing_ppm: u64,
}

fn project_blended_ppm(
    constituent_numerators: BTreeMap<MaterialId, u128>,
    stream_milligrams: u64,
) -> Result<BlendedPpmProjection, ConstituentSeparationBatchError> {
    let stream_milligrams_u128 = u128::from(stream_milligrams);
    let mut base_ppm = BTreeMap::new();
    let mut remainders = Vec::new();
    let mut base_total = 0_u64;
    let mut remainder_total = 0_u128;
    for (material, numerator) in constituent_numerators {
        let base = u32::try_from(numerator / stream_milligrams_u128)
            .map_err(|_| ConstituentSeparationBatchError::MassOverflow)?;
        let remainder = u64::try_from(numerator % stream_milligrams_u128)
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
        stream_milligrams_u128 * u128::from(missing_ppm),
        "exact stream remainders must equal the normalized ppm deficit"
    );
    Ok(BlendedPpmProjection {
        base_ppm,
        remainders,
        missing_ppm,
    })
}

fn interval_composition(
    base_ppm: &BTreeMap<MaterialId, u32>,
    active: &BTreeSet<MaterialId>,
) -> MaterialComposition {
    let materials = base_ppm
        .keys()
        .copied()
        .chain(active.iter().copied())
        .collect::<BTreeSet<_>>();
    let components = materials
        .into_iter()
        .filter_map(|material| {
            let ppm = base_ppm.get(&material).copied().unwrap_or(0)
                + u32::from(active.contains(&material));
            (ppm != 0).then_some(CompositionComponent::new(material, ppm))
        })
        .collect();
    MaterialComposition::new(components).unwrap_or_else(|error| {
        unreachable!("exact blended particulate stream must be normalized: {error}")
    })
}

type CorrectionEvents = BTreeMap<u64, Vec<(MaterialId, bool)>>;

fn build_correction_events(
    remainders: Vec<(MaterialId, u64)>,
    stream_milligrams: u64,
    missing_ppm: u64,
) -> Result<CorrectionEvents, ConstituentSeparationBatchError> {
    let stream_milligrams_u128 = u128::from(stream_milligrams);
    let mut events = CorrectionEvents::new();
    events.entry(0).or_default();
    events.entry(stream_milligrams).or_default();
    let mut cursor = 0_u128;
    for (material, remainder) in remainders {
        let start = u64::try_from(cursor % stream_milligrams_u128)
            .map_err(|_| ConstituentSeparationBatchError::MassOverflow)?;
        let end_absolute = u128::from(start) + u128::from(remainder);
        if end_absolute <= stream_milligrams_u128 {
            let end = u64::try_from(end_absolute)
                .map_err(|_| ConstituentSeparationBatchError::MassOverflow)?;
            events.entry(start).or_default().push((material, true));
            events.entry(end).or_default().push((material, false));
        } else {
            let end = u64::try_from(end_absolute - stream_milligrams_u128)
                .map_err(|_| ConstituentSeparationBatchError::MassOverflow)?;
            events.entry(0).or_default().push((material, true));
            events.entry(end).or_default().push((material, false));
            events.entry(start).or_default().push((material, true));
            events
                .entry(stream_milligrams)
                .or_default()
                .push((material, false));
        }
        cursor = cursor
            .checked_add(u128::from(remainder))
            .ok_or(ConstituentSeparationBatchError::MassOverflow)?;
    }
    assert_eq!(
        cursor,
        stream_milligrams_u128 * u128::from(missing_ppm),
        "stream correction schedule must cover an exact number of mass laps"
    );
    Ok(events)
}

/// Reconstructs one blended particulate stream without fabricating purified constituent lots.
///
/// Constituent numerators use ppm-mg units. Dividing each exact numerator by the stream mass gives
/// a common integer-ppm base composition plus a bounded set of per-ppm remainders. The remainder
/// schedule below distributes those +1 ppm corrections over deterministic mass intervals. This
/// preserves every constituent numerator exactly while keeping every output lot at, or within one
/// ppm of, the aggregate stream assay.
pub(super) fn add_blended_particulate_stream<F>(
    grouped: &mut BTreeMap<ParticulateOutputKey, Mass>,
    temperature: Temperature,
    particle_size: ParticleSizeDistribution,
    constituent_numerators: BTreeMap<MaterialId, u128>,
    stream_mass: Mass,
    commodity_for_composition: F,
) -> Result<(), ConstituentSeparationBatchError>
where
    F: Fn(&MaterialComposition) -> CommodityKey,
{
    let stream_milligrams = stream_mass.milligrams();
    let BlendedPpmProjection {
        base_ppm,
        remainders,
        missing_ppm,
    } = project_blended_ppm(constituent_numerators, stream_milligrams)?;

    let mut emit_interval = |active: &BTreeSet<MaterialId>, interval_mass: u64| {
        if interval_mass == 0 {
            return Ok(());
        }
        let composition = interval_composition(&base_ppm, active);
        add_particulate_mass(
            grouped,
            commodity_for_composition(&composition),
            temperature,
            composition,
            particle_size.clone(),
            Mass::from_milligrams(interval_mass),
        )
    };

    if missing_ppm == 0 {
        return emit_interval(&BTreeSet::new(), stream_milligrams);
    }

    // Concatenating each constituent's remainder over `missing_ppm` complete stream-mass laps and
    // projecting those positions modulo the stream mass yields a 0/1 correction matrix with exact
    // column sums and exactly `missing_ppm` corrections on every represented milligram. Sweeping
    // interval boundaries compresses that matrix to O(constituent-count) lots rather than one lot
    // per milligram.
    let events = build_correction_events(remainders, stream_milligrams, missing_ppm)?;
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

fn dominant_non_target_material(
    constituent_numerators: &BTreeMap<MaterialId, u128>,
    target_material: MaterialId,
) -> MaterialId {
    constituent_numerators
        .iter()
        .filter(|(material, numerator)| **material != target_material && **numerator != 0)
        .max_by(
            |(left_material, left_numerator), (right_material, right_numerator)| {
                left_numerator
                    .cmp(right_numerator)
                    .then_with(|| right_material.cmp(left_material))
            },
        )
        .map(|(material, _)| *material)
        .unwrap_or_else(|| unreachable!("separation residue must retain non-target gangue"))
}

pub(super) fn add_blended_residue(
    grouped: &mut BTreeMap<ParticulateOutputKey, Mass>,
    definition: ConstituentSeparationPhysics,
    temperature: Temperature,
    particle_size: ParticleSizeDistribution,
    constituent_numerators: BTreeMap<MaterialId, u128>,
    residue_mass: Mass,
) -> Result<(), ConstituentSeparationBatchError> {
    let target_material = definition.target_material();
    let residue_form = definition.residue_output_form();
    let host = dominant_non_target_material(&constituent_numerators, target_material);
    add_blended_particulate_stream(
        grouped,
        temperature,
        particle_size,
        constituent_numerators,
        residue_mass,
        |_| CommodityKey::new(host, residue_form),
    )
}

pub(super) fn build_particulate_outputs(
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

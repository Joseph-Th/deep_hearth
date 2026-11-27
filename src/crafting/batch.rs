//! Shared runtime and persistence validation for exact manual-crafting material batches.

use std::collections::BTreeMap;
use std::num::NonZeroU64;

use crate::core::quantity::{Mass, Temperature};
use crate::inventory::ConsumedMaterialTrace;
use crate::material::{CommodityKey, MaterialComposition, MaterialLotSpec, MaterialLotSpecError};

use super::ManualCraftDefinition;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ManualCraftBatchError {
    EmptyInput,
    InputCommodityMismatch,
    InputCompositionMismatch,
    InputMassNotWholeBatches { consumed: Mass, batch_mass: Mass },
}

/// Failure while scaling authored manual-craft outputs by a validated batch count.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ManualCraftOutputError {
    MassOverflow {
        commodity: CommodityKey,
        batches: NonZeroU64,
    },
    Construction {
        commodity: CommodityKey,
        error: MaterialLotSpecError,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ManualCraftThermalCohort {
    temperature: Temperature,
    mass: Mass,
}

fn apportion_output_across_thermal_cohorts(
    cohorts: &mut [ManualCraftThermalCohort],
    output_mass: Mass,
    remaining_total: Mass,
) -> Vec<ManualCraftThermalCohort> {
    let denominator = u128::from(remaining_total.milligrams());
    let output_mg = u128::from(output_mass.milligrams());
    let mut prefix_mg = 0_u128;
    let mut apportioned_through_mg = 0_u128;
    let mut apportioned = Vec::new();
    for cohort in cohorts {
        prefix_mg = prefix_mg
            .checked_add(u128::from(cohort.mass.milligrams()))
            .unwrap_or_else(|| unreachable!("thermal-cohort prefix fits u64 input mass"));
        let next_apportioned_through_mg = prefix_mg
            .checked_mul(output_mg)
            .unwrap_or_else(|| unreachable!("u64-by-u64 thermal apportionment fits u128"))
            / denominator;
        let share_mg = next_apportioned_through_mg
            .checked_sub(apportioned_through_mg)
            .unwrap_or_else(|| unreachable!("thermal apportionment is monotonic"));
        apportioned_through_mg = next_apportioned_through_mg;
        if share_mg == 0 {
            continue;
        }
        let share =
            Mass::from_milligrams(u64::try_from(share_mg).unwrap_or_else(|_| {
                unreachable!("apportioned share cannot exceed u64 input mass")
            }));
        cohort.mass = cohort.mass.checked_sub(share).unwrap_or_else(|| {
            unreachable!("proportional output share cannot exceed its remaining thermal cohort")
        });
        apportioned.push(ManualCraftThermalCohort {
            temperature: cohort.temperature,
            mass: share,
        });
    }
    assert_eq!(
        apportioned_through_mg, output_mg,
        "thermal-cohort apportionment must exhaust the authored output mass"
    );
    apportioned
}

/// Scales authored outputs while retaining every exact input-temperature cohort.
///
/// Manual shaping changes form, not phase or material identity. Each output form receives the same
/// remaining temperature distribution, subject only to exact whole-milligram apportionment. This
/// preserves exact material thermal energy without a fictitious heat-exchange step and prevents a
/// recipe from acting as an implicit temperature sorter merely because its outputs are ordered.
pub(super) fn build_manual_craft_outputs(
    definition: &ManualCraftDefinition,
    batch: &ManualCraftBatch,
) -> Result<Vec<MaterialLotSpec>, ManualCraftOutputError> {
    let batches = batch.batches();
    let scaled_outputs = definition
        .outputs()
        .iter()
        .map(|output| {
            let mass = output
                .mass()
                .milligrams()
                .checked_mul(batches.get())
                .map(Mass::from_milligrams)
                .ok_or(ManualCraftOutputError::MassOverflow {
                    commodity: output.commodity(),
                    batches,
                })?;
            Ok((output.commodity(), mass))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let mut outputs = Vec::new();
    let mut remaining_cohorts = batch.thermal_cohorts.clone();
    let mut remaining_total = remaining_cohorts
        .iter()
        .try_fold(Mass::ZERO, |total, cohort| total.checked_add(cohort.mass))
        .unwrap_or_else(|| unreachable!("validated manual-craft input mass already fits Mass"));
    for (commodity, output_mass) in scaled_outputs {
        let apportioned = apportion_output_across_thermal_cohorts(
            &mut remaining_cohorts,
            output_mass,
            remaining_total,
        );
        for cohort in apportioned {
            outputs.push(
                MaterialLotSpec::with_composition(
                    commodity,
                    cohort.mass,
                    cohort.temperature,
                    MaterialComposition::pure(commodity.material()),
                )
                .map_err(|error| ManualCraftOutputError::Construction { commodity, error })?,
            );
        }
        remaining_total = remaining_total.checked_sub(output_mass).unwrap_or_else(|| {
            unreachable!("conserved manual-craft output cannot exceed remaining input")
        });
    }
    assert!(
        remaining_total.is_zero() && remaining_cohorts.iter().all(|cohort| cohort.mass.is_zero()),
        "conserved manual-craft outputs must exhaust every input thermal cohort"
    );
    Ok(outputs)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ManualCraftBatch {
    batches: NonZeroU64,
    thermal_cohorts: Vec<ManualCraftThermalCohort>,
}

impl ManualCraftBatch {
    pub(super) const fn batches(&self) -> NonZeroU64 {
        self.batches
    }
}

fn validate_trace_profiles(
    definition: &ManualCraftDefinition,
    traces: &[ConsumedMaterialTrace],
) -> Result<Vec<ManualCraftThermalCohort>, ManualCraftBatchError> {
    if traces.is_empty() {
        return Err(ManualCraftBatchError::EmptyInput);
    }
    let expected_composition = MaterialComposition::pure(definition.input().material());
    let mut mass_by_temperature = BTreeMap::<Temperature, Mass>::new();
    for trace in traces {
        if trace.profile().commodity() != definition.input() {
            return Err(ManualCraftBatchError::InputCommodityMismatch);
        }
        if trace.profile().composition() != &expected_composition {
            return Err(ManualCraftBatchError::InputCompositionMismatch);
        }
        let cohort_mass = mass_by_temperature
            .entry(trace.profile().temperature())
            .or_insert(Mass::ZERO);
        *cohort_mass = cohort_mass.checked_add(trace.mass()).unwrap_or_else(|| {
            panic!("validated manual-craft input trace mass overflowed its consumed total")
        });
    }
    Ok(mass_by_temperature
        .into_iter()
        .map(|(temperature, mass)| ManualCraftThermalCohort { temperature, mass })
        .collect())
}

fn validate_batch_count(
    definition: &ManualCraftDefinition,
    consumed_mass: Mass,
) -> Result<NonZeroU64, ManualCraftBatchError> {
    let batch_mass = definition.input_mass();
    let quotient = consumed_mass.milligrams() / batch_mass.milligrams();
    let remainder = consumed_mass.milligrams() % batch_mass.milligrams();
    NonZeroU64::new(quotient).filter(|_| remainder == 0).ok_or(
        ManualCraftBatchError::InputMassNotWholeBatches {
            consumed: consumed_mass,
            batch_mass,
        },
    )
}

pub(super) fn validate_manual_craft_batch(
    definition: &ManualCraftDefinition,
    consumed_mass: Mass,
    traces: &[ConsumedMaterialTrace],
) -> Result<ManualCraftBatch, ManualCraftBatchError> {
    let thermal_cohorts = validate_trace_profiles(definition, traces)?;
    let batches = validate_batch_count(definition, consumed_mass)?;
    Ok(ManualCraftBatch {
        batches,
        thermal_cohorts,
    })
}

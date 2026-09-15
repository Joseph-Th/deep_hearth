//! Shared runtime and persistence validation for exact manual-crafting material batches.

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
    MixedInputTemperature,
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

/// Scales authored outputs by validated batch count with one shared mass/composition rule.
pub(super) fn build_manual_craft_outputs(
    definition: &ManualCraftDefinition,
    batches: NonZeroU64,
    temperature: Temperature,
) -> Result<Vec<MaterialLotSpec>, ManualCraftOutputError> {
    definition
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
            MaterialLotSpec::with_composition(
                output.commodity(),
                mass,
                temperature,
                MaterialComposition::pure(output.commodity().material()),
            )
            .map_err(|error| ManualCraftOutputError::Construction {
                commodity: output.commodity(),
                error,
            })
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ManualCraftBatch {
    batches: NonZeroU64,
    temperature: Temperature,
}

impl ManualCraftBatch {
    pub(super) const fn batches(self) -> NonZeroU64 {
        self.batches
    }

    pub(super) const fn temperature(self) -> Temperature {
        self.temperature
    }
}

fn validate_trace_profiles(
    definition: &ManualCraftDefinition,
    traces: &[ConsumedMaterialTrace],
) -> Result<Temperature, ManualCraftBatchError> {
    let Some(first) = traces.first() else {
        return Err(ManualCraftBatchError::EmptyInput);
    };
    let expected_composition = MaterialComposition::pure(definition.input().material());
    let temperature = first.profile().temperature();
    for trace in traces {
        if trace.profile().commodity() != definition.input() {
            return Err(ManualCraftBatchError::InputCommodityMismatch);
        }
        if trace.profile().composition() != &expected_composition {
            return Err(ManualCraftBatchError::InputCompositionMismatch);
        }
        if trace.profile().temperature() != temperature {
            return Err(ManualCraftBatchError::MixedInputTemperature);
        }
    }
    Ok(temperature)
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
    let temperature = validate_trace_profiles(definition, traces)?;
    let batches = validate_batch_count(definition, consumed_mass)?;
    Ok(ManualCraftBatch {
        batches,
        temperature,
    })
}

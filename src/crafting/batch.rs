//! Shared runtime and persistence validation for exact manual-crafting material batches.

use std::num::NonZeroU64;

use crate::core::quantity::{Mass, Temperature};
use crate::inventory::ConsumedMaterialTrace;
use crate::material::MaterialComposition;

use super::ManualCraftDefinition;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ManualCraftBatchError {
    InputCommodityMismatch,
    InputCompositionMismatch,
    MixedInputTemperature,
    InputMassNotWholeBatches { consumed: Mass, batch_mass: Mass },
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

pub(super) fn validate_manual_craft_batch(
    definition: &ManualCraftDefinition,
    consumed_mass: Mass,
    traces: &[ConsumedMaterialTrace],
) -> Result<ManualCraftBatch, ManualCraftBatchError> {
    let expected_composition = MaterialComposition::pure(definition.input().material());
    let mut temperature = None;
    for trace in traces {
        if trace.profile().commodity() != definition.input() {
            return Err(ManualCraftBatchError::InputCommodityMismatch);
        }
        if trace.profile().composition() != &expected_composition {
            return Err(ManualCraftBatchError::InputCompositionMismatch);
        }
        match temperature {
            Some(existing) if existing != trace.profile().temperature() => {
                return Err(ManualCraftBatchError::MixedInputTemperature);
            }
            Some(_) => {}
            None => temperature = Some(trace.profile().temperature()),
        }
    }
    let batch_mass = definition.input_mass();
    let quotient = consumed_mass.milligrams() / batch_mass.milligrams();
    let remainder = consumed_mass.milligrams() % batch_mass.milligrams();
    let Some(batches) = NonZeroU64::new(quotient) else {
        return Err(ManualCraftBatchError::InputMassNotWholeBatches {
            consumed: consumed_mass,
            batch_mass,
        });
    };
    if remainder != 0 {
        return Err(ManualCraftBatchError::InputMassNotWholeBatches {
            consumed: consumed_mass,
            batch_mass,
        });
    }
    let Some(temperature) = temperature else {
        return Err(ManualCraftBatchError::InputCommodityMismatch);
    };
    Ok(ManualCraftBatch {
        batches,
        temperature,
    })
}

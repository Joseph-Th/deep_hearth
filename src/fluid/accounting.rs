//! Read-only aggregate fluid-volume projection over sibling persistent fluid-store records.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::AggregateVolume;
use crate::core::state::AppState;

use super::definitions::FluidDefinitionId;

/// World-scale finite fluid volume grouped by authored fluid identity, including fluid transferred
/// into the terminal survival-consumption conservation boundary.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FluidVolumeAccounting {
    by_fluid: BTreeMap<FluidDefinitionId, AggregateVolume>,
    total: AggregateVolume,
}

impl FluidVolumeAccounting {
    #[must_use]
    pub fn get_volume(&self, fluid: FluidDefinitionId) -> AggregateVolume {
        self.by_fluid
            .get(&fluid)
            .copied()
            .unwrap_or(AggregateVolume::ZERO)
    }

    #[must_use]
    pub const fn total(&self) -> AggregateVolume {
        self.total
    }
}

/// Overflow while projecting world-scale fluid volume.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FluidVolumeAccountingError {
    FluidVolumeOverflow { fluid: FluidDefinitionId },
    TotalVolumeOverflow,
}

impl Display for FluidVolumeAccountingError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FluidVolumeOverflow { fluid } => write!(
                formatter,
                "stored fluid {} exceeds aggregate volume range",
                fluid.value()
            ),
            Self::TotalVolumeOverflow => {
                formatter.write_str("total stored fluid exceeds aggregate volume range")
            }
        }
    }
}

impl Error for FluidVolumeAccountingError {}

/// Recomputes finite world fluid volume without trusting any cached aggregate.
pub fn calculate_fluid_volume_accounting(
    state: &AppState,
) -> Result<FluidVolumeAccounting, FluidVolumeAccountingError> {
    let mut by_fluid = BTreeMap::<FluidDefinitionId, AggregateVolume>::new();
    let mut total = AggregateVolume::ZERO;
    for store in state.fluid().stores() {
        let Some(contents) = store.contents() else {
            continue;
        };
        let volume = AggregateVolume::from_volume(contents.volume());
        let current = by_fluid
            .get(&contents.fluid())
            .copied()
            .unwrap_or(AggregateVolume::ZERO);
        let next =
            current
                .checked_add(volume)
                .ok_or(FluidVolumeAccountingError::FluidVolumeOverflow {
                    fluid: contents.fluid(),
                })?;
        by_fluid.insert(contents.fluid(), next);
        total = total
            .checked_add(volume)
            .ok_or(FluidVolumeAccountingError::TotalVolumeOverflow)?;
    }
    for (fluid, volume) in state.survival().consumed_fluids() {
        let current = by_fluid
            .get(&fluid)
            .copied()
            .unwrap_or(AggregateVolume::ZERO);
        let next = current
            .checked_add(volume)
            .ok_or(FluidVolumeAccountingError::FluidVolumeOverflow { fluid })?;
        by_fluid.insert(fluid, next);
        total = total
            .checked_add(volume)
            .ok_or(FluidVolumeAccountingError::TotalVolumeOverflow)?;
    }
    Ok(FluidVolumeAccounting { by_fluid, total })
}

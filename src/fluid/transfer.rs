//! Exact same-voxel manual decant between existing finite fluid stores.
//!
//! This is a player-local operation, not a pipe, pump, or remote fluid network.
//! Neither endpoint may disappear, change its phase, or silently round away
//! thermal energy during a temperature blend. Both stores and supported weights
//! change atomically, and exact explicit heat is conserved.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Temperature, Volume};
use crate::core::state::AppState;
use crate::logistics::{PlayerFluidStoreAccessError, validate_player_fluid_store_access};
use crate::player::{PlayerActionGuard, validate_player_action};
use crate::registry::Registries;
use crate::structural::{StructuralAnalysis, StructuralCommitError, StructuralMutationOutcome};

use super::state::FluidContents;
use super::structural_integration::{
    FluidContentsChange, FluidStructuralLoadError, ValidatedFluidStructuralLoad,
    validate_fluid_contents_changes, validate_unreserved_fluid_structural_load_headroom,
};
use super::{FluidDefinitionId, FluidStoreId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FluidTransferError {
    PlayerNotAdmitted,
    PlayerDead,
    SourceAccess(PlayerFluidStoreAccessError),
    DestinationAccess(PlayerFluidStoreAccessError),
    UnknownStore {
        store: FluidStoreId,
    },
    SameStore,
    ZeroVolume,
    EmptySource {
        store: FluidStoreId,
    },
    InsufficientVolume {
        available: Volume,
        requested: Volume,
    },
    UnknownDefinition {
        definition: FluidDefinitionId,
    },
    IncompatibleFluid {
        expected: FluidDefinitionId,
        found: FluidDefinitionId,
    },
    UnrepresentableEquilibriumTemperature {
        weighted_millikelvin_microliters: u128,
        combined_volume: Volume,
    },
    DestinationOverflow,
    InsufficientCapacity {
        available: Volume,
        requested: Volume,
    },
    RevisionExhausted,
    StructuralLoad(FluidStructuralLoadError),
}

impl Display for FluidTransferError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}
impl Error for FluidTransferError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FluidTransferCommitError {
    StaleFluidRevision { expected: u64, actual: u64 },
    StaleLogisticsRevision { expected: u64, actual: u64 },
    StaleSurvivalRevision { expected: u64, actual: u64 },
    ContentsChanged { store: FluidStoreId },
    Structure(StructuralCommitError),
}

impl Display for FluidTransferCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}
impl Error for FluidTransferCommitError {}

/// Computes same-fluid equilibrium without any external heat source or sink.
///
/// At the current authored fixed-density, fixed-specific-heat fluid model,
/// sensible heat is proportional to volume times absolute millikelvin for a
/// given fluid identity. Latent heat depends only on volume and phase. A
/// weighted mean conserves both exact terms, but only if millikelvin arithmetic
/// divides evenly: rounding is forbidden.
fn equilibrium_temperature(
    source: FluidContents,
    destination: FluidContents,
    moved: Volume,
    combined_volume: Volume,
) -> Result<Temperature, FluidTransferError> {
    let weighted_source =
        u128::from(moved.microliters()) * u128::from(source.temperature().millikelvin());
    let weighted_destination = u128::from(destination.volume().microliters())
        * u128::from(destination.temperature().millikelvin());
    let weighted = weighted_source + weighted_destination;
    let divisor = u128::from(combined_volume.microliters());
    if weighted % divisor != 0 {
        return Err(FluidTransferError::UnrepresentableEquilibriumTemperature {
            weighted_millikelvin_microliters: weighted,
            combined_volume,
        });
    }
    let millikelvin = u32::try_from(weighted / divisor)
        .expect("a convex combination of two u32 temperatures fits u32");
    Ok(Temperature::from_millikelvin(millikelvin))
}

/// Revision-bound proof of an authorized single-voxel, two-store volume move.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub struct ValidatedFluidTransfer {
    source: FluidStoreId,
    destination: FluidStoreId,
    before_source: FluidContents,
    before_destination: Option<FluidContents>,
    after_source: Option<FluidContents>,
    after_destination: FluidContents,
    volume: Volume,
    expected_fluid_revision: u64,
    expected_logistics_revision: u64,
    player_action: PlayerActionGuard,
    structural: Option<ValidatedFluidStructuralLoad>,
}

/// Completed transfer and any structural consequence of its redistributed weight.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub struct FluidTransferOutcome {
    structural: Option<StructuralMutationOutcome>,
}

impl FluidTransferOutcome {
    #[must_use]
    pub fn structural_analysis(&self) -> Option<&StructuralAnalysis> {
        self.structural
            .as_ref()
            .map(StructuralMutationOutcome::analysis)
    }
}

impl ValidatedFluidTransfer {
    #[must_use]
    pub const fn volume(&self) -> Volume {
        self.volume
    }

    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<FluidTransferOutcome, FluidTransferCommitError> {
        let actual_logistics = state.logistics().revision();
        if actual_logistics != self.expected_logistics_revision {
            return Err(FluidTransferCommitError::StaleLogisticsRevision {
                expected: self.expected_logistics_revision,
                actual: actual_logistics,
            });
        }
        self.player_action.precheck(state).map_err(|stale| {
            FluidTransferCommitError::StaleSurvivalRevision {
                expected: stale.expected,
                actual: stale.actual,
            }
        })?;
        let actual_fluid = state.fluid().revision();
        if actual_fluid != self.expected_fluid_revision {
            return Err(FluidTransferCommitError::StaleFluidRevision {
                expected: self.expected_fluid_revision,
                actual: actual_fluid,
            });
        }
        for (store, expected) in [
            (self.source, Some(self.before_source)),
            (self.destination, self.before_destination),
        ] {
            if state
                .fluid()
                .get_store(store)
                .map(|record| record.contents())
                != Some(expected)
            {
                return Err(FluidTransferCommitError::ContentsChanged { store });
            }
        }
        let structural = match self.structural {
            Some(structural) => structural
                .commit(state)
                .map_err(FluidTransferCommitError::Structure)?,
            None => None,
        };
        state.fluid_state_mut().apply_store_transfer(
            self.source,
            self.after_source,
            self.destination,
            self.after_destination,
            self.expected_fluid_revision + 1,
        );
        Ok(FluidTransferOutcome { structural })
    }
}

/// Moves existing homogeneous liquid between two stores at the player's exact voxel.
///
/// Different fluid identities cannot be mixed. The same fluid at different
/// temperatures equilibrates at the exact volume-weighted temperature, or
/// refuses the transfer if the result would require sub-millikelvin rounding.
/// This assumes constant authored density and specific heat, and does not
/// model phase transitions or environmental heat exchange. An empty destination
/// adopts the source temperature. Structure receives the joint final load.
pub fn validate_transfer_fluid_between_local_stores(
    registries: &Registries,
    state: &AppState,
    source: FluidStoreId,
    destination: FluidStoreId,
    volume: Volume,
) -> Result<ValidatedFluidTransfer, FluidTransferError> {
    let player_action = validate_player_action(state).map_err(|error| match error {
        crate::player::PlayerActionValidationError::PlayerNotAdmitted => {
            FluidTransferError::PlayerNotAdmitted
        }
        crate::player::PlayerActionValidationError::PlayerDead => FluidTransferError::PlayerDead,
    })?;
    if source == destination {
        return Err(FluidTransferError::SameStore);
    }
    if volume.is_zero() {
        return Err(FluidTransferError::ZeroVolume);
    }
    let fluid = state.fluid();
    let source_record = fluid
        .get_store(source)
        .ok_or(FluidTransferError::UnknownStore { store: source })?;
    let destination_record = fluid
        .get_store(destination)
        .ok_or(FluidTransferError::UnknownStore { store: destination })?;
    validate_player_fluid_store_access(state, source).map_err(FluidTransferError::SourceAccess)?;
    validate_player_fluid_store_access(state, destination)
        .map_err(FluidTransferError::DestinationAccess)?;
    let before_source = source_record
        .contents()
        .ok_or(FluidTransferError::EmptySource { store: source })?;
    if registries
        .fluid()
        .get_fluid(before_source.fluid())
        .is_none()
    {
        return Err(FluidTransferError::UnknownDefinition {
            definition: before_source.fluid(),
        });
    }
    let remaining = before_source.volume().checked_sub(volume).ok_or(
        FluidTransferError::InsufficientVolume {
            available: before_source.volume(),
            requested: volume,
        },
    )?;
    let before_destination = destination_record.contents();
    if let Some(existing) = before_destination {
        if existing.fluid() != before_source.fluid() {
            return Err(FluidTransferError::IncompatibleFluid {
                expected: existing.fluid(),
                found: before_source.fluid(),
            });
        }
    }
    let available = destination_record
        .capacity()
        .checked_sub(destination_record.stored_volume())
        .ok_or(FluidTransferError::DestinationOverflow)?;
    if volume > available {
        return Err(FluidTransferError::InsufficientCapacity {
            available,
            requested: volume,
        });
    }
    let next_destination_volume = destination_record
        .stored_volume()
        .checked_add(volume)
        .ok_or(FluidTransferError::DestinationOverflow)?;
    let destination_temperature = match before_destination {
        Some(existing) => {
            equilibrium_temperature(before_source, existing, volume, next_destination_volume)?
        }
        None => before_source.temperature(),
    };
    let after_source = (!remaining.is_zero()).then_some(FluidContents {
        fluid: before_source.fluid(),
        volume: remaining,
        temperature: before_source.temperature(),
    });
    let after_destination = FluidContents {
        fluid: before_source.fluid(),
        volume: next_destination_volume,
        temperature: destination_temperature,
    };
    let structural = validate_fluid_contents_changes(
        registries,
        state,
        [
            FluidContentsChange::new(source, after_source),
            FluidContentsChange::new(destination, Some(after_destination)),
        ],
    )
    .map_err(FluidTransferError::StructuralLoad)?;
    validate_unreserved_fluid_structural_load_headroom(state, structural.as_ref())
        .map_err(FluidTransferError::StructuralLoad)?;
    fluid
        .revision()
        .checked_add(1)
        .ok_or(FluidTransferError::RevisionExhausted)?;
    Ok(ValidatedFluidTransfer {
        source,
        destination,
        before_source,
        before_destination,
        after_source,
        after_destination,
        volume,
        expected_fluid_revision: fluid.revision(),
        expected_logistics_revision: state.logistics().revision(),
        player_action,
        structural,
    })
}

#[cfg(test)]
#[path = "transfer_tests.rs"]
mod tests;

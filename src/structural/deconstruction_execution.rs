//! Conserved deconstruction and damaged-member recovery into inventory.
//!
//! Undamaged members preserve embodied traces exactly. Cracked or failed members irreversibly reform
//! those same traces into the structural profile's authored damaged-recovery form so failure cannot
//! become a free pristine-material reset. Direct structural deletion can never destroy matter.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::{
    MaterialIngressEntry, MaterialIngressError, MaterialLotId, StockpileId, StockpileStorageError,
    StockpileStoredMassChange, StockpileStructuralLoadError, ValidatedMaterialIngress,
    apply_material_ingress, resolve_stockpile_stored_loads, validate_material_ingress,
};
use crate::registry::Registries;

use super::state::{StructuralElementId, StructuralLoadKind};
use super::structural_execution::{
    StructuralCommitError, StructuralMutationError, StructuralMutationOutcome,
    ValidatedStructuralRemovalWithLoads, validate_remove_structural_element_with_owned_loads,
};

/// Opaque result of a future dismantling/demolition authorization system.
///
/// There is no public constructor. At present the canonical transaction returns the member's exact
/// embodied traces to inventory; tool/labor/time and non-identity salvage physics remain separate.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub struct StructuralDeconstructionResolution {
    element: StructuralElementId,
    destination: StockpileId,
}

impl StructuralDeconstructionResolution {
    #[must_use]
    pub const fn element(&self) -> StructuralElementId {
        self.element
    }

    #[must_use]
    pub const fn destination(&self) -> StockpileId {
        self.destination
    }
}

/// Failure while validating a resolved structural deconstruction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StructuralDeconstructionError {
    UnknownElement {
        element: StructuralElementId,
    },
    UnknownProfile {
        element: StructuralElementId,
        profile: super::StructuralProfileId,
    },
    NoEmbodiedMatter {
        element: StructuralElementId,
    },
    UnknownDestination {
        stockpile: StockpileId,
    },
    InvalidEmbodiedMatter {
        element: StructuralElementId,
    },
    DestinationStorage(StockpileStorageError),
    DestinationMassOverflow {
        stockpile: StockpileId,
    },
    DestinationCapacityExceeded {
        stockpile: StockpileId,
        capacity: Mass,
        committed: Mass,
        requested: Mass,
    },
    LotIdExhausted,
    InventoryRevisionExhausted,
    StoredMatterLoad(StockpileStructuralLoadError),
    Structure(StructuralMutationError),
}

impl Display for StructuralDeconstructionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownElement { element } => {
                write!(formatter, "unknown structural element {}", element.value())
            }
            Self::UnknownProfile { element, profile } => write!(
                formatter,
                "structural element {} references unknown profile {} during recovery",
                element.value(),
                profile.value()
            ),
            Self::NoEmbodiedMatter { element } => write!(
                formatter,
                "structural element {} has no embodied matter to recover",
                element.value()
            ),
            Self::UnknownDestination { stockpile } => write!(
                formatter,
                "structural deconstruction destination stockpile {} does not exist",
                stockpile.value()
            ),
            Self::InvalidEmbodiedMatter { element } => write!(
                formatter,
                "structural element {} contains embodied matter that cannot enter inventory",
                element.value()
            ),
            Self::DestinationStorage(error) => write!(
                formatter,
                "structural recovery destination rejects embodied material: {error}"
            ),
            Self::DestinationMassOverflow { stockpile } => write!(
                formatter,
                "structural recovery overflows stockpile {} mass accounting",
                stockpile.value()
            ),
            Self::DestinationCapacityExceeded {
                stockpile,
                capacity,
                committed,
                requested,
            } => write!(
                formatter,
                "structural recovery exceeds stockpile {} capacity {} mg: {} mg committed, {} mg requested",
                stockpile.value(),
                capacity.milligrams(),
                committed.milligrams(),
                requested.milligrams()
            ),
            Self::LotIdExhausted => {
                formatter.write_str("material lot identifier space is exhausted during recovery")
            }
            Self::InventoryRevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted during recovery")
            }
            Self::StoredMatterLoad(error) => write!(
                formatter,
                "structural recovery cannot update destination stored-matter load: {error}"
            ),
            Self::Structure(error) => {
                write!(formatter, "structural removal cannot proceed: {error}")
            }
        }
    }
}

impl Error for StructuralDeconstructionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::DestinationStorage(error) => Some(error),
            Self::StoredMatterLoad(error) => Some(error),
            Self::UnknownElement { .. }
            | Self::UnknownProfile { .. }
            | Self::NoEmbodiedMatter { .. }
            | Self::InvalidEmbodiedMatter { .. } => None,
            Self::UnknownDestination {
                stockpile: _stockpile,
            }
            | Self::DestinationMassOverflow {
                stockpile: _stockpile,
            } => None,
            Self::DestinationCapacityExceeded {
                stockpile: _stockpile,
                capacity: _capacity,
                committed: _committed,
                requested: _requested,
            } => None,
            Self::LotIdExhausted | Self::InventoryRevisionExhausted => None,
        }
    }
}

fn map_ingress_error(
    element: StructuralElementId,
    error: MaterialIngressError,
) -> StructuralDeconstructionError {
    match error {
        MaterialIngressError::Empty => StructuralDeconstructionError::NoEmbodiedMatter { element },
        MaterialIngressError::UnknownStockpile { stockpile } => {
            StructuralDeconstructionError::UnknownDestination { stockpile }
        }
        MaterialIngressError::MassOverflow { stockpile } => {
            StructuralDeconstructionError::DestinationMassOverflow { stockpile }
        }
        MaterialIngressError::CapacityExceeded {
            stockpile,
            capacity,
            committed,
            requested,
        } => StructuralDeconstructionError::DestinationCapacityExceeded {
            stockpile,
            capacity,
            committed,
            requested,
        },
        MaterialIngressError::LotIdExhausted => StructuralDeconstructionError::LotIdExhausted,
        MaterialIngressError::RevisionExhausted => {
            StructuralDeconstructionError::InventoryRevisionExhausted
        }
        MaterialIngressError::Storage(error) => {
            StructuralDeconstructionError::DestinationStorage(error)
        }
        MaterialIngressError::UnknownMaterial {
            material: _material,
        }
        | MaterialIngressError::UnknownCompositionMaterial {
            material: _material,
        } => StructuralDeconstructionError::InvalidEmbodiedMatter { element },
        MaterialIngressError::UnknownForm { form: _form } => {
            StructuralDeconstructionError::InvalidEmbodiedMatter { element }
        }
        MaterialIngressError::ZeroMass => {
            StructuralDeconstructionError::InvalidEmbodiedMatter { element }
        }
        MaterialIngressError::InvalidComposition { error: _error } => {
            StructuralDeconstructionError::InvalidEmbodiedMatter { element }
        }
        MaterialIngressError::CompositionMissingHost { host: _host } => {
            StructuralDeconstructionError::InvalidEmbodiedMatter { element }
        }
        MaterialIngressError::InvalidProvenance => {
            StructuralDeconstructionError::InvalidEmbodiedMatter { element }
        }
        MaterialIngressError::ProvenanceInFuture {
            latest: _latest,
            current: _current,
        } => StructuralDeconstructionError::InvalidEmbodiedMatter { element },
    }
}

/// A validated cross-owner recovery token became stale before commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StructuralDeconstructionCommitError {
    StaleInventoryRevision { expected: u64, actual: u64 },
    Structure(StructuralCommitError),
}

impl Display for StructuralDeconstructionCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "validated deconstruction expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::Structure(error) => write!(
                formatter,
                "structural deconstruction commit failed: {error}"
            ),
        }
    }
}

impl Error for StructuralDeconstructionCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::StaleInventoryRevision {
                expected: _expected,
                actual: _actual,
            } => None,
        }
    }
}

/// Successful removal plus recovered inventory ownership.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructuralDeconstructionOutcome {
    structural: StructuralMutationOutcome,
    recovered_lots: Vec<MaterialLotId>,
}

impl StructuralDeconstructionOutcome {
    #[must_use]
    pub const fn structural(&self) -> &StructuralMutationOutcome {
        &self.structural
    }

    #[must_use]
    pub fn recovered_lots(&self) -> &[MaterialLotId] {
        &self.recovered_lots
    }
}

/// Consumed proof that removing a member and transferring all its matter is currently valid.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub struct ValidatedStructuralDeconstruction {
    removal: ValidatedStructuralRemovalWithLoads,
    ingress: ValidatedMaterialIngress,
}

impl ValidatedStructuralDeconstruction {
    #[must_use]
    pub const fn structural_analysis(&self) -> &crate::structural::StructuralAnalysis {
        self.removal.analysis()
    }

    /// Commits structural consequences first after prechecking inventory. Structural commit does
    /// not mutate inventory, so the validated ingress remains current during this synchronous call.
    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<StructuralDeconstructionOutcome, StructuralDeconstructionCommitError> {
        let actual_inventory_revision = state.inventory().revision();
        if actual_inventory_revision != self.ingress.expected_revision() {
            return Err(
                StructuralDeconstructionCommitError::StaleInventoryRevision {
                    expected: self.ingress.expected_revision(),
                    actual: actual_inventory_revision,
                },
            );
        }
        let structural = self
            .removal
            .commit(state)
            .map_err(StructuralDeconstructionCommitError::Structure)?;
        let recovered_lots = apply_material_ingress(state.inventory_state_mut(), self.ingress);
        Ok(StructuralDeconstructionOutcome {
            structural,
            recovered_lots,
        })
    }
}

/// Validates conserved recovery of all embodied matter from one structural member.
pub fn validate_structural_deconstruction(
    registries: &Registries,
    state: &AppState,
    resolution: StructuralDeconstructionResolution,
) -> Result<ValidatedStructuralDeconstruction, StructuralDeconstructionError> {
    let element = resolution.element;
    let record = state
        .structures()
        .get_element(element)
        .ok_or(StructuralDeconstructionError::UnknownElement { element })?;
    if record.embodied_mass().is_zero() || record.embodied_material().is_empty() {
        return Err(StructuralDeconstructionError::NoEmbodiedMatter { element });
    }
    let profile = registries
        .structural()
        .get_profile(record.profile())
        .ok_or(StructuralDeconstructionError::UnknownProfile {
            element,
            profile: record.profile(),
        })?;
    let entries = if record.is_cracked() {
        record
            .embodied_material()
            .iter()
            .map(|trace| {
                MaterialIngressEntry::from_reformed_consumed_trace(
                    trace,
                    profile.damaged_recovery_form(),
                )
            })
            .collect::<Vec<_>>()
    } else {
        record
            .embodied_material()
            .iter()
            .map(MaterialIngressEntry::from_consumed_trace)
            .collect::<Vec<_>>()
    };
    let ingress = validate_material_ingress(
        registries,
        state.inventory(),
        resolution.destination,
        entries,
        state.tick(),
    )
    .map_err(|error| map_ingress_error(element, error))?;
    let destination = state
        .inventory()
        .get_stockpile(resolution.destination)
        .ok_or(StructuralDeconstructionError::UnknownDestination {
            stockpile: resolution.destination,
        })?;
    let destination_after = destination
        .stored_mass()
        .checked_add(record.embodied_mass())
        .ok_or(StructuralDeconstructionError::DestinationMassOverflow {
            stockpile: resolution.destination,
        })?;
    let stored_matter_loads = resolve_stockpile_stored_loads(
        registries,
        state,
        [StockpileStoredMassChange::new(
            resolution.destination,
            destination_after,
        )],
    )
    .map_err(StructuralDeconstructionError::StoredMatterLoad)?;
    let removal = validate_remove_structural_element_with_owned_loads(
        registries,
        state,
        element,
        StructuralLoadKind::StoredMatter,
        stored_matter_loads,
    )
    .map_err(StructuralDeconstructionError::Structure)?;
    debug_assert_eq!(
        removal.expected_revision(),
        state.structures().revision(),
        "structural recovery plan must bind current owner revision"
    );
    Ok(ValidatedStructuralDeconstruction { removal, ingress })
}

#[cfg(test)]
pub(crate) const fn make_test_deconstruction_resolution(
    element: StructuralElementId,
    destination: StockpileId,
) -> StructuralDeconstructionResolution {
    StructuralDeconstructionResolution {
        element,
        destination,
    }
}

#[cfg(test)]
#[path = "deconstruction_execution_tests.rs"]
mod tests;

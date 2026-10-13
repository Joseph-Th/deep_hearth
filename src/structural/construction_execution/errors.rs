//! Failure types for structural materialization validation and commit.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::inventory::StockpileStructuralLoadError;
use crate::material::{FormId, MaterialId};

use super::super::geometry::StructuralGeometryError;
use super::super::{
    StructuralCommitError, StructuralElementId, StructuralLifecycle, StructuralProfileId,
};

/// Failure while validating an already-resolved construction batch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StructuralConstructionError {
    UnknownElement {
        element: StructuralElementId,
    },
    UnknownProfile {
        element: StructuralElementId,
        profile: StructuralProfileId,
    },
    ElementNotPlanned {
        element: StructuralElementId,
        lifecycle: StructuralLifecycle,
    },
    AlreadyMaterialized {
        element: StructuralElementId,
    },
    MaterialMismatch {
        element: StructuralElementId,
        expected: MaterialId,
        found: MaterialId,
    },
    UnsupportedComposition {
        element: StructuralElementId,
        material: MaterialId,
    },
    UnknownMaterialForm {
        element: StructuralElementId,
        form: FormId,
    },
    UnconsolidatedForm {
        element: StructuralElementId,
        form: FormId,
    },
    Geometry {
        element: StructuralElementId,
        error: StructuralGeometryError,
    },
    MaterialQuantityMismatch {
        element: StructuralElementId,
        required: Mass,
        selected: Mass,
    },
    InventorySelectionStale {
        expected: u64,
        actual: u64,
    },
    InventoryRevisionExhausted,
    StructureRevisionExhausted,
    SelfWeightOverflow {
        element: StructuralElementId,
    },
    StructuralLoad(StockpileStructuralLoadError),
}

impl Display for StructuralConstructionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownElement { element } => {
                write!(formatter, "unknown structural element {}", element.value())
            }
            Self::UnknownProfile { element, profile } => write!(
                formatter,
                "structural element {} references unknown profile {}",
                element.value(),
                profile.value()
            ),
            Self::ElementNotPlanned { element, lifecycle } => write!(
                formatter,
                "structural element {} is {lifecycle:?} and cannot receive construction matter",
                element.value()
            ),
            Self::UnknownMaterialForm { element, form } => write!(
                formatter,
                "structural element {} construction batch references unknown material form {}",
                element.value(),
                form.value()
            ),
            Self::UnconsolidatedForm { element, form } => write!(
                formatter,
                "structural element {} cannot directly embody unconsolidated form {}; shaping or consolidation must first produce rigid construction stock",
                element.value(),
                form.value()
            ),
            Self::Geometry { element, error } => write!(
                formatter,
                "structural element {} construction geometry is invalid: {error}",
                element.value()
            ),
            Self::MaterialQuantityMismatch {
                element,
                required,
                selected,
            } => write!(
                formatter,
                "structural element {} requires {} mg from geometry and density but construction selected {} mg",
                element.value(),
                required.milligrams(),
                selected.milligrams()
            ),
            Self::AlreadyMaterialized { element } => write!(
                formatter,
                "structural element {} already owns construction matter",
                element.value()
            ),
            Self::MaterialMismatch {
                element,
                expected,
                found,
            } => write!(
                formatter,
                "structural element {} requires material {} but construction batch contains material {}",
                element.value(),
                expected.value(),
                found.value()
            ),
            Self::UnsupportedComposition { element, material } => write!(
                formatter,
                "structural element {} requires pure material {} because the structural strength model is single-material",
                element.value(),
                material.value()
            ),
            Self::InventorySelectionStale { expected, actual } => write!(
                formatter,
                "construction selection expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::InventoryRevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted during construction")
            }
            Self::StructureRevisionExhausted => {
                formatter.write_str("structural revision space is exhausted during construction")
            }
            Self::SelfWeightOverflow { element } => write!(
                formatter,
                "structural element {} construction mass exceeds self-weight force range",
                element.value()
            ),
            Self::StructuralLoad(error) => write!(
                formatter,
                "construction cannot update source stockpile stored-matter load: {error}"
            ),
        }
    }
}

impl Error for StructuralConstructionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Geometry {
                element: _element,
                error,
            } => Some(error),
            Self::StructuralLoad(error) => Some(error),
            Self::UnknownElement { element: _element }
            | Self::AlreadyMaterialized { element: _element }
            | Self::SelfWeightOverflow { element: _element } => None,
            Self::UnknownProfile {
                element: _element,
                profile: _profile,
            } => None,
            Self::ElementNotPlanned {
                element: _element,
                lifecycle: _lifecycle,
            } => None,
            Self::MaterialMismatch {
                element: _element,
                expected: _expected,
                found: _found,
            } => None,
            Self::UnsupportedComposition {
                element: _element,
                material: _material,
            } => None,
            Self::UnknownMaterialForm {
                element: _element,
                form: _form,
            }
            | Self::UnconsolidatedForm {
                element: _element,
                form: _form,
            } => None,
            Self::MaterialQuantityMismatch {
                element: _element,
                required: _required,
                selected: _selected,
            } => None,
            Self::InventorySelectionStale {
                expected: _expected,
                actual: _actual,
            } => None,
            Self::InventoryRevisionExhausted | Self::StructureRevisionExhausted => None,
        }
    }
}

/// A validated construction transfer can no longer commit because an owning subsystem changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StructuralConstructionCommitError {
    StaleStructureRevision { expected: u64, actual: u64 },
    StaleInventoryRevision { expected: u64, actual: u64 },
    StateChanged { element: StructuralElementId },
    Structure(StructuralCommitError),
}

impl Display for StructuralConstructionCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleStructureRevision { expected, actual } => write!(
                formatter,
                "validated construction expected structural revision {expected} but current revision is {actual}"
            ),
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "validated construction expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::StateChanged { element } => write!(
                formatter,
                "structural element {} changed before construction commit",
                element.value()
            ),
            Self::Structure(error) => write!(
                formatter,
                "construction could not commit source stockpile stored-matter load: {error}"
            ),
        }
    }
}

impl Error for StructuralConstructionCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::StaleStructureRevision {
                expected: _expected,
                actual: _actual,
            }
            | Self::StaleInventoryRevision {
                expected: _expected,
                actual: _actual,
            } => None,
            Self::StateChanged { element: _element } => None,
        }
    }
}

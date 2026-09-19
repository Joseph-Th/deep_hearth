//! Equipment-owned structural-load reconstruction and consistency checks.

use crate::core::quantity::{AggregateMass, Force};
use crate::core::state::AppState;
use crate::equipment::EquipmentId;
use crate::registry::Registries;
use crate::structural::{
    StructuralElementId, StructuralLoadKind, StructuralMutationError,
    calculate_aggregate_weight_force_ceiling,
};

use super::EquipmentSupportError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EquipmentStructuralLoadConsistencyError {
    AggregateMassOverflow {
        element: StructuralElementId,
    },
    WeightForceOverflow {
        element: StructuralElementId,
    },
    ExistingLoadMismatch {
        element: StructuralElementId,
        stored: Force,
        expected: Force,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SupportedEquipmentMassError {
    AggregateMassOverflow { element: StructuralElementId },
}

impl From<SupportedEquipmentMassError> for EquipmentSupportError {
    fn from(error: SupportedEquipmentMassError) -> Self {
        match error {
            SupportedEquipmentMassError::AggregateMassOverflow { element } => {
                Self::AggregateMassOverflow { element }
            }
        }
    }
}

pub(super) fn supported_mass(
    state: &AppState,
    element: StructuralElementId,
    excluded: Option<EquipmentId>,
) -> Result<AggregateMass, SupportedEquipmentMassError> {
    let mut total = AggregateMass::ZERO;
    for equipment in state.equipment().supported_equipment(element) {
        if excluded == Some(equipment) {
            continue;
        }
        let record = match state.equipment().get_equipment(equipment) {
            Some(record) => record,
            None => panic!(
                "runtime invariant broken: support index references missing equipment {}",
                equipment.value()
            ),
        };
        total = total
            .checked_add(AggregateMass::from_mass(record.embodied_mass()))
            .ok_or(SupportedEquipmentMassError::AggregateMassOverflow { element })?;
    }
    Ok(total)
}

pub(super) fn support_force(
    registries: &Registries,
    element: StructuralElementId,
    mass: AggregateMass,
) -> Result<Force, EquipmentSupportError> {
    calculate_aggregate_weight_force_ceiling(mass, registries.core().gravity())
        .ok_or(EquipmentSupportError::WeightForceOverflow { element })
}

pub(super) fn validate_existing_load(
    registries: &Registries,
    state: &AppState,
    element: StructuralElementId,
) -> Result<AggregateMass, EquipmentSupportError> {
    let stored = state
        .structures()
        .get_element(element)
        .ok_or(EquipmentSupportError::Structure(
            StructuralMutationError::UnknownElement { element },
        ))?
        .load(StructuralLoadKind::Equipment);
    validate_existing_equipment_structural_load(registries, state, element, stored).map_err(
        |error| match error {
            EquipmentStructuralLoadConsistencyError::AggregateMassOverflow { element } => {
                EquipmentSupportError::AggregateMassOverflow { element }
            }
            EquipmentStructuralLoadConsistencyError::WeightForceOverflow { element } => {
                EquipmentSupportError::WeightForceOverflow { element }
            }
            EquipmentStructuralLoadConsistencyError::ExistingLoadMismatch {
                element,
                stored,
                expected,
            } => EquipmentSupportError::ExistingEquipmentLoadMismatch {
                element,
                stored,
                expected,
            },
        },
    )
}

pub(crate) fn validate_existing_equipment_structural_load(
    registries: &Registries,
    state: &AppState,
    element: StructuralElementId,
    stored: Force,
) -> Result<AggregateMass, EquipmentStructuralLoadConsistencyError> {
    let mass = supported_mass(state, element, None).map_err(|error| match error {
        SupportedEquipmentMassError::AggregateMassOverflow { element } => {
            EquipmentStructuralLoadConsistencyError::AggregateMassOverflow { element }
        }
    })?;
    let expected = calculate_aggregate_weight_force_ceiling(mass, registries.core().gravity())
        .ok_or(EquipmentStructuralLoadConsistencyError::WeightForceOverflow { element })?;
    if stored != expected {
        return Err(
            EquipmentStructuralLoadConsistencyError::ExistingLoadMismatch {
                element,
                stored,
                expected,
            },
        );
    }
    Ok(mass)
}

//! Authored equipment-maintenance resolution.
//!
//! This module owns the read-only policy step that binds one equipment definition's replacement
//! material, service target, and spent-material form to an exact inventory selection. The separate
//! maintenance execution module owns the later cross-owner transaction and commit checks.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::inventory::{
    ConsumptionSelection, ConsumptionSelectionError, StockpileId, validate_consumption_selection,
};
use crate::maintenance::Condition;
use crate::material::{CommodityKey, MaterialInputSpec};
use crate::registry::Registries;
use crate::survival::SurvivalExertion;

use super::definitions::EquipmentDefinitionId;
use super::state::EquipmentId;

/// Opaque result of physical maintenance resolution.
///
/// Production callers cannot construct this directly. The maintenance resolver binds exact authored
/// replacement material and resulting equipment condition before the maintenance transaction can be
/// validated.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub struct EquipmentMaintenanceResolution {
    pub(super) equipment: EquipmentId,
    pub(super) expected_equipment_revision: u64,
    pub(super) condition_before: Condition,
    pub(super) condition_after: Condition,
    pub(super) material: ConsumptionSelection,
    pub(super) spent: CommodityKey,
    pub(super) spent_destination: StockpileId,
    pub(super) material_mode: EquipmentMaintenanceMaterialResolution,
    pub(super) duration: TickSpan,
    pub(super) exertion: SurvivalExertion,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EquipmentMaintenanceMaterialResolution {
    AggregateWearStock,
    EmbodiedComponentReplacement { component: CommodityKey },
}

/// Player/system request to service one idle equipment instance from explicit replacement stock.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EquipmentMaintenanceRequest {
    equipment: EquipmentId,
    material_source: StockpileId,
    spent_destination: StockpileId,
}

impl EquipmentMaintenanceRequest {
    pub const fn new(
        equipment: EquipmentId,
        material_source: StockpileId,
        spent_destination: StockpileId,
    ) -> Self {
        Self {
            equipment,
            material_source,
            spent_destination,
        }
    }
}

/// Failure while resolving an authored replacement-material maintenance action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EquipmentMaintenanceResolutionError {
    UnknownEquipment {
        equipment: EquipmentId,
    },
    UnknownDefinition {
        equipment: EquipmentId,
        definition: EquipmentDefinitionId,
    },
    NoMaintenanceProfile {
        equipment: EquipmentId,
        definition: EquipmentDefinitionId,
    },
    ConditionAtOrAboveServiceTarget {
        equipment: EquipmentId,
        current: Condition,
        target: Condition,
    },
    UnknownMaterialSource {
        stockpile: StockpileId,
    },
    InsufficientReplacementMaterial {
        stockpile: StockpileId,
        commodity: CommodityKey,
        available: Mass,
        required: Mass,
    },
    MaterialSelectionMassOverflow {
        stockpile: StockpileId,
    },
}

impl Display for EquipmentMaintenanceResolutionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownEquipment { equipment } => {
                write!(formatter, "unknown equipment id {}", equipment.value())
            }
            Self::UnknownDefinition {
                equipment,
                definition,
            } => write!(
                formatter,
                "equipment {} references unknown definition {} during maintenance resolution",
                equipment.value(),
                definition.value()
            ),
            Self::NoMaintenanceProfile {
                equipment,
                definition,
            } => write!(
                formatter,
                "equipment {} definition {} has no authored maintenance profile",
                equipment.value(),
                definition.value()
            ),
            Self::ConditionAtOrAboveServiceTarget {
                equipment,
                current,
                target,
            } => write!(
                formatter,
                "equipment {} condition {} ppm is already at or above maintenance target {} ppm",
                equipment.value(),
                current.parts_per_million(),
                target.parts_per_million()
            ),
            Self::UnknownMaterialSource { stockpile } => write!(
                formatter,
                "maintenance replacement source stockpile {} does not exist",
                stockpile.value()
            ),
            Self::InsufficientReplacementMaterial {
                stockpile,
                commodity,
                available,
                required,
            } => write!(
                formatter,
                "maintenance source stockpile {} has {} mg of commodity {} but {} mg is required",
                stockpile.value(),
                available.milligrams(),
                commodity.value(),
                required.milligrams()
            ),
            Self::MaterialSelectionMassOverflow { stockpile } => write!(
                formatter,
                "maintenance replacement selection overflows mass accounting in stockpile {}",
                stockpile.value()
            ),
        }
    }
}

impl Error for EquipmentMaintenanceResolutionError {}

/// Resolves the equipment definition's authored replacement-material service against current state.
pub fn resolve_equipment_maintenance(
    registries: &Registries,
    state: &AppState,
    request: EquipmentMaintenanceRequest,
) -> Result<EquipmentMaintenanceResolution, EquipmentMaintenanceResolutionError> {
    let record = state.equipment().get_equipment(request.equipment).ok_or(
        EquipmentMaintenanceResolutionError::UnknownEquipment {
            equipment: request.equipment,
        },
    )?;
    let definition = registries
        .equipment()
        .get_equipment(record.definition())
        .ok_or(EquipmentMaintenanceResolutionError::UnknownDefinition {
            equipment: request.equipment,
            definition: record.definition(),
        })?;
    let profile = definition.maintenance_profile().ok_or(
        EquipmentMaintenanceResolutionError::NoMaintenanceProfile {
            equipment: request.equipment,
            definition: record.definition(),
        },
    )?;
    let condition_before = record.condition();
    if condition_before >= profile.restored_condition() {
        return Err(
            EquipmentMaintenanceResolutionError::ConditionAtOrAboveServiceTarget {
                equipment: request.equipment,
                current: condition_before,
                target: profile.restored_condition(),
            },
        );
    }
    let required_replacement_mass = profile.required_replacement_mass(condition_before);
    let material = validate_consumption_selection(
        state.inventory(),
        request.material_source,
        &[MaterialInputSpec::pure(
            profile.replacement(),
            required_replacement_mass,
        )],
    )
    .map_err(|error| match error {
        ConsumptionSelectionError::UnknownStockpile { stockpile } => {
            EquipmentMaintenanceResolutionError::UnknownMaterialSource { stockpile }
        }
        ConsumptionSelectionError::InsufficientMass {
            stockpile,
            commodity,
            available,
            requested,
        } => EquipmentMaintenanceResolutionError::InsufficientReplacementMaterial {
            stockpile,
            commodity,
            available,
            required: requested,
        },
        ConsumptionSelectionError::MassOverflow { stockpile } => {
            EquipmentMaintenanceResolutionError::MaterialSelectionMassOverflow { stockpile }
        }
    })?;
    Ok(EquipmentMaintenanceResolution {
        equipment: request.equipment,
        expected_equipment_revision: state.equipment().revision(),
        condition_before,
        condition_after: profile.restored_condition(),
        material,
        spent: profile.spent(),
        spent_destination: request.spent_destination,
        material_mode: if profile.is_component_replacement() {
            EquipmentMaintenanceMaterialResolution::EmbodiedComponentReplacement {
                component: profile.replacement(),
            }
        } else {
            EquipmentMaintenanceMaterialResolution::AggregateWearStock
        },
        duration: profile.required_service_duration(condition_before),
        exertion: profile.exertion(),
    })
}

pub(super) fn impure_replacement_commodity(
    selection: &ConsumptionSelection,
) -> Option<CommodityKey> {
    selection.consumed_inputs().iter().find_map(|trace| {
        let commodity = trace.profile().commodity();
        (trace.profile().composition().pure_material() != Some(commodity.material()))
            .then_some(commodity)
    })
}

impl EquipmentMaintenanceResolution {
    #[must_use]
    pub const fn equipment(&self) -> EquipmentId {
        self.equipment
    }

    #[must_use]
    pub const fn condition_before(&self) -> Condition {
        self.condition_before
    }

    #[must_use]
    pub const fn condition_after(&self) -> Condition {
        self.condition_after
    }

    #[must_use]
    pub const fn material_source(&self) -> StockpileId {
        self.material.source()
    }

    #[must_use]
    pub const fn spent_destination(&self) -> StockpileId {
        self.spent_destination
    }

    #[must_use]
    pub const fn spent_commodity(&self) -> CommodityKey {
        self.spent
    }

    #[must_use]
    pub fn material_mass(&self) -> Mass {
        self.material.total_consumed()
    }

    #[must_use]
    pub const fn replaces_embodied_component(&self) -> bool {
        matches!(
            self.material_mode,
            EquipmentMaintenanceMaterialResolution::EmbodiedComponentReplacement { .. }
        )
    }

    #[must_use]
    pub const fn duration(&self) -> TickSpan {
        self.duration
    }
}

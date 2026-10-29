//! Conserved inventory-to-equipment assembly transactions.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Mass, Temperature};
use crate::core::state::AppState;
use crate::inventory::{
    ConsumptionSelection, ConsumptionSelectionError, InventoryState, StockpileId,
    StockpileStoredMassChange, StockpileStructuralLoadError, TemperatureLimitedSelectionError,
    ValidatedMaterialEgress, ValidatedStockpileStructuralLoad, apply_material_egress,
    validate_material_egress_from_selection, validate_stockpile_stored_mass_changes,
    validate_temperature_limited_consumption_selection,
    validate_unreserved_stockpile_structural_load_headroom,
};
use crate::logistics::{PlayerStockpileAccessError, validate_player_stockpile_access};
use crate::maintenance::Condition;
use crate::material::{CommodityKey, MAX_HAND_SAFE_ASSEMBLY_TEMPERATURE, MaterialInputSpec};
use crate::player::{PlayerActionGuard, validate_optional_player_action};
use crate::registry::Registries;
use crate::structural::StructuralCommitError;

use super::EquipmentDefinitionId;
use super::state::{EquipmentId, EquipmentRecord};

fn map_assembly_selection_error(error: ConsumptionSelectionError) -> EquipmentAssemblyError {
    match error {
        ConsumptionSelectionError::UnknownStockpile { stockpile } => {
            EquipmentAssemblyError::UnknownSource { stockpile }
        }
        ConsumptionSelectionError::InsufficientMass {
            stockpile,
            commodity,
            available,
            requested,
        } => EquipmentAssemblyError::InsufficientMaterial {
            stockpile,
            commodity,
            available,
            required: requested,
        },
        ConsumptionSelectionError::MassOverflow { stockpile } => {
            EquipmentAssemblyError::SourceMassOverflow { stockpile }
        }
    }
}

/// Material readiness and canonical assembly admission resolve the same physical selection.
pub(super) fn validate_assembly_material_selection(
    inventory: &InventoryState,
    source: StockpileId,
    inputs: &[MaterialInputSpec],
) -> Result<ConsumptionSelection, EquipmentAssemblyError> {
    validate_temperature_limited_consumption_selection(
        inventory,
        source,
        inputs,
        MAX_HAND_SAFE_ASSEMBLY_TEMPERATURE,
    )
    .map_err(|error| match error {
        TemperatureLimitedSelectionError::Selection(error) => map_assembly_selection_error(error),
        TemperatureLimitedSelectionError::TooHot {
            commodity,
            temperature,
            maximum,
        } => EquipmentAssemblyError::AssemblyMaterialTooHot {
            commodity,
            temperature,
            maximum,
        },
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EquipmentAssemblyError {
    PlayerDead,
    PlayerBusy,
    Access(PlayerStockpileAccessError),
    UnknownDefinition {
        definition: EquipmentDefinitionId,
    },
    NoAssemblyProfile {
        definition: EquipmentDefinitionId,
    },
    UnknownSource {
        stockpile: StockpileId,
    },
    InsufficientMaterial {
        stockpile: StockpileId,
        commodity: CommodityKey,
        available: Mass,
        required: Mass,
    },
    AssemblyMaterialTooHot {
        commodity: CommodityKey,
        temperature: Temperature,
        maximum: Temperature,
    },
    SourceMassOverflow {
        stockpile: StockpileId,
    },
    StaleInventorySelection {
        expected: u64,
        actual: u64,
    },
    InventoryRevisionExhausted,
    EquipmentIdExhausted,
    EquipmentRevisionExhausted,
    LogisticsRevisionExhausted,
    StructuralLoad(StockpileStructuralLoadError),
}

impl Display for EquipmentAssemblyError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerDead => formatter.write_str("dead player cannot assemble equipment"),
            Self::PlayerBusy => {
                formatter.write_str("player cannot assemble equipment while work is active")
            }
            Self::Access(error) => write!(formatter, "equipment assembly access failed: {error}"),
            Self::UnknownDefinition { definition } => write!(
                formatter,
                "unknown equipment definition {}",
                definition.value()
            ),
            Self::NoAssemblyProfile { definition } => write!(
                formatter,
                "equipment definition {} has no authored assembly material",
                definition.value()
            ),
            Self::UnknownSource { stockpile } => write!(
                formatter,
                "unknown assembly stockpile {}",
                stockpile.value()
            ),
            Self::InsufficientMaterial {
                stockpile,
                commodity,
                available,
                required,
            } => write!(
                formatter,
                "assembly stockpile {} contains {} mg of commodity {} but {} mg is required",
                stockpile.value(),
                available.milligrams(),
                commodity.value(),
                required.milligrams()
            ),
            Self::AssemblyMaterialTooHot {
                commodity,
                temperature,
                maximum,
            } => write!(
                formatter,
                "equipment assembly material {commodity:?} is {} mK, above safe {} mK; cool it first",
                temperature.millikelvin(),
                maximum.millikelvin(),
            ),
            Self::SourceMassOverflow { stockpile } => write!(
                formatter,
                "assembly source {} mass accounting overflowed",
                stockpile.value()
            ),
            Self::StaleInventorySelection { expected, actual } => write!(
                formatter,
                "equipment assembly material selection expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::InventoryRevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted")
            }
            Self::EquipmentIdExhausted => {
                formatter.write_str("equipment identifier space is exhausted")
            }
            Self::EquipmentRevisionExhausted => {
                formatter.write_str("equipment revision space is exhausted")
            }
            Self::LogisticsRevisionExhausted => {
                formatter.write_str("logistics revision space is exhausted")
            }
            Self::StructuralLoad(error) => {
                write!(formatter, "assembly source load failed: {error}")
            }
        }
    }
}

impl Error for EquipmentAssemblyError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Access(error) => Some(error),
            Self::StructuralLoad(error) => Some(error),
            Self::PlayerDead
            | Self::PlayerBusy
            | Self::UnknownDefinition { definition: _ }
            | Self::NoAssemblyProfile { definition: _ }
            | Self::UnknownSource { stockpile: _ }
            | Self::InsufficientMaterial {
                stockpile: _,
                commodity: _,
                available: _,
                required: _,
            }
            | Self::AssemblyMaterialTooHot { .. }
            | Self::SourceMassOverflow { stockpile: _ }
            | Self::StaleInventorySelection {
                expected: _,
                actual: _,
            }
            | Self::InventoryRevisionExhausted
            | Self::EquipmentIdExhausted
            | Self::EquipmentRevisionExhausted
            | Self::LogisticsRevisionExhausted => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EquipmentAssemblyCommitError {
    PlayerBecameBusy,
    StaleSurvival { expected: u64, actual: u64 },
    StaleLogistics { expected: u64, actual: u64 },
    StaleInventory { expected: u64, actual: u64 },
    StaleEquipment { expected: u64, actual: u64 },
    Structure(StructuralCommitError),
}

impl Display for EquipmentAssemblyCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerBecameBusy => {
                formatter.write_str("player work started before equipment assembly commit")
            }
            Self::StaleSurvival { expected, actual } => write!(
                formatter,
                "equipment assembly expected survival revision {expected} but current revision is {actual}"
            ),
            Self::StaleLogistics { expected, actual } => write!(
                formatter,
                "equipment assembly expected logistics revision {expected} but current revision is {actual}"
            ),
            Self::StaleInventory { expected, actual } => write!(
                formatter,
                "equipment assembly expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::StaleEquipment { expected, actual } => write!(
                formatter,
                "equipment assembly expected equipment revision {expected} but current revision is {actual}"
            ),
            Self::Structure(error) => {
                write!(formatter, "equipment assembly structure failed: {error}")
            }
        }
    }
}

impl Error for EquipmentAssemblyCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::PlayerBecameBusy
            | Self::StaleSurvival { .. }
            | Self::StaleLogistics { .. }
            | Self::StaleInventory {
                expected: _,
                actual: _,
            }
            | Self::StaleEquipment {
                expected: _,
                actual: _,
            } => None,
        }
    }
}

#[must_use]
pub struct ValidatedEquipmentAssembly {
    player_action: PlayerActionGuard,
    record: EquipmentRecord,
    next_equipment_id: u32,
    expected_equipment_revision: u64,
    expected_logistics_revision: u64,
    logistics_placement: Option<(u64, crate::spatial::VoxelCoord)>,
    next_equipment_revision: u64,
    egress: ValidatedMaterialEgress,
    structural_load: Option<ValidatedStockpileStructuralLoad>,
}

impl ValidatedEquipmentAssembly {
    pub fn commit(self, state: &mut AppState) -> Result<EquipmentId, EquipmentAssemblyCommitError> {
        if state.logistics().revision() != self.expected_logistics_revision {
            return Err(EquipmentAssemblyCommitError::StaleLogistics {
                expected: self.expected_logistics_revision,
                actual: state.logistics().revision(),
            });
        }
        self.player_action.precheck(state).map_err(|stale| {
            EquipmentAssemblyCommitError::StaleSurvival {
                expected: stale.expected,
                actual: stale.actual,
            }
        })?;
        if state.logistics().player().is_some() && state.player_work().active().is_some() {
            return Err(EquipmentAssemblyCommitError::PlayerBecameBusy);
        }
        if state.inventory().revision() != self.egress.expected_revision() {
            return Err(EquipmentAssemblyCommitError::StaleInventory {
                expected: self.egress.expected_revision(),
                actual: state.inventory().revision(),
            });
        }
        if state.equipment().revision() != self.expected_equipment_revision {
            return Err(EquipmentAssemblyCommitError::StaleEquipment {
                expected: self.expected_equipment_revision,
                actual: state.equipment().revision(),
            });
        }
        state.equipment().assert_allocation_available(
            &self.record,
            self.next_equipment_id,
            self.next_equipment_revision,
        );
        self.egress.assert_matches_state(state.inventory());
        if let Some(load) = self.structural_load {
            load.commit(state)
                .map_err(EquipmentAssemblyCommitError::Structure)?;
        }
        let id = self.record.id();
        apply_material_egress(state.inventory_state_mut(), self.egress);
        state.equipment_state_mut().insert_equipment(
            self.record,
            self.next_equipment_id,
            self.next_equipment_revision,
        );
        if let Some((next_revision, position)) = self.logistics_placement {
            state.logistics_state_mut().apply_equipment_placement(
                self.expected_logistics_revision,
                next_revision,
                id,
                position,
            );
        }
        Ok(id)
    }
}

/// Validates construction of one authored equipment instance from its exact conserved assembly stock.
pub fn validate_assemble_equipment(
    registries: &Registries,
    state: &AppState,
    definition: EquipmentDefinitionId,
    source: StockpileId,
) -> Result<ValidatedEquipmentAssembly, EquipmentAssemblyError> {
    let player_action =
        validate_optional_player_action(state).map_err(|_| EquipmentAssemblyError::PlayerDead)?;
    if state.logistics().player().is_some() && state.player_work().active().is_some() {
        return Err(EquipmentAssemblyError::PlayerBusy);
    }
    let definition_record = registries
        .equipment()
        .get_equipment(definition)
        .ok_or(EquipmentAssemblyError::UnknownDefinition { definition })?;
    let assembly = definition_record
        .assembly_profile()
        .ok_or(EquipmentAssemblyError::NoAssemblyProfile { definition })?;
    validate_player_stockpile_access(state, source).map_err(EquipmentAssemblyError::Access)?;
    let selection =
        validate_assembly_material_selection(state.inventory(), source, assembly.inputs())?;
    let embodied_material = selection.consumed_inputs().to_vec();
    let egress =
        validate_material_egress_from_selection(state.inventory(), selection).map_err(|error| {
            match error {
                crate::inventory::MaterialEgressError::StaleSelection { expected, actual } => {
                    EquipmentAssemblyError::StaleInventorySelection { expected, actual }
                }
                crate::inventory::MaterialEgressError::RevisionExhausted => {
                    EquipmentAssemblyError::InventoryRevisionExhausted
                }
            }
        })?;
    let source_after = egress.source_stored_mass_after(state.inventory());
    let structural_load = validate_stockpile_stored_mass_changes(
        registries,
        state,
        [StockpileStoredMassChange::new(source, source_after)],
    )
    .map_err(EquipmentAssemblyError::StructuralLoad)?;
    validate_unreserved_stockpile_structural_load_headroom(state, structural_load.as_ref())
        .map_err(EquipmentAssemblyError::StructuralLoad)?;

    let id_value = state.equipment().next_equipment_id();
    let next_equipment_id = id_value
        .checked_add(1)
        .ok_or(EquipmentAssemblyError::EquipmentIdExhausted)?;
    let id = EquipmentId::new(id_value);
    let expected_equipment_revision = state.equipment().revision();
    if !state.can_spend_inventory_revisions(1) {
        return Err(EquipmentAssemblyError::InventoryRevisionExhausted);
    }
    if !state.can_spend_equipment_revisions(1) {
        return Err(EquipmentAssemblyError::EquipmentRevisionExhausted);
    }
    let next_equipment_revision = expected_equipment_revision
        .checked_add(1)
        .unwrap_or_else(|| unreachable!("equipment headroom check includes assembly revision"));
    let expected_logistics_revision = state.logistics().revision();
    let logistics_placement = match state.logistics().player().map(|player| player.position()) {
        Some(position) => {
            if !state.can_spend_logistics_revisions(1) {
                return Err(EquipmentAssemblyError::LogisticsRevisionExhausted);
            }
            Some((
                expected_logistics_revision
                    .checked_add(1)
                    .unwrap_or_else(|| {
                        unreachable!(
                            "logistics headroom check includes equipment assembly placement"
                        )
                    }),
                position,
            ))
        }
        None => None,
    };
    Ok(ValidatedEquipmentAssembly {
        player_action,
        record: EquipmentRecord {
            id,
            definition,
            condition: Condition::PRISTINE,
            embodied_mass: definition_record.mass(),
            embodied_material,
            supported_by: None,
            created_at: state.tick(),
            last_maintenance_admission: None,
        },
        next_equipment_id,
        expected_equipment_revision,
        expected_logistics_revision,
        logistics_placement,
        next_equipment_revision,
        egress,
        structural_load,
    })
}

#[cfg(test)]
#[path = "construction_execution_tests.rs"]
mod tests;

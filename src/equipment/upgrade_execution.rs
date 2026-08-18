//! Additive, matter-conserving upgrades of existing equipment instances.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::{
    ConsumedMaterialTrace, StockpileId, StockpileStoredMassChange, StockpileStructuralLoadError,
    ValidatedMaterialEgress, ValidatedStockpileStructuralLoad, apply_material_egress,
    validate_consumption_selection, validate_material_egress_from_selection,
    validate_stockpile_stored_mass_changes,
};
use crate::material::MaterialComposition;
use crate::mining::MiningJobId;
use crate::production::{ProductionJobId, ProductionOccupancyRelease};
use crate::registry::Registries;
use crate::structural::{StructuralCommitError, StructuralElementId};

use super::state::EquipmentUpgradeMutation;
use super::{EquipmentDefinitionId, EquipmentId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EquipmentUpgradeError {
    UnknownEquipment {
        equipment: EquipmentId,
    },
    UnknownTargetDefinition {
        target: EquipmentDefinitionId,
    },
    NoUpgradeProfile {
        target: EquipmentDefinitionId,
    },
    WrongBaseDefinition {
        equipment: EquipmentId,
        required: EquipmentDefinitionId,
        actual: EquipmentDefinitionId,
    },
    EquipmentMounted {
        equipment: EquipmentId,
        element: StructuralElementId,
    },
    EquipmentBusyProduction {
        equipment: EquipmentId,
        job: ProductionJobId,
        release: ProductionOccupancyRelease,
    },
    EquipmentBusyMining {
        equipment: EquipmentId,
        job: MiningJobId,
    },
    EquipmentBusyManualPower {
        equipment: EquipmentId,
    },
    UnknownSource {
        stockpile: StockpileId,
    },
    InsufficientMaterial {
        stockpile: StockpileId,
        available: Mass,
        required: Mass,
    },
    SourceMassOverflow {
        stockpile: StockpileId,
    },
    ImpureUpgradeMaterial,
    StaleInventorySelection {
        expected: u64,
        actual: u64,
    },
    SourceBusy {
        stockpile: StockpileId,
        job: ProductionJobId,
        release: ProductionOccupancyRelease,
    },
    InventoryRevisionExhausted,
    EquipmentRevisionExhausted,
    StructuralLoad(StockpileStructuralLoadError),
}

impl Display for EquipmentUpgradeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownEquipment { equipment } => {
                write!(formatter, "unknown equipment id {}", equipment.value())
            }
            Self::UnknownTargetDefinition { target } => write!(
                formatter,
                "unknown target equipment definition {}",
                target.value()
            ),
            Self::NoUpgradeProfile { target } => write!(
                formatter,
                "equipment definition {} has no authored additive upgrade path",
                target.value()
            ),
            Self::WrongBaseDefinition {
                equipment,
                required,
                actual,
            } => write!(
                formatter,
                "equipment {} uses definition {} but upgrade requires base definition {}",
                equipment.value(),
                actual.value(),
                required.value()
            ),
            Self::EquipmentMounted { equipment, element } => write!(
                formatter,
                "equipment {} is mounted on structural element {} and must be unmounted before its mass changes",
                equipment.value(),
                element.value()
            ),
            Self::EquipmentBusyProduction {
                equipment,
                job,
                release,
            } => write!(
                formatter,
                "equipment {} is occupied by production job {} {release} and cannot be upgraded",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyMining { equipment, job } => write!(
                formatter,
                "equipment {} is occupied by mining job {} and cannot be upgraded",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyManualPower { equipment } => write!(
                formatter,
                "equipment {} is occupied by direct player-powered generation and cannot be upgraded",
                equipment.value()
            ),
            Self::UnknownSource { stockpile } => write!(
                formatter,
                "unknown equipment-upgrade material stockpile {}",
                stockpile.value()
            ),
            Self::InsufficientMaterial {
                stockpile,
                available,
                required,
            } => write!(
                formatter,
                "equipment-upgrade stockpile {} contains {} mg but {} mg of authored addition material is required",
                stockpile.value(),
                available.milligrams(),
                required.milligrams()
            ),
            Self::SourceMassOverflow { stockpile } => write!(
                formatter,
                "equipment-upgrade source {} mass accounting overflowed",
                stockpile.value()
            ),
            Self::ImpureUpgradeMaterial => formatter.write_str(
                "equipment upgrade requires pure matter matching the authored additive material",
            ),
            Self::StaleInventorySelection { expected, actual } => write!(
                formatter,
                "equipment-upgrade material selection expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::SourceBusy {
                stockpile,
                job,
                release,
            } => write!(
                formatter,
                "equipment-upgrade source {} is occupied by production job {} {release}",
                stockpile.value(),
                job.value()
            ),
            Self::InventoryRevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted")
            }
            Self::EquipmentRevisionExhausted => {
                formatter.write_str("equipment revision space is exhausted")
            }
            Self::StructuralLoad(error) => {
                write!(formatter, "equipment-upgrade source load failed: {error}")
            }
        }
    }
}

impl Error for EquipmentUpgradeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::StructuralLoad(error) => Some(error),
            Self::UnknownEquipment { .. }
            | Self::UnknownTargetDefinition { .. }
            | Self::NoUpgradeProfile { .. }
            | Self::WrongBaseDefinition { .. }
            | Self::EquipmentMounted { .. }
            | Self::EquipmentBusyProduction { .. }
            | Self::EquipmentBusyMining { .. }
            | Self::EquipmentBusyManualPower { .. }
            | Self::UnknownSource { .. }
            | Self::InsufficientMaterial { .. }
            | Self::SourceMassOverflow { .. }
            | Self::ImpureUpgradeMaterial
            | Self::StaleInventorySelection { .. }
            | Self::SourceBusy { .. }
            | Self::InventoryRevisionExhausted
            | Self::EquipmentRevisionExhausted => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EquipmentUpgradeCommitError {
    StaleInventory {
        expected: u64,
        actual: u64,
    },
    StaleEquipment {
        expected: u64,
        actual: u64,
    },
    UnknownEquipment {
        equipment: EquipmentId,
    },
    DefinitionChanged {
        equipment: EquipmentId,
        expected: EquipmentDefinitionId,
        actual: EquipmentDefinitionId,
    },
    EquipmentMounted {
        equipment: EquipmentId,
        element: StructuralElementId,
    },
    EquipmentBusyProduction {
        equipment: EquipmentId,
        job: ProductionJobId,
    },
    EquipmentBusyMining {
        equipment: EquipmentId,
        job: MiningJobId,
    },
    EquipmentBusyManualPower {
        equipment: EquipmentId,
    },
    SourceBusy {
        stockpile: StockpileId,
        job: ProductionJobId,
    },
    Structure(StructuralCommitError),
}

impl Display for EquipmentUpgradeCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleInventory { expected, actual } => write!(
                formatter,
                "equipment upgrade expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::StaleEquipment { expected, actual } => write!(
                formatter,
                "equipment upgrade expected equipment revision {expected} but current revision is {actual}"
            ),
            Self::UnknownEquipment { equipment } => write!(
                formatter,
                "equipment {} disappeared before upgrade commit",
                equipment.value()
            ),
            Self::DefinitionChanged {
                equipment,
                expected,
                actual,
            } => write!(
                formatter,
                "equipment {} changed definition from expected {} to {} before upgrade commit",
                equipment.value(),
                expected.value(),
                actual.value()
            ),
            Self::EquipmentMounted { equipment, element } => write!(
                formatter,
                "equipment {} became mounted on structural element {} before upgrade commit",
                equipment.value(),
                element.value()
            ),
            Self::EquipmentBusyProduction { equipment, job } => write!(
                formatter,
                "equipment {} became occupied by production job {} before upgrade commit",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyMining { equipment, job } => write!(
                formatter,
                "equipment {} became occupied by mining job {} before upgrade commit",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyManualPower { equipment } => write!(
                formatter,
                "equipment {} became occupied by direct player-powered generation before upgrade commit",
                equipment.value()
            ),
            Self::SourceBusy { stockpile, job } => write!(
                formatter,
                "equipment-upgrade source {} became occupied by production job {} before commit",
                stockpile.value(),
                job.value()
            ),
            Self::Structure(error) => {
                write!(formatter, "equipment upgrade structure failed: {error}")
            }
        }
    }
}

impl Error for EquipmentUpgradeCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::StaleInventory { .. }
            | Self::StaleEquipment { .. }
            | Self::UnknownEquipment { .. }
            | Self::DefinitionChanged { .. }
            | Self::EquipmentMounted { .. }
            | Self::EquipmentBusyProduction { .. }
            | Self::EquipmentBusyMining { .. }
            | Self::EquipmentBusyManualPower { .. }
            | Self::SourceBusy { .. } => None,
        }
    }
}

#[must_use]
pub struct ValidatedEquipmentUpgrade {
    equipment: EquipmentId,
    expected_definition: EquipmentDefinitionId,
    target_definition: EquipmentDefinitionId,
    expected_embodied_mass: Mass,
    target_embodied_mass: Mass,
    additions: Vec<ConsumedMaterialTrace>,
    expected_equipment_revision: u64,
    next_equipment_revision: u64,
    egress: ValidatedMaterialEgress,
    structural_load: Option<ValidatedStockpileStructuralLoad>,
}

impl ValidatedEquipmentUpgrade {
    pub fn commit(self, state: &mut AppState) -> Result<EquipmentId, EquipmentUpgradeCommitError> {
        if state.inventory().revision() != self.egress.expected_revision() {
            return Err(EquipmentUpgradeCommitError::StaleInventory {
                expected: self.egress.expected_revision(),
                actual: state.inventory().revision(),
            });
        }
        if state.equipment().revision() != self.expected_equipment_revision {
            return Err(EquipmentUpgradeCommitError::StaleEquipment {
                expected: self.expected_equipment_revision,
                actual: state.equipment().revision(),
            });
        }
        let record = state.equipment().get_equipment(self.equipment).ok_or(
            EquipmentUpgradeCommitError::UnknownEquipment {
                equipment: self.equipment,
            },
        )?;
        if record.definition() != self.expected_definition {
            return Err(EquipmentUpgradeCommitError::DefinitionChanged {
                equipment: self.equipment,
                expected: self.expected_definition,
                actual: record.definition(),
            });
        }
        if let Some(element) = record.supported_by() {
            return Err(EquipmentUpgradeCommitError::EquipmentMounted {
                equipment: self.equipment,
                element,
            });
        }
        if let Some(job) = state.production().get_equipment_occupant(self.equipment) {
            return Err(EquipmentUpgradeCommitError::EquipmentBusyProduction {
                equipment: self.equipment,
                job: job.id(),
            });
        }
        if let Some(job) = state.mining().get_equipment_occupant(self.equipment) {
            return Err(EquipmentUpgradeCommitError::EquipmentBusyMining {
                equipment: self.equipment,
                job,
            });
        }
        if state
            .player_work()
            .get_manual_power_equipment_occupant(self.equipment)
            .is_some()
        {
            return Err(EquipmentUpgradeCommitError::EquipmentBusyManualPower {
                equipment: self.equipment,
            });
        }
        if let Some(job) = state
            .production()
            .get_stockpile_occupant(self.egress.source())
        {
            return Err(EquipmentUpgradeCommitError::SourceBusy {
                stockpile: self.egress.source(),
                job: job.id(),
            });
        }
        if let Some(load) = self.structural_load {
            load.commit(state)
                .map_err(EquipmentUpgradeCommitError::Structure)?;
        }
        apply_material_egress(state.inventory_state_mut(), self.egress);
        state.equipment_state_mut().apply_upgrade(
            EquipmentUpgradeMutation {
                equipment: self.equipment,
                expected_definition: self.expected_definition,
                target_definition: self.target_definition,
                expected_embodied_mass: self.expected_embodied_mass,
                target_embodied_mass: self.target_embodied_mass,
                additions: self.additions,
            },
            self.expected_equipment_revision,
            self.next_equipment_revision,
        );
        Ok(self.equipment)
    }
}

/// Validates one authored additive upgrade of an existing, unmounted, idle equipment instance.
pub fn validate_upgrade_equipment(
    registries: &Registries,
    state: &AppState,
    equipment: EquipmentId,
    target: EquipmentDefinitionId,
    source: StockpileId,
) -> Result<ValidatedEquipmentUpgrade, EquipmentUpgradeError> {
    let record = state
        .equipment()
        .get_equipment(equipment)
        .ok_or(EquipmentUpgradeError::UnknownEquipment { equipment })?;
    let target_definition = registries
        .equipment()
        .get_equipment(target)
        .ok_or(EquipmentUpgradeError::UnknownTargetDefinition { target })?;
    let upgrade = target_definition
        .upgrade_profile()
        .ok_or(EquipmentUpgradeError::NoUpgradeProfile { target })?;
    if record.definition() != upgrade.from() {
        return Err(EquipmentUpgradeError::WrongBaseDefinition {
            equipment,
            required: upgrade.from(),
            actual: record.definition(),
        });
    }
    if let Some(element) = record.supported_by() {
        return Err(EquipmentUpgradeError::EquipmentMounted { equipment, element });
    }
    if let Some(job) = state.production().get_equipment_occupant(equipment) {
        return Err(EquipmentUpgradeError::EquipmentBusyProduction {
            equipment,
            job: job.id(),
            release: job.occupancy_release(),
        });
    }
    if let Some(job) = state.mining().get_equipment_occupant(equipment) {
        return Err(EquipmentUpgradeError::EquipmentBusyMining { equipment, job });
    }
    if state
        .player_work()
        .get_manual_power_equipment_occupant(equipment)
        .is_some()
    {
        return Err(EquipmentUpgradeError::EquipmentBusyManualPower { equipment });
    }

    let selection =
        validate_consumption_selection(state.inventory(), source, upgrade.additions().inputs())
            .map_err(|error| match error {
                crate::inventory::ConsumptionSelectionError::UnknownStockpile { stockpile } => {
                    EquipmentUpgradeError::UnknownSource { stockpile }
                }
                crate::inventory::ConsumptionSelectionError::InsufficientMass {
                    stockpile,
                    available,
                    requested,
                    ..
                } => EquipmentUpgradeError::InsufficientMaterial {
                    stockpile,
                    available,
                    required: requested,
                },
                crate::inventory::ConsumptionSelectionError::MassOverflow { stockpile } => {
                    EquipmentUpgradeError::SourceMassOverflow { stockpile }
                }
            })?;
    if selection.consumed_inputs().iter().any(|trace| {
        trace.profile().composition()
            != &MaterialComposition::pure(trace.profile().commodity().material())
    }) {
        return Err(EquipmentUpgradeError::ImpureUpgradeMaterial);
    }
    let additions = selection.consumed_inputs().to_vec();
    if let Some(job) = state.production().get_stockpile_occupant(source) {
        return Err(EquipmentUpgradeError::SourceBusy {
            stockpile: source,
            job: job.id(),
            release: job.occupancy_release(),
        });
    }
    let egress =
        validate_material_egress_from_selection(state.inventory(), selection).map_err(|error| {
            match error {
                crate::inventory::MaterialEgressError::StaleSelection { expected, actual } => {
                    EquipmentUpgradeError::StaleInventorySelection { expected, actual }
                }
                crate::inventory::MaterialEgressError::RevisionExhausted => {
                    EquipmentUpgradeError::InventoryRevisionExhausted
                }
            }
        })?;
    let source_record = state
        .inventory()
        .get_stockpile(source)
        .ok_or(EquipmentUpgradeError::UnknownSource { stockpile: source })?;
    let source_after = source_record
        .stored_mass()
        .checked_sub(egress.total_consumed())
        .ok_or(EquipmentUpgradeError::SourceMassOverflow { stockpile: source })?;
    let structural_load = validate_stockpile_stored_mass_changes(
        registries,
        state,
        [StockpileStoredMassChange::new(source, source_after)],
    )
    .map_err(EquipmentUpgradeError::StructuralLoad)?;
    let expected_equipment_revision = state.equipment().revision();
    let next_equipment_revision = expected_equipment_revision
        .checked_add(1)
        .ok_or(EquipmentUpgradeError::EquipmentRevisionExhausted)?;

    Ok(ValidatedEquipmentUpgrade {
        equipment,
        expected_definition: record.definition(),
        target_definition: target,
        expected_embodied_mass: record.embodied_mass(),
        target_embodied_mass: target_definition.mass(),
        additions,
        expected_equipment_revision,
        next_equipment_revision,
        egress,
        structural_load,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{
        ENERGY_STONE_FLYWHEEL_DRIVE, EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
        EQUIPMENT_COPPER_REINFORCED_PICK, EQUIPMENT_STONE_HAND_CRANK, EQUIPMENT_STONE_PICK,
        FORM_FLYWHEEL, FORM_HANDLE, FORM_REINFORCEMENT, FORM_TOOL, MANUAL_POWER_HAND_CRANK,
        MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD, build_registries,
    };
    use crate::core::quantity::{Energy, Temperature};
    use crate::core::state::validate_loaded_state;
    use crate::core::time::WorldSeed;
    use crate::energy::validate_assemble_energy_store;
    use crate::equipment::{
        apply_equipment_condition_plan, decide_equipment_wear, validate_assemble_equipment,
    };
    use crate::inventory::{add_solid_stockpile_for_test, deposit_lot_for_test};
    use crate::labor::{ManualPowerRequest, validate_start_manual_power};
    use crate::material::CommodityKey;
    use crate::matter::calculate_matter_accounting;
    use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
    use crate::survival::initialize_player_survival;

    fn assemble_stone_pick(registries: &Registries, state: &mut AppState) -> EquipmentId {
        let source = add_solid_stockpile_for_test(state, Mass::from_milligrams(1_000_000))
            .unwrap_or_else(|error| panic!("upgrade pick source fixture failed: {error}"));
        for (commodity, mass) in [
            (
                CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
                Mass::from_milligrams(800_000),
            ),
            (
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(200_000),
            ),
        ] {
            deposit_lot_for_test(
                registries,
                state,
                source,
                commodity,
                mass,
                Temperature::from_millikelvin(293_150),
            )
            .unwrap_or_else(|error| panic!("upgrade pick material fixture failed: {error}"));
        }
        validate_assemble_equipment(registries, state, EQUIPMENT_STONE_PICK, source)
            .unwrap_or_else(|error| panic!("upgrade pick assembly validation failed: {error}"))
            .commit(state)
            .unwrap_or_else(|error| panic!("upgrade pick assembly commit failed: {error}"))
    }

    fn reinforcement_source(registries: &Registries, state: &mut AppState) -> StockpileId {
        let source = add_solid_stockpile_for_test(state, Mass::from_milligrams(20_000))
            .unwrap_or_else(|error| panic!("upgrade reinforcement source failed: {error}"));
        deposit_lot_for_test(
            registries,
            state,
            source,
            CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
            Mass::from_milligrams(20_000),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("upgrade reinforcement material failed: {error}"));
        source
    }

    fn assemble_stone_crank(registries: &Registries, state: &mut AppState) -> EquipmentId {
        let source = add_solid_stockpile_for_test(state, Mass::from_milligrams(1_100_000))
            .unwrap_or_else(|error| panic!("upgrade crank source fixture failed: {error}"));
        for (commodity, mass) in [
            (
                CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
                Mass::from_milligrams(900_000),
            ),
            (
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(200_000),
            ),
        ] {
            deposit_lot_for_test(
                registries,
                state,
                source,
                commodity,
                mass,
                Temperature::from_millikelvin(293_150),
            )
            .unwrap_or_else(|error| panic!("upgrade crank material fixture failed: {error}"));
        }
        validate_assemble_equipment(registries, state, EQUIPMENT_STONE_HAND_CRANK, source)
            .unwrap_or_else(|error| panic!("upgrade crank assembly validation failed: {error}"))
            .commit(state)
            .unwrap_or_else(|error| panic!("upgrade crank assembly commit failed: {error}"))
    }

    fn assemble_store(
        registries: &Registries,
        state: &mut AppState,
    ) -> crate::energy::EnergyStoreId {
        let source = add_solid_stockpile_for_test(state, Mass::from_milligrams(1_100_000))
            .unwrap_or_else(|error| panic!("upgrade store source fixture failed: {error}"));
        for (commodity, mass) in [
            (
                CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
                Mass::from_milligrams(900_000),
            ),
            (
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(200_000),
            ),
        ] {
            deposit_lot_for_test(
                registries,
                state,
                source,
                commodity,
                mass,
                Temperature::from_millikelvin(293_150),
            )
            .unwrap_or_else(|error| panic!("upgrade store material fixture failed: {error}"));
        }
        validate_assemble_energy_store(registries, state, ENERGY_STONE_FLYWHEEL_DRIVE, source)
            .unwrap_or_else(|error| panic!("upgrade store assembly validation failed: {error}"))
            .commit(state)
            .unwrap_or_else(|error| panic!("upgrade store assembly commit failed: {error}"))
    }

    #[test]
    fn additive_upgrade_preserves_identity_wear_and_world_matter() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0xA66D_0001));
        let pick = assemble_stone_pick(&registries, &mut state);
        let reinforcement = reinforcement_source(&registries, &mut state);
        let wear = decide_equipment_wear(&state, pick, 87_654)
            .unwrap_or_else(|error| panic!("upgrade wear decision failed: {error}"));
        apply_equipment_condition_plan(&mut state, wear)
            .unwrap_or_else(|error| panic!("upgrade wear commit failed: {error}"));
        let before_record = state
            .equipment()
            .get_equipment(pick)
            .unwrap_or_else(|| panic!("upgrade pick disappeared before upgrade"));
        let condition_before = before_record.condition();
        let created_at_before = before_record.created_at();
        let matter_before = calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("upgrade initial matter accounting failed: {error}"))
            .total();

        let upgraded = validate_upgrade_equipment(
            &registries,
            &state,
            pick,
            EQUIPMENT_COPPER_REINFORCED_PICK,
            reinforcement,
        )
        .unwrap_or_else(|error| panic!("pick upgrade validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("pick upgrade commit failed: {error}"));

        assert_eq!(upgraded, pick);
        let record = state
            .equipment()
            .get_equipment(pick)
            .unwrap_or_else(|| panic!("upgraded pick disappeared"));
        assert_eq!(record.definition(), EQUIPMENT_COPPER_REINFORCED_PICK);
        assert_eq!(record.condition(), condition_before);
        assert_eq!(record.created_at(), created_at_before);
        assert_eq!(record.embodied_mass(), Mass::from_milligrams(1_020_000));
        assert_eq!(record.embodied_material().len(), 3);
        assert_eq!(
            calculate_matter_accounting(&state)
                .unwrap_or_else(|error| panic!("upgrade final matter accounting failed: {error}"))
                .total(),
            matter_before
        );
        validate_loaded_state(&registries, &state)
            .unwrap_or_else(|error| panic!("upgraded state audit failed: {error}"));

        let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
            .unwrap_or_else(|error| panic!("upgraded state serialization failed: {error}"));
        let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
            .unwrap_or_else(|error| panic!("upgraded state decode failed: {error}"));
        let loaded = decoded
            .into_state(&registries)
            .unwrap_or_else(|error| panic!("upgraded state load validation failed: {error}"));
        assert_eq!(loaded, state);
    }

    #[test]
    fn intervening_equipment_mutation_invalidates_upgrade_token() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0xA66D_0002));
        let pick = assemble_stone_pick(&registries, &mut state);
        let reinforcement = reinforcement_source(&registries, &mut state);
        let token = validate_upgrade_equipment(
            &registries,
            &state,
            pick,
            EQUIPMENT_COPPER_REINFORCED_PICK,
            reinforcement,
        )
        .unwrap_or_else(|error| panic!("stale upgrade validation failed: {error}"));
        let expected = state.equipment().revision();
        let wear = decide_equipment_wear(&state, pick, 1)
            .unwrap_or_else(|error| panic!("stale upgrade wear decision failed: {error}"));
        apply_equipment_condition_plan(&mut state, wear)
            .unwrap_or_else(|error| panic!("stale upgrade wear commit failed: {error}"));

        assert_eq!(
            token.commit(&mut state),
            Err(EquipmentUpgradeCommitError::StaleEquipment {
                expected,
                actual: expected + 1,
            })
        );
        assert_eq!(
            state
                .equipment()
                .get_equipment(pick)
                .map(|record| record.definition()),
            Some(EQUIPMENT_STONE_PICK)
        );
        assert_eq!(
            state
                .inventory()
                .get_stockpile(reinforcement)
                .map(|stockpile| stockpile.stored_mass()),
            Some(Mass::from_milligrams(20_000))
        );
    }

    #[test]
    fn manual_power_start_invalidates_prior_crank_upgrade_without_equipment_revision_change() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0xA66D_0003));
        initialize_player_survival(&registries, &mut state)
            .unwrap_or_else(|error| panic!("upgrade race survival setup failed: {error}"));
        let crank = assemble_stone_crank(&registries, &mut state);
        let store = assemble_store(&registries, &mut state);
        let reinforcement = reinforcement_source(&registries, &mut state);
        let token = validate_upgrade_equipment(
            &registries,
            &state,
            crank,
            EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
            reinforcement,
        )
        .unwrap_or_else(|error| panic!("upgrade race validation failed: {error}"));
        let equipment_revision = state.equipment().revision();
        validate_start_manual_power(
            &registries,
            &state,
            ManualPowerRequest::new(
                MANUAL_POWER_HAND_CRANK,
                crank,
                store,
                Energy::from_nanojoules(1_000_000_000),
            ),
        )
        .unwrap_or_else(|error| panic!("upgrade race manual-power validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("upgrade race manual-power commit failed: {error}"));
        assert_eq!(
            state.equipment().revision(),
            equipment_revision,
            "manual-power admission should reserve the crank without front-loading wear"
        );

        assert_eq!(
            token.commit(&mut state),
            Err(EquipmentUpgradeCommitError::EquipmentBusyManualPower { equipment: crank })
        );
        assert_eq!(
            state
                .equipment()
                .get_equipment(crank)
                .map(|record| record.definition()),
            Some(EQUIPMENT_STONE_HAND_CRANK)
        );
        assert_eq!(
            state
                .inventory()
                .get_stockpile(reinforcement)
                .map(|stockpile| stockpile.stored_mass()),
            Some(Mass::from_milligrams(20_000))
        );
    }
}

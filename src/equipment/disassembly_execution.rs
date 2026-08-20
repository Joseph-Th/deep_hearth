//! Conserved recovery of assembled equipment.
//!
//! Pristine equipment reverses assembly exactly. Worn equipment with an authored recovery form is
//! destructively decommissioned into same-material scrap so wear cannot be erased and failed tools do
//! not permanently trap matter. Equipment without an authored worn-recovery policy remains intact.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::{
    MaterialIngressEntry, MaterialIngressError, MaterialLotId, StockpileId, StockpileStorageError,
    StockpileStoredMassChange, StockpileStructuralLoadError, ValidatedMaterialIngress,
    ValidatedStockpileStructuralLoad, apply_material_ingress, validate_material_ingress,
    validate_stockpile_stored_mass_changes,
};
use crate::maintenance::Condition;
use crate::mining::MiningJobId;
use crate::production::{ProductionJobId, ProductionOccupancyRelease};
use crate::registry::Registries;
use crate::structural::{StructuralCommitError, StructuralElementId};

use super::EquipmentId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EquipmentDisassemblyError {
    UnknownEquipment {
        equipment: EquipmentId,
    },
    NoEmbodiedMatter {
        equipment: EquipmentId,
    },
    WornRecoveryUnavailable {
        equipment: EquipmentId,
        condition: Condition,
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
    UnknownDestination {
        stockpile: StockpileId,
    },
    InvalidEmbodiedMatter {
        equipment: EquipmentId,
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
    EquipmentRevisionExhausted,
    StoredMatterLoad(StockpileStructuralLoadError),
}

impl Display for EquipmentDisassemblyError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownEquipment { equipment } => {
                write!(formatter, "unknown equipment id {}", equipment.value())
            }
            Self::NoEmbodiedMatter { equipment } => write!(
                formatter,
                "equipment {} has no embodied matter to disassemble",
                equipment.value()
            ),
            Self::WornRecoveryUnavailable {
                equipment,
                condition,
            } => write!(
                formatter,
                "equipment {} is at {} ppm condition and its definition has no destructive worn-recovery form",
                equipment.value(),
                condition.parts_per_million()
            ),
            Self::EquipmentMounted { equipment, element } => write!(
                formatter,
                "equipment {} is mounted on structural element {} and must be unmounted before disassembly",
                equipment.value(),
                element.value()
            ),
            Self::EquipmentBusyProduction {
                equipment,
                job,
                release,
            } => write!(
                formatter,
                "equipment {} is occupied by production job {} {release} and cannot be disassembled",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyMining { equipment, job } => write!(
                formatter,
                "equipment {} is occupied by mining job {} and cannot be disassembled",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyManualPower { equipment } => write!(
                formatter,
                "equipment {} is occupied by direct player-powered generation and cannot be disassembled",
                equipment.value()
            ),
            Self::UnknownDestination { stockpile } => write!(
                formatter,
                "equipment disassembly destination stockpile {} does not exist",
                stockpile.value()
            ),
            Self::InvalidEmbodiedMatter { equipment } => write!(
                formatter,
                "equipment {} contains embodied matter that cannot re-enter inventory",
                equipment.value()
            ),
            Self::DestinationStorage(error) => write!(
                formatter,
                "equipment disassembly destination rejects recovered material: {error}"
            ),
            Self::DestinationMassOverflow { stockpile } => write!(
                formatter,
                "equipment disassembly overflows stockpile {} mass accounting",
                stockpile.value()
            ),
            Self::DestinationCapacityExceeded {
                stockpile,
                capacity,
                committed,
                requested,
            } => write!(
                formatter,
                "equipment disassembly exceeds stockpile {} capacity {} mg: {} mg committed, {} mg requested",
                stockpile.value(),
                capacity.milligrams(),
                committed.milligrams(),
                requested.milligrams()
            ),
            Self::LotIdExhausted => {
                formatter.write_str("material lot identifier space is exhausted during disassembly")
            }
            Self::InventoryRevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted during disassembly")
            }
            Self::EquipmentRevisionExhausted => {
                formatter.write_str("equipment revision space is exhausted during disassembly")
            }
            Self::StoredMatterLoad(error) => write!(
                formatter,
                "equipment disassembly cannot update destination stored-matter load: {error}"
            ),
        }
    }
}

impl Error for EquipmentDisassemblyError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::DestinationStorage(error) => Some(error),
            Self::StoredMatterLoad(error) => Some(error),
            Self::UnknownEquipment { .. }
            | Self::NoEmbodiedMatter { .. }
            | Self::WornRecoveryUnavailable { .. }
            | Self::EquipmentMounted { .. }
            | Self::EquipmentBusyProduction { .. }
            | Self::EquipmentBusyMining { .. }
            | Self::EquipmentBusyManualPower { .. }
            | Self::UnknownDestination { .. }
            | Self::InvalidEmbodiedMatter { .. }
            | Self::DestinationMassOverflow { .. }
            | Self::DestinationCapacityExceeded { .. }
            | Self::LotIdExhausted
            | Self::InventoryRevisionExhausted
            | Self::EquipmentRevisionExhausted => None,
        }
    }
}

fn map_ingress_error(
    equipment: EquipmentId,
    error: MaterialIngressError,
) -> EquipmentDisassemblyError {
    match error {
        MaterialIngressError::Empty => EquipmentDisassemblyError::NoEmbodiedMatter { equipment },
        MaterialIngressError::UnknownStockpile { stockpile } => {
            EquipmentDisassemblyError::UnknownDestination { stockpile }
        }
        MaterialIngressError::MassOverflow { stockpile } => {
            EquipmentDisassemblyError::DestinationMassOverflow { stockpile }
        }
        MaterialIngressError::CapacityExceeded {
            stockpile,
            capacity,
            committed,
            requested,
        } => EquipmentDisassemblyError::DestinationCapacityExceeded {
            stockpile,
            capacity,
            committed,
            requested,
        },
        MaterialIngressError::LotIdExhausted => EquipmentDisassemblyError::LotIdExhausted,
        MaterialIngressError::RevisionExhausted => {
            EquipmentDisassemblyError::InventoryRevisionExhausted
        }
        MaterialIngressError::Storage(error) => {
            EquipmentDisassemblyError::DestinationStorage(error)
        }
        MaterialIngressError::UnknownMaterial { .. }
        | MaterialIngressError::UnknownForm { .. }
        | MaterialIngressError::UnknownCompositionMaterial { .. }
        | MaterialIngressError::ZeroMass
        | MaterialIngressError::InvalidComposition { .. }
        | MaterialIngressError::CompositionMissingHost { .. }
        | MaterialIngressError::InvalidProvenance
        | MaterialIngressError::ProvenanceInFuture { .. } => {
            EquipmentDisassemblyError::InvalidEmbodiedMatter { equipment }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EquipmentDisassemblyCommitError {
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
    EquipmentChanged {
        equipment: EquipmentId,
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
    Structure(StructuralCommitError),
}

impl Display for EquipmentDisassemblyCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleInventory { expected, actual } => write!(
                formatter,
                "equipment disassembly expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::StaleEquipment { expected, actual } => write!(
                formatter,
                "equipment disassembly expected equipment revision {expected} but current revision is {actual}"
            ),
            Self::UnknownEquipment { equipment } => write!(
                formatter,
                "equipment {} disappeared before disassembly commit",
                equipment.value()
            ),
            Self::EquipmentChanged { equipment } => write!(
                formatter,
                "equipment {} changed after disassembly validation",
                equipment.value()
            ),
            Self::EquipmentMounted { equipment, element } => write!(
                formatter,
                "equipment {} became mounted on structural element {} before disassembly commit",
                equipment.value(),
                element.value()
            ),
            Self::EquipmentBusyProduction { equipment, job } => write!(
                formatter,
                "equipment {} became occupied by production job {} before disassembly commit",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyMining { equipment, job } => write!(
                formatter,
                "equipment {} became occupied by mining job {} before disassembly commit",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyManualPower { equipment } => write!(
                formatter,
                "equipment {} became occupied by direct player-powered generation before disassembly commit",
                equipment.value()
            ),
            Self::Structure(error) => {
                write!(formatter, "equipment disassembly structure failed: {error}")
            }
        }
    }
}

impl Error for EquipmentDisassemblyCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::StaleInventory { .. }
            | Self::StaleEquipment { .. }
            | Self::UnknownEquipment { .. }
            | Self::EquipmentChanged { .. }
            | Self::EquipmentMounted { .. }
            | Self::EquipmentBusyProduction { .. }
            | Self::EquipmentBusyMining { .. }
            | Self::EquipmentBusyManualPower { .. } => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EquipmentDisassemblyOutcome {
    recovered_lots: Vec<MaterialLotId>,
}

impl EquipmentDisassemblyOutcome {
    #[must_use]
    pub fn recovered_lots(&self) -> &[MaterialLotId] {
        &self.recovered_lots
    }
}

#[must_use]
pub struct ValidatedEquipmentDisassembly {
    equipment: EquipmentId,
    expected_equipment_revision: u64,
    next_equipment_revision: u64,
    expected_condition: Condition,
    expected_embodied_mass: Mass,
    ingress: ValidatedMaterialIngress,
    structural_load: Option<ValidatedStockpileStructuralLoad>,
}

impl ValidatedEquipmentDisassembly {
    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<EquipmentDisassemblyOutcome, EquipmentDisassemblyCommitError> {
        if state.inventory().revision() != self.ingress.expected_revision() {
            return Err(EquipmentDisassemblyCommitError::StaleInventory {
                expected: self.ingress.expected_revision(),
                actual: state.inventory().revision(),
            });
        }
        if state.equipment().revision() != self.expected_equipment_revision {
            return Err(EquipmentDisassemblyCommitError::StaleEquipment {
                expected: self.expected_equipment_revision,
                actual: state.equipment().revision(),
            });
        }
        let record = state.equipment().get_equipment(self.equipment).ok_or(
            EquipmentDisassemblyCommitError::UnknownEquipment {
                equipment: self.equipment,
            },
        )?;
        if record.condition() != self.expected_condition
            || record.embodied_mass() != self.expected_embodied_mass
        {
            return Err(EquipmentDisassemblyCommitError::EquipmentChanged {
                equipment: self.equipment,
            });
        }
        if let Some(element) = record.supported_by() {
            return Err(EquipmentDisassemblyCommitError::EquipmentMounted {
                equipment: self.equipment,
                element,
            });
        }
        if let Some(job) = state.production().get_equipment_occupant(self.equipment) {
            return Err(EquipmentDisassemblyCommitError::EquipmentBusyProduction {
                equipment: self.equipment,
                job: job.id(),
            });
        }
        if let Some(job) = state.mining().get_equipment_occupant(self.equipment) {
            return Err(EquipmentDisassemblyCommitError::EquipmentBusyMining {
                equipment: self.equipment,
                job,
            });
        }
        if state
            .player_work()
            .get_manual_power_equipment_occupant(self.equipment)
            .is_some()
        {
            return Err(EquipmentDisassemblyCommitError::EquipmentBusyManualPower {
                equipment: self.equipment,
            });
        }
        if let Some(load) = self.structural_load {
            load.commit(state)
                .map_err(EquipmentDisassemblyCommitError::Structure)?;
        }
        state.equipment_state_mut().remove_equipment(
            self.equipment,
            self.expected_equipment_revision,
            self.next_equipment_revision,
        );
        let recovered_lots = apply_material_ingress(state.inventory_state_mut(), self.ingress);
        Ok(EquipmentDisassemblyOutcome { recovered_lots })
    }
}

/// Recovers idle, unmounted assembled equipment without allowing wear to reset into pristine parts.
pub fn validate_disassemble_equipment(
    registries: &Registries,
    state: &AppState,
    equipment: EquipmentId,
    destination: StockpileId,
) -> Result<ValidatedEquipmentDisassembly, EquipmentDisassemblyError> {
    let record = state
        .equipment()
        .get_equipment(equipment)
        .ok_or(EquipmentDisassemblyError::UnknownEquipment { equipment })?;
    if record.embodied_mass().is_zero() || record.embodied_material().is_empty() {
        return Err(EquipmentDisassemblyError::NoEmbodiedMatter { equipment });
    }
    let worn_recovery_form = if record.condition() == Condition::PRISTINE {
        None
    } else {
        let definition = registries
            .equipment()
            .get_equipment(record.definition())
            .ok_or(EquipmentDisassemblyError::InvalidEmbodiedMatter { equipment })?;
        Some(definition.worn_recovery_form().ok_or(
            EquipmentDisassemblyError::WornRecoveryUnavailable {
                equipment,
                condition: record.condition(),
            },
        )?)
    };
    if let Some(element) = record.supported_by() {
        return Err(EquipmentDisassemblyError::EquipmentMounted { equipment, element });
    }
    if let Some(job) = state.production().get_equipment_occupant(equipment) {
        return Err(EquipmentDisassemblyError::EquipmentBusyProduction {
            equipment,
            job: job.id(),
            release: job.occupancy_release(),
        });
    }
    if let Some(job) = state.mining().get_equipment_occupant(equipment) {
        return Err(EquipmentDisassemblyError::EquipmentBusyMining { equipment, job });
    }
    if state
        .player_work()
        .get_manual_power_equipment_occupant(equipment)
        .is_some()
    {
        return Err(EquipmentDisassemblyError::EquipmentBusyManualPower { equipment });
    }

    let entries = record
        .embodied_material()
        .iter()
        .map(|trace| match worn_recovery_form {
            Some(form) => MaterialIngressEntry::from_reformed_consumed_trace(trace, form),
            None => MaterialIngressEntry::from_consumed_trace(trace),
        })
        .collect::<Vec<_>>();
    let ingress = validate_material_ingress(
        registries,
        state.inventory(),
        destination,
        entries,
        state.tick(),
    )
    .map_err(|error| map_ingress_error(equipment, error))?;
    let destination_record = state.inventory().get_stockpile(destination).ok_or(
        EquipmentDisassemblyError::UnknownDestination {
            stockpile: destination,
        },
    )?;
    let destination_after = destination_record
        .stored_mass()
        .checked_add(record.embodied_mass())
        .ok_or(EquipmentDisassemblyError::DestinationMassOverflow {
            stockpile: destination,
        })?;
    let structural_load = validate_stockpile_stored_mass_changes(
        registries,
        state,
        [StockpileStoredMassChange::new(
            destination,
            destination_after,
        )],
    )
    .map_err(EquipmentDisassemblyError::StoredMatterLoad)?;
    let expected_equipment_revision = state.equipment().revision();
    let next_equipment_revision = expected_equipment_revision
        .checked_add(1)
        .ok_or(EquipmentDisassemblyError::EquipmentRevisionExhausted)?;

    Ok(ValidatedEquipmentDisassembly {
        equipment,
        expected_equipment_revision,
        next_equipment_revision,
        expected_condition: record.condition(),
        expected_embodied_mass: record.embodied_mass(),
        ingress,
        structural_load,
    })
}

#[cfg(all(
    test,
    any(not(feature = "test-unit-sharded"), feature = "test-unit-resources")
))]
mod tests {
    use super::*;
    use crate::content::{
        ENERGY_STONE_FLYWHEEL_DRIVE, EQUIPMENT_STONE_HAND_CRANK, EQUIPMENT_STONE_PICK,
        FORM_FLYWHEEL, FORM_HANDLE, FORM_SCRAP, FORM_TOOL, MANUAL_POWER_HAND_CRANK, MATERIAL_STONE,
        MATERIAL_WOOD, build_registries,
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
    use crate::survival::initialize_player_survival;

    fn assembled_pick(registries: &Registries, state: &mut AppState) -> EquipmentId {
        let source = add_solid_stockpile_for_test(state, Mass::from_milligrams(1_000_000))
            .unwrap_or_else(|error| panic!("disassembly pick source failed: {error}"));
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
            .unwrap_or_else(|error| panic!("disassembly pick material failed: {error}"));
        }
        validate_assemble_equipment(registries, state, EQUIPMENT_STONE_PICK, source)
            .unwrap_or_else(|error| panic!("disassembly pick assembly failed: {error}"))
            .commit(state)
            .unwrap_or_else(|error| panic!("disassembly pick assembly commit failed: {error}"))
    }

    fn assembled_crank(registries: &Registries, state: &mut AppState) -> EquipmentId {
        let source = add_solid_stockpile_for_test(state, Mass::from_milligrams(1_100_000))
            .unwrap_or_else(|error| panic!("disassembly crank source failed: {error}"));
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
            .unwrap_or_else(|error| panic!("disassembly crank material failed: {error}"));
        }
        validate_assemble_equipment(registries, state, EQUIPMENT_STONE_HAND_CRANK, source)
            .unwrap_or_else(|error| panic!("disassembly crank assembly failed: {error}"))
            .commit(state)
            .unwrap_or_else(|error| panic!("disassembly crank assembly commit failed: {error}"))
    }

    fn assembled_store(
        registries: &Registries,
        state: &mut AppState,
    ) -> crate::energy::EnergyStoreId {
        let source = add_solid_stockpile_for_test(state, Mass::from_milligrams(1_100_000))
            .unwrap_or_else(|error| panic!("disassembly store source failed: {error}"));
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
            .unwrap_or_else(|error| panic!("disassembly store material failed: {error}"));
        }
        validate_assemble_energy_store(registries, state, ENERGY_STONE_FLYWHEEL_DRIVE, source)
            .unwrap_or_else(|error| panic!("disassembly store assembly failed: {error}"))
            .commit(state)
            .unwrap_or_else(|error| panic!("disassembly store assembly commit failed: {error}"))
    }

    #[test]
    fn pristine_disassembly_recovers_exact_matter_without_reusing_identity() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0xD15A_0001));
        let pick = assembled_pick(&registries, &mut state);
        let destination =
            add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
                .unwrap_or_else(|error| panic!("disassembly destination failed: {error}"));
        let matter_before = calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("disassembly matter before failed: {error}"))
            .total();

        let outcome = validate_disassemble_equipment(&registries, &state, pick, destination)
            .unwrap_or_else(|error| panic!("disassembly validation failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("disassembly commit failed: {error}"));
        assert_eq!(outcome.recovered_lots().len(), 2);
        assert!(state.equipment().get_equipment(pick).is_none());
        assert_eq!(
            state
                .inventory()
                .get_stockpile(destination)
                .map(|stockpile| stockpile.stored_mass()),
            Some(Mass::from_milligrams(1_000_000))
        );
        assert_eq!(
            calculate_matter_accounting(&state)
                .unwrap_or_else(|error| panic!("disassembly matter after failed: {error}"))
                .total(),
            matter_before
        );
        validate_loaded_state(&registries, &state)
            .unwrap_or_else(|error| panic!("disassembly state audit failed: {error}"));

        let replacement = assembled_pick(&registries, &mut state);
        assert!(
            replacement > pick,
            "equipment IDs must remain monotonic after disassembly"
        );
    }

    #[test]
    fn worn_equipment_recovers_as_same_material_scrap_without_resetting_components() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0xD15A_0002));
        let pick = assembled_pick(&registries, &mut state);
        let destination =
            add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
                .unwrap_or_else(|error| panic!("worn disassembly destination failed: {error}"));
        let wear = decide_equipment_wear(&state, pick, 1)
            .unwrap_or_else(|error| panic!("worn disassembly wear decision failed: {error}"));
        apply_equipment_condition_plan(&mut state, wear)
            .unwrap_or_else(|error| panic!("worn disassembly wear commit failed: {error}"));
        let matter_before = calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("worn disassembly matter before failed: {error}"))
            .total();

        let outcome = validate_disassemble_equipment(&registries, &state, pick, destination)
            .unwrap_or_else(|error| panic!("worn disassembly validation failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("worn disassembly commit failed: {error}"));
        assert!(state.equipment().get_equipment(pick).is_none());
        let recovered = outcome
            .recovered_lots()
            .iter()
            .map(|lot| {
                state
                    .inventory()
                    .get_lot(*lot)
                    .unwrap_or_else(|| panic!("worn recovery lot disappeared"))
                    .commodity()
            })
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            recovered,
            std::collections::BTreeSet::from([
                CommodityKey::new(MATERIAL_STONE, FORM_SCRAP),
                CommodityKey::new(MATERIAL_WOOD, FORM_SCRAP),
            ])
        );
        assert_eq!(
            calculate_matter_accounting(&state)
                .unwrap_or_else(|error| panic!("worn disassembly matter after failed: {error}"))
                .total(),
            matter_before
        );
        validate_loaded_state(&registries, &state)
            .unwrap_or_else(|error| panic!("worn disassembly state audit failed: {error}"));
    }

    #[test]
    fn manual_power_start_invalidates_prior_pristine_disassembly_without_equipment_revision_change()
    {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0xD15A_0003));
        initialize_player_survival(&registries, &mut state)
            .unwrap_or_else(|error| panic!("disassembly race survival setup failed: {error}"));
        let crank = assembled_crank(&registries, &mut state);
        let store = assembled_store(&registries, &mut state);
        let destination =
            add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_100_000))
                .unwrap_or_else(|error| panic!("disassembly race destination failed: {error}"));
        let token = validate_disassemble_equipment(&registries, &state, crank, destination)
            .unwrap_or_else(|error| panic!("disassembly race validation failed: {error}"));
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
        .unwrap_or_else(|error| panic!("disassembly race manual-power validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("disassembly race manual-power commit failed: {error}"));
        assert_eq!(
            state.equipment().revision(),
            equipment_revision,
            "manual-power admission should reserve the crank without front-loading wear"
        );

        assert_eq!(
            token.commit(&mut state),
            Err(EquipmentDisassemblyCommitError::EquipmentBusyManualPower { equipment: crank })
        );
        assert!(state.equipment().get_equipment(crank).is_some());
        assert_eq!(
            state
                .inventory()
                .get_stockpile(destination)
                .map(|stockpile| stockpile.stored_mass()),
            Some(Mass::ZERO)
        );
    }
}

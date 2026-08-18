//! Identity-preserving reversal of empty, material-backed energy-store construction.
//!
//! Dismantling is intentionally limited to empty, idle stores. Stored work remains an authoritative
//! resource and can never disappear merely because its container is being removed.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Energy, Mass};
use crate::core::state::AppState;
use crate::inventory::{
    MaterialIngressEntry, MaterialIngressError, MaterialLotId, StockpileId, StockpileStorageError,
    StockpileStoredMassChange, StockpileStructuralLoadError, ValidatedMaterialIngress,
    ValidatedStockpileStructuralLoad, apply_material_ingress, validate_material_ingress,
    validate_stockpile_stored_mass_changes,
};
use crate::production::{ProductionJobId, ProductionOccupancyRelease};
use crate::registry::Registries;
use crate::structural::StructuralCommitError;

use super::EnergyStoreId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EnergyStoreDisassemblyError {
    UnknownStore {
        store: EnergyStoreId,
    },
    NoEmbodiedMatter {
        store: EnergyStoreId,
    },
    StoreNotEmpty {
        store: EnergyStoreId,
        stored: Energy,
    },
    StoreBusyProduction {
        store: EnergyStoreId,
        job: ProductionJobId,
        release: ProductionOccupancyRelease,
    },
    StoreBusyManualPower {
        store: EnergyStoreId,
    },
    UnknownDestination {
        stockpile: StockpileId,
    },
    InvalidEmbodiedMatter {
        store: EnergyStoreId,
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
    EnergyRevisionExhausted,
    StoredMatterLoad(StockpileStructuralLoadError),
}

impl Display for EnergyStoreDisassemblyError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownStore { store } => {
                write!(formatter, "unknown energy store {}", store.value())
            }
            Self::NoEmbodiedMatter { store } => write!(
                formatter,
                "energy store {} has no embodied matter to disassemble",
                store.value()
            ),
            Self::StoreNotEmpty { store, stored } => write!(
                formatter,
                "energy store {} still owns {} nJ and cannot be disassembled",
                store.value(),
                stored.nanojoules()
            ),
            Self::StoreBusyProduction {
                store,
                job,
                release,
            } => write!(
                formatter,
                "energy store {} is occupied by production job {} {release} and cannot be disassembled",
                store.value(),
                job.value()
            ),
            Self::StoreBusyManualPower { store } => write!(
                formatter,
                "energy store {} is reserved by direct player-powered generation and cannot be disassembled",
                store.value()
            ),
            Self::UnknownDestination { stockpile } => write!(
                formatter,
                "energy-store disassembly destination stockpile {} does not exist",
                stockpile.value()
            ),
            Self::InvalidEmbodiedMatter { store } => write!(
                formatter,
                "energy store {} contains embodied matter that cannot re-enter inventory",
                store.value()
            ),
            Self::DestinationStorage(error) => write!(
                formatter,
                "energy-store disassembly destination rejects recovered material: {error}"
            ),
            Self::DestinationMassOverflow { stockpile } => write!(
                formatter,
                "energy-store disassembly overflows stockpile {} mass accounting",
                stockpile.value()
            ),
            Self::DestinationCapacityExceeded {
                stockpile,
                capacity,
                committed,
                requested,
            } => write!(
                formatter,
                "energy-store disassembly exceeds stockpile {} capacity {} mg: {} mg committed, {} mg requested",
                stockpile.value(),
                capacity.milligrams(),
                committed.milligrams(),
                requested.milligrams()
            ),
            Self::LotIdExhausted => formatter
                .write_str("material lot identifier space is exhausted during store disassembly"),
            Self::InventoryRevisionExhausted => formatter
                .write_str("inventory revision space is exhausted during store disassembly"),
            Self::EnergyRevisionExhausted => {
                formatter.write_str("energy revision space is exhausted during store disassembly")
            }
            Self::StoredMatterLoad(error) => write!(
                formatter,
                "energy-store disassembly cannot update destination stored-matter load: {error}"
            ),
        }
    }
}

impl Error for EnergyStoreDisassemblyError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::DestinationStorage(error) => Some(error),
            Self::StoredMatterLoad(error) => Some(error),
            Self::UnknownStore { .. }
            | Self::NoEmbodiedMatter { .. }
            | Self::StoreNotEmpty { .. }
            | Self::StoreBusyProduction { .. }
            | Self::StoreBusyManualPower { .. }
            | Self::UnknownDestination { .. }
            | Self::InvalidEmbodiedMatter { .. }
            | Self::DestinationMassOverflow { .. }
            | Self::DestinationCapacityExceeded { .. }
            | Self::LotIdExhausted
            | Self::InventoryRevisionExhausted
            | Self::EnergyRevisionExhausted => None,
        }
    }
}

fn map_ingress_error(
    store: EnergyStoreId,
    error: MaterialIngressError,
) -> EnergyStoreDisassemblyError {
    match error {
        MaterialIngressError::Empty => EnergyStoreDisassemblyError::NoEmbodiedMatter { store },
        MaterialIngressError::UnknownStockpile { stockpile } => {
            EnergyStoreDisassemblyError::UnknownDestination { stockpile }
        }
        MaterialIngressError::MassOverflow { stockpile } => {
            EnergyStoreDisassemblyError::DestinationMassOverflow { stockpile }
        }
        MaterialIngressError::CapacityExceeded {
            stockpile,
            capacity,
            committed,
            requested,
        } => EnergyStoreDisassemblyError::DestinationCapacityExceeded {
            stockpile,
            capacity,
            committed,
            requested,
        },
        MaterialIngressError::LotIdExhausted => EnergyStoreDisassemblyError::LotIdExhausted,
        MaterialIngressError::RevisionExhausted => {
            EnergyStoreDisassemblyError::InventoryRevisionExhausted
        }
        MaterialIngressError::Storage(error) => {
            EnergyStoreDisassemblyError::DestinationStorage(error)
        }
        MaterialIngressError::UnknownMaterial { .. }
        | MaterialIngressError::UnknownForm { .. }
        | MaterialIngressError::UnknownCompositionMaterial { .. }
        | MaterialIngressError::ZeroMass
        | MaterialIngressError::InvalidComposition { .. }
        | MaterialIngressError::CompositionMissingHost { .. }
        | MaterialIngressError::InvalidProvenance
        | MaterialIngressError::ProvenanceInFuture { .. } => {
            EnergyStoreDisassemblyError::InvalidEmbodiedMatter { store }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EnergyStoreDisassemblyCommitError {
    StaleInventory {
        expected: u64,
        actual: u64,
    },
    StaleEnergy {
        expected: u64,
        actual: u64,
    },
    UnknownStore {
        store: EnergyStoreId,
    },
    StoreChanged {
        store: EnergyStoreId,
    },
    StoreBusyProduction {
        store: EnergyStoreId,
        job: ProductionJobId,
    },
    StoreBusyManualPower {
        store: EnergyStoreId,
    },
    Structure(StructuralCommitError),
}

impl Display for EnergyStoreDisassemblyCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleInventory { expected, actual } => write!(
                formatter,
                "energy-store disassembly expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::StaleEnergy { expected, actual } => write!(
                formatter,
                "energy-store disassembly expected energy revision {expected} but current revision is {actual}"
            ),
            Self::UnknownStore { store } => write!(
                formatter,
                "energy store {} disappeared before disassembly commit",
                store.value()
            ),
            Self::StoreChanged { store } => write!(
                formatter,
                "energy store {} changed after disassembly validation",
                store.value()
            ),
            Self::StoreBusyProduction { store, job } => write!(
                formatter,
                "energy store {} became occupied by production job {} before disassembly commit",
                store.value(),
                job.value()
            ),
            Self::StoreBusyManualPower { store } => write!(
                formatter,
                "energy store {} became reserved by direct player-powered generation before disassembly commit",
                store.value()
            ),
            Self::Structure(error) => {
                write!(
                    formatter,
                    "energy-store disassembly structure failed: {error}"
                )
            }
        }
    }
}

impl Error for EnergyStoreDisassemblyCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::StaleInventory { .. }
            | Self::StaleEnergy { .. }
            | Self::UnknownStore { .. }
            | Self::StoreChanged { .. }
            | Self::StoreBusyProduction { .. }
            | Self::StoreBusyManualPower { .. } => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnergyStoreDisassemblyOutcome {
    recovered_lots: Vec<MaterialLotId>,
}

impl EnergyStoreDisassemblyOutcome {
    #[must_use]
    pub fn recovered_lots(&self) -> &[MaterialLotId] {
        &self.recovered_lots
    }
}

#[must_use]
pub struct ValidatedEnergyStoreDisassembly {
    store: EnergyStoreId,
    expected_energy_revision: u64,
    next_energy_revision: u64,
    expected_embodied_mass: Mass,
    ingress: ValidatedMaterialIngress,
    structural_load: Option<ValidatedStockpileStructuralLoad>,
}

impl ValidatedEnergyStoreDisassembly {
    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<EnergyStoreDisassemblyOutcome, EnergyStoreDisassemblyCommitError> {
        if state.inventory().revision() != self.ingress.expected_revision() {
            return Err(EnergyStoreDisassemblyCommitError::StaleInventory {
                expected: self.ingress.expected_revision(),
                actual: state.inventory().revision(),
            });
        }
        if state.energy().revision() != self.expected_energy_revision {
            return Err(EnergyStoreDisassemblyCommitError::StaleEnergy {
                expected: self.expected_energy_revision,
                actual: state.energy().revision(),
            });
        }
        let record = state
            .energy()
            .get_store(self.store)
            .ok_or(EnergyStoreDisassemblyCommitError::UnknownStore { store: self.store })?;
        if record.stored() != Energy::ZERO || record.embodied_mass() != self.expected_embodied_mass
        {
            return Err(EnergyStoreDisassemblyCommitError::StoreChanged { store: self.store });
        }
        if let Some(job) = state.production().get_energy_occupant(self.store) {
            return Err(EnergyStoreDisassemblyCommitError::StoreBusyProduction {
                store: self.store,
                job,
            });
        }
        if state
            .player_work()
            .get_manual_power_energy_occupant(self.store)
            .is_some()
        {
            return Err(EnergyStoreDisassemblyCommitError::StoreBusyManualPower {
                store: self.store,
            });
        }
        if let Some(load) = self.structural_load {
            load.commit(state)
                .map_err(EnergyStoreDisassemblyCommitError::Structure)?;
        }
        state.energy_state_mut().remove_store(
            self.store,
            self.expected_energy_revision,
            self.next_energy_revision,
        );
        let recovered_lots = apply_material_ingress(state.inventory_state_mut(), self.ingress);
        Ok(EnergyStoreDisassemblyOutcome { recovered_lots })
    }
}

/// Reverses construction of one empty, idle, material-backed energy store.
pub fn validate_disassemble_energy_store(
    registries: &Registries,
    state: &AppState,
    store: EnergyStoreId,
    destination: StockpileId,
) -> Result<ValidatedEnergyStoreDisassembly, EnergyStoreDisassemblyError> {
    let record = state
        .energy()
        .get_store(store)
        .ok_or(EnergyStoreDisassemblyError::UnknownStore { store })?;
    if record.embodied_mass().is_zero() || record.embodied_material().is_empty() {
        return Err(EnergyStoreDisassemblyError::NoEmbodiedMatter { store });
    }
    if record.stored() != Energy::ZERO {
        return Err(EnergyStoreDisassemblyError::StoreNotEmpty {
            store,
            stored: record.stored(),
        });
    }
    if let Some(job) = state.production().get_energy_occupant(store) {
        let release = state
            .production()
            .get_job(job)
            .unwrap_or_else(|| {
                panic!(
                    "runtime invariant broken: energy occupancy references missing production job {}",
                    job.value()
                )
            })
            .occupancy_release();
        return Err(EnergyStoreDisassemblyError::StoreBusyProduction {
            store,
            job,
            release,
        });
    }
    if state
        .player_work()
        .get_manual_power_energy_occupant(store)
        .is_some()
    {
        return Err(EnergyStoreDisassemblyError::StoreBusyManualPower { store });
    }

    let ingress = validate_material_ingress(
        registries,
        state.inventory(),
        destination,
        record
            .embodied_material()
            .iter()
            .map(MaterialIngressEntry::from_consumed_trace),
        state.tick(),
    )
    .map_err(|error| map_ingress_error(store, error))?;
    let destination_record = state.inventory().get_stockpile(destination).ok_or(
        EnergyStoreDisassemblyError::UnknownDestination {
            stockpile: destination,
        },
    )?;
    let destination_after = destination_record
        .stored_mass()
        .checked_add(record.embodied_mass())
        .ok_or(EnergyStoreDisassemblyError::DestinationMassOverflow {
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
    .map_err(EnergyStoreDisassemblyError::StoredMatterLoad)?;
    let expected_energy_revision = state.energy().revision();
    let next_energy_revision = expected_energy_revision
        .checked_add(1)
        .ok_or(EnergyStoreDisassemblyError::EnergyRevisionExhausted)?;

    Ok(ValidatedEnergyStoreDisassembly {
        store,
        expected_energy_revision,
        next_energy_revision,
        expected_embodied_mass: record.embodied_mass(),
        ingress,
        structural_load,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{
        ENERGY_STONE_FLYWHEEL_DRIVE, EQUIPMENT_STONE_HAND_CRANK, FORM_FLYWHEEL, FORM_HANDLE,
        MANUAL_POWER_HAND_CRANK, MATERIAL_STONE, MATERIAL_WOOD, build_registries,
    };
    use crate::core::quantity::Temperature;
    use crate::core::state::validate_loaded_state;
    use crate::core::time::WorldSeed;
    use crate::energy::validate_assemble_energy_store;
    use crate::equipment::validate_assemble_equipment;
    use crate::inventory::{add_solid_stockpile_for_test, deposit_lot_for_test};
    use crate::labor::{ManualPowerRequest, validate_start_manual_power};
    use crate::material::CommodityKey;
    use crate::matter::calculate_matter_accounting;
    use crate::survival::initialize_player_survival;

    fn assembled_store(registries: &Registries, state: &mut AppState) -> EnergyStoreId {
        let source = add_solid_stockpile_for_test(state, Mass::from_milligrams(1_100_000))
            .unwrap_or_else(|error| panic!("store disassembly source failed: {error}"));
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
            .unwrap_or_else(|error| panic!("store disassembly material failed: {error}"));
        }
        validate_assemble_energy_store(registries, state, ENERGY_STONE_FLYWHEEL_DRIVE, source)
            .unwrap_or_else(|error| panic!("store disassembly assembly failed: {error}"))
            .commit(state)
            .unwrap_or_else(|error| panic!("store disassembly assembly commit failed: {error}"))
    }

    fn assembled_crank(
        registries: &Registries,
        state: &mut AppState,
    ) -> crate::equipment::EquipmentId {
        let source = add_solid_stockpile_for_test(state, Mass::from_milligrams(1_100_000))
            .unwrap_or_else(|error| panic!("store-disassembly crank source failed: {error}"));
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
            .unwrap_or_else(|error| panic!("store-disassembly crank material failed: {error}"));
        }
        validate_assemble_equipment(registries, state, EQUIPMENT_STONE_HAND_CRANK, source)
            .unwrap_or_else(|error| panic!("store-disassembly crank assembly failed: {error}"))
            .commit(state)
            .unwrap_or_else(|error| panic!("store-disassembly crank commit failed: {error}"))
    }

    #[test]
    fn empty_store_disassembly_recovers_exact_matter_without_reusing_identity() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0xD15E_0001));
        let store = assembled_store(&registries, &mut state);
        let destination =
            add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_100_000))
                .unwrap_or_else(|error| panic!("store disassembly destination failed: {error}"));
        let matter_before = calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("store disassembly matter before failed: {error}"))
            .total();

        let outcome = validate_disassemble_energy_store(&registries, &state, store, destination)
            .unwrap_or_else(|error| panic!("store disassembly validation failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("store disassembly commit failed: {error}"));
        assert_eq!(outcome.recovered_lots().len(), 2);
        assert!(state.energy().get_store(store).is_none());
        assert_eq!(
            state
                .inventory()
                .get_stockpile(destination)
                .map(|stockpile| stockpile.stored_mass()),
            Some(Mass::from_milligrams(1_100_000))
        );
        assert_eq!(
            calculate_matter_accounting(&state)
                .unwrap_or_else(|error| panic!("store disassembly matter after failed: {error}"))
                .total(),
            matter_before
        );
        validate_loaded_state(&registries, &state)
            .unwrap_or_else(|error| panic!("store disassembly state audit failed: {error}"));

        let replacement = assembled_store(&registries, &mut state);
        assert!(
            replacement > store,
            "energy-store IDs must remain monotonic after disassembly"
        );
    }

    #[test]
    fn nonempty_store_cannot_be_disassembled_and_destroy_energy() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0xD15E_0002));
        let store = assembled_store(&registries, &mut state);
        let destination =
            add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_100_000))
                .unwrap_or_else(|error| panic!("nonempty store destination failed: {error}"));
        state
            .energy_state_mut()
            .add_stored_energy(store, Energy::from_nanojoules(1));
        let before = state.clone();

        assert_eq!(
            validate_disassemble_energy_store(&registries, &state, store, destination).err(),
            Some(EnergyStoreDisassemblyError::StoreNotEmpty {
                store,
                stored: Energy::from_nanojoules(1),
            })
        );
        assert_eq!(state, before);
    }

    #[test]
    fn manual_power_start_invalidates_prior_empty_store_disassembly_without_energy_revision_change()
    {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0xD15E_0003));
        initialize_player_survival(&registries, &mut state)
            .unwrap_or_else(|error| panic!("store-disassembly survival setup failed: {error}"));
        let store = assembled_store(&registries, &mut state);
        let crank = assembled_crank(&registries, &mut state);
        let destination =
            add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_100_000))
                .unwrap_or_else(|error| {
                    panic!("store-disassembly race destination failed: {error}")
                });
        let token = validate_disassemble_energy_store(&registries, &state, store, destination)
            .unwrap_or_else(|error| panic!("store-disassembly race validation failed: {error}"));
        let energy_revision = state.energy().revision();
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
        .unwrap_or_else(|error| {
            panic!("store-disassembly race manual-power validation failed: {error}")
        })
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("store-disassembly race manual-power commit failed: {error}")
        });
        assert_eq!(
            state.energy().revision(),
            energy_revision,
            "manual-power admission should not need an energy mutation to reserve an empty destination"
        );

        assert_eq!(
            token.commit(&mut state),
            Err(EnergyStoreDisassemblyCommitError::StoreBusyManualPower { store })
        );
        assert!(state.energy().get_store(store).is_some());
        assert_eq!(
            state
                .inventory()
                .get_stockpile(destination)
                .map(|stockpile| stockpile.stored_mass()),
            Some(Mass::ZERO)
        );
    }
}

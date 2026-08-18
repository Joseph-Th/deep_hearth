//! Conserved inventory-to-energy-storage construction transactions.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Energy, Mass};
use crate::core::state::AppState;
use crate::inventory::{
    StockpileId, StockpileStoredMassChange, StockpileStructuralLoadError, ValidatedMaterialEgress,
    ValidatedStockpileStructuralLoad, apply_material_egress, validate_consumption_selection,
    validate_material_egress_from_selection, validate_stockpile_stored_mass_changes,
};
use crate::material::MaterialComposition;
use crate::production::{ProductionJobId, ProductionOccupancyRelease};
use crate::registry::Registries;
use crate::structural::StructuralCommitError;

use super::state::EnergyStoreRecord;
use super::{EnergyStoreDefinitionId, EnergyStoreId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EnergyStoreAssemblyError {
    UnknownDefinition {
        definition: EnergyStoreDefinitionId,
    },
    NoAssemblyProfile {
        definition: EnergyStoreDefinitionId,
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
    ImpureAssemblyMaterial,
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
    StoreIdExhausted,
    EnergyRevisionExhausted,
    StructuralLoad(StockpileStructuralLoadError),
}

impl Display for EnergyStoreAssemblyError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownDefinition { definition } => write!(
                formatter,
                "unknown energy store definition {}",
                definition.value()
            ),
            Self::NoAssemblyProfile { definition } => write!(
                formatter,
                "energy store definition {} has no authored construction material",
                definition.value()
            ),
            Self::UnknownSource { stockpile } => write!(
                formatter,
                "unknown storage-construction stockpile {}",
                stockpile.value()
            ),
            Self::InsufficientMaterial {
                stockpile,
                available,
                required,
            } => write!(
                formatter,
                "stockpile {} contains {} mg of construction material but {} mg is required",
                stockpile.value(),
                available.milligrams(),
                required.milligrams()
            ),
            Self::SourceMassOverflow { stockpile } => write!(
                formatter,
                "energy-store construction source {} mass accounting overflowed",
                stockpile.value()
            ),
            Self::ImpureAssemblyMaterial => formatter.write_str(
                "energy-store construction requires pure matter matching the authored inputs",
            ),
            Self::StaleInventorySelection { expected, actual } => write!(
                formatter,
                "energy-store construction material selection expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::SourceBusy {
                stockpile,
                job,
                release,
            } => write!(
                formatter,
                "energy-store construction source {} is occupied by production job {} {release}",
                stockpile.value(),
                job.value()
            ),
            Self::InventoryRevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted")
            }
            Self::StoreIdExhausted => {
                formatter.write_str("energy store identifier space is exhausted")
            }
            Self::EnergyRevisionExhausted => {
                formatter.write_str("energy state revision space is exhausted")
            }
            Self::StructuralLoad(error) => write!(
                formatter,
                "energy-store construction source load failed: {error}"
            ),
        }
    }
}

impl Error for EnergyStoreAssemblyError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::StructuralLoad(error) => Some(error),
            Self::UnknownDefinition { .. }
            | Self::NoAssemblyProfile { .. }
            | Self::UnknownSource { .. }
            | Self::InsufficientMaterial { .. }
            | Self::SourceMassOverflow { .. }
            | Self::ImpureAssemblyMaterial
            | Self::StaleInventorySelection { .. }
            | Self::SourceBusy { .. }
            | Self::InventoryRevisionExhausted
            | Self::StoreIdExhausted
            | Self::EnergyRevisionExhausted => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EnergyStoreAssemblyCommitError {
    StaleInventory {
        expected: u64,
        actual: u64,
    },
    StaleEnergy {
        expected: u64,
        actual: u64,
    },
    SourceBusy {
        stockpile: StockpileId,
        job: ProductionJobId,
    },
    Structure(StructuralCommitError),
}

impl Display for EnergyStoreAssemblyCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleInventory { expected, actual } => write!(
                formatter,
                "energy-store construction expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::StaleEnergy { expected, actual } => write!(
                formatter,
                "energy-store construction expected energy revision {expected} but current revision is {actual}"
            ),
            Self::SourceBusy { stockpile, job } => write!(
                formatter,
                "energy-store construction source {} became occupied by production job {}",
                stockpile.value(),
                job.value()
            ),
            Self::Structure(error) => write!(
                formatter,
                "energy-store construction structure failed: {error}"
            ),
        }
    }
}

impl Error for EnergyStoreAssemblyCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::StaleInventory { .. } | Self::StaleEnergy { .. } | Self::SourceBusy { .. } => {
                None
            }
        }
    }
}

#[must_use]
pub struct ValidatedEnergyStoreAssembly {
    record: EnergyStoreRecord,
    next_store_id: u64,
    expected_energy_revision: u64,
    next_energy_revision: u64,
    egress: ValidatedMaterialEgress,
    structural_load: Option<ValidatedStockpileStructuralLoad>,
}

impl ValidatedEnergyStoreAssembly {
    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<EnergyStoreId, EnergyStoreAssemblyCommitError> {
        if state.inventory().revision() != self.egress.expected_revision() {
            return Err(EnergyStoreAssemblyCommitError::StaleInventory {
                expected: self.egress.expected_revision(),
                actual: state.inventory().revision(),
            });
        }
        if state.energy().revision() != self.expected_energy_revision {
            return Err(EnergyStoreAssemblyCommitError::StaleEnergy {
                expected: self.expected_energy_revision,
                actual: state.energy().revision(),
            });
        }
        if let Some(job) = state
            .production()
            .get_stockpile_occupant(self.egress.source())
        {
            return Err(EnergyStoreAssemblyCommitError::SourceBusy {
                stockpile: self.egress.source(),
                job: job.id(),
            });
        }
        if let Some(load) = self.structural_load {
            load.commit(state)
                .map_err(EnergyStoreAssemblyCommitError::Structure)?;
        }
        let id = self.record.id();
        apply_material_egress(state.inventory_state_mut(), self.egress);
        state.energy_state_mut().insert_store(
            self.record,
            self.next_store_id,
            self.next_energy_revision,
        );
        Ok(id)
    }
}

/// Validates construction of one authored finite-energy store from exact conserved material.
pub fn validate_assemble_energy_store(
    registries: &Registries,
    state: &AppState,
    definition: EnergyStoreDefinitionId,
    source: StockpileId,
) -> Result<ValidatedEnergyStoreAssembly, EnergyStoreAssemblyError> {
    let definition_record = registries
        .energy()
        .get_store(definition)
        .ok_or(EnergyStoreAssemblyError::UnknownDefinition { definition })?;
    let assembly = definition_record
        .assembly_profile()
        .ok_or(EnergyStoreAssemblyError::NoAssemblyProfile { definition })?;
    let selection = validate_consumption_selection(state.inventory(), source, assembly.inputs())
        .map_err(|error| match error {
            crate::inventory::ConsumptionSelectionError::UnknownStockpile { stockpile } => {
                EnergyStoreAssemblyError::UnknownSource { stockpile }
            }
            crate::inventory::ConsumptionSelectionError::InsufficientMass {
                stockpile,
                available,
                requested,
                ..
            } => EnergyStoreAssemblyError::InsufficientMaterial {
                stockpile,
                available,
                required: requested,
            },
            crate::inventory::ConsumptionSelectionError::MassOverflow { stockpile } => {
                EnergyStoreAssemblyError::SourceMassOverflow { stockpile }
            }
        })?;
    if selection.consumed_inputs().iter().any(|trace| {
        trace.profile().composition()
            != &MaterialComposition::pure(trace.profile().commodity().material())
    }) {
        return Err(EnergyStoreAssemblyError::ImpureAssemblyMaterial);
    }
    let embodied_material = selection.consumed_inputs().to_vec();
    if let Some(job) = state.production().get_stockpile_occupant(source) {
        return Err(EnergyStoreAssemblyError::SourceBusy {
            stockpile: source,
            job: job.id(),
            release: job.occupancy_release(),
        });
    }
    let egress =
        validate_material_egress_from_selection(state.inventory(), selection).map_err(|error| {
            match error {
                crate::inventory::MaterialEgressError::StaleSelection { expected, actual } => {
                    EnergyStoreAssemblyError::StaleInventorySelection { expected, actual }
                }
                crate::inventory::MaterialEgressError::RevisionExhausted => {
                    EnergyStoreAssemblyError::InventoryRevisionExhausted
                }
            }
        })?;
    let source_record = state
        .inventory()
        .get_stockpile(source)
        .ok_or(EnergyStoreAssemblyError::UnknownSource { stockpile: source })?;
    let source_after = source_record
        .stored_mass()
        .checked_sub(egress.total_consumed())
        .ok_or(EnergyStoreAssemblyError::SourceMassOverflow { stockpile: source })?;
    let structural_load = validate_stockpile_stored_mass_changes(
        registries,
        state,
        [StockpileStoredMassChange::new(source, source_after)],
    )
    .map_err(EnergyStoreAssemblyError::StructuralLoad)?;

    let id_value = state.energy().next_store_id();
    let next_store_id = id_value
        .checked_add(1)
        .ok_or(EnergyStoreAssemblyError::StoreIdExhausted)?;
    let id = EnergyStoreId::new(id_value);
    let expected_energy_revision = state.energy().revision();
    let next_energy_revision = expected_energy_revision
        .checked_add(1)
        .ok_or(EnergyStoreAssemblyError::EnergyRevisionExhausted)?;
    Ok(ValidatedEnergyStoreAssembly {
        record: EnergyStoreRecord {
            id,
            definition,
            stored: Energy::ZERO,
            embodied_mass: assembly.input_mass(),
            embodied_material,
            created_at: state.tick(),
        },
        next_store_id,
        expected_energy_revision,
        next_energy_revision,
        egress,
        structural_load,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{
        ENERGY_STONE_FLYWHEEL_DRIVE, FORM_FLYWHEEL, FORM_HANDLE, MATERIAL_STONE, MATERIAL_WOOD,
        build_registries,
    };
    use crate::core::quantity::Temperature;
    use crate::core::state::{StateValidationError, validate_loaded_state};
    use crate::core::time::WorldSeed;
    use crate::energy::{AddEnergyStoreError, EnergyValidationError, add_energy_store};
    use crate::inventory::{add_solid_stockpile_for_test, deposit_lot_for_test};
    use crate::material::CommodityKey;
    use crate::matter::calculate_matter_accounting;
    use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
    use crate::simulation::advance_tick;

    fn assembled_store_fixture() -> (Registries, AppState, EnergyStoreId) {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0xE57E_0001));
        let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_100_000))
            .unwrap_or_else(|error| panic!("energy assembly stockpile fixture failed: {error}"));
        deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
            Mass::from_milligrams(900_000),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("energy assembly flywheel fixture failed: {error}"));
        deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(200_000),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("energy assembly shaft fixture failed: {error}"));

        let store = validate_assemble_energy_store(
            &registries,
            &state,
            ENERGY_STONE_FLYWHEEL_DRIVE,
            source,
        )
        .unwrap_or_else(|error| panic!("energy-store assembly fixture validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("energy-store assembly fixture commit failed: {error}"));
        (registries, state, store)
    }

    #[test]
    fn buildable_energy_store_requires_material_and_preserves_world_matter() {
        let registries = build_registries();
        let mut empty_state = AppState::new(WorldSeed::new(0xE57E_0000));
        assert_eq!(
            add_energy_store(&registries, &mut empty_state, ENERGY_STONE_FLYWHEEL_DRIVE,),
            Err(AddEnergyStoreError::RequiresAssembly {
                definition: ENERGY_STONE_FLYWHEEL_DRIVE,
            })
        );

        let (registries, state, store) = assembled_store_fixture();
        let record = state
            .energy()
            .get_store(store)
            .unwrap_or_else(|| panic!("assembled energy store disappeared"));
        assert_eq!(record.stored(), Energy::ZERO);
        assert_eq!(record.embodied_mass(), Mass::from_milligrams(1_100_000));
        assert_eq!(record.embodied_material().len(), 2);
        let accounting = calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("assembled-store matter accounting failed: {error}"));
        assert_eq!(
            accounting.energy_storage(),
            crate::core::quantity::AggregateMass::from_milligrams(1_100_000)
        );
        assert_eq!(accounting.total(), accounting.energy_storage());
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }

    #[test]
    fn load_rejects_forged_energy_store_embodied_mass() {
        let (registries, state, store) = assembled_store_fixture();
        let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
            .unwrap_or_else(|error| {
                panic!("energy embodiment tamper serialization failed: {error}")
            });
        encoded["state"]["systems"]["energy"]["records"][store.value().to_string()]["embodied_mass"] =
            serde_json::json!(1_000_000_u64);
        let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
            .unwrap_or_else(|error| panic!("energy embodiment tamper decode failed: {error}"));

        assert_eq!(
            decoded.into_state(&registries),
            Err(LoadError::InvalidState(StateValidationError::Energy(
                EnergyValidationError::EmbodiedTraceMassMismatch {
                    store,
                    stored: Mass::from_milligrams(1_000_000),
                    traced: Mass::from_milligrams(1_100_000),
                }
            )))
        );
    }

    #[test]
    fn load_rejects_energy_store_material_created_after_construction() {
        let (registries, mut state, store) = assembled_store_fixture();
        advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("energy provenance audit tick failed: {error}"));
        let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
            .unwrap_or_else(|error| {
                panic!("energy provenance tamper serialization failed: {error}")
            });
        let trace = &mut encoded["state"]["systems"]["energy"]["records"]
            [store.value().to_string()]["embodied_material"][0]["provenance"];
        trace["earliest_created_at"] = serde_json::json!(1_u64);
        trace["latest_created_at"] = serde_json::json!(1_u64);
        let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
            .unwrap_or_else(|error| panic!("energy provenance tamper decode failed: {error}"));

        assert_eq!(
            decoded.into_state(&registries),
            Err(LoadError::InvalidState(StateValidationError::Energy(
                EnergyValidationError::EmbodiedProvenanceAfterConstruction {
                    store,
                    latest_created_at: crate::core::time::SimulationTick::new(1),
                    created_at: crate::core::time::SimulationTick::ZERO,
                }
            )))
        );
    }
}

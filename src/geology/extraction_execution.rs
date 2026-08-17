//! Generated-deposit insertion and revision-bound transfer of conserved geological matter into inventory.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::{
    MaterialIngressError, MaterialLotId, StockpileId, StockpileStorageError,
    StockpileStoredMassChange, StockpileStructuralLoadError, ValidatedMaterialIngress,
    ValidatedStockpileStructuralLoad, apply_material_ingress, validate_material_ingress,
    validate_stockpile_stored_mass_changes,
};
use crate::material::{
    CompositionError, FormId, MaterialId, MaterialLotSpec, MaterialLotSpecError, MaterialPhase,
    MaterialPhaseStateError, ParticleSizeStatePolicy, validate_material_phase_state,
};
use crate::registry::Registries;
use crate::structural::StructuralCommitError;

use super::state::{
    GeneratedDepositSpec, GeologicalDepositId, GeologicalDepositLifecycle, GeologicalDepositRecord,
};

/// Failure while admitting a finite world-generated geological deposit into authoritative state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InsertGeneratedDepositError {
    UnknownMaterial { material: MaterialId },
    UnknownForm { form: FormId },
    UnsupportedPhase { form: FormId, phase: MaterialPhase },
    UnsupportedParticulateForm { form: FormId },
    InvalidPhaseState(MaterialPhaseStateError),
    UnknownCompositionMaterial { material: MaterialId },
    IdExhausted,
    RevisionExhausted,
}

impl Display for InsertGeneratedDepositError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownMaterial { material } => write!(
                formatter,
                "generated geological deposit references unknown material {}",
                material.value()
            ),
            Self::UnknownForm { form } => write!(
                formatter,
                "generated geological deposit references unknown form {}",
                form.value()
            ),
            Self::UnsupportedPhase { form, phase } => write!(
                formatter,
                "generated geological deposit form {} is {phase:?}; finite geological deposits must be solid",
                form.value()
            ),
            Self::UnsupportedParticulateForm { form } => write!(
                formatter,
                "generated geological deposit form {} requires processed particle-size state; natural geological deposits cannot own it",
                form.value()
            ),
            Self::InvalidPhaseState(error) => write!(
                formatter,
                "generated geological deposit has invalid material phase state: {error}"
            ),
            Self::UnknownCompositionMaterial { material } => write!(
                formatter,
                "generated geological deposit composition references unknown material {}",
                material.value()
            ),
            Self::IdExhausted => {
                formatter.write_str("geological deposit identifier space is exhausted")
            }
            Self::RevisionExhausted => formatter.write_str("geology revision space is exhausted"),
        }
    }
}

impl Error for InsertGeneratedDepositError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidPhaseState(error) => Some(error),
            Self::UnknownMaterial { .. }
            | Self::UnknownForm { .. }
            | Self::UnsupportedPhase { .. }
            | Self::UnsupportedParticulateForm { .. }
            | Self::UnknownCompositionMaterial { .. }
            | Self::IdExhausted
            | Self::RevisionExhausted => None,
        }
    }
}

/// Inserts matter supplied by a world-generation owner, preserving its physical profile exactly.
///
/// This is not a player mining operation. It establishes finite geological matter that later
/// physical mining resolvers may authorize for extraction.
pub fn insert_generated_deposit(
    registries: &Registries,
    state: &mut AppState,
    spec: GeneratedDepositSpec,
) -> Result<GeologicalDepositId, InsertGeneratedDepositError> {
    if registries
        .materials()
        .get_material(spec.commodity().material())
        .is_none()
    {
        return Err(InsertGeneratedDepositError::UnknownMaterial {
            material: spec.commodity().material(),
        });
    }
    let Some(form) = registries.materials().get_form(spec.commodity().form()) else {
        return Err(InsertGeneratedDepositError::UnknownForm {
            form: spec.commodity().form(),
        });
    };
    if form.phase() != MaterialPhase::Solid {
        return Err(InsertGeneratedDepositError::UnsupportedPhase {
            form: spec.commodity().form(),
            phase: form.phase(),
        });
    }
    if form.particle_size_policy() == ParticleSizeStatePolicy::Required {
        return Err(InsertGeneratedDepositError::UnsupportedParticulateForm {
            form: spec.commodity().form(),
        });
    }
    for component in spec.composition().components() {
        if registries
            .materials()
            .get_material(component.material())
            .is_none()
        {
            return Err(InsertGeneratedDepositError::UnknownCompositionMaterial {
                material: component.material(),
            });
        }
    }
    validate_material_phase_state(
        registries.materials(),
        spec.commodity(),
        spec.composition(),
        spec.temperature(),
    )
    .map_err(InsertGeneratedDepositError::InvalidPhaseState)?;

    let geology = state.geology();
    let id = GeologicalDepositId::new(geology.next_deposit_id());
    let Some(next_id) = geology.next_deposit_id().checked_add(1) else {
        return Err(InsertGeneratedDepositError::IdExhausted);
    };
    let Some(next_revision) = geology.revision().checked_add(1) else {
        return Err(InsertGeneratedDepositError::RevisionExhausted);
    };
    let generated_at = state.tick();
    let record = GeologicalDepositRecord {
        id,
        bounds: spec.bounds(),
        commodity: spec.commodity(),
        initial_mass: spec.mass(),
        remaining_mass: spec.mass(),
        temperature: spec.temperature(),
        composition: spec.composition().clone(),
        lifecycle: GeologicalDepositLifecycle::Available,
        generated_at,
    };

    let geology = state.geology_state_mut();
    geology.insert_deposit(record, next_id, next_revision);
    Ok(id)
}

/// Immutable result produced by a future physical mining resolver for one exact deposit snapshot.
///
/// There is deliberately no public constructor. Tool capability, labor, excavation geometry,
/// extraction rate, waste streams, and risk must eventually be resolved before gameplay code can
/// create this authorization value.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExtractionResolution {
    deposit: GeologicalDepositId,
    mass: Mass,
}

impl ExtractionResolution {
    #[must_use]
    pub const fn deposit(self) -> GeologicalDepositId {
        self.deposit
    }

    #[must_use]
    pub const fn mass(self) -> Mass {
        self.mass
    }
}

/// Failure while validating one authorized geological extraction transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GeologicalExtractionError {
    UnknownDeposit {
        deposit: GeologicalDepositId,
    },
    DepositDepleted {
        deposit: GeologicalDepositId,
    },
    ZeroMass,
    InsufficientMass {
        deposit: GeologicalDepositId,
        available: Mass,
        requested: Mass,
    },
    InvalidOutput(MaterialLotSpecError),
    UnknownDestination {
        stockpile: StockpileId,
    },
    UnknownDepositMaterial {
        material: MaterialId,
    },
    UnknownDepositForm {
        form: FormId,
    },
    UnknownDepositCompositionMaterial {
        material: MaterialId,
    },
    InvalidDepositComposition {
        error: CompositionError,
    },
    DepositCompositionMissingHost {
        host: MaterialId,
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
    GeologyRevisionExhausted,
    StructuralLoad(StockpileStructuralLoadError),
}

impl Display for GeologicalExtractionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownDeposit { deposit } => write!(
                formatter,
                "unknown geological deposit id {}",
                deposit.value()
            ),
            Self::DepositDepleted { deposit } => write!(
                formatter,
                "geological deposit {} is depleted",
                deposit.value()
            ),
            Self::ZeroMass => formatter.write_str("geological extraction mass must be nonzero"),
            Self::InsufficientMass {
                deposit,
                available,
                requested,
            } => write!(
                formatter,
                "geological deposit {} has {} mg remaining but {} mg was requested",
                deposit.value(),
                available.milligrams(),
                requested.milligrams()
            ),
            Self::InvalidOutput(error) => write!(
                formatter,
                "geological extraction cannot preserve deposit material profile: {error}"
            ),
            Self::UnknownDestination { stockpile } => write!(
                formatter,
                "geological extraction destination stockpile {} does not exist",
                stockpile.value()
            ),
            Self::UnknownDepositMaterial { material } => write!(
                formatter,
                "geological extraction deposit references unknown material {}",
                material.value()
            ),
            Self::UnknownDepositForm { form } => write!(
                formatter,
                "geological extraction deposit references unknown form {}",
                form.value()
            ),
            Self::UnknownDepositCompositionMaterial { material } => write!(
                formatter,
                "geological extraction deposit composition references unknown material {}",
                material.value()
            ),
            Self::InvalidDepositComposition { error } => write!(
                formatter,
                "geological extraction deposit has invalid composition: {error}"
            ),
            Self::DepositCompositionMissingHost { host } => write!(
                formatter,
                "geological extraction deposit composition omits host material {}",
                host.value()
            ),
            Self::DestinationStorage(error) => write!(
                formatter,
                "geological extraction destination rejects material: {error}"
            ),
            Self::DestinationMassOverflow { stockpile } => write!(
                formatter,
                "geological extraction overflows destination stockpile {} mass accounting",
                stockpile.value()
            ),
            Self::DestinationCapacityExceeded {
                stockpile,
                capacity,
                committed,
                requested,
            } => write!(
                formatter,
                "geological extraction exceeds stockpile {} capacity {} mg: {} mg committed, {} mg requested",
                stockpile.value(),
                capacity.milligrams(),
                committed.milligrams(),
                requested.milligrams()
            ),
            Self::LotIdExhausted => {
                formatter.write_str("material lot identifier space is exhausted during extraction")
            }
            Self::InventoryRevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted during extraction")
            }
            Self::GeologyRevisionExhausted => {
                formatter.write_str("geology revision space is exhausted")
            }
            Self::StructuralLoad(error) => write!(
                formatter,
                "geological extraction cannot update stored-matter structural load: {error}"
            ),
        }
    }
}

impl Error for GeologicalExtractionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidOutput(error) => Some(error),
            Self::InvalidDepositComposition { error } => Some(error),
            Self::DestinationStorage(error) => Some(error),
            Self::StructuralLoad(error) => Some(error),
            Self::UnknownDeposit { .. }
            | Self::DepositDepleted { .. }
            | Self::ZeroMass
            | Self::InsufficientMass { .. }
            | Self::UnknownDestination { .. }
            | Self::UnknownDepositMaterial { .. }
            | Self::UnknownDepositForm { .. }
            | Self::UnknownDepositCompositionMaterial { .. }
            | Self::DepositCompositionMissingHost { .. }
            | Self::DestinationMassOverflow { .. }
            | Self::DestinationCapacityExceeded { .. }
            | Self::LotIdExhausted
            | Self::InventoryRevisionExhausted
            | Self::GeologyRevisionExhausted => None,
        }
    }
}

fn map_material_ingress_error(error: MaterialIngressError) -> GeologicalExtractionError {
    match error {
        MaterialIngressError::UnknownStockpile { stockpile } => {
            GeologicalExtractionError::UnknownDestination { stockpile }
        }
        MaterialIngressError::UnknownMaterial { material } => {
            GeologicalExtractionError::UnknownDepositMaterial { material }
        }
        MaterialIngressError::UnknownForm { form } => {
            GeologicalExtractionError::UnknownDepositForm { form }
        }
        MaterialIngressError::UnknownCompositionMaterial { material } => {
            GeologicalExtractionError::UnknownDepositCompositionMaterial { material }
        }
        MaterialIngressError::ZeroMass => GeologicalExtractionError::ZeroMass,
        MaterialIngressError::InvalidComposition { error } => {
            GeologicalExtractionError::InvalidDepositComposition { error }
        }
        MaterialIngressError::CompositionMissingHost { host } => {
            GeologicalExtractionError::DepositCompositionMissingHost { host }
        }
        MaterialIngressError::Storage(error) => {
            GeologicalExtractionError::DestinationStorage(error)
        }
        MaterialIngressError::MassOverflow { stockpile } => {
            GeologicalExtractionError::DestinationMassOverflow { stockpile }
        }
        MaterialIngressError::CapacityExceeded {
            stockpile,
            capacity,
            committed,
            requested,
        } => GeologicalExtractionError::DestinationCapacityExceeded {
            stockpile,
            capacity,
            committed,
            requested,
        },
        MaterialIngressError::LotIdExhausted => GeologicalExtractionError::LotIdExhausted,
        MaterialIngressError::RevisionExhausted => {
            GeologicalExtractionError::InventoryRevisionExhausted
        }
    }
}

/// Failure to commit a revision-bound geological extraction after validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GeologicalExtractionCommitError {
    StaleGeologyRevision { expected: u64, actual: u64 },
    StaleInventoryRevision { expected: u64, actual: u64 },
    StaleStructureRevision { expected: u64, actual: u64 },
    Structure(StructuralCommitError),
}

impl Display for GeologicalExtractionCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleGeologyRevision { expected, actual } => write!(
                formatter,
                "validated extraction expected geology revision {expected} but current revision is {actual}"
            ),
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "validated extraction expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::StaleStructureRevision { expected, actual } => write!(
                formatter,
                "validated extraction expected structural revision {expected} but current revision is {actual}"
            ),
            Self::Structure(error) => write!(
                formatter,
                "validated extraction could not commit stored-matter structural load: {error}"
            ),
        }
    }
}

impl Error for GeologicalExtractionCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::StaleGeologyRevision { .. }
            | Self::StaleInventoryRevision { .. }
            | Self::StaleStructureRevision { .. } => None,
        }
    }
}

/// Successful conserved transfer from one finite deposit into one stockpile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GeologicalExtractionOutcome {
    deposit: GeologicalDepositId,
    destination: StockpileId,
    lot: MaterialLotId,
    mass: Mass,
    is_depleted: bool,
}

impl GeologicalExtractionOutcome {
    #[must_use]
    pub const fn deposit(self) -> GeologicalDepositId {
        self.deposit
    }

    #[must_use]
    pub const fn destination(self) -> StockpileId {
        self.destination
    }

    #[must_use]
    pub const fn lot(self) -> MaterialLotId {
        self.lot
    }

    #[must_use]
    pub const fn mass(self) -> Mass {
        self.mass
    }

    #[must_use]
    pub const fn is_depleted(self) -> bool {
        self.is_depleted
    }
}

/// Consumed proof binding exact geology and inventory owner revisions for one extraction.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedGeologicalExtraction {
    expected_geology_revision: u64,
    next_geology_revision: u64,
    deposit: GeologicalDepositId,
    remaining_after: Mass,
    destination: StockpileId,
    mass: Mass,
    ingress: ValidatedMaterialIngress,
    structural_load: Option<ValidatedStockpileStructuralLoad>,
}

impl ValidatedGeologicalExtraction {
    /// Atomically moves the already validated mass between geological and inventory ownership.
    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<GeologicalExtractionOutcome, GeologicalExtractionCommitError> {
        let actual_geology_revision = state.geology().revision();
        if actual_geology_revision != self.expected_geology_revision {
            return Err(GeologicalExtractionCommitError::StaleGeologyRevision {
                expected: self.expected_geology_revision,
                actual: actual_geology_revision,
            });
        }
        let expected_inventory_revision = self.ingress.expected_revision();
        let actual_inventory_revision = state.inventory().revision();
        if actual_inventory_revision != expected_inventory_revision {
            return Err(GeologicalExtractionCommitError::StaleInventoryRevision {
                expected: expected_inventory_revision,
                actual: actual_inventory_revision,
            });
        }
        if let Some(structural_load) = &self.structural_load {
            let expected_structure_revision = structural_load.expected_revision();
            let actual_structure_revision = state.structures().revision();
            if actual_structure_revision != expected_structure_revision {
                return Err(GeologicalExtractionCommitError::StaleStructureRevision {
                    expected: expected_structure_revision,
                    actual: actual_structure_revision,
                });
            }
        }
        if let Some(structural_load) = self.structural_load {
            structural_load
                .commit(state)
                .map_err(GeologicalExtractionCommitError::Structure)?;
        }

        let lot = apply_material_ingress(state.inventory_state_mut(), self.ingress);
        state.geology_state_mut().apply_extraction(
            self.deposit,
            self.remaining_after,
            self.next_geology_revision,
        );

        Ok(GeologicalExtractionOutcome {
            deposit: self.deposit,
            destination: self.destination,
            lot,
            mass: self.mass,
            is_depleted: self.remaining_after.is_zero(),
        })
    }
}

/// Validates conserved geological matter movement without supplying mining authorization itself.
pub fn validate_geological_extraction(
    registries: &Registries,
    state: &AppState,
    resolution: &ExtractionResolution,
    destination: StockpileId,
) -> Result<ValidatedGeologicalExtraction, GeologicalExtractionError> {
    if resolution.mass.is_zero() {
        return Err(GeologicalExtractionError::ZeroMass);
    }
    let record = state.geology().get_deposit(resolution.deposit).ok_or(
        GeologicalExtractionError::UnknownDeposit {
            deposit: resolution.deposit,
        },
    )?;
    if record.lifecycle() == GeologicalDepositLifecycle::Depleted {
        return Err(GeologicalExtractionError::DepositDepleted {
            deposit: resolution.deposit,
        });
    }
    if record.remaining_mass() < resolution.mass {
        return Err(GeologicalExtractionError::InsufficientMass {
            deposit: resolution.deposit,
            available: record.remaining_mass(),
            requested: resolution.mass,
        });
    }
    let remaining_after = match record.remaining_mass().checked_sub(resolution.mass) {
        Some(remaining) => remaining,
        None => panic!("validated geological extraction underflowed remaining mass"),
    };
    let output = MaterialLotSpec::with_composition(
        record.commodity(),
        resolution.mass,
        record.temperature(),
        record.composition().clone(),
    )
    .map_err(GeologicalExtractionError::InvalidOutput)?;
    let ingress = validate_material_ingress(
        registries,
        state.inventory(),
        destination,
        output,
        state.tick(),
    )
    .map_err(map_material_ingress_error)?;
    let destination_record = state.inventory().get_stockpile(destination).ok_or(
        GeologicalExtractionError::UnknownDestination {
            stockpile: destination,
        },
    )?;
    let destination_after = destination_record
        .stored_mass()
        .checked_add(resolution.mass)
        .ok_or(GeologicalExtractionError::DestinationMassOverflow {
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
    .map_err(GeologicalExtractionError::StructuralLoad)?;
    let expected_geology_revision = state.geology().revision();
    let Some(next_geology_revision) = expected_geology_revision.checked_add(1) else {
        return Err(GeologicalExtractionError::GeologyRevisionExhausted);
    };

    Ok(ValidatedGeologicalExtraction {
        expected_geology_revision,
        next_geology_revision,
        deposit: resolution.deposit,
        remaining_after,
        destination,
        mass: resolution.mass,
        ingress,
        structural_load,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{
        FORM_CRUSHED, FORM_INGOT, FORM_MOLTEN, FORM_ORE, MATERIAL_COPPER,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
    };
    use crate::core::quantity::{AggregateMass, Area, Energy, Force, Length, Temperature};
    use crate::core::state::validate_loaded_state;
    use crate::core::time::WorldSeed;
    use crate::energy::calculate_explicit_energy_accounting;
    use crate::inventory::{add_solid_stockpile_for_test, validate_mount_stockpile};
    use crate::material::{
        CommodityKey, CompositionComponent, MaterialComposition, MaterialId, MaterialPhase,
    };
    use crate::matter::calculate_matter_accounting;
    use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
    #[cfg(feature = "test-soak")]
    use crate::simulation::advance_tick;
    use crate::spatial::{VoxelBounds, VoxelCoord};
    use crate::structural::{
        StructuralElementId, StructuralLoadKind, add_structural_element,
        materialize_structural_element_for_test, validate_activate_structural_element,
    };

    fn active_support(registries: &Registries, state: &mut AppState) -> StructuralElementId {
        let bounds = match VoxelBounds::new(VoxelCoord::new(100, 0, 0), VoxelCoord::new(101, 1, 1))
        {
            Ok(bounds) => bounds,
            Err(error) => panic!("geological support bounds failed: {error}"),
        };
        let element = match add_structural_element(
            registries,
            state,
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_COPPER,
            crate::structural::make_test_structural_geometry(
                bounds,
                Length::from_micrometers(1),
                Area::from_square_millimeters(1_000),
            ),
            true,
        ) {
            Ok(element) => element,
            Err(error) => panic!("geological support fixture failed: {error}"),
        };
        materialize_structural_element_for_test(registries, state, element, FORM_INGOT);
        let activation = match validate_activate_structural_element(registries, state, element) {
            Ok(activation) => activation,
            Err(error) => panic!("geological support activation failed: {error}"),
        };
        if let Err(error) = activation.commit(state) {
            panic!("geological support activation commit failed: {error}");
        }
        element
    }

    fn bounds(x: i64) -> VoxelBounds {
        match VoxelBounds::new(VoxelCoord::new(x, -12, 0), VoxelCoord::new(x + 4, -8, 4)) {
            Ok(bounds) => bounds,
            Err(error) => panic!("geological extraction bounds fixture failed: {error}"),
        }
    }

    #[test]
    fn generated_geological_owner_rejects_liquid_material_form_without_mutation() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x6E00_0011));
        let spec = match GeneratedDepositSpec::new(
            bounds(0),
            CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN),
            Mass::from_milligrams(100),
            Temperature::from_millikelvin(1_357_770),
            MaterialComposition::pure(MATERIAL_COPPER),
        ) {
            Ok(spec) => spec,
            Err(error) => panic!("liquid geology specification fixture failed: {error}"),
        };
        let before = state.clone();

        assert_eq!(
            insert_generated_deposit(&registries, &mut state, spec),
            Err(InsertGeneratedDepositError::UnsupportedPhase {
                form: FORM_MOLTEN,
                phase: MaterialPhase::Liquid,
            })
        );
        assert_eq!(state, before);
    }

    #[test]
    fn generated_geological_owner_rejects_processed_particulate_form_without_mutation() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x6E00_0012));
        let spec = match GeneratedDepositSpec::new(
            bounds(0),
            CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED),
            Mass::from_milligrams(100),
            Temperature::from_millikelvin(300_000),
            MaterialComposition::pure(MATERIAL_COPPER),
        ) {
            Ok(spec) => spec,
            Err(error) => panic!("particulate geology specification fixture failed: {error}"),
        };
        let before = state.clone();

        assert_eq!(
            insert_generated_deposit(&registries, &mut state, spec),
            Err(InsertGeneratedDepositError::UnsupportedParticulateForm { form: FORM_CRUSHED })
        );
        assert_eq!(state, before);
    }

    fn deposit_spec(x: i64, mass: u64) -> GeneratedDepositSpec {
        match GeneratedDepositSpec::new(
            bounds(x),
            CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            Mass::from_milligrams(mass),
            Temperature::from_millikelvin(300_000),
            MaterialComposition::pure(MATERIAL_COPPER),
        ) {
            Ok(spec) => spec,
            Err(error) => panic!("geological extraction deposit fixture failed: {error}"),
        }
    }

    fn resolution(deposit: GeologicalDepositId, mass: u64) -> ExtractionResolution {
        ExtractionResolution {
            deposit,
            mass: Mass::from_milligrams(mass),
        }
    }

    fn total_explicit_energy(registries: &Registries, state: &AppState) -> Energy {
        match calculate_explicit_energy_accounting(registries, state).and_then(|accounting| {
            accounting
                .total()
                .ok_or(crate::energy::ExplicitEnergyAccountingError::Overflow)
        }) {
            Ok(total) => total,
            Err(error) => panic!("geological extraction energy accounting failed: {error}"),
        }
    }

    #[test]
    fn generated_deposit_insertion_resolves_all_material_references_before_mutation() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x6E00_0000));
        let unknown = MaterialId::new(999_999);
        let unknown_host = match GeneratedDepositSpec::new(
            bounds(0),
            CommodityKey::new(unknown, FORM_ORE),
            Mass::from_milligrams(10),
            Temperature::from_millikelvin(300_000),
            MaterialComposition::pure(unknown),
        ) {
            Ok(spec) => spec,
            Err(error) => panic!("unknown-host deposit specification failed locally: {error}"),
        };
        let before = state.clone();
        assert_eq!(
            insert_generated_deposit(&registries, &mut state, unknown_host),
            Err(InsertGeneratedDepositError::UnknownMaterial { material: unknown })
        );
        assert_eq!(state, before);

        let mixed = match MaterialComposition::new(vec![
            CompositionComponent::new(MATERIAL_COPPER, 500_000),
            CompositionComponent::new(unknown, 500_000),
        ]) {
            Ok(composition) => composition,
            Err(error) => panic!("unknown-constituent composition fixture failed: {error}"),
        };
        let unknown_constituent = match GeneratedDepositSpec::new(
            bounds(0),
            CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            Mass::from_milligrams(10),
            Temperature::from_millikelvin(300_000),
            mixed,
        ) {
            Ok(spec) => spec,
            Err(error) => {
                panic!("unknown-constituent deposit specification failed locally: {error}")
            }
        };
        assert_eq!(
            insert_generated_deposit(&registries, &mut state, unknown_constituent),
            Err(InsertGeneratedDepositError::UnknownCompositionMaterial { material: unknown })
        );
        assert_eq!(state, before);
    }

    #[test]
    fn extraction_moves_exact_profile_and_conserves_modeled_matter_and_energy() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x6E00_0001));
        let support = active_support(&registries, &mut state);
        let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        {
            Ok(destination) => destination,
            Err(error) => panic!("geological extraction destination failed: {error}"),
        };
        let mount = match validate_mount_stockpile(&registries, &state, destination, support) {
            Ok(mount) => mount,
            Err(error) => panic!("geological extraction destination mount failed: {error}"),
        };
        if let Err(error) = mount.commit(&mut state) {
            panic!("geological extraction destination mount commit failed: {error}");
        }
        let deposit = match insert_generated_deposit(&registries, &mut state, deposit_spec(0, 100))
        {
            Ok(deposit) => deposit,
            Err(error) => panic!("geological deposit insertion failed: {error}"),
        };
        let initial_matter = match calculate_matter_accounting(&state) {
            Ok(accounting) => accounting.total(),
            Err(error) => panic!("initial geological matter accounting failed: {error}"),
        };
        let initial_energy = total_explicit_energy(&registries, &state);

        let token = match validate_geological_extraction(
            &registries,
            &state,
            &resolution(deposit, 40),
            destination,
        ) {
            Ok(token) => token,
            Err(error) => panic!("geological extraction validation failed: {error}"),
        };
        let outcome = match token.commit(&mut state) {
            Ok(outcome) => outcome,
            Err(error) => panic!("geological extraction commit failed: {error}"),
        };

        assert_eq!(outcome.mass(), Mass::from_milligrams(40));
        assert!(!outcome.is_depleted());
        let record = match state.geology().get_deposit(deposit) {
            Some(record) => record,
            None => panic!("geological deposit disappeared after extraction"),
        };
        assert_eq!(record.remaining_mass(), Mass::from_milligrams(60));
        assert_eq!(record.lifecycle(), GeologicalDepositLifecycle::Available);
        let lot = match state.inventory().get_lot(outcome.lot()) {
            Some(lot) => lot,
            None => panic!("extracted material lot disappeared"),
        };
        assert_eq!(lot.mass(), Mass::from_milligrams(40));
        assert_eq!(
            lot.commodity(),
            CommodityKey::new(MATERIAL_COPPER, FORM_ORE)
        );
        assert_eq!(lot.temperature(), Temperature::from_millikelvin(300_000));
        assert_eq!(
            lot.composition(),
            &MaterialComposition::pure(MATERIAL_COPPER)
        );
        let matter = match calculate_matter_accounting(&state) {
            Ok(accounting) => accounting,
            Err(error) => panic!("post-extraction matter accounting failed: {error}"),
        };
        assert_eq!(matter.geological(), AggregateMass::from_milligrams(60));
        assert_eq!(matter.stored(), AggregateMass::from_milligrams(40));
        assert_eq!(matter.total(), initial_matter);
        assert_eq!(
            state
                .structures()
                .get_element(support)
                .map(|record| record.load(StructuralLoadKind::StoredMatter)),
            Some(Force::from_millinewtons(1))
        );
        assert_eq!(total_explicit_energy(&registries, &state), initial_energy);
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }

    #[test]
    fn extraction_capacity_failure_is_atomic() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x6E00_0002));
        let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10))
        {
            Ok(destination) => destination,
            Err(error) => panic!("capacity fixture destination failed: {error}"),
        };
        let deposit = match insert_generated_deposit(&registries, &mut state, deposit_spec(0, 50)) {
            Ok(deposit) => deposit,
            Err(error) => panic!("capacity fixture deposit failed: {error}"),
        };
        let before = state.clone();

        assert!(matches!(
            validate_geological_extraction(
                &registries,
                &state,
                &resolution(deposit, 20),
                destination,
            ),
            Err(GeologicalExtractionError::DestinationCapacityExceeded { .. })
        ));
        assert_eq!(state, before);
    }

    #[test]
    fn stale_owner_revisions_reject_cross_owner_commit_without_partial_mutation() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x6E00_0003));
        let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        {
            Ok(destination) => destination,
            Err(error) => panic!("stale fixture destination failed: {error}"),
        };
        let deposit = match insert_generated_deposit(&registries, &mut state, deposit_spec(0, 50)) {
            Ok(deposit) => deposit,
            Err(error) => panic!("stale fixture deposit failed: {error}"),
        };
        let stale_geology = match validate_geological_extraction(
            &registries,
            &state,
            &resolution(deposit, 10),
            destination,
        ) {
            Ok(token) => token,
            Err(error) => panic!("stale geology validation failed: {error}"),
        };
        if let Err(error) = insert_generated_deposit(&registries, &mut state, deposit_spec(10, 5)) {
            panic!("stale geology independent mutation failed: {error}");
        }
        let before_geology_commit = state.clone();
        assert_eq!(
            stale_geology.commit(&mut state),
            Err(GeologicalExtractionCommitError::StaleGeologyRevision {
                expected: 1,
                actual: 2,
            })
        );
        assert_eq!(state, before_geology_commit);

        let stale_inventory = match validate_geological_extraction(
            &registries,
            &state,
            &resolution(deposit, 10),
            destination,
        ) {
            Ok(token) => token,
            Err(error) => panic!("stale inventory validation failed: {error}"),
        };
        if let Err(error) = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1)) {
            panic!("stale inventory independent mutation failed: {error}");
        }
        let before_inventory_commit = state.clone();
        assert_eq!(
            stale_inventory.commit(&mut state),
            Err(GeologicalExtractionCommitError::StaleInventoryRevision {
                expected: 1,
                actual: 2,
            })
        );
        assert_eq!(state, before_inventory_commit);
    }

    #[test]
    fn partially_extracted_deposit_round_trips_and_continues_deterministically() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x6E00_0004));
        let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        {
            Ok(destination) => destination,
            Err(error) => panic!("round-trip destination failed: {error}"),
        };
        let deposit = match insert_generated_deposit(&registries, &mut state, deposit_spec(0, 100))
        {
            Ok(deposit) => deposit,
            Err(error) => panic!("round-trip deposit failed: {error}"),
        };
        let first = match validate_geological_extraction(
            &registries,
            &state,
            &resolution(deposit, 35),
            destination,
        ) {
            Ok(token) => token,
            Err(error) => panic!("round-trip first extraction validation failed: {error}"),
        };
        if let Err(error) = first.commit(&mut state) {
            panic!("round-trip first extraction commit failed: {error}");
        }

        let encoded = match serde_json::to_vec(&SaveEnvelope::new(&registries, &state)) {
            Ok(encoded) => encoded,
            Err(error) => panic!("geology save serialization failed: {error}"),
        };
        let decoded: LoadedSaveEnvelope = match serde_json::from_slice(&encoded) {
            Ok(decoded) => decoded,
            Err(error) => panic!("geology save deserialization failed: {error}"),
        };
        let mut loaded = match decoded.into_state(&registries) {
            Ok(loaded) => loaded,
            Err(error) => panic!("geology save validation failed: {error}"),
        };
        assert_eq!(loaded, state);

        for candidate in [&mut state, &mut loaded] {
            let second = match validate_geological_extraction(
                &registries,
                candidate,
                &resolution(deposit, 65),
                destination,
            ) {
                Ok(token) => token,
                Err(error) => panic!("round-trip continuation validation failed: {error}"),
            };
            let outcome = match second.commit(candidate) {
                Ok(outcome) => outcome,
                Err(error) => panic!("round-trip continuation commit failed: {error}"),
            };
            assert!(outcome.is_depleted());
        }
        assert_eq!(loaded, state);
        assert_eq!(
            state
                .geology()
                .get_deposit(deposit)
                .map(|record| record.lifecycle()),
            Some(GeologicalDepositLifecycle::Depleted)
        );
    }

    #[cfg(feature = "test-soak")]
    fn run_extraction_soak(seed: WorldSeed) -> AppState {
        let registries = build_registries();
        let mut state = AppState::new(seed);
        let destination =
            match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2_000)) {
                Ok(destination) => destination,
                Err(error) => panic!("extraction soak destination failed: {error}"),
            };
        let deposit =
            match insert_generated_deposit(&registries, &mut state, deposit_spec(0, 2_000)) {
                Ok(deposit) => deposit,
                Err(error) => panic!("extraction soak deposit failed: {error}"),
            };
        let initial_matter = match calculate_matter_accounting(&state) {
            Ok(accounting) => accounting.total(),
            Err(error) => panic!("extraction soak initial matter accounting failed: {error}"),
        };
        let initial_energy = total_explicit_energy(&registries, &state);

        for step in 0_u64..2_000 {
            let token = match validate_geological_extraction(
                &registries,
                &state,
                &resolution(deposit, 1),
                destination,
            ) {
                Ok(token) => token,
                Err(error) => panic!("extraction soak validation failed at step {step}: {error}"),
            };
            if let Err(error) = token.commit(&mut state) {
                panic!("extraction soak commit failed at step {step}: {error}");
            }
            if let Err(error) = advance_tick(&registries, &mut state) {
                panic!("extraction soak tick failed at step {step}: {error}");
            }
            if step.is_multiple_of(97) {
                if let Err(error) = validate_loaded_state(&registries, &state) {
                    panic!("extraction soak exhaustive audit failed at step {step}: {error}");
                }
                let matter = match calculate_matter_accounting(&state) {
                    Ok(accounting) => accounting.total(),
                    Err(error) => {
                        panic!("extraction soak matter accounting failed at step {step}: {error}")
                    }
                };
                assert_eq!(matter, initial_matter);
                assert_eq!(total_explicit_energy(&registries, &state), initial_energy);
            }
        }

        assert_eq!(
            state
                .geology()
                .get_deposit(deposit)
                .map(|record| record.lifecycle()),
            Some(GeologicalDepositLifecycle::Depleted)
        );
        let destination_record = match state.inventory().get_stockpile(destination) {
            Some(record) => record,
            None => panic!("extraction soak destination disappeared"),
        };
        assert_eq!(
            destination_record.stored_mass(),
            Mass::from_milligrams(2_000)
        );
        assert_eq!(destination_record.lot_ids().count(), 1);
        assert_eq!(state.tick().value(), 2_000);
        state
    }

    #[cfg(feature = "test-soak")]
    #[test]
    fn extraction_soak_preserves_determinism_matter_and_modeled_energy() {
        let seed = WorldSeed::new(0x6E00_5000);
        let first = run_extraction_soak(seed);
        let second = run_extraction_soak(seed);

        assert_eq!(first, second);
    }
}

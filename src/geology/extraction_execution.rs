//! Generated-deposit insertion and revision-bound transfer of conserved geological matter into inventory.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::{
    MaterialIngressError, MaterialLotId, StockpileId, ValidatedMaterialIngress,
    apply_material_ingress, validate_material_ingress,
};
use crate::material::{
    CompositionError, FormId, MaterialId, MaterialLotSpec, MaterialLotSpecError,
};
use crate::registry::Registries;

use super::state::{
    GeneratedDepositSpec, GeologicalDepositId, GeologicalDepositLifecycle, GeologicalDepositRecord,
};

/// Failure while admitting a finite world-generated geological deposit into authoritative state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InsertGeneratedDepositError {
    UnknownMaterial { material: MaterialId },
    UnknownForm { form: FormId },
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

impl Error for InsertGeneratedDepositError {}

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
    if registries
        .materials()
        .get_form(spec.commodity().form())
        .is_none()
    {
        return Err(InsertGeneratedDepositError::UnknownForm {
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

    let geology = state.geology();
    let id = GeologicalDepositId::new(geology.next_deposit_id);
    let Some(next_id) = geology.next_deposit_id.checked_add(1) else {
        return Err(InsertGeneratedDepositError::IdExhausted);
    };
    let Some(next_revision) = geology.revision.checked_add(1) else {
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
    geology.next_deposit_id = next_id;
    geology.revision = next_revision;
    let replaced = geology.deposits.insert(id, record);
    debug_assert!(
        replaced.is_none(),
        "geological deposit ID allocation must be unique"
    );
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
        }
    }
}

impl Error for GeologicalExtractionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidOutput(error) => Some(error),
            Self::InvalidDepositComposition { error } => Some(error),
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeologicalExtractionCommitError {
    StaleGeologyRevision { expected: u64, actual: u64 },
    StaleInventoryRevision { expected: u64, actual: u64 },
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
        }
    }
}

impl Error for GeologicalExtractionCommitError {}

/// Successful conserved transfer from one finite deposit into one stockpile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GeologicalExtractionOutcome {
    deposit: GeologicalDepositId,
    destination: StockpileId,
    lot: MaterialLotId,
    mass: Mass,
    depleted: bool,
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
    pub const fn depleted(self) -> bool {
        self.depleted
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
        let actual_inventory_revision = state.inventory_state().revision();
        if actual_inventory_revision != expected_inventory_revision {
            return Err(GeologicalExtractionCommitError::StaleInventoryRevision {
                expected: expected_inventory_revision,
                actual: actual_inventory_revision,
            });
        }

        let lot = apply_material_ingress(state.inventory_state_mut(), self.ingress);
        let geology = state.geology_state_mut();
        let record = match geology.deposits.get_mut(&self.deposit) {
            Some(record) => record,
            None => panic!("validated geological deposit disappeared without revision change"),
        };
        record.remaining_mass = self.remaining_after;
        if self.remaining_after.is_zero() {
            record.lifecycle = GeologicalDepositLifecycle::Depleted;
        }
        geology.revision = self.next_geology_revision;

        Ok(GeologicalExtractionOutcome {
            deposit: self.deposit,
            destination: self.destination,
            lot,
            mass: self.mass,
            depleted: self.remaining_after.is_zero(),
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
        state.inventory_state(),
        destination,
        output,
        state.tick(),
    )
    .map_err(map_material_ingress_error)?;
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
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{FORM_ORE, MATERIAL_COPPER, build_registries};
    use crate::core::quantity::{AggregateMass, Energy, Temperature};
    use crate::core::state::validate_loaded_state;
    use crate::core::time::WorldSeed;
    use crate::energy::calculate_explicit_energy_accounting;
    use crate::inventory::add_stockpile;
    use crate::material::{CommodityKey, CompositionComponent, MaterialComposition, MaterialId};
    use crate::matter::calculate_matter_accounting;
    use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
    use crate::simulation::advance_tick;
    use crate::spatial::{VoxelBounds, VoxelCoord};

    fn bounds(x: i64) -> VoxelBounds {
        match VoxelBounds::new(VoxelCoord::new(x, -12, 0), VoxelCoord::new(x + 4, -8, 4)) {
            Ok(bounds) => bounds,
            Err(error) => panic!("geological extraction bounds fixture failed: {error}"),
        }
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
        let destination = match add_stockpile(&mut state, Mass::from_milligrams(100)) {
            Ok(destination) => destination,
            Err(error) => panic!("geological extraction destination failed: {error}"),
        };
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
        assert!(!outcome.depleted());
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
        assert_eq!(total_explicit_energy(&registries, &state), initial_energy);
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }

    #[test]
    fn extraction_capacity_failure_is_atomic() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x6E00_0002));
        let destination = match add_stockpile(&mut state, Mass::from_milligrams(10)) {
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
        let destination = match add_stockpile(&mut state, Mass::from_milligrams(100)) {
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
        if let Err(error) = add_stockpile(&mut state, Mass::from_milligrams(1)) {
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
        let destination = match add_stockpile(&mut state, Mass::from_milligrams(100)) {
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
            assert!(outcome.depleted());
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

    fn run_extraction_soak(seed: WorldSeed) -> AppState {
        let registries = build_registries();
        let mut state = AppState::new(seed);
        let destination = match add_stockpile(&mut state, Mass::from_milligrams(2_000)) {
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

    #[test]
    fn extraction_soak_preserves_determinism_matter_and_modeled_energy() {
        let seed = WorldSeed::new(0x6E00_5000);
        let first = run_extraction_soak(seed);
        let second = run_extraction_soak(seed);

        assert_eq!(first, second);
    }
}

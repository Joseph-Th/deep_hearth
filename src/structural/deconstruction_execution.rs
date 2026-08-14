//! Conserved deconstruction of structural members back into inventory.
//!
//! The current resolution preserves every embodied material trace exactly. Future physical
//! dismantling/demolition resolvers may replace that identity-preserving result with explicit
//! salvage, debris, and waste streams, but direct structural deletion can never destroy matter.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::{
    MaterialBatchIngressError, MaterialLotId, StockpileId, StockpileStorageError,
    ValidatedMaterialBatchIngress, apply_material_batch_ingress, validate_material_batch_ingress,
};
use crate::registry::Registries;

use super::state::StructuralElementId;
use super::structural_execution::{
    StructuralCommitError, StructuralMutationError, StructuralMutationOutcome,
    ValidatedStructuralMutation, validate_remove_structural_element_with_recovery,
};

/// Opaque result of a future dismantling/demolition authorization system.
///
/// There is no public constructor. At present the canonical transaction returns the member's exact
/// embodied traces to inventory; tool/labor/time and non-identity salvage physics remain separate.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StructuralDeconstructionResolution {
    element: StructuralElementId,
    destination: StockpileId,
}

impl StructuralDeconstructionResolution {
    #[must_use]
    pub const fn element(self) -> StructuralElementId {
        self.element
    }

    #[must_use]
    pub const fn destination(self) -> StockpileId {
        self.destination
    }
}

/// Failure while validating a resolved structural deconstruction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StructuralDeconstructionError {
    UnknownElement {
        element: StructuralElementId,
    },
    NoEmbodiedMatter {
        element: StructuralElementId,
    },
    UnknownDestination {
        stockpile: StockpileId,
    },
    InvalidEmbodiedMatter {
        element: StructuralElementId,
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
    Structure(StructuralMutationError),
}

impl Display for StructuralDeconstructionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownElement { element } => {
                write!(formatter, "unknown structural element {}", element.value())
            }
            Self::NoEmbodiedMatter { element } => write!(
                formatter,
                "structural element {} has no embodied matter to recover",
                element.value()
            ),
            Self::UnknownDestination { stockpile } => write!(
                formatter,
                "structural deconstruction destination stockpile {} does not exist",
                stockpile.value()
            ),
            Self::InvalidEmbodiedMatter { element } => write!(
                formatter,
                "structural element {} contains embodied matter that cannot enter inventory",
                element.value()
            ),
            Self::DestinationStorage(error) => write!(
                formatter,
                "structural recovery destination rejects embodied material: {error}"
            ),
            Self::DestinationMassOverflow { stockpile } => write!(
                formatter,
                "structural recovery overflows stockpile {} mass accounting",
                stockpile.value()
            ),
            Self::DestinationCapacityExceeded {
                stockpile,
                capacity,
                committed,
                requested,
            } => write!(
                formatter,
                "structural recovery exceeds stockpile {} capacity {} mg: {} mg committed, {} mg requested",
                stockpile.value(),
                capacity.milligrams(),
                committed.milligrams(),
                requested.milligrams()
            ),
            Self::LotIdExhausted => {
                formatter.write_str("material lot identifier space is exhausted during recovery")
            }
            Self::InventoryRevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted during recovery")
            }
            Self::Structure(error) => {
                write!(formatter, "structural removal cannot proceed: {error}")
            }
        }
    }
}

impl Error for StructuralDeconstructionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::DestinationStorage(error) => Some(error),
            Self::UnknownElement { .. }
            | Self::NoEmbodiedMatter { .. }
            | Self::UnknownDestination { .. }
            | Self::InvalidEmbodiedMatter { .. }
            | Self::DestinationMassOverflow { .. }
            | Self::DestinationCapacityExceeded { .. }
            | Self::LotIdExhausted
            | Self::InventoryRevisionExhausted => None,
        }
    }
}

fn map_batch_error(
    element: StructuralElementId,
    error: MaterialBatchIngressError,
) -> StructuralDeconstructionError {
    match error {
        MaterialBatchIngressError::EmptyBatch => {
            StructuralDeconstructionError::NoEmbodiedMatter { element }
        }
        MaterialBatchIngressError::UnknownStockpile { stockpile } => {
            StructuralDeconstructionError::UnknownDestination { stockpile }
        }
        MaterialBatchIngressError::MassOverflow { stockpile } => {
            StructuralDeconstructionError::DestinationMassOverflow { stockpile }
        }
        MaterialBatchIngressError::CapacityExceeded {
            stockpile,
            capacity,
            committed,
            requested,
        } => StructuralDeconstructionError::DestinationCapacityExceeded {
            stockpile,
            capacity,
            committed,
            requested,
        },
        MaterialBatchIngressError::LotIdExhausted => StructuralDeconstructionError::LotIdExhausted,
        MaterialBatchIngressError::RevisionExhausted => {
            StructuralDeconstructionError::InventoryRevisionExhausted
        }
        MaterialBatchIngressError::Storage(error) => {
            StructuralDeconstructionError::DestinationStorage(error)
        }
        MaterialBatchIngressError::UnknownMaterial { .. }
        | MaterialBatchIngressError::UnknownForm { .. }
        | MaterialBatchIngressError::UnknownCompositionMaterial { .. }
        | MaterialBatchIngressError::ZeroMass
        | MaterialBatchIngressError::InvalidComposition { .. }
        | MaterialBatchIngressError::CompositionMissingHost { .. }
        | MaterialBatchIngressError::InvalidProvenance
        | MaterialBatchIngressError::ProvenanceInFuture { .. } => {
            StructuralDeconstructionError::InvalidEmbodiedMatter { element }
        }
    }
}

/// A validated cross-owner recovery token became stale before commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StructuralDeconstructionCommitError {
    StaleInventoryRevision { expected: u64, actual: u64 },
    Structure(StructuralCommitError),
}

impl Display for StructuralDeconstructionCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "validated deconstruction expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::Structure(error) => write!(
                formatter,
                "structural deconstruction commit failed: {error}"
            ),
        }
    }
}

impl Error for StructuralDeconstructionCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::StaleInventoryRevision { .. } => None,
        }
    }
}

/// Successful removal plus recovered inventory ownership.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructuralDeconstructionOutcome {
    structural: StructuralMutationOutcome,
    recovered_lots: Vec<MaterialLotId>,
}

impl StructuralDeconstructionOutcome {
    #[must_use]
    pub const fn structural(&self) -> &StructuralMutationOutcome {
        &self.structural
    }

    #[must_use]
    pub fn recovered_lots(&self) -> &[MaterialLotId] {
        &self.recovered_lots
    }
}

/// Consumed proof that removing a member and transferring all its matter is currently valid.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedStructuralDeconstruction {
    removal: ValidatedStructuralMutation,
    ingress: ValidatedMaterialBatchIngress,
}

impl ValidatedStructuralDeconstruction {
    #[must_use]
    pub const fn structural_analysis(&self) -> &crate::structural::StructuralAnalysis {
        self.removal.analysis()
    }

    /// Commits structural consequences first after prechecking inventory. Structural commit does
    /// not mutate inventory, so the validated ingress remains current during this synchronous call.
    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<StructuralDeconstructionOutcome, StructuralDeconstructionCommitError> {
        let actual_inventory_revision = state.inventory_state().revision();
        if actual_inventory_revision != self.ingress.expected_revision() {
            return Err(
                StructuralDeconstructionCommitError::StaleInventoryRevision {
                    expected: self.ingress.expected_revision(),
                    actual: actual_inventory_revision,
                },
            );
        }
        let structural = self
            .removal
            .commit(state)
            .map_err(StructuralDeconstructionCommitError::Structure)?;
        let recovered_lots =
            apply_material_batch_ingress(state.inventory_state_mut(), self.ingress);
        Ok(StructuralDeconstructionOutcome {
            structural,
            recovered_lots,
        })
    }
}

/// Validates identity-preserving recovery of all embodied matter from one structural member.
pub fn validate_structural_deconstruction(
    registries: &Registries,
    state: &AppState,
    resolution: StructuralDeconstructionResolution,
) -> Result<ValidatedStructuralDeconstruction, StructuralDeconstructionError> {
    let element = resolution.element;
    let record = state
        .structures()
        .get_element(element)
        .ok_or(StructuralDeconstructionError::UnknownElement { element })?;
    if record.embodied_mass().is_zero() || record.embodied_material().is_empty() {
        return Err(StructuralDeconstructionError::NoEmbodiedMatter { element });
    }
    let ingress = validate_material_batch_ingress(
        registries,
        state.inventory_state(),
        resolution.destination,
        record.embodied_material(),
        state.tick(),
    )
    .map_err(|error| map_batch_error(element, error))?;
    let removal = validate_remove_structural_element_with_recovery(registries, state, element)
        .map_err(StructuralDeconstructionError::Structure)?;
    debug_assert_eq!(
        removal.expected_revision(),
        state.structures().revision(),
        "structural recovery plan must bind current owner revision"
    );
    Ok(ValidatedStructuralDeconstruction { removal, ingress })
}

#[cfg(test)]
pub(crate) const fn make_test_deconstruction_resolution(
    element: StructuralElementId,
    destination: StockpileId,
) -> StructuralDeconstructionResolution {
    StructuralDeconstructionResolution {
        element,
        destination,
    }
}

#[cfg(test)]
mod tests {
    use super::super::construction_execution::bind_structural_construction_selection;
    use super::*;
    use crate::content::{
        FORM_LOG, MATERIAL_WOOD, STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
    };
    use crate::core::quantity::{Area, Energy, Length, Mass, Temperature};
    use crate::core::state::validate_loaded_state;
    use crate::core::time::WorldSeed;
    use crate::energy::{ExplicitEnergyAccountingError, calculate_explicit_energy_accounting};
    use crate::inventory::{MaterialLotSelection, add_stockpile, deposit_lot_for_test};
    use crate::material::CommodityKey;
    use crate::matter::calculate_matter_accounting;
    use crate::spatial::{VoxelBounds, VoxelCoord};
    use crate::structural::{
        StructuralMutationError, add_structural_element, materialize_structural_element_for_test,
        validate_remove_structural_element, validate_structural_construction,
    };

    fn wood_length_for_mass(mass: Mass) -> Length {
        assert!(!mass.is_zero(), "test member mass must be nonzero");
        let numerator = (u128::from(mass.milligrams()) - 1) * 1_000_000;
        let denominator = 1_000_u128 * 650_u128;
        Length::from_micrometers((numerator / denominator + 1) as u64)
    }

    fn materialized_member(
        registries: &Registries,
        state: &mut AppState,
        mass: Mass,
    ) -> StructuralElementId {
        let bounds = match VoxelBounds::new(VoxelCoord::new(0, 0, 0), VoxelCoord::new(1, 2, 1)) {
            Ok(bounds) => bounds,
            Err(error) => panic!("deconstruction bounds fixture failed: {error}"),
        };
        let element = match add_structural_element(
            registries,
            state,
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            crate::structural::make_test_structural_geometry(
                bounds,
                wood_length_for_mass(mass),
                Area::from_square_millimeters(1_000),
            ),
            true,
        ) {
            Ok(element) => element,
            Err(error) => panic!("deconstruction member fixture failed: {error}"),
        };
        materialize_structural_element_for_test(registries, state, element, FORM_LOG);
        element
    }

    fn explicit_energy(registries: &Registries, state: &AppState) -> Energy {
        match calculate_explicit_energy_accounting(registries, state).and_then(|accounting| {
            accounting
                .total()
                .ok_or(ExplicitEnergyAccountingError::Overflow)
        }) {
            Ok(total) => total,
            Err(error) => panic!("deconstruction explicit energy accounting failed: {error}"),
        }
    }

    #[test]
    fn direct_removal_cannot_destroy_embodied_matter() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5D00_0001));
        let element = materialized_member(&registries, &mut state, Mass::from_milligrams(10));

        assert_eq!(
            validate_remove_structural_element(&registries, &state, element),
            Err(StructuralMutationError::ElementOwnsMatter {
                element,
                mass: Mass::from_milligrams(10),
            })
        );
    }

    #[test]
    fn deconstruction_preserves_matter_profile_and_provenance() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5D00_0002));
        let element = materialized_member(&registries, &mut state, Mass::from_milligrams(10));
        let trace = match state.structures().get_element(element) {
            Some(record) => record.embodied_material()[0].clone(),
            None => panic!("deconstruction member disappeared"),
        };
        let destination = match add_stockpile(&mut state, Mass::from_milligrams(10)) {
            Ok(destination) => destination,
            Err(error) => panic!("deconstruction destination failed: {error}"),
        };
        let initial = match calculate_matter_accounting(&state) {
            Ok(accounting) => accounting.total(),
            Err(error) => panic!("deconstruction initial accounting failed: {error}"),
        };
        let initial_energy = explicit_energy(&registries, &state);
        let token = match validate_structural_deconstruction(
            &registries,
            &state,
            make_test_deconstruction_resolution(element, destination),
        ) {
            Ok(token) => token,
            Err(error) => panic!("deconstruction validation failed: {error}"),
        };
        let outcome = match token.commit(&mut state) {
            Ok(outcome) => outcome,
            Err(error) => panic!("deconstruction commit failed: {error}"),
        };
        assert!(state.structures().get_element(element).is_none());
        assert_eq!(outcome.recovered_lots().len(), 1);
        let lot = match state.inventory().get_lot(outcome.recovered_lots()[0]) {
            Some(lot) => lot,
            None => panic!("recovered material lot disappeared"),
        };
        assert_eq!(lot.mass(), trace.mass());
        assert_eq!(lot.commodity(), trace.profile().commodity());
        assert_eq!(lot.temperature(), trace.profile().temperature());
        assert_eq!(lot.composition(), trace.profile().composition());
        assert_eq!(lot.created_at(), trace.provenance().earliest_created_at());
        assert_eq!(
            lot.latest_created_at(),
            trace.provenance().latest_created_at()
        );
        assert_eq!(
            calculate_matter_accounting(&state).map(|accounting| accounting.total()),
            Ok(initial)
        );
        assert_eq!(explicit_energy(&registries, &state), initial_energy);
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }

    #[test]
    fn deconstruction_restores_multiple_distinct_embodied_traces() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5D00_0005));
        let bounds = match VoxelBounds::new(VoxelCoord::new(0, 0, 0), VoxelCoord::new(1, 2, 1)) {
            Ok(bounds) => bounds,
            Err(error) => panic!("multi-trace deconstruction bounds failed: {error}"),
        };
        let element = match add_structural_element(
            &registries,
            &mut state,
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            crate::structural::make_test_structural_geometry(
                bounds,
                wood_length_for_mass(Mass::from_milligrams(20)),
                Area::from_square_millimeters(1_000),
            ),
            true,
        ) {
            Ok(element) => element,
            Err(error) => panic!("multi-trace structural member failed: {error}"),
        };
        let source = match add_stockpile(&mut state, Mass::from_milligrams(20)) {
            Ok(source) => source,
            Err(error) => panic!("multi-trace construction source failed: {error}"),
        };
        let cold = match deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(8),
            Temperature::from_millikelvin(290_000),
        ) {
            Ok(lot) => lot,
            Err(error) => panic!("cold construction lot failed: {error}"),
        };
        let warm = match deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(12),
            Temperature::from_millikelvin(310_000),
        ) {
            Ok(lot) => lot,
            Err(error) => panic!("warm construction lot failed: {error}"),
        };
        let initial_matter = match calculate_matter_accounting(&state) {
            Ok(accounting) => accounting.total(),
            Err(error) => panic!("multi-trace initial matter accounting failed: {error}"),
        };
        let initial_energy = explicit_energy(&registries, &state);
        let resolution = match bind_structural_construction_selection(
            &state,
            element,
            source,
            &[
                MaterialLotSelection::new(cold, Mass::from_milligrams(8)),
                MaterialLotSelection::new(warm, Mass::from_milligrams(12)),
            ],
        ) {
            Ok(resolution) => resolution,
            Err(error) => panic!("multi-trace construction binding failed: {error:?}"),
        };
        let construction = match validate_structural_construction(&registries, &state, &resolution)
        {
            Ok(token) => token,
            Err(error) => panic!("multi-trace construction validation failed: {error}"),
        };
        if let Err(error) = construction.commit(&mut state) {
            panic!("multi-trace construction commit failed: {error}");
        }
        let record = match state.structures().get_element(element) {
            Some(record) => record,
            None => panic!("multi-trace member disappeared after construction"),
        };
        assert_eq!(record.embodied_material().len(), 2);
        assert_eq!(record.embodied_mass(), Mass::from_milligrams(20));

        let destination = match add_stockpile(&mut state, Mass::from_milligrams(20)) {
            Ok(destination) => destination,
            Err(error) => panic!("multi-trace recovery destination failed: {error}"),
        };
        let deconstruction = match validate_structural_deconstruction(
            &registries,
            &state,
            make_test_deconstruction_resolution(element, destination),
        ) {
            Ok(token) => token,
            Err(error) => panic!("multi-trace deconstruction validation failed: {error}"),
        };
        let outcome = match deconstruction.commit(&mut state) {
            Ok(outcome) => outcome,
            Err(error) => panic!("multi-trace deconstruction commit failed: {error}"),
        };
        assert_eq!(outcome.recovered_lots().len(), 2);
        let mut recovered = outcome
            .recovered_lots()
            .iter()
            .map(|id| match state.inventory().get_lot(*id) {
                Some(lot) => (lot.mass(), lot.temperature()),
                None => panic!("multi-trace recovered lot disappeared"),
            })
            .collect::<Vec<_>>();
        recovered.sort_by_key(|(_, temperature)| temperature.millikelvin());
        assert_eq!(
            recovered,
            vec![
                (
                    Mass::from_milligrams(8),
                    Temperature::from_millikelvin(290_000)
                ),
                (
                    Mass::from_milligrams(12),
                    Temperature::from_millikelvin(310_000)
                ),
            ]
        );
        assert_eq!(
            calculate_matter_accounting(&state).map(|accounting| accounting.total()),
            Ok(initial_matter)
        );
        assert_eq!(explicit_energy(&registries, &state), initial_energy);
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }

    #[test]
    fn deconstruction_capacity_failure_is_atomic() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5D00_0003));
        let element = materialized_member(&registries, &mut state, Mass::from_milligrams(10));
        let destination = match add_stockpile(&mut state, Mass::from_milligrams(5)) {
            Ok(destination) => destination,
            Err(error) => panic!("deconstruction capacity destination failed: {error}"),
        };
        let before = state.clone();
        assert!(matches!(
            validate_structural_deconstruction(
                &registries,
                &state,
                make_test_deconstruction_resolution(element, destination),
            ),
            Err(StructuralDeconstructionError::DestinationCapacityExceeded { .. })
        ));
        assert_eq!(state, before);
    }

    #[test]
    fn deconstruction_rechecks_inventory_and_structure_before_any_cross_owner_commit() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5D00_0004));
        let element = materialized_member(&registries, &mut state, Mass::from_milligrams(10));
        let destination = match add_stockpile(&mut state, Mass::from_milligrams(20)) {
            Ok(destination) => destination,
            Err(error) => panic!("stale deconstruction destination failed: {error}"),
        };

        let stale_inventory = match validate_structural_deconstruction(
            &registries,
            &state,
            make_test_deconstruction_resolution(element, destination),
        ) {
            Ok(token) => token,
            Err(error) => panic!("stale inventory deconstruction validation failed: {error}"),
        };
        if let Err(error) = add_stockpile(&mut state, Mass::from_milligrams(1)) {
            panic!("stale deconstruction inventory mutation failed: {error}");
        }
        let before_inventory_commit = state.clone();
        assert!(matches!(
            stale_inventory.commit(&mut state),
            Err(StructuralDeconstructionCommitError::StaleInventoryRevision { .. })
        ));
        assert_eq!(state, before_inventory_commit);
        assert!(state.structures().get_element(element).is_some());

        let stale_structure = match validate_structural_deconstruction(
            &registries,
            &state,
            make_test_deconstruction_resolution(element, destination),
        ) {
            Ok(token) => token,
            Err(error) => panic!("stale structure deconstruction validation failed: {error}"),
        };
        let bounds = match VoxelBounds::new(VoxelCoord::new(4, 0, 0), VoxelCoord::new(5, 2, 1)) {
            Ok(bounds) => bounds,
            Err(error) => panic!("stale deconstruction bounds failed: {error}"),
        };
        if let Err(error) = add_structural_element(
            &registries,
            &mut state,
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            crate::structural::make_test_structural_geometry(
                bounds,
                crate::core::quantity::Length::from_micrometers(1),
                Area::from_square_millimeters(1_000),
            ),
            true,
        ) {
            panic!("stale deconstruction structural mutation failed: {error}");
        }
        let before_structure_commit = state.clone();
        assert!(matches!(
            stale_structure.commit(&mut state),
            Err(StructuralDeconstructionCommitError::Structure(
                StructuralCommitError::StaleRevision { .. }
            ))
        ));
        assert_eq!(state, before_structure_commit);
        assert!(state.structures().get_element(element).is_some());
        assert_eq!(
            state
                .inventory()
                .get_stockpile(destination)
                .map(|stockpile| stockpile.stored_mass()),
            Some(Mass::ZERO)
        );
    }
}

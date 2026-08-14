//! Geometry-constrained construction-material transfer into planned structural members.
//!
//! Member geometry and material density now determine the exact conservative solid-mass requirement.
//! Labor, tools, joints, cutting/placement waste, and build duration remain future physical resolver
//! responsibilities, so arbitrary runtime construction authorization is still intentionally absent.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{AggregateMass, Force, Mass, Volume};
use crate::core::state::AppState;
use crate::inventory::{
    ConsumedMaterialTrace, MaterialEgressError, StockpileId, ValidatedMaterialEgress,
    apply_material_egress, validate_material_egress_from_selection,
};
#[cfg(test)]
use crate::inventory::{
    ExplicitConsumptionSelectionError, MaterialLotSelection,
    validate_explicit_consumption_selection,
};
use crate::material::{FormId, MaterialComposition, MaterialId, MaterialPhase};
use crate::registry::Registries;

use super::geometry::{
    StructuralGeometryError, calculate_prismatic_material_mass_ceiling,
    calculate_prismatic_volume_ceiling,
};
use super::load::calculate_aggregate_weight_force_ceiling;
use super::state::{StructuralElementId, StructuralLifecycle, StructuralLoadKind};

/// Read-only physical material requirement for one prismatic structural member.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StructuralMaterialRequirement {
    element: StructuralElementId,
    material: MaterialId,
    solid_volume_ceiling: Volume,
    required_mass: Mass,
}

impl StructuralMaterialRequirement {
    #[must_use]
    pub const fn element(self) -> StructuralElementId {
        self.element
    }

    #[must_use]
    pub const fn material(self) -> MaterialId {
        self.material
    }

    #[must_use]
    pub const fn solid_volume_ceiling(self) -> Volume {
        self.solid_volume_ceiling
    }

    #[must_use]
    pub const fn required_mass(self) -> Mass {
        self.required_mass
    }
}

/// Failure while deriving a member's physical solid-material requirement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructuralMaterialRequirementError {
    UnknownElement {
        element: StructuralElementId,
    },
    Geometry {
        element: StructuralElementId,
        error: StructuralGeometryError,
    },
}

impl Display for StructuralMaterialRequirementError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownElement { element } => {
                write!(formatter, "unknown structural element {}", element.value())
            }
            Self::Geometry { element, error } => write!(
                formatter,
                "structural element {} material requirement cannot be resolved: {error}",
                element.value()
            ),
        }
    }
}

impl Error for StructuralMaterialRequirementError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Geometry { error, .. } => Some(error),
            Self::UnknownElement { .. } => None,
        }
    }
}

/// Derives conservative solid volume and exact milligram ownership from member geometry and density.
pub fn resolve_structural_material_requirement(
    registries: &Registries,
    state: &AppState,
    element: StructuralElementId,
) -> Result<StructuralMaterialRequirement, StructuralMaterialRequirementError> {
    let record = state
        .structures()
        .get_element(element)
        .ok_or(StructuralMaterialRequirementError::UnknownElement { element })?;
    let solid_volume_ceiling =
        calculate_prismatic_volume_ceiling(record.cross_section(), record.length())
            .map_err(|error| StructuralMaterialRequirementError::Geometry { element, error })?;
    let required_mass = calculate_prismatic_material_mass_ceiling(
        registries.materials(),
        record.material(),
        record.cross_section(),
        record.length(),
    )
    .map_err(|error| StructuralMaterialRequirementError::Geometry { element, error })?;
    Ok(StructuralMaterialRequirement {
        element,
        material: record.material(),
        solid_volume_ceiling,
        required_mass,
    })
}

/// Immutable output of a future physical construction resolver.
///
/// There is no public constructor. A resolver must decide the required batch from actual member
/// geometry, joinery, wastage, tooling, and construction method before this transfer can occur.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructuralConstructionResolution {
    element: StructuralElementId,
    selection: crate::inventory::ConsumptionSelection,
}

impl StructuralConstructionResolution {
    #[must_use]
    pub const fn element(&self) -> StructuralElementId {
        self.element
    }

    #[must_use]
    pub const fn source(&self) -> StockpileId {
        self.selection.source()
    }

    #[must_use]
    pub const fn mass(&self) -> Mass {
        self.selection.total_consumed()
    }

    #[must_use]
    pub fn material_traces(&self) -> &[ConsumedMaterialTrace] {
        self.selection.consumed_inputs()
    }
}

/// Test-side binding failure standing in for a future physical construction resolver.
#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StructuralConstructionBindingError {
    Inventory(ExplicitConsumptionSelectionError),
}

#[cfg(test)]
pub(crate) fn bind_structural_construction_selection(
    state: &AppState,
    element: StructuralElementId,
    source: StockpileId,
    selections: &[MaterialLotSelection],
) -> Result<StructuralConstructionResolution, StructuralConstructionBindingError> {
    let selection =
        validate_explicit_consumption_selection(state.inventory_state(), source, selections)
            .map_err(StructuralConstructionBindingError::Inventory)?;
    Ok(StructuralConstructionResolution { element, selection })
}

/// Failure while validating an already-resolved construction batch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StructuralConstructionError {
    UnknownElement {
        element: StructuralElementId,
    },
    ElementNotPlanned {
        element: StructuralElementId,
        lifecycle: StructuralLifecycle,
    },
    AlreadyMaterialized {
        element: StructuralElementId,
    },
    MaterialMismatch {
        element: StructuralElementId,
        expected: MaterialId,
        found: MaterialId,
    },
    UnsupportedComposition {
        element: StructuralElementId,
        material: MaterialId,
    },
    UnknownMaterialForm {
        element: StructuralElementId,
        form: FormId,
    },
    UnsupportedPhase {
        element: StructuralElementId,
        form: FormId,
        phase: MaterialPhase,
    },
    Geometry {
        element: StructuralElementId,
        error: StructuralGeometryError,
    },
    MaterialQuantityMismatch {
        element: StructuralElementId,
        required: Mass,
        selected: Mass,
    },
    InventorySelectionStale {
        expected: u64,
        actual: u64,
    },
    InventoryRevisionExhausted,
    StructureRevisionExhausted,
    SelfWeightOverflow {
        element: StructuralElementId,
    },
}

impl Display for StructuralConstructionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownElement { element } => {
                write!(formatter, "unknown structural element {}", element.value())
            }
            Self::ElementNotPlanned { element, lifecycle } => write!(
                formatter,
                "structural element {} is {lifecycle:?} and cannot receive construction matter",
                element.value()
            ),
            Self::UnknownMaterialForm { element, form } => write!(
                formatter,
                "structural element {} construction batch references unknown material form {}",
                element.value(),
                form.value()
            ),
            Self::Geometry { element, error } => write!(
                formatter,
                "structural element {} construction geometry is invalid: {error}",
                element.value()
            ),
            Self::MaterialQuantityMismatch {
                element,
                required,
                selected,
            } => write!(
                formatter,
                "structural element {} requires {} mg from geometry and density but construction selected {} mg",
                element.value(),
                required.milligrams(),
                selected.milligrams()
            ),
            Self::AlreadyMaterialized { element } => write!(
                formatter,
                "structural element {} already owns construction matter",
                element.value()
            ),
            Self::MaterialMismatch {
                element,
                expected,
                found,
            } => write!(
                formatter,
                "structural element {} requires material {} but construction batch contains material {}",
                element.value(),
                expected.value(),
                found.value()
            ),
            Self::UnsupportedComposition { element, material } => write!(
                formatter,
                "structural element {} currently requires pure material {} because mixed-composition strength is not yet modeled",
                element.value(),
                material.value()
            ),
            Self::UnsupportedPhase {
                element,
                form,
                phase,
            } => write!(
                formatter,
                "structural element {} cannot embody {phase:?} material form {}; construction requires solid matter",
                element.value(),
                form.value()
            ),
            Self::InventorySelectionStale { expected, actual } => write!(
                formatter,
                "construction selection expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::InventoryRevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted during construction")
            }
            Self::StructureRevisionExhausted => {
                formatter.write_str("structural revision space is exhausted during construction")
            }
            Self::SelfWeightOverflow { element } => write!(
                formatter,
                "structural element {} construction mass exceeds self-weight force range",
                element.value()
            ),
        }
    }
}

impl Error for StructuralConstructionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Geometry { error, .. } => Some(error),
            _ => None,
        }
    }
}

/// A validated construction transfer can no longer commit because an owning subsystem changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructuralConstructionCommitError {
    StaleStructureRevision { expected: u64, actual: u64 },
    StaleInventoryRevision { expected: u64, actual: u64 },
    StateChanged { element: StructuralElementId },
}

impl Display for StructuralConstructionCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleStructureRevision { expected, actual } => write!(
                formatter,
                "validated construction expected structural revision {expected} but current revision is {actual}"
            ),
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "validated construction expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::StateChanged { element } => write!(
                formatter,
                "structural element {} changed before construction commit",
                element.value()
            ),
        }
    }
}

impl Error for StructuralConstructionCommitError {}

/// Consumed proof that exact inventory matter can become one member's embodied matter atomically.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedStructuralConstruction {
    element: StructuralElementId,
    expected_structure_revision: u64,
    next_structure_revision: u64,
    material: Vec<ConsumedMaterialTrace>,
    mass: Mass,
    self_weight: Force,
    egress: ValidatedMaterialEgress,
}

impl ValidatedStructuralConstruction {
    #[must_use]
    pub const fn mass(&self) -> Mass {
        self.mass
    }

    #[must_use]
    pub const fn self_weight(&self) -> Force {
        self.self_weight
    }

    /// Commits both owners only after rechecking both revisions and the target lifecycle.
    pub fn commit(self, state: &mut AppState) -> Result<(), StructuralConstructionCommitError> {
        let actual_structure_revision = state.structures().revision();
        if actual_structure_revision != self.expected_structure_revision {
            return Err(StructuralConstructionCommitError::StaleStructureRevision {
                expected: self.expected_structure_revision,
                actual: actual_structure_revision,
            });
        }
        let actual_inventory_revision = state.inventory_state().revision();
        if actual_inventory_revision != self.egress.expected_revision() {
            return Err(StructuralConstructionCommitError::StaleInventoryRevision {
                expected: self.egress.expected_revision(),
                actual: actual_inventory_revision,
            });
        }
        let Some(record) = state.structures().get_element(self.element) else {
            return Err(StructuralConstructionCommitError::StateChanged {
                element: self.element,
            });
        };
        if record.lifecycle() != StructuralLifecycle::Planned || !record.embodied_mass().is_zero() {
            return Err(StructuralConstructionCommitError::StateChanged {
                element: self.element,
            });
        }

        apply_material_egress(state.inventory_state_mut(), self.egress);
        let structures = state.structure_state_mut();
        let record = match structures.elements.get_mut(&self.element) {
            Some(record) => record,
            None => panic!("prechecked structural construction target disappeared"),
        };
        record.embodied_mass = self.mass;
        record.embodied_material = self.material;
        if self.self_weight.is_zero() {
            record.loads.remove(&StructuralLoadKind::SelfWeight);
        } else {
            record
                .loads
                .insert(StructuralLoadKind::SelfWeight, self.self_weight);
        }
        structures.revision = self.next_structure_revision;
        Ok(())
    }
}

/// Validates a physically resolved material batch for one still-planned member.
pub fn validate_structural_construction(
    registries: &Registries,
    state: &AppState,
    resolution: &StructuralConstructionResolution,
) -> Result<ValidatedStructuralConstruction, StructuralConstructionError> {
    let element = resolution.element;
    let record = state
        .structures()
        .get_element(element)
        .ok_or(StructuralConstructionError::UnknownElement { element })?;
    if record.lifecycle() != StructuralLifecycle::Planned {
        return Err(StructuralConstructionError::ElementNotPlanned {
            element,
            lifecycle: record.lifecycle(),
        });
    }
    if !record.embodied_mass().is_zero() || !record.embodied_material().is_empty() {
        return Err(StructuralConstructionError::AlreadyMaterialized { element });
    }
    for trace in resolution.selection.consumed_inputs() {
        let form_id = trace.profile().commodity().form();
        let Some(form) = registries.materials().get_form(form_id) else {
            return Err(StructuralConstructionError::UnknownMaterialForm {
                element,
                form: form_id,
            });
        };
        if form.phase() != MaterialPhase::Solid {
            return Err(StructuralConstructionError::UnsupportedPhase {
                element,
                form: form_id,
                phase: form.phase(),
            });
        }
        let found = trace.profile().commodity().material();
        if found != record.material() {
            return Err(StructuralConstructionError::MaterialMismatch {
                element,
                expected: record.material(),
                found,
            });
        }
        if trace.profile().composition() != &MaterialComposition::pure(record.material()) {
            return Err(StructuralConstructionError::UnsupportedComposition {
                element,
                material: record.material(),
            });
        }
    }

    let required_mass = calculate_prismatic_material_mass_ceiling(
        registries.materials(),
        record.material(),
        record.cross_section(),
        record.length(),
    )
    .map_err(|error| StructuralConstructionError::Geometry { element, error })?;
    if resolution.mass() != required_mass {
        return Err(StructuralConstructionError::MaterialQuantityMismatch {
            element,
            required: required_mass,
            selected: resolution.mass(),
        });
    }

    let egress = validate_material_egress_from_selection(
        state.inventory_state(),
        resolution.selection.clone(),
    )
    .map_err(|error| match error {
        MaterialEgressError::StaleSelection { expected, actual } => {
            StructuralConstructionError::InventorySelectionStale { expected, actual }
        }
        MaterialEgressError::RevisionExhausted => {
            StructuralConstructionError::InventoryRevisionExhausted
        }
    })?;
    let expected_structure_revision = state.structures().revision();
    let next_structure_revision = expected_structure_revision
        .checked_add(1)
        .ok_or(StructuralConstructionError::StructureRevisionExhausted)?;
    debug_assert_eq!(egress.total_consumed(), required_mass);
    let self_weight = calculate_aggregate_weight_force_ceiling(
        AggregateMass::from_mass(required_mass),
        registries.core().gravity(),
    )
    .ok_or(StructuralConstructionError::SelfWeightOverflow { element })?;
    Ok(ValidatedStructuralConstruction {
        element,
        expected_structure_revision,
        next_structure_revision,
        material: egress.consumed_inputs().to_vec(),
        mass: egress.total_consumed(),
        self_weight,
        egress,
    })
}

#[cfg(test)]
pub(crate) fn materialize_structural_element_for_test(
    registries: &Registries,
    state: &mut AppState,
    element: StructuralElementId,
    form: crate::material::FormId,
) {
    use crate::core::quantity::Temperature;
    use crate::inventory::{add_stockpile, deposit_lot_for_test};
    use crate::material::CommodityKey;

    let requirement = match resolve_structural_material_requirement(registries, state, element) {
        Ok(requirement) => requirement,
        Err(error) => panic!("construction test material requirement failed: {error}"),
    };
    let material = requirement.material();
    let mass = requirement.required_mass();
    let source = match add_stockpile(state, mass) {
        Ok(source) => source,
        Err(error) => panic!("construction test stockpile failed: {error}"),
    };
    let lot = match deposit_lot_for_test(
        registries,
        state,
        source,
        CommodityKey::new(material, form),
        mass,
        Temperature::from_millikelvin(293_150),
    ) {
        Ok(lot) => lot,
        Err(error) => panic!("construction test material deposit failed: {error}"),
    };
    let resolution = match bind_structural_construction_selection(
        state,
        element,
        source,
        &[MaterialLotSelection::new(lot, mass)],
    ) {
        Ok(resolution) => resolution,
        Err(error) => panic!("construction test material binding failed: {error:?}"),
    };
    let token = match validate_structural_construction(registries, state, &resolution) {
        Ok(token) => token,
        Err(error) => panic!("construction test validation failed: {error}"),
    };
    if let Err(error) = token.commit(state) {
        panic!("construction test commit failed: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{
        FORM_LOG, FORM_MOLTEN, MATERIAL_CHARCOAL, MATERIAL_COPPER, MATERIAL_WOOD,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
    };
    use crate::core::quantity::{Area, Energy, Length};
    use crate::core::state::validate_loaded_state;
    use crate::core::time::WorldSeed;
    use crate::energy::{ExplicitEnergyAccountingError, calculate_explicit_energy_accounting};
    use crate::inventory::{
        StockpileStorageProfile, add_stockpile, add_stockpile_with_storage_profile,
        deposit_composed_lot_for_test, deposit_lot_for_test,
    };
    use crate::material::{CommodityKey, CompositionComponent, MaterialComposition};
    use crate::matter::calculate_matter_accounting;
    use crate::simulation::advance_tick;
    use crate::spatial::{VoxelBounds, VoxelCoord};
    use crate::structural::{
        add_structural_element, make_test_deconstruction_resolution,
        validate_activate_structural_element, validate_structural_deconstruction,
    };

    fn wood_length_for_mass(mass: Mass) -> Length {
        assert!(!mass.is_zero(), "test member mass must be nonzero");
        let numerator = (u128::from(mass.milligrams()) - 1) * 1_000_000;
        let denominator = 1_000_u128 * 650_u128;
        let micrometers = numerator / denominator + 1;
        Length::from_micrometers(micrometers as u64)
    }

    #[test]
    fn liquid_material_cannot_become_structural_embodiment() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5C00_0012));
        let bounds = match VoxelBounds::new(VoxelCoord::new(0, 0, 0), VoxelCoord::new(1, 1, 1)) {
            Ok(bounds) => bounds,
            Err(error) => panic!("liquid construction bounds failed: {error}"),
        };
        let element = match add_structural_element(
            &registries,
            &mut state,
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
            Err(error) => panic!("liquid construction member failed: {error}"),
        };
        let requirement =
            match resolve_structural_material_requirement(&registries, &state, element) {
                Ok(requirement) => requirement,
                Err(error) => panic!("liquid construction requirement failed: {error}"),
            };
        let vessel_profile = match StockpileStorageProfile::new(
            false,
            true,
            crate::core::quantity::Temperature::from_millikelvin(1_500_000),
        ) {
            Ok(profile) => profile,
            Err(error) => panic!("liquid construction vessel profile failed: {error}"),
        };
        let source = match add_stockpile_with_storage_profile(
            &mut state,
            requirement.required_mass(),
            vessel_profile,
        ) {
            Ok(source) => source,
            Err(error) => panic!("liquid construction source failed: {error}"),
        };
        let lot = match deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN),
            requirement.required_mass(),
            crate::core::quantity::Temperature::from_millikelvin(1_357_770),
        ) {
            Ok(lot) => lot,
            Err(error) => panic!("liquid construction lot failed: {error}"),
        };
        let resolution = match bind_structural_construction_selection(
            &state,
            element,
            source,
            &[MaterialLotSelection::new(lot, requirement.required_mass())],
        ) {
            Ok(resolution) => resolution,
            Err(error) => panic!("liquid construction binding failed: {error:?}"),
        };
        let before = state.clone();

        assert_eq!(
            validate_structural_construction(&registries, &state, &resolution),
            Err(StructuralConstructionError::UnsupportedPhase {
                element,
                form: FORM_MOLTEN,
                phase: MaterialPhase::Liquid,
            })
        );
        assert_eq!(state, before);
    }

    fn member(
        registries: &Registries,
        state: &mut AppState,
        required_mass: Mass,
    ) -> StructuralElementId {
        let bounds = match VoxelBounds::new(VoxelCoord::new(0, 0, 0), VoxelCoord::new(1, 2, 1)) {
            Ok(bounds) => bounds,
            Err(error) => panic!("construction bounds fixture failed: {error}"),
        };
        match add_structural_element(
            registries,
            state,
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            crate::structural::make_test_structural_geometry(
                bounds,
                wood_length_for_mass(required_mass),
                Area::from_square_millimeters(1_000),
            ),
            true,
        ) {
            Ok(element) => element,
            Err(error) => panic!("construction member fixture failed: {error}"),
        }
    }

    fn explicit_energy(registries: &Registries, state: &AppState) -> Energy {
        match calculate_explicit_energy_accounting(registries, state).and_then(|accounting| {
            accounting
                .total()
                .ok_or(ExplicitEnergyAccountingError::Overflow)
        }) {
            Ok(total) => total,
            Err(error) => panic!("construction explicit energy accounting failed: {error}"),
        }
    }

    #[test]
    fn material_requirement_uses_member_geometry_and_authored_density() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5C00_0010));
        let bounds = match VoxelBounds::new(VoxelCoord::new(0, 0, 0), VoxelCoord::new(1, 1, 1)) {
            Ok(bounds) => bounds,
            Err(error) => panic!("material-requirement bounds failed: {error}"),
        };
        let element = match add_structural_element(
            &registries,
            &mut state,
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            crate::structural::make_test_structural_geometry(
                bounds,
                Length::from_micrometers(10_000),
                Area::from_square_millimeters(1_000),
            ),
            true,
        ) {
            Ok(element) => element,
            Err(error) => panic!("material-requirement member failed: {error}"),
        };
        let requirement =
            match resolve_structural_material_requirement(&registries, &state, element) {
                Ok(requirement) => requirement,
                Err(error) => panic!("material requirement failed: {error}"),
            };

        assert_eq!(requirement.element(), element);
        assert_eq!(requirement.material(), MATERIAL_WOOD);
        assert_eq!(
            requirement.solid_volume_ceiling(),
            Volume::from_microliters(10_000)
        );
        assert_eq!(requirement.required_mass(), Mass::from_milligrams(6_500));
    }

    #[test]
    fn construction_rejects_under_and_over_materialization_without_mutation() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5C00_0011));
        let element = member(&registries, &mut state, Mass::from_milligrams(10));
        let source = match add_stockpile(&mut state, Mass::from_milligrams(20)) {
            Ok(source) => source,
            Err(error) => panic!("quantity-mismatch source failed: {error}"),
        };
        let lot = match deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(20),
            crate::core::quantity::Temperature::from_millikelvin(300_000),
        ) {
            Ok(lot) => lot,
            Err(error) => panic!("quantity-mismatch lot failed: {error}"),
        };
        let before = state.clone();

        for selected in [9_u64, 11_u64] {
            let resolution = match bind_structural_construction_selection(
                &state,
                element,
                source,
                &[MaterialLotSelection::new(
                    lot,
                    Mass::from_milligrams(selected),
                )],
            ) {
                Ok(resolution) => resolution,
                Err(error) => panic!("quantity-mismatch binding failed: {error:?}"),
            };
            assert_eq!(
                validate_structural_construction(&registries, &state, &resolution),
                Err(StructuralConstructionError::MaterialQuantityMismatch {
                    element,
                    required: Mass::from_milligrams(10),
                    selected: Mass::from_milligrams(selected),
                })
            );
            assert_eq!(state, before);
        }
    }

    #[test]
    fn activation_requires_conserved_construction_matter() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5C00_0001));
        let element = member(&registries, &mut state, Mass::from_milligrams(1));
        assert_eq!(
            validate_activate_structural_element(&registries, &state, element),
            Err(super::super::StructuralMutationError::ActivationUnmaterialized { element })
        );
    }

    #[test]
    fn construction_moves_exact_matter_and_derives_self_weight() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5C00_0002));
        let element = member(&registries, &mut state, Mass::from_milligrams(2_000_000));
        let source = match add_stockpile(&mut state, Mass::from_milligrams(2_000_000)) {
            Ok(source) => source,
            Err(error) => panic!("construction source failed: {error}"),
        };
        let lot = match deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(2_000_000),
            crate::core::quantity::Temperature::from_millikelvin(300_000),
        ) {
            Ok(lot) => lot,
            Err(error) => panic!("construction material failed: {error}"),
        };
        let initial = match calculate_matter_accounting(&state) {
            Ok(accounting) => accounting.total(),
            Err(error) => panic!("construction initial matter accounting failed: {error}"),
        };
        let initial_energy = explicit_energy(&registries, &state);
        let resolution = match bind_structural_construction_selection(
            &state,
            element,
            source,
            &[MaterialLotSelection::new(
                lot,
                Mass::from_milligrams(2_000_000),
            )],
        ) {
            Ok(resolution) => resolution,
            Err(error) => panic!("construction binding failed: {error:?}"),
        };
        let token = match validate_structural_construction(&registries, &state, &resolution) {
            Ok(token) => token,
            Err(error) => panic!("construction validation failed: {error}"),
        };
        let expected_weight = token.self_weight();
        if let Err(error) = token.commit(&mut state) {
            panic!("construction commit failed: {error}");
        }
        let record = match state.structures().get_element(element) {
            Some(record) => record,
            None => panic!("constructed member disappeared"),
        };
        assert_eq!(record.embodied_mass(), Mass::from_milligrams(2_000_000));
        assert_eq!(record.embodied_material().len(), 1);
        assert_eq!(record.load(StructuralLoadKind::SelfWeight), expected_weight);
        assert_eq!(
            state
                .inventory()
                .get_stockpile(source)
                .map(|stockpile| stockpile.stored_mass()),
            Some(Mass::ZERO)
        );
        let final_total = match calculate_matter_accounting(&state) {
            Ok(accounting) => accounting.total(),
            Err(error) => panic!("construction final matter accounting failed: {error}"),
        };
        assert_eq!(final_total, initial);
        assert_eq!(explicit_energy(&registries, &state), initial_energy);
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }

    #[test]
    fn wrong_material_cannot_become_structural_strength_material() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5C00_0003));
        let element = member(&registries, &mut state, Mass::from_milligrams(100));
        let source = match add_stockpile(&mut state, Mass::from_milligrams(100)) {
            Ok(source) => source,
            Err(error) => panic!("wrong-material source failed: {error}"),
        };
        let lot = match deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(MATERIAL_COPPER, FORM_LOG),
            Mass::from_milligrams(100),
            crate::core::quantity::Temperature::from_millikelvin(300_000),
        ) {
            Ok(lot) => lot,
            Err(error) => panic!("wrong-material fixture failed: {error}"),
        };
        let resolution = match bind_structural_construction_selection(
            &state,
            element,
            source,
            &[MaterialLotSelection::new(lot, Mass::from_milligrams(100))],
        ) {
            Ok(resolution) => resolution,
            Err(error) => panic!("wrong-material binding failed: {error:?}"),
        };
        let before = state.clone();
        assert_eq!(
            validate_structural_construction(&registries, &state, &resolution),
            Err(StructuralConstructionError::MaterialMismatch {
                element,
                expected: MATERIAL_WOOD,
                found: MATERIAL_COPPER,
            })
        );
        assert_eq!(state, before);
    }

    #[test]
    fn mixed_composition_cannot_claim_pure_material_structural_strength() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5C00_0004));
        let element = member(&registries, &mut state, Mass::from_milligrams(100));
        let source = match add_stockpile(&mut state, Mass::from_milligrams(100)) {
            Ok(source) => source,
            Err(error) => panic!("mixed construction source failed: {error}"),
        };
        let composition = match MaterialComposition::new(vec![
            CompositionComponent::new(MATERIAL_WOOD, 900_000),
            CompositionComponent::new(MATERIAL_CHARCOAL, 100_000),
        ]) {
            Ok(composition) => composition,
            Err(error) => panic!("mixed construction composition failed: {error}"),
        };
        let lot = match deposit_composed_lot_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(100),
            crate::core::quantity::Temperature::from_millikelvin(300_000),
            composition,
        ) {
            Ok(lot) => lot,
            Err(error) => panic!("mixed construction lot failed: {error}"),
        };
        let resolution = match bind_structural_construction_selection(
            &state,
            element,
            source,
            &[MaterialLotSelection::new(lot, Mass::from_milligrams(100))],
        ) {
            Ok(resolution) => resolution,
            Err(error) => panic!("mixed construction binding failed: {error:?}"),
        };
        let before = state.clone();
        assert_eq!(
            validate_structural_construction(&registries, &state, &resolution),
            Err(StructuralConstructionError::UnsupportedComposition {
                element,
                material: MATERIAL_WOOD,
            })
        );
        assert_eq!(state, before);
    }

    #[test]
    fn construction_rechecks_both_owner_revisions_before_consuming_matter() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5C00_0005));
        let element = member(&registries, &mut state, Mass::from_milligrams(10));
        let source = match add_stockpile(&mut state, Mass::from_milligrams(20)) {
            Ok(source) => source,
            Err(error) => panic!("stale construction source failed: {error}"),
        };
        let lot = match deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(20),
            crate::core::quantity::Temperature::from_millikelvin(300_000),
        ) {
            Ok(lot) => lot,
            Err(error) => panic!("stale construction material failed: {error}"),
        };
        let selection = [MaterialLotSelection::new(lot, Mass::from_milligrams(10))];

        let inventory_resolution =
            match bind_structural_construction_selection(&state, element, source, &selection) {
                Ok(resolution) => resolution,
                Err(error) => panic!("stale inventory construction binding failed: {error:?}"),
            };
        let stale_inventory =
            match validate_structural_construction(&registries, &state, &inventory_resolution) {
                Ok(token) => token,
                Err(error) => panic!("stale inventory construction validation failed: {error}"),
            };
        if let Err(error) = add_stockpile(&mut state, Mass::from_milligrams(1)) {
            panic!("stale inventory independent mutation failed: {error}");
        }
        let before_inventory_commit = state.clone();
        assert!(matches!(
            stale_inventory.commit(&mut state),
            Err(StructuralConstructionCommitError::StaleInventoryRevision { .. })
        ));
        assert_eq!(state, before_inventory_commit);

        let structure_resolution =
            match bind_structural_construction_selection(&state, element, source, &selection) {
                Ok(resolution) => resolution,
                Err(error) => panic!("stale structure construction binding failed: {error:?}"),
            };
        let stale_structure =
            match validate_structural_construction(&registries, &state, &structure_resolution) {
                Ok(token) => token,
                Err(error) => panic!("stale structure construction validation failed: {error}"),
            };
        member(&registries, &mut state, Mass::from_milligrams(1));
        let before_structure_commit = state.clone();
        assert!(matches!(
            stale_structure.commit(&mut state),
            Err(StructuralConstructionCommitError::StaleStructureRevision { .. })
        ));
        assert_eq!(state, before_structure_commit);
        assert_eq!(
            state
                .inventory()
                .get_stockpile(source)
                .map(|stockpile| stockpile.stored_mass()),
            Some(Mass::from_milligrams(20))
        );
    }

    fn run_construction_ownership_soak(seed: WorldSeed) -> AppState {
        let registries = build_registries();
        let mut state = AppState::new(seed);
        let mut source = match add_stockpile(&mut state, Mass::from_milligrams(10)) {
            Ok(stockpile) => stockpile,
            Err(error) => panic!("construction soak source failed: {error}"),
        };
        let mut destination = match add_stockpile(&mut state, Mass::from_milligrams(10)) {
            Ok(stockpile) => stockpile,
            Err(error) => panic!("construction soak destination failed: {error}"),
        };
        let mut lot = match deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(10),
            crate::core::quantity::Temperature::from_millikelvin(293_150),
        ) {
            Ok(lot) => lot,
            Err(error) => panic!("construction soak initial material failed: {error}"),
        };
        let initial_matter = match calculate_matter_accounting(&state) {
            Ok(accounting) => accounting.total(),
            Err(error) => panic!("construction soak initial matter accounting failed: {error}"),
        };
        let initial_energy = explicit_energy(&registries, &state);

        for step in 0_u64..1_000 {
            let element = member(&registries, &mut state, Mass::from_milligrams(10));
            let construction = match bind_structural_construction_selection(
                &state,
                element,
                source,
                &[MaterialLotSelection::new(lot, Mass::from_milligrams(10))],
            ) {
                Ok(resolution) => resolution,
                Err(error) => panic!("construction soak binding failed at step {step}: {error:?}"),
            };
            let token = match validate_structural_construction(&registries, &state, &construction) {
                Ok(token) => token,
                Err(error) => {
                    panic!("construction soak validation failed at step {step}: {error}")
                }
            };
            if let Err(error) = token.commit(&mut state) {
                panic!("construction soak commit failed at step {step}: {error}");
            }

            let activation =
                match validate_activate_structural_element(&registries, &state, element) {
                    Ok(token) => token,
                    Err(error) => {
                        panic!("construction soak activation failed at step {step}: {error}")
                    }
                };
            if let Err(error) = activation.commit(&mut state) {
                panic!("construction soak activation commit failed at step {step}: {error}");
            }

            let deconstruction = match validate_structural_deconstruction(
                &registries,
                &state,
                make_test_deconstruction_resolution(element, destination),
            ) {
                Ok(token) => token,
                Err(error) => {
                    panic!("construction soak deconstruction failed at step {step}: {error}")
                }
            };
            let outcome = match deconstruction.commit(&mut state) {
                Ok(outcome) => outcome,
                Err(error) => {
                    panic!("construction soak deconstruction commit failed at step {step}: {error}")
                }
            };
            assert_eq!(outcome.recovered_lots().len(), 1);
            lot = outcome.recovered_lots()[0];
            std::mem::swap(&mut source, &mut destination);

            if let Err(error) = advance_tick(&registries, &mut state) {
                panic!("construction soak tick failed at step {step}: {error}");
            }
            if step.is_multiple_of(97) {
                if let Err(error) = validate_loaded_state(&registries, &state) {
                    panic!("construction soak exhaustive audit failed at step {step}: {error}");
                }
                let matter = match calculate_matter_accounting(&state) {
                    Ok(accounting) => accounting.total(),
                    Err(error) => {
                        panic!("construction soak matter accounting failed at step {step}: {error}")
                    }
                };
                assert_eq!(matter, initial_matter);
                assert_eq!(explicit_energy(&registries, &state), initial_energy);
            }
        }

        assert_eq!(state.structures().elements().count(), 0);
        assert_eq!(
            state
                .inventory()
                .get_stockpile(source)
                .map(|stockpile| stockpile.stored_mass()),
            Some(Mass::from_milligrams(10))
        );
        assert_eq!(
            state
                .inventory()
                .get_stockpile(destination)
                .map(|stockpile| stockpile.stored_mass()),
            Some(Mass::ZERO)
        );
        assert_eq!(state.tick().value(), 1_000);
        assert_eq!(
            calculate_matter_accounting(&state).map(|accounting| accounting.total()),
            Ok(initial_matter)
        );
        assert_eq!(explicit_energy(&registries, &state), initial_energy);
        state
    }

    #[test]
    fn construction_deconstruction_soak_preserves_conservation_and_replay() {
        let seed = WorldSeed::new(0x5C00_5000);
        let first = run_construction_ownership_soak(seed);
        let second = run_construction_ownership_soak(seed);
        assert_eq!(first, second);
    }
}

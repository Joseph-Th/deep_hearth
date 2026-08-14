//! Persistent structural members and synchronized support indexes; sibling execution owns every mutation path.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Deserializer, Serialize};

use crate::core::quantity::{Acceleration, AggregateMass, Area, Force, Length, Mass};
use crate::core::time::SimulationTick;
use crate::inventory::ConsumedMaterialTrace;
use crate::material::{
    MaterialId, MaterialPhase, MaterialPhaseStateError, MaterialRegistry, ParticleSizeStatePolicy,
    validate_material_phase_state,
};
use crate::spatial::VoxelBounds;

use super::definitions::{StructuralProfileId, StructuralRegistry};
use super::geometry::{StructuralGeometryError, calculate_prismatic_material_mass_ceiling};
use super::load::calculate_aggregate_weight_force_ceiling;

/// Persistent identifier for one structural member record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct StructuralElementId(u32);

impl StructuralElementId {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        assert!(value != 0, "structural element id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Structural lifecycle separates construction configuration from load-bearing participation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum StructuralLifecycle {
    Planned,
    Active,
    Failed,
}

/// Physical origin of an externally resolved load contribution.
///
/// Each owning system updates only its own contribution so unrelated causes cannot overwrite one
/// another. Structural analysis consumes the sum and does not invent these source values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum StructuralLoadKind {
    SelfWeight,
    Permanent,
    StoredMatter,
    Equipment,
    Fluid,
    Snow,
    Wind,
    Occupancy,
}

/// Immutable physical geometry of one structural member.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct StructuralElementGeometry {
    pub(super) bounds: VoxelBounds,
    pub(super) length: Length,
    pub(super) cross_section: Area,
}

impl StructuralElementGeometry {
    /// Builds validated prismatic member geometry before it can enter authoritative state.
    pub fn new(
        bounds: VoxelBounds,
        length: Length,
        cross_section: Area,
    ) -> Result<Self, StructuralGeometryError> {
        let geometry = Self {
            bounds,
            length,
            cross_section,
        };
        geometry.validate()?;
        Ok(geometry)
    }

    /// Rechecks geometry after a serialization or internal trust boundary.
    pub fn validate(self) -> Result<(), StructuralGeometryError> {
        if self.cross_section.is_zero() {
            return Err(StructuralGeometryError::ZeroCrossSection);
        }
        if self.length.is_zero() {
            return Err(StructuralGeometryError::ZeroLength);
        }
        Ok(())
    }

    #[must_use]
    pub const fn bounds(self) -> VoxelBounds {
        self.bounds
    }

    #[must_use]
    pub const fn length(self) -> Length {
        self.length
    }

    #[must_use]
    pub const fn cross_section(self) -> Area {
        self.cross_section
    }
}

#[derive(Deserialize)]
struct StructuralElementGeometryRepresentation {
    bounds: VoxelBounds,
    length: Length,
    cross_section: Area,
}

impl<'de> Deserialize<'de> for StructuralElementGeometry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let representation = StructuralElementGeometryRepresentation::deserialize(deserializer)?;
        Self::new(
            representation.bounds,
            representation.length,
            representation.cross_section,
        )
        .map_err(serde::de::Error::custom)
    }
}

/// Immutable authored/runtime specification of one structural member.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct StructuralElementConfiguration {
    pub(super) profile: StructuralProfileId,
    pub(super) material: MaterialId,
    pub(super) geometry: StructuralElementGeometry,
    #[serde(rename = "grounded")]
    pub(super) is_grounded: bool,
}

/// Persistent physical and lifecycle state for one structural member.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructuralElementRecord {
    pub(super) id: StructuralElementId,
    pub(super) configuration: StructuralElementConfiguration,
    pub(super) embodied_mass: Mass,
    pub(super) embodied_material: Vec<ConsumedMaterialTrace>,
    pub(super) loads: BTreeMap<StructuralLoadKind, Force>,
    pub(super) lifecycle: StructuralLifecycle,
    #[serde(rename = "cracked")]
    pub(super) is_cracked: bool,
    pub(super) created_at: SimulationTick,
}

impl StructuralElementRecord {
    #[must_use]
    pub const fn id(&self) -> StructuralElementId {
        self.id
    }

    #[must_use]
    pub const fn profile(&self) -> StructuralProfileId {
        self.configuration.profile
    }

    #[must_use]
    pub const fn material(&self) -> MaterialId {
        self.configuration.material
    }

    #[must_use]
    pub const fn bounds(&self) -> VoxelBounds {
        self.configuration.geometry.bounds
    }

    #[must_use]
    pub const fn cross_section(&self) -> Area {
        self.configuration.geometry.cross_section
    }

    #[must_use]
    pub const fn length(&self) -> Length {
        self.configuration.geometry.length
    }

    #[must_use]
    pub const fn geometry(&self) -> StructuralElementGeometry {
        self.configuration.geometry
    }

    #[must_use]
    pub const fn is_grounded(&self) -> bool {
        self.configuration.is_grounded
    }

    /// Exact matter currently owned by this structural member.
    #[must_use]
    pub const fn embodied_mass(&self) -> Mass {
        self.embodied_mass
    }

    /// Physical/provenance traces transferred into this member at construction.
    #[must_use]
    pub fn embodied_material(&self) -> &[ConsumedMaterialTrace] {
        &self.embodied_material
    }

    #[must_use]
    pub fn load(&self, kind: StructuralLoadKind) -> Force {
        self.loads.get(&kind).copied().unwrap_or(Force::ZERO)
    }

    pub fn loads(&self) -> impl Iterator<Item = (StructuralLoadKind, Force)> + '_ {
        self.loads.iter().map(|(kind, force)| (*kind, *force))
    }

    #[must_use]
    pub const fn lifecycle(&self) -> StructuralLifecycle {
        self.lifecycle
    }

    #[must_use]
    pub const fn is_cracked(&self) -> bool {
        self.is_cracked
    }

    #[must_use]
    pub const fn created_at(&self) -> SimulationTick {
        self.created_at
    }
}

/// Authoritative structural records plus synchronized forward and reverse support indexes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructureState {
    pub(super) revision: u64,
    pub(super) next_element_id: u32,
    pub(super) elements: BTreeMap<StructuralElementId, StructuralElementRecord>,
    pub(super) supports_by_element: BTreeMap<StructuralElementId, BTreeSet<StructuralElementId>>,
    pub(super) dependents_by_support: BTreeMap<StructuralElementId, BTreeSet<StructuralElementId>>,
}

impl StructureState {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            revision: 0,
            next_element_id: 1,
            elements: BTreeMap::new(),
            supports_by_element: BTreeMap::new(),
            dependents_by_support: BTreeMap::new(),
        }
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn get_element(&self, id: StructuralElementId) -> Option<&StructuralElementRecord> {
        self.elements.get(&id)
    }

    pub fn elements(&self) -> impl Iterator<Item = &StructuralElementRecord> {
        self.elements.values()
    }

    pub fn supports(
        &self,
        id: StructuralElementId,
    ) -> Option<impl Iterator<Item = StructuralElementId> + '_> {
        self.supports_by_element
            .get(&id)
            .map(|entries| entries.iter().copied())
    }

    pub fn dependents(
        &self,
        id: StructuralElementId,
    ) -> Option<impl Iterator<Item = StructuralElementId> + '_> {
        self.dependents_by_support
            .get(&id)
            .map(|entries| entries.iter().copied())
    }

    pub(crate) fn has_valid_id_cursor(&self) -> bool {
        self.next_element_id != 0
            && self
                .elements
                .keys()
                .next_back()
                .is_none_or(|id| id.value() < self.next_element_id)
    }

    pub(crate) fn has_valid_geometry(&self) -> bool {
        self.elements
            .values()
            .all(|record| record.geometry().validate().is_ok())
    }

    pub(crate) fn has_path(&self, from: StructuralElementId, target: StructuralElementId) -> bool {
        let mut pending = BTreeSet::from([from]);
        let mut visited = BTreeSet::new();
        while let Some(current) = pending.pop_first() {
            if current == target {
                return true;
            }
            if !visited.insert(current) {
                continue;
            }
            let Some(supports) = self.supports_by_element.get(&current) else {
                continue;
            };
            pending.extend(supports.iter().copied());
        }
        false
    }
}

/// Exhaustive failure found while validating decoded structural state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StructureValidationError {
    ZeroNextElementId,
    NextElementIdNotAboveAllocated {
        next: u32,
        highest: StructuralElementId,
    },
    ElementKeyMismatch {
        key: StructuralElementId,
        record: StructuralElementId,
    },
    UnknownProfile {
        element: StructuralElementId,
        profile: StructuralProfileId,
    },
    UnknownMaterial {
        element: StructuralElementId,
        material: MaterialId,
    },
    ZeroCrossSection {
        element: StructuralElementId,
    },
    ZeroLength {
        element: StructuralElementId,
    },
    Geometry {
        element: StructuralElementId,
        error: StructuralGeometryError,
    },
    EmbodiedMassGeometryMismatch {
        element: StructuralElementId,
        stored: Mass,
        required: Mass,
    },
    UnmaterializedLoadBearingElement {
        element: StructuralElementId,
        lifecycle: StructuralLifecycle,
    },
    EmbodiedMassMismatch {
        element: StructuralElementId,
        stored: Mass,
        traced: Mass,
    },
    EmbodiedMassOverflow {
        element: StructuralElementId,
    },
    ZeroEmbodiedTrace {
        element: StructuralElementId,
    },
    EmbodiedMaterialMismatch {
        element: StructuralElementId,
        expected: MaterialId,
        found: MaterialId,
    },
    UnsupportedEmbodiedComposition {
        element: StructuralElementId,
        material: MaterialId,
    },
    UnknownEmbodiedCommodity {
        element: StructuralElementId,
    },
    UnsupportedEmbodiedPhase {
        element: StructuralElementId,
        form: crate::material::FormId,
        phase: MaterialPhase,
    },
    UnsupportedEmbodiedParticulateForm {
        element: StructuralElementId,
        form: crate::material::FormId,
    },
    InvalidEmbodiedPhaseState {
        element: StructuralElementId,
        error: MaterialPhaseStateError,
    },
    UnknownEmbodiedCompositionMaterial {
        element: StructuralElementId,
        material: MaterialId,
    },
    InvalidEmbodiedProvenanceRange {
        element: StructuralElementId,
    },
    EmbodiedProvenanceInFuture {
        element: StructuralElementId,
        latest_created_at: SimulationTick,
        current: SimulationTick,
    },
    SelfWeightOverflow {
        element: StructuralElementId,
    },
    SelfWeightMismatch {
        element: StructuralElementId,
        stored: Force,
        expected: Force,
    },
    ZeroLoadContribution {
        element: StructuralElementId,
        kind: StructuralLoadKind,
    },
    CreatedInFuture {
        element: StructuralElementId,
        created_at: SimulationTick,
        current: SimulationTick,
    },
    PlannedElementCracked {
        element: StructuralElementId,
    },
    FailedElementNotCracked {
        element: StructuralElementId,
    },
    MissingSupportIndex {
        element: StructuralElementId,
    },
    OrphanSupportIndex {
        element: StructuralElementId,
    },
    UnknownSupportReference {
        element: StructuralElementId,
        support: StructuralElementId,
    },
    SelfSupport {
        element: StructuralElementId,
    },
    GroundedElementHasSupport {
        element: StructuralElementId,
        support: StructuralElementId,
    },
    ReverseIndexMismatch {
        element: StructuralElementId,
        support: StructuralElementId,
    },
    SupportCycle {
        element: StructuralElementId,
        support: StructuralElementId,
    },
    ActiveElementUnsupported {
        element: StructuralElementId,
    },
}

impl Display for StructureValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroNextElementId => {
                formatter.write_str("structural next-id cursor must be nonzero")
            }
            Self::NextElementIdNotAboveAllocated { next, highest } => write!(
                formatter,
                "structural next-id cursor {next} is not above allocated element {}",
                highest.value()
            ),
            Self::UnsupportedEmbodiedParticulateForm { element, form } => write!(
                formatter,
                "structural element {} directly embodies particulate form {} without consolidation physics",
                element.value(),
                form.value()
            ),
            Self::ZeroLength { element } => write!(
                formatter,
                "structural element {} has zero physical length",
                element.value()
            ),
            Self::Geometry { element, error } => write!(
                formatter,
                "structural element {} has invalid physical geometry: {error}",
                element.value()
            ),
            Self::EmbodiedMassGeometryMismatch {
                element,
                stored,
                required,
            } => write!(
                formatter,
                "structural element {} owns {} mg but its geometry and material density require {} mg",
                element.value(),
                stored.milligrams(),
                required.milligrams()
            ),
            Self::ElementKeyMismatch { key, record } => write!(
                formatter,
                "structural element map key {} disagrees with record id {}",
                key.value(),
                record.value()
            ),
            Self::UnknownProfile { element, profile } => write!(
                formatter,
                "structural element {} references unknown profile {}",
                element.value(),
                profile.value()
            ),
            Self::UnknownMaterial { element, material } => write!(
                formatter,
                "structural element {} references unknown material {}",
                element.value(),
                material.value()
            ),
            Self::ZeroCrossSection { element } => write!(
                formatter,
                "structural element {} has zero cross-sectional area",
                element.value()
            ),
            Self::UnmaterializedLoadBearingElement { element, lifecycle } => write!(
                formatter,
                "structural element {} is {lifecycle:?} without embodied construction matter",
                element.value()
            ),
            Self::EmbodiedMassMismatch {
                element,
                stored,
                traced,
            } => write!(
                formatter,
                "structural element {} stores {} mg embodied mass but traces own {} mg",
                element.value(),
                stored.milligrams(),
                traced.milligrams()
            ),
            Self::EmbodiedMassOverflow { element } => write!(
                formatter,
                "structural element {} embodied traces overflow single-member mass storage",
                element.value()
            ),
            Self::ZeroEmbodiedTrace { element } => write!(
                formatter,
                "structural element {} contains a zero-mass embodied trace",
                element.value()
            ),
            Self::EmbodiedMaterialMismatch {
                element,
                expected,
                found,
            } => write!(
                formatter,
                "structural element {} is authored as material {} but owns commodity material {}",
                element.value(),
                expected.value(),
                found.value()
            ),
            Self::UnsupportedEmbodiedComposition { element, material } => write!(
                formatter,
                "structural element {} uses single-material strength for material {} but its embodied matter is not pure",
                element.value(),
                material.value()
            ),
            Self::UnknownEmbodiedCommodity { element } => write!(
                formatter,
                "structural element {} owns an unknown material/form commodity",
                element.value()
            ),
            Self::UnsupportedEmbodiedPhase {
                element,
                form,
                phase,
            } => write!(
                formatter,
                "structural element {} owns {phase:?} material form {}; structural embodiment must be solid",
                element.value(),
                form.value()
            ),
            Self::InvalidEmbodiedPhaseState { element, error } => write!(
                formatter,
                "structural element {} has invalid embodied material phase state: {error}",
                element.value()
            ),
            Self::UnknownEmbodiedCompositionMaterial { element, material } => write!(
                formatter,
                "structural element {} embodied composition references unknown material {}",
                element.value(),
                material.value()
            ),
            Self::InvalidEmbodiedProvenanceRange { element } => write!(
                formatter,
                "structural element {} embodied material has an inverted provenance range",
                element.value()
            ),
            Self::EmbodiedProvenanceInFuture {
                element,
                latest_created_at,
                current,
            } => write!(
                formatter,
                "structural element {} owns material provenance through tick {} after current tick {}",
                element.value(),
                latest_created_at.value(),
                current.value()
            ),
            Self::SelfWeightOverflow { element } => write!(
                formatter,
                "structural element {} embodied mass exceeds self-weight force range",
                element.value()
            ),
            Self::SelfWeightMismatch {
                element,
                stored,
                expected,
            } => write!(
                formatter,
                "structural element {} stores {} mN self-weight but embodied matter requires {} mN",
                element.value(),
                stored.millinewtons(),
                expected.millinewtons()
            ),
            Self::ZeroLoadContribution { element, kind } => write!(
                formatter,
                "structural element {} stores redundant zero {kind:?} load contribution",
                element.value()
            ),
            Self::CreatedInFuture {
                element,
                created_at,
                current,
            } => write!(
                formatter,
                "structural element {} was created at tick {} after current tick {}",
                element.value(),
                created_at.value(),
                current.value()
            ),
            Self::PlannedElementCracked { element } => write!(
                formatter,
                "planned structural element {} cannot already contain irreversible crack damage",
                element.value()
            ),
            Self::FailedElementNotCracked { element } => write!(
                formatter,
                "failed structural element {} is missing persistent crack damage",
                element.value()
            ),
            Self::MissingSupportIndex { element } => write!(
                formatter,
                "structural element {} is missing synchronized support index entries",
                element.value()
            ),
            Self::OrphanSupportIndex { element } => write!(
                formatter,
                "structural support index contains missing element {}",
                element.value()
            ),
            Self::UnknownSupportReference { element, support } => write!(
                formatter,
                "structural element {} references missing support {}",
                element.value(),
                support.value()
            ),
            Self::SelfSupport { element } => write!(
                formatter,
                "structural element {} cannot support itself",
                element.value()
            ),
            Self::GroundedElementHasSupport { element, support } => write!(
                formatter,
                "ground-anchored structural element {} cannot also route load through support {}",
                element.value(),
                support.value()
            ),
            Self::ReverseIndexMismatch { element, support } => write!(
                formatter,
                "structural support indexes disagree for element {} and support {}",
                element.value(),
                support.value()
            ),
            Self::SupportCycle { element, support } => write!(
                formatter,
                "structural support edge {} -> {} participates in a cycle",
                element.value(),
                support.value()
            ),
            Self::ActiveElementUnsupported { element } => write!(
                formatter,
                "active structural element {} has no active support or ground anchor",
                element.value()
            ),
        }
    }
}

impl Error for StructureValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Geometry { error, .. } => Some(error),
            Self::InvalidEmbodiedPhaseState { error, .. } => Some(error),
            Self::ZeroNextElementId
            | Self::NextElementIdNotAboveAllocated { .. }
            | Self::ElementKeyMismatch { .. }
            | Self::UnknownProfile { .. }
            | Self::UnknownMaterial { .. }
            | Self::ZeroCrossSection { .. }
            | Self::ZeroLength { .. }
            | Self::EmbodiedMassGeometryMismatch { .. }
            | Self::UnmaterializedLoadBearingElement { .. }
            | Self::EmbodiedMassMismatch { .. }
            | Self::EmbodiedMassOverflow { .. }
            | Self::ZeroEmbodiedTrace { .. }
            | Self::EmbodiedMaterialMismatch { .. }
            | Self::UnsupportedEmbodiedComposition { .. }
            | Self::UnknownEmbodiedCommodity { .. }
            | Self::UnsupportedEmbodiedPhase { .. }
            | Self::UnsupportedEmbodiedParticulateForm { .. }
            | Self::UnknownEmbodiedCompositionMaterial { .. }
            | Self::InvalidEmbodiedProvenanceRange { .. }
            | Self::EmbodiedProvenanceInFuture { .. }
            | Self::SelfWeightOverflow { .. }
            | Self::SelfWeightMismatch { .. }
            | Self::ZeroLoadContribution { .. }
            | Self::CreatedInFuture { .. }
            | Self::PlannedElementCracked { .. }
            | Self::FailedElementNotCracked { .. }
            | Self::MissingSupportIndex { .. }
            | Self::OrphanSupportIndex { .. }
            | Self::UnknownSupportReference { .. }
            | Self::SelfSupport { .. }
            | Self::GroundedElementHasSupport { .. }
            | Self::ReverseIndexMismatch { .. }
            | Self::SupportCycle { .. }
            | Self::ActiveElementUnsupported { .. } => None,
        }
    }
}

pub(crate) fn validate_loaded_structure(
    profiles: &StructuralRegistry,
    materials: &MaterialRegistry,
    state: &StructureState,
    current_tick: SimulationTick,
    gravity: Acceleration,
) -> Result<(), StructureValidationError> {
    if state.next_element_id == 0 {
        return Err(StructureValidationError::ZeroNextElementId);
    }
    if let Some(highest) = state.elements.keys().next_back().copied()
        && highest.value() >= state.next_element_id
    {
        return Err(StructureValidationError::NextElementIdNotAboveAllocated {
            next: state.next_element_id,
            highest,
        });
    }

    for (id, record) in &state.elements {
        if *id != record.id {
            return Err(StructureValidationError::ElementKeyMismatch {
                key: *id,
                record: record.id,
            });
        }
        if profiles.get_profile(record.profile()).is_none() {
            return Err(StructureValidationError::UnknownProfile {
                element: record.id,
                profile: record.profile(),
            });
        }
        if materials.get_material(record.material()).is_none() {
            return Err(StructureValidationError::UnknownMaterial {
                element: record.id,
                material: record.material(),
            });
        }
        if record.cross_section().is_zero() {
            return Err(StructureValidationError::ZeroCrossSection { element: record.id });
        }
        if record.length().is_zero() {
            return Err(StructureValidationError::ZeroLength { element: record.id });
        }
        if record.lifecycle != StructuralLifecycle::Planned && record.embodied_mass.is_zero() {
            return Err(StructureValidationError::UnmaterializedLoadBearingElement {
                element: record.id,
                lifecycle: record.lifecycle,
            });
        }
        let mut traced_mass = Mass::ZERO;
        for trace in &record.embodied_material {
            if trace.mass().is_zero() {
                return Err(StructureValidationError::ZeroEmbodiedTrace { element: record.id });
            }
            traced_mass = traced_mass
                .checked_add(trace.mass())
                .ok_or(StructureValidationError::EmbodiedMassOverflow { element: record.id })?;
            let commodity = trace.profile().commodity();
            if !materials.has_commodity(commodity) {
                return Err(StructureValidationError::UnknownEmbodiedCommodity {
                    element: record.id,
                });
            }
            let form = match materials.get_form(commodity.form()) {
                Some(form) => form,
                None => {
                    return Err(StructureValidationError::UnknownEmbodiedCommodity {
                        element: record.id,
                    });
                }
            };
            if form.phase() != MaterialPhase::Solid {
                return Err(StructureValidationError::UnsupportedEmbodiedPhase {
                    element: record.id,
                    form: commodity.form(),
                    phase: form.phase(),
                });
            }
            if form.particle_size_policy() == ParticleSizeStatePolicy::Required {
                return Err(
                    StructureValidationError::UnsupportedEmbodiedParticulateForm {
                        element: record.id,
                        form: commodity.form(),
                    },
                );
            }
            validate_material_phase_state(
                materials,
                commodity,
                trace.profile().composition(),
                trace.profile().temperature(),
            )
            .map_err(
                |error| StructureValidationError::InvalidEmbodiedPhaseState {
                    element: record.id,
                    error,
                },
            )?;
            if commodity.material() != record.material() {
                return Err(StructureValidationError::EmbodiedMaterialMismatch {
                    element: record.id,
                    expected: record.material(),
                    found: commodity.material(),
                });
            }
            if trace.profile().composition()
                != &crate::material::MaterialComposition::pure(record.material())
            {
                return Err(StructureValidationError::UnsupportedEmbodiedComposition {
                    element: record.id,
                    material: record.material(),
                });
            }
            for component in trace.profile().composition().components() {
                if materials.get_material(component.material()).is_none() {
                    return Err(
                        StructureValidationError::UnknownEmbodiedCompositionMaterial {
                            element: record.id,
                            material: component.material(),
                        },
                    );
                }
            }
            let provenance = trace.provenance();
            if provenance.latest_created_at() < provenance.earliest_created_at() {
                return Err(StructureValidationError::InvalidEmbodiedProvenanceRange {
                    element: record.id,
                });
            }
            if provenance.latest_created_at() > current_tick {
                return Err(StructureValidationError::EmbodiedProvenanceInFuture {
                    element: record.id,
                    latest_created_at: provenance.latest_created_at(),
                    current: current_tick,
                });
            }
        }
        if traced_mass != record.embodied_mass {
            return Err(StructureValidationError::EmbodiedMassMismatch {
                element: record.id,
                stored: record.embodied_mass,
                traced: traced_mass,
            });
        }
        if !record.embodied_mass.is_zero() {
            let required = calculate_prismatic_material_mass_ceiling(
                materials,
                record.material(),
                record.cross_section(),
                record.length(),
            )
            .map_err(|error| StructureValidationError::Geometry {
                element: record.id,
                error,
            })?;
            if record.embodied_mass != required {
                return Err(StructureValidationError::EmbodiedMassGeometryMismatch {
                    element: record.id,
                    stored: record.embodied_mass,
                    required,
                });
            }
        }
        let expected_self_weight = calculate_aggregate_weight_force_ceiling(
            AggregateMass::from_mass(record.embodied_mass),
            gravity,
        )
        .ok_or(StructureValidationError::SelfWeightOverflow { element: record.id })?;
        let stored_self_weight = record.load(StructuralLoadKind::SelfWeight);
        if stored_self_weight != expected_self_weight {
            return Err(StructureValidationError::SelfWeightMismatch {
                element: record.id,
                stored: stored_self_weight,
                expected: expected_self_weight,
            });
        }
        if let Some((kind, _)) = record.loads.iter().find(|(_, load)| load.is_zero()) {
            return Err(StructureValidationError::ZeroLoadContribution {
                element: record.id,
                kind: *kind,
            });
        }
        if record.created_at > current_tick {
            return Err(StructureValidationError::CreatedInFuture {
                element: record.id,
                created_at: record.created_at,
                current: current_tick,
            });
        }
        if record.lifecycle == StructuralLifecycle::Planned && record.is_cracked {
            return Err(StructureValidationError::PlannedElementCracked { element: record.id });
        }
        if record.lifecycle == StructuralLifecycle::Failed && !record.is_cracked {
            return Err(StructureValidationError::FailedElementNotCracked { element: record.id });
        }
        if !state.supports_by_element.contains_key(id)
            || !state.dependents_by_support.contains_key(id)
        {
            return Err(StructureValidationError::MissingSupportIndex { element: *id });
        }
    }

    for id in state
        .supports_by_element
        .keys()
        .chain(state.dependents_by_support.keys())
    {
        if !state.elements.contains_key(id) {
            return Err(StructureValidationError::OrphanSupportIndex { element: *id });
        }
    }

    for (element, supports) in &state.supports_by_element {
        for support in supports {
            if element == support {
                return Err(StructureValidationError::SelfSupport { element: *element });
            }
            if state.elements[element].is_grounded() {
                return Err(StructureValidationError::GroundedElementHasSupport {
                    element: *element,
                    support: *support,
                });
            }
            if !state.elements.contains_key(support) {
                return Err(StructureValidationError::UnknownSupportReference {
                    element: *element,
                    support: *support,
                });
            }
            if !state
                .dependents_by_support
                .get(support)
                .is_some_and(|dependents| dependents.contains(element))
            {
                return Err(StructureValidationError::ReverseIndexMismatch {
                    element: *element,
                    support: *support,
                });
            }
            if state.has_path(*support, *element) {
                return Err(StructureValidationError::SupportCycle {
                    element: *element,
                    support: *support,
                });
            }
        }
    }
    for (support, dependents) in &state.dependents_by_support {
        for element in dependents {
            if !state
                .supports_by_element
                .get(element)
                .is_some_and(|supports| supports.contains(support))
            {
                return Err(StructureValidationError::ReverseIndexMismatch {
                    element: *element,
                    support: *support,
                });
            }
        }
    }

    for record in state.elements.values() {
        if record.lifecycle != StructuralLifecycle::Active || record.is_grounded() {
            continue;
        }
        let has_active_support =
            state
                .supports_by_element
                .get(&record.id)
                .is_some_and(|supports| {
                    supports.iter().any(|support| {
                        state.elements.get(support).is_some_and(|candidate| {
                            candidate.lifecycle == StructuralLifecycle::Active
                        })
                    })
                });
        if !has_active_support {
            return Err(StructureValidationError::ActiveElementUnsupported { element: record.id });
        }
    }

    Ok(())
}

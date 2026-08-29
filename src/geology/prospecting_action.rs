//! Timed player field prospecting that converts bounded regional observation into geological knowledge.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::labor::{
    PlayerWork, PlayerWorkCommitError, PlayerWorkStartError, ProspectingMethodId, ProspectingWork,
    ValidatedPlayerWorkStart, validate_player_work_start,
};
use crate::material::MaterialId;
use crate::registry::Registries;
use crate::spatial::{VoxelBounds, VoxelCoord};

use super::prospecting_execution::validate_record_prospecting_at;
use super::{
    GeologicalDepositLifecycle, GeologicalEvidenceKind, GeologicalObservationId,
    MaterialAbundanceEstimate, ProspectingResolution, RecordProspectingError,
    ValidatedGeologicalObservation,
};

/// One player-selected geological prospecting action over an authored-bounded region.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldProspectingRequest {
    method: ProspectingMethodId,
    region: VoxelBounds,
    material: MaterialId,
}

impl FieldProspectingRequest {
    pub const fn new(
        method: ProspectingMethodId,
        region: VoxelBounds,
        material: MaterialId,
    ) -> Self {
        Self {
            method,
            region,
            material,
        }
    }

    #[must_use]
    pub const fn method(self) -> ProspectingMethodId {
        self.method
    }

    #[must_use]
    pub const fn region(self) -> VoxelBounds {
        self.region
    }

    #[must_use]
    pub const fn material(self) -> MaterialId {
        self.material
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldProspectingStartError {
    UnknownMethod { method: ProspectingMethodId },
    UnknownMaterial { material: MaterialId },
    RegionVolumeOverflow,
    RegionTooLarge { actual: u128, maximum: u128 },
    CompletionTickOverflow,
    Work(PlayerWorkStartError),
}

impl Display for FieldProspectingStartError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownMethod { method } => {
                write!(formatter, "unknown prospecting method {}", method.value())
            }
            Self::UnknownMaterial { material } => {
                write!(
                    formatter,
                    "unknown prospecting material {}",
                    material.value()
                )
            }
            Self::RegionVolumeOverflow => {
                formatter.write_str("prospecting region voxel count overflowed")
            }
            Self::RegionTooLarge { actual, maximum } => write!(
                formatter,
                "prospecting region contains {actual} voxels but method allows at most {maximum}"
            ),
            Self::CompletionTickOverflow => {
                formatter.write_str("prospecting completion tick overflowed")
            }
            Self::Work(error) => write!(formatter, "prospecting labor admission failed: {error}"),
        }
    }
}

impl Error for FieldProspectingStartError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Work(error) => Some(error),
            Self::UnknownMethod { .. }
            | Self::UnknownMaterial { .. }
            | Self::RegionVolumeOverflow
            | Self::RegionTooLarge { .. }
            | Self::CompletionTickOverflow => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldProspectingCommitError {
    Work(PlayerWorkCommitError),
}

impl Display for FieldProspectingCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Work(error) => write!(formatter, "prospecting labor commit failed: {error}"),
        }
    }
}

impl Error for FieldProspectingCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Work(error) => Some(error),
        }
    }
}

#[must_use]
pub struct ValidatedFieldProspectingStart {
    work_start: ValidatedPlayerWorkStart,
    work: ProspectingWork,
}

impl ValidatedFieldProspectingStart {
    pub fn commit(self, state: &mut AppState) -> Result<(), FieldProspectingCommitError> {
        self.work_start
            .precheck(state)
            .map_err(FieldProspectingCommitError::Work)?;
        self.work_start.apply(state);
        Ok(())
    }

    #[must_use]
    pub const fn work(&self) -> ProspectingWork {
        self.work
    }
}

pub fn validate_start_field_prospecting(
    registries: &Registries,
    state: &AppState,
    request: FieldProspectingRequest,
) -> Result<ValidatedFieldProspectingStart, FieldProspectingStartError> {
    let method = registries
        .labor()
        .get_prospecting(request.method)
        .copied()
        .ok_or(FieldProspectingStartError::UnknownMethod {
            method: request.method,
        })?;
    if registries
        .materials()
        .get_material(request.material)
        .is_none()
    {
        return Err(FieldProspectingStartError::UnknownMaterial {
            material: request.material,
        });
    }
    let region_voxels = request
        .region
        .voxel_count()
        .ok_or(FieldProspectingStartError::RegionVolumeOverflow)?;
    if region_voxels > method.maximum_region_voxels() {
        return Err(FieldProspectingStartError::RegionTooLarge {
            actual: region_voxels,
            maximum: method.maximum_region_voxels(),
        });
    }
    let completes_at = state
        .tick()
        .checked_add_span(method.duration())
        .ok_or(FieldProspectingStartError::CompletionTickOverflow)?;
    let work = ProspectingWork::new(
        request.method,
        request.region,
        request.material,
        state.tick(),
        completes_at,
    );
    let work_start = validate_player_work_start(
        registries,
        state,
        PlayerWork::Prospecting { work },
        method.duration(),
        method.exertion(),
    )
    .map_err(FieldProspectingStartError::Work)?;
    Ok(ValidatedFieldProspectingStart { work_start, work })
}

/// Observable completion of one field-prospecting action. The hidden geological owner is intentionally absent.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldProspectingOutcome {
    observation: GeologicalObservationId,
    method: ProspectingMethodId,
    region: VoxelBounds,
    material: MaterialId,
    evidence: GeologicalEvidenceKind,
}

impl FieldProspectingOutcome {
    #[must_use]
    pub const fn observation(self) -> GeologicalObservationId {
        self.observation
    }

    #[must_use]
    pub const fn method(self) -> ProspectingMethodId {
        self.method
    }

    #[must_use]
    pub const fn region(self) -> VoxelBounds {
        self.region
    }

    #[must_use]
    pub const fn material(self) -> MaterialId {
        self.material
    }

    #[must_use]
    pub const fn evidence(self) -> GeologicalEvidenceKind {
        self.evidence
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FieldProspectingTickError {
    ObservationIdExhausted,
    KnowledgeRevisionExhausted,
}

pub(crate) struct FieldProspectingTickPlan {
    work: ProspectingWork,
    evidence: GeologicalEvidenceKind,
    observation: ValidatedGeologicalObservation,
}

fn resolve_region_abundance_bounds(
    state: &AppState,
    region: VoxelBounds,
    material: MaterialId,
    uncertainty_ppm: u32,
) -> (u32, u32) {
    let mut minimum = None::<u32>;
    let mut maximum = None::<u32>;
    let mut uncovered = vec![region];
    for deposit in state.geology().deposits().filter(|deposit| {
        deposit.lifecycle() == GeologicalDepositLifecycle::Available
            && deposit.bounds().has_intersection(region)
    }) {
        let abundance = deposit.composition().parts_per_million(material);
        minimum = Some(minimum.map_or(abundance, |current| current.min(abundance)));
        maximum = Some(maximum.map_or(abundance, |current| current.max(abundance)));
        if !uncovered.is_empty() {
            uncovered = uncovered
                .into_iter()
                .flat_map(|bounds| subtract_bounds(bounds, deposit.bounds()))
                .collect();
        }
    }
    let minimum = if uncovered.is_empty() {
        minimum.unwrap_or(0)
    } else {
        0
    };
    let maximum = maximum.unwrap_or(0);
    (
        minimum.saturating_sub(uncertainty_ppm),
        maximum.saturating_add(uncertainty_ppm).min(1_000_000),
    )
}

fn subtract_bounds(bounds: VoxelBounds, cover: VoxelBounds) -> Vec<VoxelBounds> {
    let Some(overlap) = bounds.intersection(cover) else {
        return vec![bounds];
    };
    let min = bounds.min();
    let max = bounds.max_exclusive();
    let overlap_min = overlap.min();
    let overlap_max = overlap.max_exclusive();
    let mut remainder = Vec::with_capacity(6);

    push_bounds(
        &mut remainder,
        VoxelCoord::new(min.x(), min.y(), min.z()),
        VoxelCoord::new(overlap_min.x(), max.y(), max.z()),
    );
    push_bounds(
        &mut remainder,
        VoxelCoord::new(overlap_max.x(), min.y(), min.z()),
        VoxelCoord::new(max.x(), max.y(), max.z()),
    );
    push_bounds(
        &mut remainder,
        VoxelCoord::new(overlap_min.x(), min.y(), min.z()),
        VoxelCoord::new(overlap_max.x(), overlap_min.y(), max.z()),
    );
    push_bounds(
        &mut remainder,
        VoxelCoord::new(overlap_min.x(), overlap_max.y(), min.z()),
        VoxelCoord::new(overlap_max.x(), max.y(), max.z()),
    );
    push_bounds(
        &mut remainder,
        VoxelCoord::new(overlap_min.x(), overlap_min.y(), min.z()),
        VoxelCoord::new(overlap_max.x(), overlap_max.y(), overlap_min.z()),
    );
    push_bounds(
        &mut remainder,
        VoxelCoord::new(overlap_min.x(), overlap_min.y(), overlap_max.z()),
        VoxelCoord::new(overlap_max.x(), overlap_max.y(), max.z()),
    );
    remainder
}

fn push_bounds(remainder: &mut Vec<VoxelBounds>, min: VoxelCoord, max: VoxelCoord) {
    if min.x() >= max.x() || min.y() >= max.y() || min.z() >= max.z() {
        return;
    }
    remainder.push(
        VoxelBounds::new(min, max)
            .unwrap_or_else(|_| unreachable!("positive prospecting remainder bounds are valid")),
    );
}

pub(crate) fn decide_field_prospecting_tick(
    registries: &Registries,
    state: &AppState,
    next_tick: SimulationTick,
) -> Result<Option<FieldProspectingTickPlan>, FieldProspectingTickError> {
    let Some(PlayerWork::Prospecting { work }) = state.player_work().active() else {
        return Ok(None);
    };
    if work.completes_at() != next_tick {
        return Ok(None);
    }
    let method = registries
        .labor()
        .get_prospecting(work.method())
        .copied()
        .unwrap_or_else(|| {
            panic!("runtime invariant broken: due prospecting work has no authored method")
        });
    let (lower_ppm, upper_ppm) = resolve_region_abundance_bounds(
        state,
        work.region(),
        work.material(),
        method.abundance_uncertainty_ppm(),
    );
    let finding = MaterialAbundanceEstimate::new(work.material(), lower_ppm, upper_ppm)
        .unwrap_or_else(|error| {
            panic!("runtime invariant broken: field prospecting derived invalid abundance: {error}")
        });
    let resolution =
        ProspectingResolution::new_runtime(work.region(), method.evidence(), vec![finding]);
    let observation = validate_record_prospecting_at(registries, state, resolution, next_tick)
        .map_err(|error| match error {
            RecordProspectingError::ObservationIdExhausted => {
                FieldProspectingTickError::ObservationIdExhausted
            }
            RecordProspectingError::RevisionExhausted => {
                FieldProspectingTickError::KnowledgeRevisionExhausted
            }
            RecordProspectingError::NoFindings
            | RecordProspectingError::FindingsNotCanonical { .. }
            | RecordProspectingError::ImpossibleLowerBoundTotal { .. }
            | RecordProspectingError::UnknownMaterial { .. } => {
                unreachable!(
                    "runtime field prospecting constructs one canonical known-material finding"
                )
            }
        })?;
    Ok(Some(FieldProspectingTickPlan {
        work,
        evidence: method.evidence(),
        observation,
    }))
}

pub(crate) fn apply_field_prospecting_tick(
    state: &mut AppState,
    plan: Option<FieldProspectingTickPlan>,
) -> Option<FieldProspectingOutcome> {
    let plan = plan?;
    let observation = plan.observation.apply_prechecked(state);
    Some(FieldProspectingOutcome {
        observation,
        method: plan.work.method(),
        region: plan.work.region(),
        material: plan.work.material(),
        evidence: plan.evidence,
    })
}

#[cfg(test)]
#[path = "prospecting_action_tests.rs"]
mod tests;

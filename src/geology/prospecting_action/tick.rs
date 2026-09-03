//! Completion planning and atomic tick application for field prospecting.

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::labor::{
    PlayerWork, ProspectingMethodId, ProspectingSpatialResolution, ProspectingWork,
};
use crate::material::MaterialId;
use crate::registry::Registries;
use crate::spatial::{VoxelBounds, VoxelCoord};

use super::super::prospecting_execution::validate_record_prospecting_batch_at;
use super::super::{
    GeologicalEvidenceKind, GeologicalObservationId, MaterialAbundanceEstimate,
    ProspectingResolution, RecordProspectingError, ValidatedGeologicalObservation,
};
use super::abundance::resolve_region_abundance_bounds;
use super::hardness::resolve_region_excavation_hardness;

/// Observable completion of one field-prospecting action. The hidden geological owner is intentionally absent.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldProspectingOutcome {
    first_observation: GeologicalObservationId,
    observation_count: u32,
    method: ProspectingMethodId,
    region: VoxelBounds,
    material: MaterialId,
    evidence: GeologicalEvidenceKind,
}

impl FieldProspectingOutcome {
    #[must_use]
    pub const fn observation(self) -> GeologicalObservationId {
        self.first_observation
    }

    /// Number of persistent observations created by this one prospecting action.
    #[must_use]
    pub const fn observation_count(self) -> u32 {
        self.observation_count
    }

    /// Persistent observation identities created by this action in stable spatial order.
    pub fn observations(self) -> impl Iterator<Item = GeologicalObservationId> {
        let first = self.first_observation.value();
        (0..self.observation_count).map(move |offset| {
            GeologicalObservationId::new(
                first
                    .checked_add(offset)
                    .unwrap_or_else(|| unreachable!("validated observation range cannot overflow")),
            )
        })
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
    ObservationId,
    KnowledgeRevision,
    EquipmentRevision,
}

pub(crate) struct FieldProspectingTickPlan {
    work: ProspectingWork,
    evidence: GeologicalEvidenceKind,
    observations: Vec<ValidatedGeologicalObservation>,
}

fn prospecting_observation_regions(
    resolution: ProspectingSpatialResolution,
    region: VoxelBounds,
) -> Vec<VoxelBounds> {
    match resolution {
        ProspectingSpatialResolution::AggregateRegion => vec![region],
        ProspectingSpatialResolution::PerVoxel => {
            let min = region.min();
            let max = region.max_exclusive();
            let mut regions = Vec::new();
            for x in min.x()..max.x() {
                for y in min.y()..max.y() {
                    for z in min.z()..max.z() {
                        regions.push(
                            VoxelBounds::new(
                                VoxelCoord::new(x, y, z),
                                VoxelCoord::new(x + 1, y + 1, z + 1),
                            )
                            .unwrap_or_else(|error| {
                                unreachable!("subdividing validated voxel bounds failed: {error}")
                            }),
                        );
                    }
                }
            }
            regions
        }
    }
}

impl FieldProspectingTickPlan {
    pub(crate) const fn equipment_revision_steps(&self) -> u64 {
        if self.work.equipment().is_some() {
            1
        } else {
            0
        }
    }
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
    if work.equipment().is_some() {
        state
            .equipment()
            .revision()
            .checked_add(1)
            .ok_or(FieldProspectingTickError::EquipmentRevision)?;
    }
    let method = registries
        .labor()
        .get_prospecting(work.method())
        .copied()
        .unwrap_or_else(|| {
            panic!("runtime invariant broken: due prospecting work has no authored method")
        });
    let resolutions = prospecting_observation_regions(method.spatial_resolution(), work.region())
        .into_iter()
        .map(|region| {
            let (lower_ppm, upper_ppm) = resolve_region_abundance_bounds(
                state,
                region,
                work.material(),
                method.abundance_uncertainty_ppm(),
            );
            let finding = MaterialAbundanceEstimate::new(work.material(), lower_ppm, upper_ppm)
                .unwrap_or_else(|error| {
                    panic!("runtime invariant broken: field prospecting derived invalid abundance: {error}")
                });
            let excavation_hardness = method
                .excavation_hardness_resolution()
                .filter(|_| finding.lower_ppm() > 0)
                .and_then(|resolution| {
                    resolve_region_excavation_hardness(
                        state,
                        region,
                        work.material(),
                        resolution,
                    )
                });
            ProspectingResolution::new_runtime(
                region,
                method.evidence(),
                vec![finding],
                excavation_hardness,
            )
        })
        .collect();
    let observations = validate_record_prospecting_batch_at(
        registries,
        state,
        resolutions,
        next_tick,
    )
    .map_err(|error| match error {
        RecordProspectingError::ObservationIdExhausted => FieldProspectingTickError::ObservationId,
        RecordProspectingError::RevisionExhausted => FieldProspectingTickError::KnowledgeRevision,
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
        observations,
    }))
}

pub(crate) fn apply_field_prospecting_tick(
    state: &mut AppState,
    plan: Option<FieldProspectingTickPlan>,
) -> Option<FieldProspectingOutcome> {
    let plan = plan?;
    if let Some(trace) = plan.work.equipment_trace() {
        let condition_after = plan.work.condition_after().unwrap_or_else(|| {
            panic!("runtime invariant broken: prospecting equipment has no wear outcome")
        });
        let equipment_revision = state.equipment().revision();
        let next_equipment_revision = equipment_revision
            .checked_add(1)
            .unwrap_or_else(|| panic!("prevalidated prospecting equipment revision exhausted"));
        let record = state
            .equipment()
            .get_equipment(trace.equipment())
            .unwrap_or_else(|| {
                panic!("runtime invariant broken: prospecting equipment disappeared")
            });
        assert_eq!(record.definition(), trace.definition());
        assert_eq!(record.condition(), trace.condition());
        state.equipment_state_mut().apply_condition_change(
            trace.equipment(),
            trace.condition(),
            condition_after,
            next_equipment_revision,
        );
    }
    let observation_count = u32::try_from(plan.observations.len())
        .unwrap_or_else(|_| unreachable!("validated prospecting observation count must fit u32"));
    let mut observations = plan.observations.into_iter();
    let first_observation = observations
        .next()
        .unwrap_or_else(|| unreachable!("prospecting completion must contain an observation"))
        .apply_prechecked(state);
    for observation in observations {
        observation.apply_prechecked(state);
    }
    Some(FieldProspectingOutcome {
        first_observation,
        observation_count,
        method: plan.work.method(),
        region: plan.work.region(),
        material: plan.work.material(),
        evidence: plan.evidence,
    })
}

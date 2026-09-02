//! Converts acquired geological evidence into an opaque extraction target without exposing hidden
//! deposit identity to player-facing code.

use std::error::Error;
use std::fmt::{Debug, Display, Formatter};

use crate::core::state::AppState;
use crate::geology::{
    GeologicalDepositId, GeologicalDepositLifecycle, GeologicalEvidenceConsistency,
    assess_geological_knowledge,
};
use crate::material::MaterialId;
use crate::spatial::VoxelBounds;

/// Player-facing request to turn acquired evidence in one region into an actionable mining target.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MiningTargetRequest {
    region: VoxelBounds,
    material: MaterialId,
}

impl MiningTargetRequest {
    pub const fn new(region: VoxelBounds, material: MaterialId) -> Self {
        Self { region, material }
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

/// Opaque proof that acquired evidence identifies one currently available geological owner.
///
/// The exact deposit identity remains crate-private. Mining re-resolves the evidence locality when
/// this proof is consumed so unrelated geological or knowledge changes do not invalidate it while
/// new local ambiguity or contradiction still does.
#[must_use]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct MiningTargetResolution {
    pub(super) deposit: GeologicalDepositId,
    region: VoxelBounds,
    material: MaterialId,
}

impl Debug for MiningTargetResolution {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MiningTargetResolution")
            .field("region", &self.region)
            .field("material", &self.material)
            .finish_non_exhaustive()
    }
}

impl MiningTargetResolution {
    #[must_use]
    pub const fn region(self) -> VoxelBounds {
        self.region
    }

    #[must_use]
    pub const fn material(self) -> MaterialId {
        self.material
    }

    pub(super) fn still_resolves(self, state: &AppState) -> bool {
        resolve_mining_target(state, MiningTargetRequest::new(self.region, self.material))
            .is_ok_and(|current| current.deposit == self.deposit)
    }
}

/// Why acquired geological evidence cannot yet be converted into one extraction target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MiningTargetResolutionError {
    NoEvidence {
        material: MaterialId,
        region: VoxelBounds,
    },
    SpatiallyIncomparableEvidence {
        material: MaterialId,
        region: VoxelBounds,
    },
    ConflictingEvidence {
        material: MaterialId,
        region: VoxelBounds,
        highest_lower_ppm: u32,
        lowest_upper_ppm: u32,
    },
    EvidenceRulesOutMaterial {
        material: MaterialId,
        region: VoxelBounds,
    },
    EvidenceInsufficientToResolveTarget {
        material: MaterialId,
        region: VoxelBounds,
    },
}

impl Display for MiningTargetResolutionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoEvidence { material, .. } => write!(
                formatter,
                "no acquired geological evidence covers material {} in the requested region",
                material.value()
            ),
            Self::SpatiallyIncomparableEvidence { material, .. } => write!(
                formatter,
                "acquired evidence for material {} covers disjoint localities and cannot identify one mining target",
                material.value()
            ),
            Self::ConflictingEvidence {
                material,
                highest_lower_ppm,
                lowest_upper_ppm,
                ..
            } => write!(
                formatter,
                "acquired evidence for material {} conflicts between {} and {} ppm",
                material.value(),
                highest_lower_ppm,
                lowest_upper_ppm
            ),
            Self::EvidenceRulesOutMaterial { material, .. } => write!(
                formatter,
                "acquired evidence rules out material {} in the requested region",
                material.value()
            ),
            Self::EvidenceInsufficientToResolveTarget { material, .. } => write!(
                formatter,
                "acquired evidence for material {} does not yet resolve a unique extraction target",
                material.value()
            ),
        }
    }
}

impl Error for MiningTargetResolutionError {}

/// Resolves acquired evidence into exactly one hidden geological owner.
///
/// Compatible evidence is allowed to narrow a locality only through overlap between the acquired
/// observation footprints themselves. A narrower request cannot manufacture spatial precision from
/// broad evidence. Evidence with no shared locality, explicit contradiction, a zero upper abundance
/// bound, or a remaining multi-voxel acquired footprint cannot authorize extraction. Once acquired
/// evidence genuinely localizes to one voxel, hidden geology is consulted only to bind the opaque
/// authorization; zero or multiple compatible live deposits use the same non-oracular failure.
pub fn resolve_mining_target(
    state: &AppState,
    request: MiningTargetRequest,
) -> Result<MiningTargetResolution, MiningTargetResolutionError> {
    let assessment = assess_geological_knowledge(
        state.geological_knowledge(),
        request.region,
        request.material,
    );
    let (evidence_region, lower_ppm, upper_ppm) = match assessment.consistency() {
        GeologicalEvidenceConsistency::NoEvidence => {
            return Err(MiningTargetResolutionError::NoEvidence {
                material: request.material,
                region: request.region,
            });
        }
        GeologicalEvidenceConsistency::SpatiallyIncomparable => {
            return Err(MiningTargetResolutionError::SpatiallyIncomparableEvidence {
                material: request.material,
                region: request.region,
            });
        }
        GeologicalEvidenceConsistency::Conflicting {
            highest_lower_ppm,
            lowest_upper_ppm,
        } => {
            return Err(MiningTargetResolutionError::ConflictingEvidence {
                material: request.material,
                region: request.region,
                highest_lower_ppm,
                lowest_upper_ppm,
            });
        }
        GeologicalEvidenceConsistency::Compatible {
            lower_ppm,
            upper_ppm,
        } => {
            if upper_ppm == 0 {
                return Err(MiningTargetResolutionError::EvidenceRulesOutMaterial {
                    material: request.material,
                    region: request.region,
                });
            }
            if lower_ppm == 0 {
                return Err(
                    MiningTargetResolutionError::EvidenceInsufficientToResolveTarget {
                        material: request.material,
                        region: assessment.common_evidence_region().unwrap_or_else(|| {
                            unreachable!(
                                "compatible geological evidence must share a common region"
                            )
                        }),
                    },
                );
            }
            (
                assessment.common_evidence_region().unwrap_or_else(|| {
                    unreachable!("compatible geological evidence must share a common region")
                }),
                lower_ppm,
                upper_ppm,
            )
        }
    };

    let acquired_region = assessment.common_acquired_region().unwrap_or_else(|| {
        unreachable!("compatible geological evidence must have a common acquired locality")
    });
    if acquired_region.voxel_count() != Some(1) {
        return Err(
            MiningTargetResolutionError::EvidenceInsufficientToResolveTarget {
                material: request.material,
                region: acquired_region,
            },
        );
    }

    let mut matching = state.geology().deposits().filter(|deposit| {
        let abundance = deposit.composition().parts_per_million(request.material);
        deposit.lifecycle() == GeologicalDepositLifecycle::Available
            && deposit.bounds().has_intersection(evidence_region)
            && abundance != 0
            && abundance >= lower_ppm
            && abundance <= upper_ppm
    });
    let Some(deposit) = matching.next() else {
        return Err(
            MiningTargetResolutionError::EvidenceInsufficientToResolveTarget {
                material: request.material,
                region: evidence_region,
            },
        );
    };
    if matching.next().is_some() {
        return Err(
            MiningTargetResolutionError::EvidenceInsufficientToResolveTarget {
                material: request.material,
                region: evidence_region,
            },
        );
    }

    Ok(MiningTargetResolution {
        deposit: deposit.id(),
        region: evidence_region,
        material: request.material,
    })
}

#[cfg(test)]
#[path = "target_resolution_tests.rs"]
mod tests;

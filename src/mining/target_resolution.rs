//! Converts acquired geological evidence into an opaque extraction target without exposing hidden
//! deposit identity to player-facing code.

use std::error::Error;
use std::fmt::{Display, Formatter};

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
/// The exact deposit identity remains crate-private. Mining consumes this proof and rechecks the
/// geology and knowledge revisions so stale evidence cannot silently authorize a different world.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MiningTargetResolution {
    pub(super) deposit: GeologicalDepositId,
    pub(super) expected_geology_revision: u64,
    pub(super) expected_knowledge_revision: u64,
    region: VoxelBounds,
    material: MaterialId,
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
/// Compatible evidence is allowed to narrow the requested region through its common spatial
/// overlap. Evidence with no shared locality, explicit contradiction, or a zero upper abundance
/// bound cannot authorize extraction. When the evidence does not identify exactly one live deposit,
/// the player needs more localized information rather than receiving a hidden presence/count tie-break.
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
        expected_geology_revision: state.geology().revision(),
        expected_knowledge_revision: state.geological_knowledge().revision(),
        region: evidence_region,
        material: request.material,
    })
}

#[cfg(test)]
#[path = "target_resolution_tests.rs"]
mod tests;

//! Lightweight focused-probe case identity shared by runners and contract-only targets.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FocusedProbeRole {
    MaintainedAnchor,
    MaintainedCoverage,
    OrganicVariation,
    ExplicitReplay,
}

impl FocusedProbeRole {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::MaintainedAnchor => "anchor",
            Self::MaintainedCoverage => "coverage",
            Self::OrganicVariation => "organic",
            Self::ExplicitReplay => "replay",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FocusedProbeCase {
    world_seed: u64,
    behavior_seed: Option<u64>,
    role: FocusedProbeRole,
}

impl FocusedProbeCase {
    pub(crate) const fn new(
        world_seed: u64,
        behavior_seed: Option<u64>,
        role: FocusedProbeRole,
    ) -> Self {
        Self {
            world_seed,
            behavior_seed,
            role,
        }
    }

    /// Physical/scenario variation only. Actor preferences must not feed back into this seed.
    pub(crate) const fn seed(self) -> u64 {
        self.world_seed
    }

    pub(crate) const fn behavior_seed(self) -> Option<u64> {
        self.behavior_seed
    }

    pub(crate) const fn role(self) -> FocusedProbeRole {
        self.role
    }
}

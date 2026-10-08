//! Lightweight focused-probe case identity shared by runners and contract-only targets.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FocusedProbeRole {
    MaintainedAnchor,
    MaintainedCoverage,
    OrganicVariation,
    ExplicitReplay,
}

impl FocusedProbeRole {
    #[allow(
        dead_code,
        reason = "lightweight contract targets may consume role identity without report labels"
    )]
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

    #[allow(
        dead_code,
        reason = "lightweight contract targets may consume world identity without actor-policy entropy"
    )]
    pub(crate) const fn behavior_seed(self) -> Option<u64> {
        self.behavior_seed
    }

    /// Returns the independent actor-policy seed required by choice-rich organic/replay probes.
    ///
    /// Missing policy entropy is a harness configuration error, not permission to fall back to a
    /// maintained baseline: doing so would silently turn organic evaluation into a rigid witness.
    #[allow(
        dead_code,
        reason = "only choice-rich focused probes consume independent actor behavior entropy"
    )]
    pub(crate) fn required_behavior_seed(self, context: &'static str) -> u64 {
        self.behavior_seed.unwrap_or_else(|| {
            panic!("gameplay harness {context} requires an independent actor behavior seed")
        })
    }

    pub(crate) const fn role(self) -> FocusedProbeRole {
        self.role
    }
}

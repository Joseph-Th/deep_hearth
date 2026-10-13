//! Authored direct player labor for materializing and setting structural members.

use serde::{Deserialize, Serialize};

use crate::core::quantity::Mass;
use crate::core::time::TickSpan;
use crate::survival::SurvivalExertion;

/// Stable authored identity for one direct structural-construction labor method.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct StructuralConstructionMethodId(u32);

impl StructuralConstructionMethodId {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        assert!(
            value != 0,
            "structural construction method id must be nonzero"
        );
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Authored attention and physiological cost for hand-setting one already-shaped member.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StructuralConstructionDefinition {
    id: StructuralConstructionMethodId,
    maximum_member_mass: Mass,
    maximum_member_duration: TickSpan,
    exertion: SurvivalExertion,
}

impl StructuralConstructionDefinition {
    #[must_use]
    pub fn new(
        id: StructuralConstructionMethodId,
        maximum_member_mass: Mass,
        maximum_member_duration: TickSpan,
        exertion: SurvivalExertion,
    ) -> Self {
        assert!(!maximum_member_mass.is_zero());
        assert!(!maximum_member_duration.is_zero());
        exertion.assert_active_player_work();
        Self {
            id,
            maximum_member_mass,
            maximum_member_duration,
            exertion,
        }
    }

    #[must_use]
    pub const fn id(self) -> StructuralConstructionMethodId {
        self.id
    }

    #[must_use]
    pub const fn maximum_member_mass(self) -> Mass {
        self.maximum_member_mass
    }

    #[must_use]
    pub const fn exertion(self) -> SurvivalExertion {
        self.exertion
    }

    #[must_use]
    pub fn duration_for_mass(self, mass: Mass) -> Option<TickSpan> {
        if mass.is_zero() || mass > self.maximum_member_mass {
            return None;
        }
        let ticks = u128::from(mass.milligrams())
            .checked_mul(u128::from(self.maximum_member_duration.value()))?
            .div_ceil(u128::from(self.maximum_member_mass.milligrams()));
        Some(TickSpan::new(u64::try_from(ticks).ok()?.max(1)))
    }
}
